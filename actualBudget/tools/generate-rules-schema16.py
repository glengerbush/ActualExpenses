"""Capture authentic schema16 state through HTTP for rules upgrade tests.

Usage: python3 generate-rules-schema16.py /path/to/preserved/schema16/server
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
assert binary_hash == "896187e0e4f617c2e6815e7f5fde949220397bd0097ca0f5d8f32b5e3a3615a1"


def identity(number):
    return f"16170000-0000-4000-8000-{number:012}"


with tempfile.TemporaryDirectory(prefix="rules-schema16-") as directory:
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

        budget = call("/budgets", {"id": identity(1), "name": "Before rules",
                                  "budget_type": "tracking"}, "budget16")
        root = "/budgets/" + identity(1)
        metadata = call(root)["data"]
        owner_headers = {"X-Actual-History-Owner": identity(9),
                         "X-Actual-Budget-Epoch": metadata["epoch"]}
        account = call(root + "/accounts", {"id": identity(2), "name": "Daily",
                       "opening_balance": 12345, "opening_date": "2026-01-01"}, "account16")
        category = call(root + "/categories", {"id": identity(4), "name": "Food",
                        "is_income": False}, "category16")
        creation = {"id": identity(3), "account_id": identity(2), "amount": -345,
                    "date": "2026-01-02", "cleared": True, "notes": "Original notes",
                    "category_id": identity(4)}
        created = call(root + "/transactions", creation, "create16", headers=owner_headers)
        edit = {"notes": "Owned edit before upgrade"}
        edited = call(root + "/transactions/" + identity(3), edit, "edit16", "PATCH", owner_headers)
        planning_body = {"action": "set", "month": "2026-01", "category": identity(4),
                         "amount": 1000, "snapshot": call(root + "/budget?month=2026-01")["data"]["snapshot"]}
        planning = call(root + "/budget", planning_body, "plan16", headers=owner_headers)
        planning_view = call(root + "/budget?month=2026-01")
        history = call(root + "/history", headers=owner_headers)
        assert history["data"]["undo"] is not None, history
        row = call(root + "/transactions/" + identity(3))["data"]
        balances = call(root + "/accounts/" + identity(2))
        stage = call("/imports/actual", (fixtures / "import-unsupported-rule-26.10.0.zip").read_bytes(), "stage16")
        assert stage["data"]["status"] == "blocked", stage
        backup = call(root + "/backups", {"label": "Before rules upgrade"}, "backup16")
        archive = call(root + "/backups/" + backup["data"]["id"] + "/download")
        with zipfile.ZipFile(io.BytesIO(archive)) as contents:
            assert json.loads(contents.read("manifest.json"))["schema_version"] == 16
        (fixtures / "rules-schema16.zip").write_bytes(archive)
        (fixtures / "rules-schema16.json").write_text(json.dumps({
            "source_commit": "150666d", "executable_sha256": binary_hash,
            "budget": budget, "metadata": metadata, "account": account,
            "owner_headers": owner_headers, "creation": creation, "created": created,
            "edit": edit, "edited": edited, "row": row, "balances": balances,
            "history": history, "stage": stage, "category": category,
            "planning_body": planning_body, "planning": planning, "planning_view": planning_view,
        }, indent=2) + "\n")
    finally:
        process.terminate()
        process.wait(timeout=15)
    shutil.copyfile(Path(directory) / "actualbudget.sqlite3",
                    fixtures / "rules-schema16.sqlite3")
print("PASS genuine schema16 owned history, immutable receipts, balances, blocked rule source, planning and archive")

