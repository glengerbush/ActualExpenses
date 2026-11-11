#!/usr/bin/env python3
"""Measure the companion against disposable copies of the browser resource fixtures."""
import argparse
from contextlib import closing
import hashlib
import http.client
import json
import math
from pathlib import Path
import shlex
import subprocess
import threading
import time
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit
from uuid import uuid4

p = argparse.ArgumentParser()
p.add_argument("lab", type=Path)
p.add_argument("output", type=Path)
p.add_argument("--adb", type=Path, required=True)
p.add_argument("--serial", default="emulator-5556")
p.add_argument("--samples", type=int, default=20)
p.add_argument("--case", choices=["empty", "typical", "large"])
p.add_argument("--baseline-source", type=Path, help="Pinned2b25b07 checkout with the shared ResourceDeviceTest built")
p.add_argument("--timeout-seconds", type=int, default=3600, help="Per-case instrumentation watchdog")
a = p.parse_args()
assert a.serial.startswith("emulator-") and a.serial[9:].isdigit()
assert 1 <= a.samples <= 20
assert 120 <= a.timeout_seconds <= 7200
a.output.mkdir(parents=True, exist_ok=False)
config = json.loads((a.lab / ("baseline.json" if a.baseline_source else "native.json")).read_text())
target = urlsplit(config["server"])
assert target.hostname == "127.0.0.1" and target.scheme == "http"
package = "org.actualexpenses.android"
root = Path(__file__).resolve().parents[2]
source_tool = root / "actualBudget/tools/android-resource-source.mjs"
app_root = a.baseline_source.resolve() if a.baseline_source else root / "actualExpenses"
if a.baseline_source:
    revision = subprocess.check_output(["git", "-C", str(app_root), "rev-parse", "HEAD"]).decode().strip()
    assert revision.startswith("2b25b07")
    assert not subprocess.check_output(["git", "-C", str(app_root), "diff", "--name-only", "HEAD"])
    prepare = ["node", str(source_tool), "prepare", str(a.lab.resolve()), str(a.output.resolve())]
    if a.case:
        prepare.append(a.case)
    with (a.output / "source-prepare.log").open("w") as log:
        subprocess.run(prepare, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=180)
    cases = json.loads((a.output / "source-cases.json").read_text())["cases"]
else:
    revision = "229be58"
    cases = config["cases"]

def adb(*args, timeout=60):
    return subprocess.check_output([str(a.adb), "-s", a.serial, *args], timeout=timeout).decode()

def api(path, body=None, epoch=None):
    headers = {"Content-Type": "application/json", "Idempotency-Key": str(uuid4())}
    if epoch:
        headers["X-Actual-Budget-Epoch"] = epoch
    request = urllib.request.Request(config["server"] + "/api/v1" + path,
                                    data=None if body is None else json.dumps(body).encode(), headers=headers)
    with urllib.request.urlopen(request, timeout=60) as response:
        return json.load(response)

