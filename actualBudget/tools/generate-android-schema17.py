"""Capture genuine schema17 source ranks, native anchors and owned history.

Usage: python3 generate-android-schema17.py SCHEMA17_BINARY [ACTUAL_SOURCE_ZIP]
The optional source defaults to the retained android-schema17-source.zip.
Only disposable HTTP-created state is used; no schema versions are rewritten.
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
assert binary_hash == "f4043c68c6b09bb848757a40b6e5c73a4636e3a8e82dbd820dd22a2fe268fab6"
source = Path(sys.argv[2]) if len(sys.argv) > 2 else fixtures / "android-schema17-source.zip"
source_bytes = source.read_bytes()
source_hash = hashlib.sha256(source_bytes).hexdigest()


def identity(number):
    return f"17180000-0000-4000-8000-{number:012}"


with tempfile.TemporaryDirectory(prefix="android-schema17-") as directory:
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

        stage = call("/imports/actual", source_bytes, "source17")
        assert stage["data"]["status"] == "ready", stage
        stage_id = stage["data"]["id"]
        activated = call(f"/imports/{stage_id}/activate", {}, "activate17")
        budget_id = stage["data"]["budget_id"]
        root = "/budgets/" + budget_id
        metadata = call(root)["data"]
        accounts = call(root + "/accounts")["data"]
        account = next(row for row in accounts if row["name"] == "Spending")
        register_path = root + "/transactions?account_id=" + account["id"]
        imported_register = call(register_path)
        expense = next(row for row in imported_register["data"]
                       if row["amount"] < 0 and not row["starting_balance"])
        owner_headers = {"X-Actual-History-Owner": identity(9),
                         "X-Actual-Budget-Epoch": metadata["epoch"]}
        creations, created = [], []
        for number, amount in [(1, -321), (2, -123)]:
            body = {"id": identity(number), "account_id": account["id"],
                    "amount": amount, "date": expense["date"], "cleared": True,
                    "notes": f"Native schema17 anchor {number}"}
            creations.append(body)
            created.append(call(root + "/transactions", body,
                                f"anchor17-{number}", headers=owner_headers))
        edit = {"notes": "Owned native anchor before upgrade"}
        edited = call(root + "/transactions/" + identity(1), edit,
                      "owned-edit17", "PATCH", owner_headers)
        history = call(root + "/history", headers=owner_headers)
        assert history["data"]["undo"] is not None, history
        register = call(register_path)
        balances = call(root + "/accounts/" + account["id"])
        assert register["balances"]["total"] == imported_register["balances"]["total"] - 444
        assert register["balances"]["cleared"] == imported_register["balances"]["cleared"] - 444
        before_ids = [row["id"] for row in imported_register["data"]]
        after_ids = [row["id"] for row in register["data"]]
        assert len(after_ids) == len(before_ids) + 2
        assert [row for row in after_ids if row not in {identity(1), identity(2)}] == before_ids
        assert after_ids.index(identity(2)) < after_ids.index(identity(1))
        checkpoint = call(root + "/checkpoint")
        backup = call(root + "/backups", {"label": "Before Android order upgrade"}, "backup17")
        archive = call(root + "/backups/" + backup["data"]["id"] + "/download")
        with zipfile.ZipFile(io.BytesIO(archive)) as contents:
            assert json.loads(contents.read("manifest.json"))["schema_version"] == 17
        (fixtures / "android-schema17.zip").write_bytes(archive)
        (fixtures / "android-schema17-source.zip").write_bytes(source_bytes)
        (fixtures / "android-schema17.json").write_text(json.dumps({
            "source_commit": "3747e04", "accepted_product_commit": "f486a7b",
            "executable_sha256": binary_hash, "schema_version": 17,
            "source_archive": "android-schema17-source.zip", "source_archive_sha256": source_hash,
            "stage": stage, "activated": activated, "metadata": metadata,
            "account": account, "imported_register": imported_register,
            "owner_headers": owner_headers, "creations": creations, "created": created,
            "creation_keys": ["anchor17-1", "anchor17-2"],
            "edit": edit, "edit_key": "owned-edit17", "edited": edited,
            "history": history, "register": register, "balances": balances,
            "checkpoint": checkpoint,
        }, indent=2) + "\n")
    finally:
        process.terminate()
        process.wait(timeout=15)
    output = fixtures / "android-schema17.sqlite3"
    shutil.copyfile(Path(directory) / "actualbudget.sqlite3", output)
    with sqlite3.connect(f"file:{output}?mode=ro&immutable=1", uri=True) as database:
        assert database.execute("PRAGMA user_version").fetchone()[0] == 17
        assert database.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        assert database.execute("PRAGMA foreign_key_check").fetchall() == []
        for number in [1, 2]:
            assert database.execute("SELECT count(*) FROM transactions WHERE id=?",
                                    (identity(number),)).fetchone()[0] == 1
print("PASS genuine schema17 source, two native anchors, owned history, original receipts and archive")
