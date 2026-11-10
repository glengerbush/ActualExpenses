"""Create genuine schema20 upgrade evidence through its preserved public API.

No database writes, schema relabeling, live volumes, or current executable used.
"""
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import urllib.request
import zipfile

fixtures = Path(__file__).resolve().parents[1] / "tests/fixtures"
binary = Path(sys.argv[1]).resolve()
binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
assert binary_hash == "fabd86650e86bbdab91e2796040d3e455ffef67e713721740f046694b80f6eed"
outputs = {ext: fixtures / ("transaction-import-schema20" + ext)
           for ext in [".sqlite3", ".zip", ".json"]}
assert not any(path.exists() for path in outputs.values()), "Original fixtures exist"


def identity(number):
    return f"20210000-0000-4000-8000-{number:012}"


with tempfile.TemporaryDirectory(prefix="transaction-import-schema20-") as directory:
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

        created = call("/budgets", {"id": identity(1), "name": "Before transaction imports",
                                   "budget_type": "tracking"}, "budget20")
        root = "/budgets/" + identity(1)
        metadata = call(root)["data"]
        headers = {"X-Actual-History-Owner": identity(90),
                   "X-Actual-Budget-Epoch": metadata["epoch"]}
        call(root + "/accounts", {"id": identity(2), "name": "Spending",
             "opening_balance": 12345, "opening_date": "2026-11-01"}, "account20")
        call(root + "/categories", {"id": identity(3), "name": "Food", "is_income": False}, "category20")
        call(root + "/payees", {"id": identity(4), "name": "Cafe"}, "payee20")
        commands = []

        def write(path, body, key, method="POST"):
            result = call(root + path, body, key, method, headers)
            commands.append({"path": path, "body": body, "key": key,
                             "method": method, "result": result})
            return result

        write("/transactions", {"id": identity(5), "account_id": identity(2),
              "category_id": identity(3), "payee_id": identity(4), "amount": -345,
              "date": "2026-11-02", "notes": "Original expense", "cleared": True}, "expense20")
        write("/transactions/" + identity(5), {"notes": "Owned edit before import support"}, "edit20", "PATCH")
        history = call(root + "/history", headers=headers)
        assert history["data"]["undo"] is not None
        register = call(root + "/transactions")
        assert register["balances"]["total"] == 12000
        stages = []
        for name in ["transaction-merge-bank-id-kept-26.10.zip",
                     "transaction-import-rules-before-match-26.10.zip"]:
            payload = (fixtures / name).read_bytes()
            key = "source20-" + str(len(stages))
            staged = call("/imports/actual", payload, key)
            if not stages:
                assert staged["data"]["status"] == "blocked", staged
                assert "transaction_features" in json.dumps(staged["data"]["report"])
            assert call(f"/imports/{staged['data']['id']}/original.zip") == payload
            stages.append({"archive": name, "sha256": hashlib.sha256(payload).hexdigest(),
                           "key": key, "result": staged,
                           "latest": call(f"/imports/{staged['data']['id']}")})
        assert call(root + "/transactions") == register
        assert call(root + "/history", headers=headers) == history
        backup = call(root + "/backups", {"label": "Before transaction imports"}, "backup20")
        archive = call(root + "/backups/" + backup["data"]["id"] + "/download")
        with zipfile.ZipFile(io.BytesIO(archive)) as contents:
            assert json.loads(contents.read("manifest.json"))["schema_version"] == 20
        evidence = {"source_commit": "32f27ab", "accepted_product_commit": "4d7498c",
            "schema_version": 20, "executable_sha256": binary_hash,
            "created": created, "metadata": metadata, "owner_headers": headers,
            "account_id": identity(2), "category_id": identity(3), "payee_id": identity(4),
            "commands": commands, "history": history, "register": register,
            "accounts": call(root + "/accounts"), "categories": call(root + "/categories"),
            "payees": call(root + "/payees"), "checkpoint": call(root + "/checkpoint"),
            "stages": stages, "backup": backup, "archive_sha256": hashlib.sha256(archive).hexdigest()}
    finally:
        process.terminate()
        process.wait(timeout=15)
    database = Path(directory) / "actualbudget.sqlite3"
    with sqlite3.connect(f"file:{database}?mode=ro&immutable=1", uri=True) as db:
        assert db.execute("PRAGMA user_version").fetchone()[0] == 20
        assert db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        assert db.execute("PRAGMA foreign_key_check").fetchall() == []
        assert db.execute("SELECT count(*) FROM staged_imports").fetchone()[0] == 2
    evidence["database_sha256"] = hashlib.sha256(database.read_bytes()).hexdigest()
    shutil.copyfile(database, outputs[".sqlite3"])
    outputs[".zip"].write_bytes(archive)
    outputs[".json"].write_text(json.dumps(evidence, indent=2) + "\n")
print("PASS genuine schema20 money, owned history/receipts, retained bank-ID/settings sources and native backup")
