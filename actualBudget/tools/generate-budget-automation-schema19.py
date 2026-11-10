"""Capture genuine schema19 goals, notes, schedules, history and blocked imports.

Usage: python3 generate-budget-automation-schema19.py SCHEMA19_BINARY
All writes go through the old public API; no version relabeling or DB mutation.
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
assert binary_hash == "eb1b47e56443f805c5ae828916329d1501ad5f1f67481e2a91c29a28b5c6b135"
source = json.loads((fixtures / "budget-goal-upgrade-source.json").read_text())
active_bytes = (fixtures / "budget-goal-upgrade-source.zip").read_bytes()
outputs = {ext: fixtures / ("budget-automation-schema19" + ext)
           for ext in [".sqlite3", ".zip", ".json"]}
assert not any(path.exists() for path in outputs.values()), "Original fixtures exist"


def identity(number):
    return f"19200000-0000-4000-8000-{number:012}"


with tempfile.TemporaryDirectory(prefix="budget-automation-schema19-") as directory:
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

        stage = call("/imports/actual", active_bytes, "active-source19")
        assert stage["data"]["status"] == "ready", stage
        activated = call(f"/imports/{stage['data']['id']}/activate", {}, "activate19")
        budget = stage["data"]["budget_id"]
        root = "/budgets/" + budget
        epoch = call(root)["data"]["epoch"]
        headers = {"X-Actual-History-Owner": identity(90), "X-Actual-Budget-Epoch": epoch}
        account, category = source["ids"]["Checking"], source["ids"]["Fixed"]
        commands = []

        def write(path, body, key, method=None):
            result = call(root + path, body, key, method, headers)
            commands.append({"path": path, "body": body, "key": key,
                             "method": method or "POST", "result": result})
            return result

        write("/transactions", {"id": identity(2), "account_id": account,
              "category_id": category, "amount": -1234, "date": "2026-11-03",
              "notes": "Before automation support", "cleared": True}, "expense19")
        write("/transactions/" + identity(2), {"notes": "Owned edit before automation support"},
              "edit19", "PATCH")
        write("/schedules/commands", {"action": "create", "id": identity(3),
              "name": "Monthly reserve", "posts_transaction": False,
              "conditions": [{"field": "account", "op": "is", "value": account},
                             {"field": "amount", "op": "is", "value": -2345},
                             {"field": "date", "op": "is", "value": {
                                 "frequency": "monthly", "start": "2026-11-15"}}],
              "snapshot": call(root + "/checkpoint")["data"]["snapshot"]}, "schedule19")
        write("/budget", {"action": "notes", "month": "2026-11",
              "notes": "Month note before automation support",
              "snapshot": call(root + "/checkpoint")["data"]["snapshot"]}, "month-note19")
        plan = call(root + "/budget?month=2026-11")
        row = next(c for c in plan["data"]["months"]["2026-11"]["categories"] if c["id"] == category)
        assert row["budgeted"] == 7299 and row["balance"] == 6065, row
        assert json.loads(row["goal"]) == 100000 and json.loads(row["long_goal"]) == 1
        assert row["notes"] == source["notes"]
        history = call(root + "/history", headers=headers)
        assert history["data"]["undo"] is not None
        blocked = []
        for name in ["budget-templates-simple-base-26.10.zip",
                     "budget-cleanup/envelope-ui-pool-ignores-stale-notes.zip"]:
            payload = (fixtures / name).read_bytes()
            key = "blocked19-" + str(len(blocked))
            result = call("/imports/actual", payload, key)
            assert result["data"]["status"] == "blocked", result
            assert "category_configuration" in json.dumps(result["data"]["report"])
            assert call(f"/imports/{result['data']['id']}/original.zip") == payload
            blocked.append({"archive": name, "sha256": hashlib.sha256(payload).hexdigest(),
                            "key": key, "result": result,
                            "latest": call(f"/imports/{result['data']['id']}")})
        assert call(root + "/history", headers=headers) == history
        backup = call(root + "/backups", {"label": "Before budget automations"}, "backup19")
        archive = call(root + "/backups/" + backup["data"]["id"] + "/download")
        with zipfile.ZipFile(io.BytesIO(archive)) as contents:
            assert json.loads(contents.read("manifest.json"))["schema_version"] == 19
        metadata = {"source_commit": "94bd0c2", "accepted_product_commit": "b7c1ca7",
            "schema_version": 19, "executable_sha256": binary_hash,
            "source_archive": "budget-goal-upgrade-source.zip",
            "source_archive_sha256": hashlib.sha256(active_bytes).hexdigest(),
            "active_stage": stage, "activated": activated, "metadata": call(root)["data"],
            "account_id": account, "category_id": category, "owner_headers": headers,
            "commands": commands, "plan": plan, "history": history,
            "register": call(root + "/transactions?account_id=" + account),
            "accounts": call(root + "/accounts"), "schedules": call(root + "/schedules"),
            "checkpoint": call(root + "/checkpoint"), "blocked": blocked,
            "backup": backup, "archive_sha256": hashlib.sha256(archive).hexdigest()}
    finally:
        process.terminate()
        process.wait(timeout=15)
    database = Path(directory) / "actualbudget.sqlite3"
    with sqlite3.connect(f"file:{database}?mode=ro&immutable=1", uri=True) as db:
        assert db.execute("PRAGMA user_version").fetchone()[0] == 19
        assert db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        assert db.execute("PRAGMA foreign_key_check").fetchall() == []
        assert db.execute("SELECT count(*) FROM staged_imports WHERE status='blocked'").fetchone()[0] == 2
    shutil.copyfile(database, outputs[".sqlite3"])
    outputs[".zip"].write_bytes(archive)
    outputs[".json"].write_text(json.dumps(metadata, indent=2) + "\n")
print("PASS genuine schema19: goals, notes, schedules, owned history/receipts, blocked source definitions and native backup")
