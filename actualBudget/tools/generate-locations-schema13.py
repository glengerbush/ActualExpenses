"""Generate genuine schema13 public-API fixtures; no SQLite or ZIP rewriting."""
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
with tempfile.TemporaryDirectory(prefix="locations-schema13-") as directory:
    process = subprocess.Popen([str(binary)], env={**os.environ,
        "ACTUALBUDGET_DATA_DIR": directory, "ACTUALBUDGET_HOST": "127.0.0.1",
        "ACTUALBUDGET_PORT": "0", "ACTUALBUDGET_WEB_DIR": directory},
        stdout=subprocess.PIPE, text=True)
    try:
        url = "http://" + process.stdout.readline().strip().removeprefix("LISTENING ")
        def call(path, body=None, key="read", method=None):
            raw = body if isinstance(body, bytes) else None if body is None else json.dumps(body).encode()
            request = urllib.request.Request(url + "/api/v1" + path, data=raw, method=method,
                headers={"Idempotency-Key": key, "Content-Type": "application/zip" if isinstance(body, bytes) else "application/json"})
            with urllib.request.urlopen(request, timeout=10) as response:
                return json.load(response) if response.headers.get("Content-Type", "").startswith("application/json") else response.read()
        def identity(n):
            return f"46130000-0000-4000-8000-{n:012}"
        budget = call("/budgets", {"id": identity(1), "name": "Schema13 protected transfers"}, "budget13")
        root = "/budgets/" + identity(1)
        for number, name, balance in [(2, "Daily", 12345), (3, "Savings", 0)]:
            call(root + "/accounts", {"id": identity(number), "name": name,
                "opening_balance": balance, "opening_date": "2026-01-01"}, "account13-" + str(number))
        transfer_body = {"id": identity(4), "counterpart_id": identity(5), "account_id": identity(2),
            "to_account_id": identity(3), "amount": -345, "date": "2026-01-02", "cleared": True, "notes": "Preserved pair"}
        transfer = call(root + "/transfers", transfer_body, "transfer13")
        path = root + "/accounts/" + identity(2) + "/reconciliation"
        preview = call(path)["data"]
        reconciliation_body = {"action": "lock", "statement_balance": 12000,
            "snapshot": preview["snapshot"], "date": "2026-02-28"}
        reconciliation = call(path, reconciliation_body, "reconcile13")
        rows = [call(root + "/transactions/" + identity(n))["data"] for n in (4, 5)]
        account = call(root + "/accounts/" + identity(2))["data"]
        stage = call("/imports/actual", (fixtures / "payee-locations-import.zip").read_bytes(), "blocked-locations13")
        assert stage["data"]["status"] == "blocked", stage
        backup = call(root + "/backups", {"label": "Schema13 original"}, "backup13")
        archive = call(root + "/backups/" + backup["data"]["id"] + "/download")
        assert json.loads(zipfile.ZipFile(io.BytesIO(archive)).read("manifest.json"))["schema_version"] == 13
        (fixtures / "locations-schema13.zip").write_bytes(archive)
        (fixtures / "locations-schema13.json").write_text(json.dumps({
            "source_commit": "1713c33", "executable_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "budget": budget, "account": account, "rows": rows,
            "transfer_body": transfer_body, "transfer": transfer,
            "reconciliation_body": reconciliation_body, "reconciliation": reconciliation,
            "stage": stage}, indent=2) + "\n")
    finally:
        process.terminate()
        process.wait()
    shutil.copyfile(Path(directory) / "actualbudget.sqlite3", fixtures / "locations-schema13.sqlite3")
print("PASS genuine schema13 protected transfer/reconciliation receipts and blocked location stage")
