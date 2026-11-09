"""Capture real schema18 state before schedule support, using only its public API.

Usage: python3 generate-schedules-schema18.py SCHEMA18_BINARY SCHEDULE_SOURCE_ZIP
Creates immutable fixtures once; never rewrites a schema version or existing fixture.
The native ZIP covers the active budget. The SQLite fixture additionally retains
the separate blocked schedule-import stage and its original receipt/archive.
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
assert binary_hash == "2dc440a9e4bc2e025b72a18a1db2ad8b4cc80912ee25402ed3373ff759690da6"
schedule_bytes = Path(sys.argv[2]).read_bytes()
active_bytes = (fixtures / "android-upgrade/pending.zip").read_bytes()
legacy = json.loads((fixtures / "android-upgrade/pending.state.json").read_text())
outputs = {extension: fixtures / ("schedules-schema18" + extension)
           for extension in [".sqlite3", ".zip", ".json", "-source.zip", "-active-source.zip"]}
assert not any(path.exists() for path in outputs.values()), "Original fixtures already exist"


def identity(number):
    return f"18190000-0000-4000-8000-{number:012}"


with tempfile.TemporaryDirectory(prefix="schedules-schema18-") as directory:
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

        active_stage = call("/imports/actual", active_bytes, "active-source18")
        assert active_stage["data"]["status"] == "ready", active_stage
        active_id = active_stage["data"]["id"]
        activated = call(f"/imports/{active_id}/activate", {}, "activate18")
        budget = active_stage["data"]["budget_id"]
        root = "/budgets/" + budget
        initial_metadata = call(root)["data"]
        source = {"file_id": legacy["budget"]["fileId"],
                  "group_id": legacy["budget"]["groupId"]}
        handoffs, handoff_results = [], []
        for operation in legacy["pending"]:
            handoff = {"source": source, "epoch": initial_metadata["epoch"],
                       "operation": operation}
            handoffs.append(handoff)
            handoff_results.append(call(root + "/android-upgrade/operations", handoff,
                                        operation["id"]))
        account = next(row for row in call(root + "/accounts")["data"]
                       if row["id"] == legacy["accountId"])
        register_path = root + "/transactions?account_id=" + account["id"]
        delivered_register = call(register_path)
        assert len(delivered_register["data"]) == 2
        assert delivered_register["balances"]["total"] == -1801
        # This fixture includes recovered fractional source-order keys as well as
        # ordinary native anchors, so later migrations cannot discard either.
        assert any("order_key" in row for row in delivered_register["data"])
        owner_headers = {"X-Actual-History-Owner": identity(9),
                         "X-Actual-Budget-Epoch": initial_metadata["epoch"]}
        creations, created, creation_keys = [], [], []
        for number, amount in [(1, -321), (2, -123)]:
            body = {"id": identity(number), "account_id": account["id"],
                    "amount": amount, "date": delivered_register["data"][0]["date"],
                    "cleared": True, "notes": f"Native schema18 anchor {number}"}
            key = f"anchor18-{number}"
            creations.append(body)
            creation_keys.append(key)
            created.append(call(root + "/transactions", body, key, headers=owner_headers))
        edit = {"notes": "Owned native history before schedules"}
        edited = call(root + "/transactions/" + identity(1), edit,
                      "owned-edit18", "PATCH", owner_headers)
        history = call(root + "/history", headers=owner_headers)
        assert history["data"]["undo"] is not None
        register = call(register_path)
        assert register["balances"]["total"] == -2245
        assert register["balances"]["cleared"] == -2245
        old_ids = [row["id"] for row in delivered_register["data"]]
        new_ids = [row["id"] for row in register["data"]]
        assert [value for value in new_ids if value not in {identity(1), identity(2)}] == old_ids
        assert new_ids.index(identity(2)) < new_ids.index(identity(1))

        blocked = call("/imports/actual", schedule_bytes, "blocked-schedules18")
        assert blocked["data"]["status"] == "blocked", blocked
        assert "schedule" in json.dumps(blocked["data"]["report"]).lower()
        blocked_id = blocked["data"]["id"]
        assert call(f"/imports/{blocked_id}/original.zip") == schedule_bytes
        blocked_latest = call(f"/imports/{blocked_id}")
        assert call(root + "/transactions?account_id=" + account["id"]) == register
        assert call(root + "/history", headers=owner_headers) == history
        backup = call(root + "/backups", {"label": "Before schedule support"}, "backup18")
        archive = call(root + "/backups/" + backup["data"]["id"] + "/download")
        with zipfile.ZipFile(io.BytesIO(archive)) as contents:
            assert json.loads(contents.read("manifest.json"))["schema_version"] == 18
        metadata = {
            "source_commit": "c8456d7", "accepted_product_commit": "5124e9a",
            "actual_reference_commit": "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
            "executable_sha256": binary_hash, "schema_version": 18,
            "source_archive": outputs["-source.zip"].name,
            "source_archive_sha256": hashlib.sha256(schedule_bytes).hexdigest(),
            "active_source_archive": outputs["-active-source.zip"].name,
            "active_source_archive_sha256": hashlib.sha256(active_bytes).hexdigest(),
            "active_stage": active_stage, "activated": activated,
            "initial_metadata": initial_metadata, "metadata": call(root)["data"],
            "account": account, "handoffs": handoffs, "handoff_results": handoff_results,
            "delivered_register": delivered_register, "owner_headers": owner_headers,
            "creations": creations, "created": created, "creation_keys": creation_keys,
            "edit": edit, "edit_key": "owned-edit18", "edited": edited,
            "history": history, "register": register,
            "balances": call(root + "/accounts/" + account["id"]),
            "checkpoint": call(root + "/checkpoint"), "backup": backup,
            "archive_sha256": hashlib.sha256(archive).hexdigest(),
            "blocked_stage": blocked, "blocked_latest": blocked_latest,
            "blocked_stage_key": "blocked-schedules18",
        }
    finally:
        process.terminate()
        process.wait(timeout=15)
    database_path = Path(directory) / "actualbudget.sqlite3"
    with sqlite3.connect(f"file:{database_path}?mode=ro&immutable=1", uri=True) as db:
        assert db.execute("PRAGMA user_version").fetchone()[0] == 18
        assert db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        assert db.execute("PRAGMA foreign_key_check").fetchall() == []
        assert db.execute("SELECT count(*) FROM transactions WHERE budget_id=? AND tombstone=0", (budget,)).fetchone()[0] == 4
        assert db.execute("SELECT status FROM staged_imports WHERE id=?", (blocked_id,)).fetchone()[0] == "blocked"
    # Publish only a complete successful capture; original fixture names are guarded above.
    shutil.copyfile(database_path, outputs[".sqlite3"])
    outputs[".zip"].write_bytes(archive)
    outputs["-source.zip"].write_bytes(schedule_bytes)
    outputs["-active-source.zip"].write_bytes(active_bytes)
    outputs[".json"].write_text(json.dumps(metadata, indent=2) + "\n")
print("PASS genuine schema18: recovered order/receipts, native anchors/history, blocked schedules and native archive")
