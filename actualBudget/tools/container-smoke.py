#!/usr/bin/env python3
"""Build and verify a disposable actualBudget container through its public HTTP API.

Uses only new, randomly named resources. Never opens an existing budget volume.
Requires Docker and Python 3. Run from any directory; no extra Python packages.
"""

import argparse
from html.parser import HTMLParser
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from urllib.parse import urljoin, urlsplit
import uuid


def docker(*args, capture=True, check=True):
    return subprocess.run(
        ["docker", *args], check=check, text=True,
        stdout=subprocess.PIPE if capture else None,
    ).stdout


def request(base, path, payload=None, headers=None):
    data = None if payload is None else json.dumps(payload).encode()
    request_headers = {"Content-Type": "application/json", **(headers or {})}
    with urllib.request.urlopen(urllib.request.Request(
        base + path, data=data, headers=request_headers,
    ), timeout=5) as response:
        assert response.status == (200 if payload is None else 201), (path, response.status)
        return json.load(response)


def start(image, container, volume):
    docker("run", "--detach", "--name", container, "--read-only",
           "--cap-drop=ALL", "--security-opt=no-new-privileges",
           "--tmpfs", "/tmp:rw,noexec,nosuid,size=16m",
           "--publish", "127.0.0.1::5006",
           "--mount", f"type=volume,src={volume},dst=/data", image)
    address = docker("port", container, "5006/tcp").strip()
    base = "http://" + address
    deadline = time.monotonic() + 30
    while True:
        try:
            info = request(base, "/api/v1/info")
            assert info["data"]["api_version"] == 1, info
            assert info["data"]["authentication"] == "none", info
            return base
        except (urllib.error.URLError, TimeoutError, ConnectionError):
            if time.monotonic() >= deadline:
                raise RuntimeError("Container did not become ready within 30 seconds")
            time.sleep(0.2)


class AssetLinks(HTMLParser):
    def __init__(self):
        super().__init__()
        self.scripts = []
        self.styles = []

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "script" and attrs.get("type") == "module" and attrs.get("src"):
            self.scripts.append(attrs["src"])
        if tag == "link" and attrs.get("rel") == "stylesheet" and attrs.get("href"):
            self.styles.append(attrs["href"])


def verify_web_assets(base):
    with urllib.request.urlopen(base, timeout=5) as response:
        assert "text/html" in response.headers["Content-Type"]
        index = response.read().decode()
    links = AssetLinks()
    links.feed(index)
    assert links.scripts and links.styles, "Built HTML must reference JavaScript and CSS"
    for path in links.scripts + links.styles:
        url = urljoin(base, path)
        assert urlsplit(url).netloc == urlsplit(base).netloc, "Assets must be self-contained"
        with urllib.request.urlopen(url, timeout=5) as response:
            assert "text/html" not in response.headers["Content-Type"], path
            assert response.read(), f"Empty bundled asset: {path}"


def verify_receipts(base):
    assert "transaction_receipt_lookup" in request(base, "/api/v1/info")["data"]["capabilities"]
    budget_id, account_id = str(uuid.uuid4()), str(uuid.uuid4())
    root = f"/api/v1/budgets/{budget_id}"
    request(base, "/api/v1/budgets", {"id": budget_id, "name": "Container receipts"},
            {"Idempotency-Key": str(uuid.uuid4())})
    request(base, root + "/accounts", {
        "id": account_id, "name": "Cash", "opening_balance": 0, "opening_date": "2026-01-01",
    }, {"Idempotency-Key": str(uuid.uuid4())})
    body = {"id": str(uuid.uuid4()), "account_id": account_id,
            "amount": -125, "date": "2026-01-01"}
    headers = {"Idempotency-Key": str(uuid.uuid4()),
               "X-Actual-Edit-Clock": "2026-01-01T00:00:00.000Z-0000-0000000000000001"}
    probe_headers = {**headers, "X-Actual-Receipt-Only": "true"}
    before = request(base, root + "/checkpoint")
    try:
        request(base, root + "/transactions", body, probe_headers)
        raise AssertionError("Receipt lookup must not apply an unknown transaction")
    except urllib.error.HTTPError as error:
        assert error.code == 404, error.code
        assert json.load(error)["error"]["code"] == "operation_uncommitted"
    assert request(base, root + "/checkpoint") == before
    saved = request(base, root + "/transactions", body, headers)
    after = request(base, root + "/checkpoint")
    assert request(base, root + "/transactions", body, probe_headers) == saved
    assert request(base, root + "/checkpoint") == after
    assert len(request(base, root + "/transactions")["data"]) == 1
    print("PASS: read-only receipt miss preserves identity; committed outcome replays without writes")


def browser_commands(project):
    """Read the existing simple command list; never reinterpret arbitrary shell code."""
    script = json.loads((project / "web/package.json").read_text())["scripts"]["test:browser"]
    commands = [shlex.split(part) for part in script.split(" && ")]
    allowed_flags = {"--check", "--extended", "--prefix", "--metadata",
                     "--history", "--budget-metadata", "--types",
                     "--recovery-kind=transaction", "--recovery-kind=action"}
    for command in commands:
        if (len(command) not in (2, 3) or command[0] not in ("node", "python3")
                or (len(command) == 3 and command[2] not in allowed_flags)
                or not (project / "web" / command[1]).resolve().is_relative_to(project.resolve())
                or not (project / "web" / command[1]).is_file()):
            raise ValueError("Keep-going requires explicit node/python3 script commands; "
                             "update this runner if test:browser changes its command format")
    return commands