events = []
class Proxy(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def handle_request(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        started = time.time()
        with closing(http.client.HTTPConnection(target.hostname, target.port, timeout=60)) as connection:
            headers = dict(self.headers)
            headers.update(Host=target.netloc, Connection="close")
            connection.request(self.command, self.path, body, headers)
            response = connection.getresponse()
            raw = response.read()
            self.send_response(response.status)
            for key, value in response.getheaders():
                if key.lower() not in ["connection", "transfer-encoding", "content-length"]:
                    self.send_header(key, value)
            self.send_header("Content-Length", str(len(raw)))
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(raw)
            events.append({"time": started, "method": self.command, "path": self.path,
                           "status": response.status, "request_body_bytes": len(body),
                           "response_body_bytes": len(raw), "elapsed_ms": (time.time()-started)*1000})
    do_GET = handle_request
    do_POST = handle_request
    do_PATCH = handle_request
    do_DELETE = handle_request

proxy = ThreadingHTTPServer(("127.0.0.1", 0), Proxy)
threading.Thread(target=proxy.serve_forever, daemon=True).start()
summary = {"image": config["image"], "samples": a.samples, "cases": [],
           "protocol": "API36 emulator, debug APK and shared Compose instrumentation; process CPU/PSS, UID network and process I/O. Instrumentation/measurement overhead included; not phone battery measurements.",
           "application": "released baseline" if a.baseline_source else "native replacement",
           "source_revision": revision,
           "device": adb("shell", "getprop", "ro.build.fingerprint").strip(),
           "screen": adb("shell", "wm", "size").strip(),
           "density": adb("shell", "wm", "density").strip(),
           "font_scale": adb("shell", "settings", "get", "system", "font_scale").strip(),
           "instrumentation_sha256": hashlib.sha256((app_root / "app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk").read_bytes()).hexdigest(),
           "apk_sha256": hashlib.sha256((app_root / "app/build/outputs/apk/debug/app-debug.apk").read_bytes()).hexdigest()}
try:
    for case in cases:
        if a.case and case["name"] != a.case:
            continue
        if a.baseline_source:
            clone, name, account_name = case["cloud_id"], case["label"], case["account_name"]
        else:
            clone = str(uuid4())
            name = "AAA Android " + case["name"] + " " + clone[:6]
            checkpoint = api(f'/budgets/{case["id"]}/checkpoint')["data"]
            api(f'/budgets/{case["id"]}/duplicate', {"id": clone, "name": name}, checkpoint["epoch"])
            account_name = next(row["name"] for row in case["accounts"] if row["id"] == case["account"])
            path = f'/budgets/{clone}/transactions?account_id={case["account"]}&limit=100'
            before = api(path)
        events.clear()
        adb("shell", "am", "force-stop", package)
        adb("shell", "pm", "clear", package)
        if a.baseline_source:
            adb("shell", "run-as", package, "mkdir", "-p", "files")
            subprocess.run([str(a.adb), "-s", a.serial, "shell", "run-as", package, "sh", "-c",
                            shlex.quote("cat > files/resource-baseline-password")],
                           input=config["password"].encode(), check=True, stdout=subprocess.DEVNULL)
        command = ["shell", "am", "instrument", "-w", "-r", "-e", "class",
                   package + ".ResourceDeviceTest", "-e", "resourceServer", f"http://10.0.2.2:{proxy.server_port}",
                   "-e", "resourceBudget", name, "-e", "resourceAccount", account_name,
                   "-e", "resourceSamples", str(a.samples), package + ".test/androidx.test.runner.AndroidJUnitRunner"]
        if a.baseline_source:
            command[-1:-1] = ["-e", "resourceBaseline", "true"]
        # adb shell joins argv; quote values containing spaces for the device shell.
        command = [command[0]] + [shlex.quote(value) for value in command[1:]]
        try:
            log = adb(*command, timeout=a.timeout_seconds)
        except subprocess.TimeoutExpired as error:
            partial = error.stdout or b""
            (a.output / (case["name"] + ".log")).write_text(partial.decode() if isinstance(partial, bytes) else partial)
            (a.output / (case["name"] + "-requests.json")).write_text(json.dumps(events, indent=2))
            raise
        (a.output / (case["name"] + ".log")).write_text(log)
        (a.output / (case["name"] + "-requests.json")).write_text(json.dumps(events, indent=2))
        raw = adb("exec-out", "cat", f"/sdcard/Android/data/{package}/files/resource-observations.json")
        try:
            observations = json.loads(raw)
        except json.JSONDecodeError:
            assert "OK (1 test)" in log, log[-3000:]
            raise
        (a.output / (case["name"] + ".json")).write_text(json.dumps(observations, indent=2))
        assert "OK (1 test)" in log, log[-3000:]
        if a.baseline_source:
            adb("shell", "run-as", package, "rm", "-f", "files/resource-baseline-password")
            with (a.output / (case["name"] + "-oracle.log")).open("w") as log:
                subprocess.run(["node", str(source_tool), "verify", str(a.lab.resolve()), str(a.output.resolve()),
                                case["name"], str(a.samples)], stdout=log, stderr=subprocess.STDOUT, check=True, timeout=120)
            proof = json.loads((a.output / (case["name"] + "-financial-proof.json")).read_text())
        else:
            after = api(path)
            payees = api(f'/budgets/{clone}/payees')["data"]
            ids = {row["id"] for row in payees if row["name"].startswith("Resource expense " + name)}
            # Backdated purchases need not appear in the register's first page.
            assert len(ids) == a.samples
            purchases = []
            for payee in ids:
                rows = api(path + "&payee_id=" + payee)["data"]
                assert len(rows) == 1, (payee, len(rows))
                purchases.extend(rows)
            assert len({row["id"] for row in purchases}) == a.samples
            assert len(purchases) == a.samples, (len(purchases), a.samples)
            assert all(row["amount"] == -427 and row["cleared"] for row in purchases)
            assert after["balances"]["total"] == before["balances"]["total"] - 427*a.samples
            proof = {"added_rows": len(purchases), "before": before["balances"], "after": after["balances"]}
        phases = {}
        for phase in sorted({row["phase"] for row in observations}):
            rows = [row for row in observations if row["phase"] == phase]
            times = sorted(row["action_elapsed_ms"] for row in rows)
            metrics = {"action_elapsed_ms": {"p50": times[math.ceil(len(times)*.5)-1], "p95": times[math.ceil(len(times)*.95)-1]}}
            for key in rows[0]["before"]:
                values = sorted(row["after"][key] if key == "pss_kib" else row["after"][key]-row["before"][key]
                                for row in rows if row["before"].get(key) is not None and row["after"].get(key) is not None)
                metrics[key] = None if not values else {"p50": values[math.ceil(len(values)*.5)-1], "p95": values[math.ceil(len(values)*.95)-1]}
            phases[phase] = {"count": len(rows), "metrics": metrics}
        summary["cases"].append({"name": case["name"], "budget_id": clone, "transactions": case["transactions"],
                                 "source_archive_sha256": case["source_archive_sha256"], "phases": phases,
                                 "financial_proof": proof})
        (a.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        print("PASS", case["name"], len(observations), "observations", flush=True)
finally:
    proxy.shutdown()
    proxy.server_close()
    if a.baseline_source:
        adb("shell", "run-as", package, "rm", "-f", "files/resource-baseline-password")
