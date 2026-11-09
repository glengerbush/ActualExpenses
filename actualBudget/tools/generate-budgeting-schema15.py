"""Capture authentic schema15 state through HTTP for budgeting upgrade tests.

Usage: python3 generate-budgeting-schema15.py /path/to/preserved/schema15/server
Only disposable data is used. Never relabel a newer database as an older schema.
"""
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import zipfile

fixtures = Path(__file__).resolve().parents[1] / "tests/fixtures"
binary = Path(sys.argv[1]).resolve()
binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
assert binary_hash == "cdcb95f12527f7b9aeb037e280d05aff894a28ff2ae6538dcbf8d181f797f06c"


def identity(number):
    return f"15160000-0000-4000-8000-{number:012}"


with tempfile.TemporaryDirectory(prefix="budgeting-schema15-") as directory:
    environment = {**os.environ, "ACTUALBUDGET_DATA_DIR": directory,
                   "ACTUALBUDGET_HOST": "127.0.0.1", "ACTUALBUDGET_PORT": "0",
                   "ACTUALBUDGET_WEB_DIR": directory}
    environment.pop("ACTUALBUDGET_PUBLIC_ORIGIN", None)
    process = subprocess.Popen([str(binary)], env=environment,
                               stdout=subprocess.PIPE, text=True)
    try:
        line = process.stdout.readline().strip()
        assert line.startswith("LISTENING "), line
        url = "http://" + line.removeprefix("LISTENING ")

        def call(path, body=None, key="read", method=None, headers=None):
            raw = body if isinstance(body, bytes) else (
                None if body is None else json.dumps(body).encode())
            request = urllib.request.Request(url + "/api/v1" + path, data=raw,
                method=method, headers={"Idempotency-Key": key,
                "Content-Type": "application/zip" if isinstance(body, bytes)
                else "application/json", **(headers or {})})
            with urllib.request.urlopen(request, timeout=15) as response:
                return json.load(response) if response.headers.get(
                    "Content-Type", "").startswith("application/json") else response.read()

        budget = call("/budgets", {"id": identity(1), "name": "Before budgeting",
                                  "budget_type": "tracking"}, "budget15")
        root = "/budgets/" + identity(1)
        metadata = call(root)["data"]
        owner_headers = {"X-Actual-History-Owner": identity(9),
                         "X-Actual-Budget-Epoch": metadata["epoch"]}
        account = call(root + "/accounts", {"id": identity(2), "name": "Daily",
                       "opening_balance": 12345, "opening_date": "2026-01-01"}, "account15")
        creation = {"id": identity(3), "account_id": identity(2), "amount": -345,
                    "date": "2026-01-02", "cleared": True, "notes": "Original notes"}
        created = call(root + "/transactions", creation, "create15", headers=owner_headers)
        edit = {"notes": "Owned edit before upgrade"}
        edited = call(root + "/transactions/" + identity(3), edit, "edit15", "PATCH", owner_headers)
        history = call(root + "/history", headers=owner_headers)
        assert history["data"]["undo"] is not None, history
        row = call(root + "/transactions/" + identity(3))["data"]
        balances = call(root + "/accounts/" + identity(2))
        stage = call("/imports/actual", (fixtures / "payee-locations-import.zip").read_bytes(), "stage15")
        assert stage["data"]["status"] == "ready", stage
        backup = call(root + "/backups", {"label": "Before planning upgrade"}, "backup15")
        archive = call(root + "/backups/" + backup["data"]["id"] + "/download")
        with zipfile.ZipFile(io.BytesIO(archive)) as contents:
            assert json.loads(contents.read("manifest.json"))["schema_version"] == 15
        (fixtures / "budgeting-schema15.zip").write_bytes(archive)
        (fixtures / "budgeting-schema15.json").write_text(json.dumps({
            "source_commit": "ce70d9c", "executable_sha256": binary_hash,
            "budget": budget, "metadata": metadata, "account": account,
            "owner_headers": owner_headers, "creation": creation, "created": created,
            "edit": edit, "edited": edited, "row": row, "balances": balances,
            "history": history, "stage": stage,
        }, indent=2) + "\n")
    finally:
        process.terminate()
        process.wait(timeout=15)
    shutil.copyfile(Path(directory) / "actualbudget.sqlite3",
                    fixtures / "budgeting-schema15.sqlite3")
print("PASS genuine schema15 owned history, immutable receipts, balances, ready source and archive")
