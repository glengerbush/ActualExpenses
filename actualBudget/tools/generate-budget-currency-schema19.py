"""Capture real schema19 preference edits after a JPY source import."""
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
assert hashlib.sha256(binary.read_bytes()).hexdigest() == "eb1b47e56443f805c5ae828916329d1501ad5f1f67481e2a91c29a28b5c6b135"
source = (fixtures / "budget-goal-upgrade-source-jpy.zip").read_bytes()
source_metadata = json.loads((fixtures / "budget-goal-upgrade-source-jpy.json").read_text())
cases = [
    ("untouched", [], 73),
    ("usd", [{"currency_code": "USD", "experimental": {"currency": True}}], 7299),
    ("disabled", [{"currency_code": "JPY", "experimental": {"currency": True}},
                  {"experimental": {"currency": False}}], 7299),
    ("unrelated", [{"date_format": "yyyy-MM-dd"}], 73),
    ("reset", [{"currency_code": "JPY", "experimental": {"currency": True}},
               {"experimental": None}], 7299),
    ("reset_only", [{"experimental": None}], 7299),
    ("ambiguous", [], 7299),
]
only = next((arg.removeprefix("--only=") for arg in sys.argv[2:] if arg.startswith("--only=")), None)
if only:
    cases = [case for case in cases if case[0] == only]
    assert cases, "Unknown fixture case"
assert not any((fixtures / f"budget-currency-schema19-{name}{ext}").exists()
               for name, _, _ in cases for ext in [".json", ".sqlite3", ".zip"])
for name, edits, expected in cases:
    with tempfile.TemporaryDirectory(prefix="budget-currency-schema19-") as directory:
        env = {**os.environ, "ACTUALBUDGET_DATA_DIR": directory,
               "ACTUALBUDGET_WEB_DIR": directory, "ACTUALBUDGET_HOST": "127.0.0.1",
               "ACTUALBUDGET_PORT": "0"}
        env.pop("ACTUALBUDGET_PUBLIC_ORIGIN", None)
        process = subprocess.Popen([str(binary)], env=env, stdout=subprocess.PIPE, text=True)
        try:
            line = process.stdout.readline().strip()
            assert line.startswith("LISTENING "), line
            url = "http://" + line.removeprefix("LISTENING ") + "/api/v1"

            def call(path, body=None, key="read", method=None, headers=None):
                raw = body if isinstance(body, bytes) else None if body is None else json.dumps(body).encode()
                request = urllib.request.Request(url + path, data=raw, method=method,
                    headers={"Idempotency-Key": key, "Content-Type": "application/zip"
                             if isinstance(body, bytes) else "application/json", **(headers or {})})
                with urllib.request.urlopen(request, timeout=15) as response:
                    return json.load(response) if response.headers.get("Content-Type", "").startswith("application/json") else response.read()

            stage = call("/imports/actual", source, "jpy-source")
            assert stage["data"]["status"] == "ready", stage
            call(f"/imports/{stage['data']['id']}/activate", {}, "activate")
            root = "/budgets/" + stage["data"]["budget_id"]
            receipts = []
            if name == "ambiguous":
                # Genuine divergent receipt history: restore revision0, then
                # make a different revision1 edit with the same resulting state.
                baseline = call(root + "/backups", {"label": "Before currency branch"}, "branch-backup")
                edit = {"experimental": {"currency": False}}
                receipts.append({"body": edit, "key": "branch-currency",
                                 "result": call(root + "/preferences", edit, "branch-currency", "PATCH")})
                call(root + "/backups/" + baseline["data"]["id"] + "/restore",
                     {"confirm_name": call(root)["data"]["name"]}, "restore-branch")
                epoch = call(root)["data"]["epoch"]
                edit = {"date_format": "MM/dd/yyyy"}
                receipts.append({"body": edit, "key": "branch-date",
                                 "result": call(root + "/preferences", edit, "branch-date", "PATCH",
                                                {"X-Actual-Budget-Epoch": epoch})})
            for index, edit in enumerate(edits):
                key = "currency-" + str(index)
                receipts.append({"body": edit, "key": key,
                                 "result": call(root + "/preferences", edit, key, "PATCH")})
            preferences = call(root + "/preferences")
            assert preferences["data"]["currency_code"] == ("USD" if name == "usd" else "")
            plan = call(root + "/budget?month=2026-11")
            category = source_metadata["ids"]["Fixed"]
            row = next(c for c in plan["data"]["months"]["2026-11"]["categories"] if c["id"] == category)
            assert row["budgeted"] == 73 and json.loads(row["goal"]) == 1000
            backup = call(root + "/backups", {"label": "Currency provenance"}, "backup",
                          headers={"X-Actual-Budget-Epoch": call(root)["data"]["epoch"]})
            archive = call(root + "/backups/" + backup["data"]["id"] + "/download")
            with zipfile.ZipFile(io.BytesIO(archive)) as contents:
                assert json.loads(contents.read("manifest.json"))["schema_version"] == 19
            result = {"name": name, "schema_version": 19, "source_commit": "94bd0c2",
                      "source_sha256": hashlib.sha256(source).hexdigest(), "metadata": call(root)["data"],
                      "category_id": category, "receipts": receipts, "preferences": preferences,
                      "plan": plan, "checkpoint": call(root + "/checkpoint"),
                      "register": call(root + "/transactions"), "expected_template_allocation": expected,
                      "archive_sha256": hashlib.sha256(archive).hexdigest()}
        finally:
            process.terminate()
            process.wait(timeout=15)
        database = Path(directory) / "actualbudget.sqlite3"
        with sqlite3.connect(f"file:{database}?mode=ro&immutable=1", uri=True) as db:
            assert db.execute("PRAGMA user_version").fetchone()[0] == 19
            assert db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
            assert db.execute("PRAGMA foreign_key_check").fetchall() == []
        prefix = fixtures / ("budget-currency-schema19-" + name)
        shutil.copyfile(database, prefix.with_suffix(".sqlite3"))
        prefix.with_suffix(".zip").write_bytes(archive)
        prefix.with_suffix(".json").write_text(json.dumps(result, indent=2) + "\n")
print("PASS genuine19 currency provenance: original JPY, later USD, later disabled currency")
