#!/usr/bin/env python3
"""Capture/check genuine pre-lifecycle receipts through the accepted public API."""
import base64
import hashlib
import http.client
import io
import json
import os
from pathlib import Path
import selectors
import shutil
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[2]
BASELINE = "8fe9b38444b98626f5e7670d228b2213ba3d376c"
FIXTURE = ROOT / "actualBudget/tests/fixtures/server-v5.sqlite3"
MANIFEST = FIXTURE.with_suffix(".json")
CHECK = "--check" in sys.argv
BUDGET = "05800000-0000-4000-8000-000000000001"
ACCOUNT = "05800000-0000-4000-8000-000000000002"
ENTRY = "05800000-0000-4000-8000-000000000003"
DUPLICATE = "05800000-0000-4000-8000-000000000004"
PAYEE = "05800000-0000-4000-8000-000000000005"
CATEGORY = "05800000-0000-4000-8000-000000000006"
SECOND = "05800000-0000-4000-8000-000000000007"
CANCELED = "05800000-0000-4000-8000-000000000008"
BASE = f"/budgets/{BUDGET}"
ZERO = "1970-01-01T00:00:00.000Z-0000-0000000000000000"
CLOCK = "2026-01-01T00:00:00.000Z-0000-0000000000000001"


def send(connection, operation, cancel=None):
    headers = {"Content-Type": operation.get("content_type", "application/json"),
               "Idempotency-Key": operation.get("key", "fixture-read"),
               **operation.get("headers", {})}
    cancel_requested = operation.get("cancel_if_uncommitted", False) if cancel is None else cancel
    if cancel_requested:
        headers["X-Actual-Cancel-If-Uncommitted"] = "true"
    body = base64.b64decode(operation["body_base64"]) if "body_base64" in operation else (
        json.dumps(operation["body"]).encode() if operation.get("body") is not None else None)
    connection.request(operation["method"], "/api/v1" + operation["path"], body, headers)
    response = connection.getresponse()
    raw = response.read().decode()
    return response.status, raw, json.loads(raw)


def operation(method, path, key, body=None, clock=None, cancel=False):
    result = {"method": method, "path": path, "key": key, "body": body}
    if clock is not None:
        result["headers"] = {"X-Actual-Edit-Clock": clock}
    if cancel:
        result["cancel_if_uncommitted"] = True
    return result


