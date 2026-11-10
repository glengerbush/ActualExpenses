"""Genuine pre-expression upgrade, immutable source receipts and native recovery.

Generate the disposable schema26 fixture with --capture /path/to/server-5e9cb42.
Normal runs copy that fixture and exercise only the running public API.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import urllib.error
import urllib.request
import uuid
import zipfile

parser = argparse.ArgumentParser()
parser.add_argument("--capture", type=Path)
args = parser.parse_args()
project = Path(__file__).resolve().parents[1]
fixtures = project / "tests/fixtures"
stem = fixtures / "rule-expressions-schema26"
oracle = json.loads((fixtures / "rule-expression-commits-source.json").read_text())
source = (fixtures / oracle["archive"]["filename"]).read_bytes()
digest = lambda value: hashlib.sha256(value).hexdigest()
assert digest(source) == oracle["archive"]["sha256"]


class Server:
    def __init__(self, directory, binary):
        env = {**os.environ, "ACTUALBUDGET_DATA_DIR": str(directory),
               "ACTUALBUDGET_HOST": "127.0.0.1", "ACTUALBUDGET_PORT": "0",
               "ACTUALBUDGET_WEB_DIR": str(project / "web/dist")}
        env.pop("ACTUALBUDGET_PUBLIC_ORIGIN", None)
        self.process = subprocess.Popen([str(binary)], env=env, stdout=subprocess.PIPE, text=True)
        line = self.process.stdout.readline().strip()
        assert line.startswith("LISTENING "), line
        self.base = "http://" + line.removeprefix("LISTENING ") + "/api/v1"

    def close(self):
        self.process.terminate()
        self.process.wait(timeout=20)

    def call(self, path, body=None, *, method=None, key=None, headers=None, status=200):
        raw = body if isinstance(body, bytes) else None if body is None else json.dumps(body).encode()
        request = urllib.request.Request(self.base + path, data=raw, method=method,
            headers={"Content-Type": "application/zip" if isinstance(body, bytes) else "application/json",
                     "Idempotency-Key": key or str(uuid.uuid4()), **(headers or {})})
        try:
            response = urllib.request.urlopen(request, timeout=30)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            value = json.load(response) if response.headers.get("Content-Type", "").startswith("application/json") else response.read()
            assert response.status == status, (path, response.status, value)
            return value


with tempfile.TemporaryDirectory(prefix="rule-expression-recovery-") as temporary:
    directory = Path(temporary)
    if args.capture:
        assert digest(args.capture.read_bytes()) == "876ca190be5a202d014f870a5fa57a93387f76f331892099e490238c49152ef3"
        # Deliberately malformed input, not an oracle financial fixture.
        with zipfile.ZipFile(io.BytesIO(source)) as original:
            entries = {name: original.read(name) for name in original.namelist()}
        database = directory / "malformed-source.sqlite3"
        database.write_bytes(entries["db.sqlite"])
        with sqlite3.connect(database) as db:
            rule_id = oracle["rules"][0]["id"]
            actions = [{"op": "set", "field": "notes", "value": "",
                        "options": {"template": "{{#if amount}}unterminated"}}]
            db.execute("UPDATE rules SET actions=? WHERE id=?", (json.dumps(actions), rule_id))
        entries["db.sqlite"] = database.read_bytes()
        malformed = io.BytesIO()
        with zipfile.ZipFile(malformed, "w", zipfile.ZIP_DEFLATED) as archive:
            for name, value in entries.items():
                archive.writestr(name, value)
        malformed = malformed.getvalue()
        server = Server(directory, args.capture)
        try:
            valid = server.call("/imports/actual", source, key="legacy-valid-upload", status=201)
            invalid = server.call("/imports/actual", malformed, key="legacy-invalid-upload", status=201)
            assert valid["data"]["status"] == invalid["data"]["status"] == "blocked"
            budget, account, transaction = [str(uuid.uuid4()) for _ in range(3)]
            root = "/budgets/" + budget
            server.call("/budgets", {"id": budget, "name": "Before calculated rules"}, status=201)
            server.call(root + "/accounts", {"id": account, "name": "Manual", "opening_balance": 0, "opening_date": "2026-01-01"}, status=201)
            body = {"id": transaction, "account_id": account, "amount": -1234, "date": "2026-01-02", "cleared": True, "notes": "Keep this manual expense"}
            receipt = server.call(root + "/transactions", body, key="legacy-expense", status=201)
            before = server.call(root + "/transactions")["data"]
            saved = server.call(root + "/backups", {"label": "Before expressions"}, status=201)["data"]
            backup = server.call(root + "/backups/" + saved["id"] + "/download")
            with zipfile.ZipFile(io.BytesIO(backup)) as archive:
                assert json.loads(archive.read("manifest.json"))["schema_version"] == 26
            live_source = (fixtures / "import-unsupported-rule-26.10.0.zip").read_bytes()
            live = server.call("/imports/actual", live_source, status=201)["data"]
            assert live["status"] == "ready", live
            server.call("/imports/" + live["id"] + "/activate", {}, status=201)
            live_root = "/budgets/" + live["budget_id"]
            live_rule = server.call(live_root + "/rules")["data"][0]
            server.call(live_root + "/rules/" + live_rule["id"],
                        {"actions": [{"op": "set", "field": "notes", "value": "Manual edit must survive upgrade"}],
                         "snapshot": server.call(live_root + "/checkpoint")["data"]["snapshot"]}, method="PATCH")
            metadata = {"source_commit": "5e9cb4244f2d5173961596eefb86e2223bbb3667",
                        "executable_sha256": digest(args.capture.read_bytes()),
                        "source_sha256": digest(source), "malformed_sha256": digest(malformed),
                        "valid": valid, "invalid": invalid, "budget": budget,
                        "body": body, "receipt": receipt, "before": before,
                        "backup_sha256": digest(backup), "live_root": live_root,
                        "live_rules": server.call(live_root + "/rules")["data"],
                        "live_transactions": server.call(live_root + "/transactions")["data"]}
        finally:
            server.close()
        database = directory / "actualbudget.sqlite3"
        with sqlite3.connect(database) as db:
            assert db.execute("PRAGMA user_version").fetchone()[0] == 26
        metadata["database_sha256"] = digest(database.read_bytes())
        shutil.copyfile(database, stem.with_suffix(".sqlite3"))
        stem.with_suffix(".zip").write_bytes(backup)
        (fixtures / "rule-expressions-malformed.zip").write_bytes(malformed)
        stem.with_suffix(".json").write_text(json.dumps(metadata, indent=2) + "\n")
        print("PASS captured genuine schema26 blocked stages, manual ledger, immutable receipts and native archive")
    else:
        metadata = json.loads(stem.with_suffix(".json").read_text())
        assert digest(stem.with_suffix(".sqlite3").read_bytes()) == metadata["database_sha256"]
        assert digest(source) == metadata["source_sha256"]
        malformed = (fixtures / "rule-expressions-malformed.zip").read_bytes()
        assert digest(malformed) == metadata["malformed_sha256"]
        shutil.copyfile(stem.with_suffix(".sqlite3"), directory / "actualbudget.sqlite3")
        binary = project / "server/target/debug/actualbudget-server"
        server = Server(directory, binary)
        try:
            call = server.call
            root = "/budgets/" + metadata["budget"]
            assert call(root + "/transactions")["data"] == metadata["before"]
            assert call(metadata["live_root"] + "/rules")["data"] == metadata["live_rules"]
            assert call(metadata["live_root"] + "/transactions")["data"] == metadata["live_transactions"]
            assert call(root + "/transactions", metadata["body"], key="legacy-expense", status=201) == metadata["receipt"]
            valid = call("/imports/" + metadata["valid"]["data"]["id"])["data"]
            assert valid["status"] == "ready", valid
            assert call("/imports/actual", source, key="legacy-valid-upload", status=201) == metadata["valid"]
            assert call("/imports/actual", source, status=201)["data"] == valid
            invalid = call("/imports/" + metadata["invalid"]["data"]["id"])["data"]
            assert invalid["status"] == "blocked"
            failures = [r for r in invalid["report"]["requirements"] if r.get("rule_id") == oracle["rules"][0]["id"]]
            assert failures and "raw" in failures[0], failures
            assert "unterminated" in json.dumps(failures[0]["raw"]), failures
            for stage, original in [(valid, source), (invalid, malformed)]:
                assert call("/imports/" + stage["id"] + "/original") == original
            call("/imports/" + invalid["id"] + "/activate", {}, status=409)
            call("/imports/" + valid["id"] + "/activate", {}, status=201)
            expression_root = "/budgets/" + valid["budget_id"]
            rule = {"id": str(uuid.uuid4()), "stage": None, "conditionsOp": "and",
                    "conditions": oracle["rules"][0]["conditions"], "actions": oracle["cases"][0]["actions"]}
            call(expression_root + "/rules", {**rule, "snapshot": call(expression_root + "/checkpoint")["data"]["snapshot"]}, status=201)
            rules = call(expression_root + "/rules")["data"]
            def check(target):
                assert call(target + "/rules")["data"] == rules
                stamp = call(target + "/checkpoint")
                preview = call(target + "/rules/preview", {"snapshot": stamp["data"]["snapshot"], "ids": [oracle["subject"]], "rule_ids": [rule["id"]]})
                assert preview["data"]["transactions"][0]["after"]["amount"] == oracle["cases"][0]["after"][0]["amount"], preview
                assert call(target + "/checkpoint") == stamp
            check(expression_root)
            saved = call(expression_root + "/backups", {"label": "Calculated expressions"}, status=201)["data"]
            native = call(expression_root + "/backups/" + saved["id"] + "/download")
            duplicate = str(uuid.uuid4())
            call(expression_root + "/duplicate", {"id": duplicate, "name": "Expression copy"}, status=201)
            check("/budgets/" + duplicate)
            # Change an expression, then restore the portable original and execute it again.
            call(expression_root + "/rules/" + rule["id"], {"actions": [{"op": "set", "field": "notes", "value": "changed"}], "snapshot": call(expression_root + "/checkpoint")["data"]["snapshot"]}, method="PATCH")
            name = call(expression_root)["data"]["name"]
            stage = call("/restores", native, status=201)["data"]
            assert call("/restores/" + stage["id"] + "/original.zip") == native
            call(expression_root + "/restores/" + stage["id"] + "/activate", {"confirm_name": name})
            check(expression_root)
            checkpoint = call(expression_root + "/checkpoint")["data"]
            apply_headers = {"X-Actual-Budget-Epoch": checkpoint["epoch"]}
            command = {"snapshot": checkpoint["snapshot"],
                       "ids": [oracle["subject"]], "rule_ids": [rule["id"]]}
            applied = call(expression_root + "/rules/apply", command, key="restored-expression-apply", headers=apply_headers)
            assert call(expression_root + "/rules/apply", command, key="restored-expression-apply", headers=apply_headers) == applied
            assert call(expression_root + "/transactions/" + oracle["subject"])["data"]["amount"] == oracle["cases"][0]["after"][0]["amount"]
            old_native = stem.with_suffix(".zip").read_bytes()
            assert digest(old_native) == metadata["backup_sha256"]
            old_stage = call("/restores", old_native, status=201)["data"]
            call(root + "/restores/" + old_stage["id"] + "/activate", {"confirm_name": "Before calculated rules"})
            assert call(root + "/transactions")["data"] == metadata["before"]
            assert call(root + "/transactions", metadata["body"], key="legacy-expense", status=201) == metadata["receipt"]
        finally:
            server.close()
        # A second startup must not repeat the migration or change restored state.
        backups = set((directory / "backups").glob("before-v26-*.sqlite3"))
        assert len(backups) == 1, backups
        server = Server(directory, binary)
        try:
            assert server.call(root + "/transactions")["data"] == metadata["before"]
            assert server.call(expression_root + "/rules")["data"] == rules
            assert server.call(metadata["live_root"] + "/rules")["data"] == metadata["live_rules"]
            assert server.call(expression_root + "/rules/apply", command, key="restored-expression-apply", headers=apply_headers) == applied
            assert server.call(expression_root + "/transactions/" + oracle["subject"])["data"]["amount"] == oracle["cases"][0]["after"][0]["amount"]
        finally:
            server.close()
        assert set((directory / "backups").glob("before-v26-*.sqlite3")) == backups
        print("PASS genuine26 upgrade, refreshed capabilities, malformed diagnostics/originals, exact receipts, native export/duplicate/restore and repeated startup")
