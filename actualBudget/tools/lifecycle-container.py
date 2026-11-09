"""Real Docker disk-full and killed-request recovery; disposable resources only."""
import argparse
import concurrent.futures
import hashlib
import json
from pathlib import Path
import secrets
import subprocess
import time
import urllib.error
import urllib.request
import uuid


def docker(*args, check=True):
    return subprocess.run(["docker", *args], check=check, capture_output=True, text=True)


def request(base, path, method="GET", body=None, key=None, epoch=None):
    raw = body if isinstance(body, bytes) else None if body is None else json.dumps(body).encode()
    headers = {"Content-Type": "application/zip" if isinstance(body, bytes) else "application/json"}
    if key: headers["Idempotency-Key"] = key
    if epoch: headers["X-Actual-Budget-Epoch"] = epoch
    req = urllib.request.Request(base + "/api/v1" + path, data=raw, method=method, headers=headers)
    try: response = urllib.request.urlopen(req, timeout=60)
    except urllib.error.HTTPError as error: response = error
    with response:
        content = response.read()
        return response.status, content if response.headers.get_content_type() == "application/zip" else json.loads(content)


def good(*args, **kwargs):
    status, body = request(*args, **kwargs)
    assert status < 300, (status, body)
    return body["data"] if isinstance(body, dict) else body


def ready(name):
    address = docker("port", name, "5006/tcp").stdout.strip()
    base = "http://" + address
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        try:
            good(base, "/info")
            return base
        except (OSError, AssertionError): time.sleep(.05)
    raise RuntimeError("Container did not start: " + docker("logs", name).stdout)


def launch(name, image, mount):
    docker("run", "-d", "--name", name, "--read-only", "--cap-drop=ALL",
           "--security-opt=no-new-privileges", "--tmpfs", "/tmp:rw,size=64m",
           *mount, "-p", "127.0.0.1::5006", image)
    return ready(name)


def seed(base):
    budget, account = str(uuid.uuid4()), str(uuid.uuid4())
    good(base, "/budgets", "POST", {"id": budget, "name": "Fault rehearsal"}, "budget")
    root = "/budgets/" + budget
    good(base, root + "/accounts", "POST", {"id": account, "name": "Spending", "opening_balance": 0, "opening_date": "2026-01-01"}, "account")
    # Incompressible note content makes a recovery point allocate real filesystem blocks.
    for n in range(350):
        good(base, root + "/transactions", "POST", {"id": str(uuid.uuid4()), "account_id": account,
             "date": "2026-01-01", "amount": -1, "notes": secrets.token_hex(3000)}, f"expense-{n}")
    point = good(base, root + "/backups", "POST", {"label": "Before change"}, "backup")
    original = good(base, root + "/backups/" + point["id"] + "/download")
    good(base, root, "PATCH", {"name": "Current state"}, "rename")
    return root, point, hashlib.sha256(original).hexdigest()


def snapshot(base, root):
    budget = good(base, root)
    return {"name": budget["name"], "epoch": budget["epoch"], "generation": budget["generation"],
            "accounts": good(base, root + "/accounts"), "transactions": good(base, root + "/transactions?limit=1000")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", help="Use an existing local image; never remove it")
    args = parser.parse_args()
    suffix = uuid.uuid4().hex[:12]
    image = args.image or "actualbudget-lifecycle-fault:" + suffix
    containers = []
    volume = "actualbudget-lifecycle-fault-" + suffix
    try:
        if not args.image:
            subprocess.run(["docker", "build", "-t", image, str(Path(__file__).resolve().parents[1])], check=True)
        print("Image", docker("image", "inspect", image, "--format", "{{.Id}} {{.Size}} {{.Config.User}}").stdout.strip(), flush=True)
        name = "actualbudget-full-" + suffix
        containers.append(name)
        base = launch(name, image, ["--tmpfs", "/data:rw,size=128m,uid=10001,gid=10001"])
        root, point, sha = seed(base)
        before = snapshot(base, root)
        fill = docker("exec", name, "dd", "if=/dev/zero", "of=/data/fault-rehearsal-fill", "bs=4096", check=False)
        assert fill.returncode != 0 and "No space left" in fill.stderr, fill.stderr
        status, failure = request(base, root + "/backups/" + point["id"] + "/restore", "POST",
                                  {"confirm_name": "Current state"}, "disk-full-restore", before["epoch"])
        assert status >= 500, ("Expected genuine ENOSPC failure", status, failure)
        docker("exec", name, "rm", "/data/fault-rehearsal-fill")
        assert snapshot(base, root) == before, "Failed restore changed financial state or generation"
        assert hashlib.sha256(good(base, root + "/backups/" + point["id"] + "/download")).hexdigest() == sha
        result = good(base, root + "/backups/" + point["id"] + "/restore", "POST",
                      {"confirm_name": "Current state"}, "disk-full-restore", before["epoch"])
        assert result["budget"]["generation"] == before["generation"] + 1
        assert good(base, root)["name"] == "Fault rehearsal"
        print("PASS real ENOSPC restore rolls back, retains original backup, retries after freeing space", flush=True)
        docker("rm", "-f", name)

        docker("volume", "create", volume)
        name = "actualbudget-kill-" + suffix
        containers.append(name)
        base = launch(name, image, ["--mount", f"type=volume,src={volume},dst=/data"])
        root, point, sha = seed(base)
        before = snapshot(base, root)
        path = root + "/backups/" + point["id"] + "/restore"
        with concurrent.futures.ThreadPoolExecutor(max_workers=1) as executor:
            future = executor.submit(request, base, path, "POST", {"confirm_name": "Current state"}, "interrupted-restore", before["epoch"])
            time.sleep(.025)
            assert not future.done(), "Restore completed before fault injection; increase the fixture before claiming interrupted-request coverage"
            docker("kill", "--signal=KILL", name)
            try: future.result(timeout=65)
            except (OSError, ValueError): pass
        docker("start", name)
        base = ready(name)
        recovered = snapshot(base, root)
        assert (recovered["name"], recovered["generation"]) in [("Current state", before["generation"]), ("Fault rehearsal", before["generation"] + 1)]
        assert recovered["accounts"] == before["accounts"]
        assert recovered["transactions"] == before["transactions"]
        first = good(base, path, "POST", {"confirm_name": "Current state"}, "interrupted-restore", before["epoch"])
        second = good(base, path, "POST", {"confirm_name": "Current state"}, "interrupted-restore", before["epoch"])
        assert first == second and first["budget"]["generation"] == before["generation"] + 1
        assert hashlib.sha256(good(base, root + "/backups/" + point["id"] + "/download")).hexdigest() == sha
        print("PASS SIGKILL during in-flight restore: whole old/new state, durable volume, exact replay once", flush=True)
    finally:
        for name in containers: docker("rm", "-f", name, check=False)
        docker("volume", "rm", volume, check=False)
        if not args.image: docker("image", "rm", image, check=False)


if __name__ == "__main__": main()