with tempfile.TemporaryDirectory(prefix="actualbudget-v5-receipts-") as temporary:
    directory = Path(temporary)
    source_bytes = subprocess.check_output(
        ["git", "archive", BASELINE, "actualBudget/server"], cwd=ROOT)
    with tarfile.open(fileobj=io.BytesIO(source_bytes)) as source:
        source.extractall(directory / "baseline", filter="data")
    target = ROOT / ".scratch/pre-lifecycle-oracle-target"
    subprocess.run(["cargo", "build", "--locked", "--manifest-path",
                    str(directory / "baseline/actualBudget/server/Cargo.toml"),
                    "--target-dir", str(target)], check=True)
    data = directory / "data"
    data.mkdir()
    if CHECK:
        evidence = json.loads(MANIFEST.read_text())
        assert evidence["baseline_commit"] == BASELINE and evidence["schema_version"] == 5
        assert hashlib.sha256(FIXTURE.read_bytes()).hexdigest() == evidence["sha256"]
        shutil.copyfile(FIXTURE, data / "actualbudget.sqlite3")
    environment = dict(os.environ, ACTUALBUDGET_DATA_DIR=str(data),
                       ACTUALBUDGET_HOST="127.0.0.1", ACTUALBUDGET_PORT="0")
    environment.pop("ACTUALBUDGET_PUBLIC_ORIGIN", None)
    child = subprocess.Popen([str(target / "debug/actualbudget-server")], env=environment,
                             stdout=subprocess.PIPE, text=True)
    connection = None
    try:
        with selectors.DefaultSelector() as selector:
            selector.register(child.stdout, selectors.EVENT_READ)
            assert selector.select(15), "Disposable baseline did not start"
            line = child.stdout.readline().strip()
        assert line.startswith("LISTENING "), line
        connection = http.client.HTTPConnection(line.removeprefix("LISTENING "), timeout=15)
        if not CHECK:
            receipts = []

            def capture(item, expected=201):
                status, raw, response = send(connection, item)
                assert status == expected, (status, response)
                receipts.append({**item, "status": status, "response_text": raw, "response": response})
                return response

            capture(operation("POST", "/budgets", "v5-budget", {
                "id": BUDGET, "name": "Pre-lifecycle receipts", "budget_type": "tracking"}))
            capture(operation("POST", "/budgets", "v5-second", {
                "id": SECOND, "name": "Separate receipt scope", "budget_type": "envelope"}))
            capture(operation("POST", BASE + "/accounts", "shared-reference-key", {
                "id": ACCOUNT, "name": "Spending", "opening_balance": 12500,
                "opening_date": "2024-02-29"}))
            capture(operation("POST", BASE + "/payees", "shared-reference-key", {
                "id": PAYEE, "name": "Cafe"}))
            capture(operation("POST", BASE + "/categories", "shared-reference-key", {
                "id": CATEGORY, "name": "Food", "is_income": False}))
            for budget in (BUDGET, SECOND):
                capture(operation("PATCH", f"/budgets/{budget}/preferences", "shared-preference-key",
                                  {"number_format": "dot-comma"}), 200)
            capture(operation("POST", BASE + "/transactions", "v5-create", {
                "id": ENTRY, "account_id": ACCOUNT, "date": "2024-02-29", "amount": -1234,
                "notes": "Original", "cleared": True, "payee_id": PAYEE,
                "category_id": CATEGORY}, ZERO))
            capture(operation("PATCH", BASE + f"/transactions/{ENTRY}", "v5-edit", {
                "amount": -234, "notes": "Edited before lifecycle upgrade"}, CLOCK), 200)
            capture(operation("POST", BASE + f"/transactions/{ENTRY}/duplicate", "v5-duplicate",
                              {"id": DUPLICATE}, CLOCK))
            capture(operation("DELETE", BASE + f"/transactions/{DUPLICATE}", "v5-delete",
                              clock=CLOCK), 200)
            capture(operation("POST", BASE + "/transactions", "v5-canceled", {
                "id": CANCELED, "account_id": ACCOUNT, "amount": -999, "date": "2024-02-29"
            }, CLOCK, True), 409)
            archive = (ROOT / "actualBudget/tests/fixtures/import-26.10.0.zip").read_bytes()
            stage = capture({"method": "POST", "path": "/imports/actual", "key": "v5-stage",
                             "content_type": "application/zip",
                             "body_base64": base64.b64encode(archive).decode()})["data"]
            capture(operation("POST", f"/imports/{stage['id']}/activate", "v5-activate", {}))
            paths = ["/budgets", BASE + "/accounts", BASE + "/transactions", BASE + "/payees",
                     BASE + "/categories", BASE + "/preferences", f"/budgets/{SECOND}/preferences",
                     "/imports", f"/imports/{stage['id']}",
                     f"/budgets/{stage['budget_id']}/accounts",
                     f"/budgets/{stage['budget_id']}/transactions"]
            snapshots = []
            for path in paths:
                status, raw, response = send(connection, {"method": "GET", "path": path})
                assert status == 200
                snapshots.append({"path": path, "response": response})
            register = next(s["response"] for s in snapshots if s["path"] == BASE + "/transactions")
            assert len(register["data"]) == 2 and register["balances"]["total"] == 12266
            evidence = {"baseline_commit": BASELINE, "schema_version": 5,
                        "provenance": "Accepted committed binary, public HTTP only; copied after graceful shutdown",
                        "budget_id": BUDGET, "second_budget_id": SECOND, "account_id": ACCOUNT,
                        "imported_budget_id": stage["budget_id"], "import_id": stage["id"],
                        "receipts": receipts, "snapshots": snapshots}
        for receipt in evidence["receipts"]:
            status, raw, _ = send(connection, receipt)
            assert (status, raw) == (receipt["status"], receipt["response_text"]), receipt["key"]
            if receipt.get("cancel_if_uncommitted"):
                assert send(connection, receipt, False)[:2] == (status, raw)
        for snapshot in evidence["snapshots"]:
            status, _, response = send(connection, {"method": "GET", "path": snapshot["path"]})
            assert status == 200 and response == snapshot["response"], snapshot["path"]
    finally:
        if connection:
            connection.close()
        child.terminate()
        child.wait(timeout=10)
        child.stdout.close()
    if not CHECK:
        shutil.copyfile(data / "actualbudget.sqlite3", FIXTURE)
        evidence["sha256"] = hashlib.sha256(FIXTURE.read_bytes()).hexdigest()
        MANIFEST.write_text(json.dumps(evidence, indent=2) + "\n")
    print(f"PASS {len(evidence['receipts'])} genuine schema5 receipts, original byte-exact replay and snapshots")