def main():
    project = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", help="Test an already-built image instead of building")
    browser_checks = parser.add_mutually_exclusive_group()
    browser_checks.add_argument("--browser", action="store_true",
                                help="Run the complete browser acceptance suite")
    browser_checks.add_argument("--browser-test", action="append",
                                choices=sorted(path.name for path in (project / "web/tests").glob("*.mjs")),
                                help="Run a selected browser script; repeat for related regressions")
    parser.add_argument("--keep-going", action="store_true",
                        help="Collect every browser-script result, then fail if any script failed")
    parser.add_argument('--shard-index', type=int, default=0)
    parser.add_argument('--shard-count', type=int, default=1)
    args = parser.parse_args()
    if args.shard_count != 1 and not (args.browser and args.keep_going):
        parser.error('Sharding requires --browser --keep-going')
    if not 0 <= args.shard_index < args.shard_count:
        parser.error('Invalid shard index/count')
    if args.keep_going and not (args.browser or args.browser_test):
        parser.error("--keep-going requires --browser or --browser-test")
    commands = None
    if args.keep_going:
        commands = (browser_commands(project) if args.browser else
                    [["node", str(project / "web/tests" / script)] for script in args.browser_test])
    sys.path.insert(0, str(project.parent / 'tools'))
    from qualification import select_shard, run_commands
    if commands is not None:
        commands = select_shard([{'cwd': 'web', 'argv': c} for c in commands],
                                args.shard_index, args.shard_count)
    prefix = "actualbudget-smoke-" + uuid.uuid4().hex
    container, volume = prefix + "-app", prefix + "-data"
    image = args.image or prefix + ":test"
    extracted = tempfile.TemporaryDirectory(prefix="actualbudget-qualified-")
    try:
        if not args.image:
            docker("build", "--tag", image, "--file", str(project / "Dockerfile"),
                   str(project), capture=False)
        docker("volume", "create", volume)
        base = start(image, container, volume)
        verify_web_assets(base)

        budget_id, account_id = str(uuid.uuid4()), str(uuid.uuid4())
        budget = request(base, "/api/v1/budgets", {
            "id": budget_id, "name": "Container persistence", "budget_type": "tracking",
        }, {"Idempotency-Key": str(uuid.uuid4())})["data"]
        assert budget["id"] == budget_id, budget
        account_path = f"/api/v1/budgets/{budget_id}/accounts"
        account_key = {"Idempotency-Key": str(uuid.uuid4())}
        account_body = {
            "id": account_id, "name": "Spending", "notes": "", "off_budget": False,
            "opening_balance": 123456, "opening_date": "2026-11-08",
        }
        account = request(base, account_path, account_body, account_key)["data"]
        assert account["balance"] == 123456, account

        # Recreate, not just restart, to prove the image's writable layer is dispensable.
        docker("stop", "--time", "10", container)
        docker("rm", container)
        base = start(image, container, volume)
        assert request(base, account_path, account_body, account_key)["data"] == account
        budgets = request(base, "/api/v1/budgets")["data"]
        assert any(item["id"] == budget_id for item in budgets), budgets
        accounts = request(base, f"/api/v1/budgets/{budget_id}/accounts")["data"]
        restored = next(item for item in accounts if item["id"] == account_id)
        assert restored["name"] == "Spending" and restored["balance"] == 123456, restored
        assert len(accounts) == 1, accounts
        transactions = request(base, f"/api/v1/budgets/{budget_id}/transactions?account_id={account_id}")["data"]
        assert len(transactions) == 1, transactions
        opening = transactions[0]
        assert opening["amount"] == 123456 and opening["date"] == "2026-11-08", opening
        assert opening["starting_balance"] is True, opening
        verify_receipts(base)
        if args.browser or args.browser_test:
            environment = dict(os.environ, ACTUALBUDGET_TEST_URL=f"http://actualbudget.test:{urlsplit(base).port}")
            # Recovery fault-injection needs a disposable database it can stop and
            # reopen. Use the exact image's executable/assets, never a debug build.
            binary = Path(extracted.name) / "actualbudget-server"
            web = Path(extracted.name) / "web"
            docker("cp", f"{container}:/usr/local/bin/actualbudget-server", str(binary))
            docker("cp", f"{container}:/app/web", str(web))
            environment.update(ACTUALBUDGET_TEST_BINARY=str(binary),
                               ACTUALBUDGET_TEST_WEB_DIR=str(web))
            if commands is not None:
                run_commands(commands, project, environment, project.parent / f"qualification-results/browser-{args.shard_index}.json")
            elif args.browser:
                subprocess.run(["npm", "--prefix", str(project / "web"), "run", "test:browser"],
                               env=environment, check=True)
            else:
                for script in args.browser_test:
                    subprocess.run(["node", str(project / "web/tests" / script)],
                                   cwd=project / "web", env=environment, check=True)
        print("PASS: plain HTTP, no login, bundled web assets, exact opening balance, "
              "persistence and idempotent retry after container replacement")
        print(docker("image", "inspect", "--format",
                     'image={{.Id}} bytes={{.Size}} user={{.Config.User}}', image).strip())
    except BaseException:
        docker("logs", container, capture=False, check=False)
        raise
    finally:
        extracted.cleanup()
        docker("rm", "--force", container, check=False)
        docker("volume", "rm", volume, check=False)
        if not args.image:
            docker("image", "rm", image, check=False)


if __name__ == "__main__":
    main()
