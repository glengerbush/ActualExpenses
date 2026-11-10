"""Real accepted24 capture through HTTP; original SQLite is inspected read-only."""
import base64
import hashlib
import io
import json
import os
from pathlib import Path
import select
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import threading
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import urllib.error
import urllib.request
import zipfile

fixtures = Path(__file__).resolve().parents[1] / "tests/fixtures"
binary = Path(sys.argv[1]).resolve()
sha = lambda b: hashlib.sha256(b).hexdigest()
binary_sha = sha(binary.read_bytes())
assert binary_sha == "e0d425e2c5c9809f673f98ce700987f8286be0957fb12f03d05331513249edd9"
outputs = {e: fixtures / ("gocardless-schema24" + e) for e in [".json", ".zip", ".sqlite3"]}
assert not any(p.exists() for p in outputs.values()), "Refusing to overwrite original fixtures"
linked = json.loads((fixtures / "bank-linked-source-26.10.json").read_text())["cases"][0]
source = fixtures / "bank-sync-source" / linked["archive"]["filename"]
assert sha(source.read_bytes()) == linked["archive"]["sha256"]
observed, release = threading.Event(), threading.Event()
state = {"remote": None, "hold": False, "calls": 0}


class Provider(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        assert self.path == "/claim"
        self.send_response(200)
        self.end_headers()
        self.wfile.write(f"http://synthetic:NOT-A-REAL-SECRET@127.0.0.1:{self.server.server_port}/access".encode())

    def do_GET(self):
        assert self.path.startswith("/access/accounts")
        state["calls"] += 1
        if state["hold"]:
            observed.set()
            assert release.wait(10), "Held fixture download was not released"
        remote = state["remote"]
        data = {"accounts": [{"id": remote["remote_id"], "conn_id": remote["remote_connection_id"],
                 "name": "Genuine24 retained checking", "org": {"id": remote["remote_connection_id"], "name": "Synthetic institution"},
                 "currency": "USD", "balance": "100.00", "balance-date": 1794225600,
                 "transactions": [] if "balances-only" in self.path else [{"id": "before-gocardless-bank-row", "posted": 1794009600, "amount": "-1.23", "payee": "UPGRADE SHOP", "description": "Retained bank #note"}]}], "errlist": [], "connections": []}
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(data).encode())


provider = ThreadingHTTPServer(("127.0.0.1", 0), Provider)
threading.Thread(target=provider.serve_forever, daemon=True).start()
provider_origin = f"http://127.0.0.1:{provider.server_port}"
token = base64.b64encode((provider_origin + "/claim").encode()).decode()
owner = "29290000-0000-4000-8000-000000000024"
stamp = datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")
receipts = []
try:
    with tempfile.TemporaryDirectory(prefix="gocardless-schema24-") as directory:
        env = {**os.environ, "ACTUALBUDGET_DATA_DIR": directory, "ACTUALBUDGET_HOST": "127.0.0.1", "ACTUALBUDGET_PORT": "0", "ACTUALBUDGET_BANK_TEST_ORIGIN": provider_origin}
        env.pop("ACTUALBUDGET_PUBLIC_ORIGIN", None)
        process = subprocess.Popen([str(binary)], env=env, stdout=subprocess.PIPE, text=True)
        try:
            assert select.select([process.stdout], [], [], 15)[0]
            line = process.stdout.readline().strip()
            assert line.startswith("LISTENING "), line
            base = "http://" + line.removeprefix("LISTENING ") + "/api/v1"

            def call(path, body=None, *, key="fixture", headers=None, status=200, method=None, raw=False):
                data = body if isinstance(body, bytes) else json.dumps(body, separators=(",", ":")).encode() if body is not None else None
                h = {"Idempotency-Key": key, **(headers or {})}
                if data is not None:
                    h.setdefault("Content-Type", "application/json")
                req = urllib.request.Request(base + path, data=data, headers=h, method=method or ("POST" if data is not None else "GET"))
                try:
                    response = urllib.request.urlopen(req, timeout=15)
                except urllib.error.HTTPError as error:
                    response = error
                with response:
                    result = response.read()
                    assert response.status == status, (path, response.status, result[:1000])
                return result if raw else json.loads(result)

            stage = call("/imports/actual", source.read_bytes(), key="genuine-source-stage", headers={"Content-Type": "application/zip"}, status=201)["data"]
            assert stage["status"] == "ready"
            budget = call("/imports/" + stage["id"] + "/activate", {}, key="genuine-source-activate", status=201)["data"]["id"]
            root = "/budgets/" + budget
            bank = call(root + "/banking")["data"]
            assert bank["connections"] and bank["accounts"] and bank["links"]
            connection = bank["connections"][0]
            state["remote"] = bank["accounts"][0]
            account = bank["links"][0]["id"]
            epoch = bank["epoch"]
            headers = {"X-Actual-Budget-Epoch": epoch}

            def owned(n):
                return {**headers, "X-Actual-History-Owner": owner, "X-Actual-Edit-Clock": f"{stamp}-{n:04x}-2929292929292929"}

            path = root + "/banking/connections/" + connection["id"] + "/commands"
            connection = call(path, {"action": "replace", "generation": connection["generation"], "setup_token": token}, key="old-simplefin-replace", headers=headers)["data"]
            connection = call(path, {"action": "claim", "generation": connection["generation"]}, key="old-simplefin-claim", headers=headers)["data"]
            call(path, {"action": "discover", "generation": connection["generation"]}, key="old-simplefin-discover", headers=headers)
            bank = call(root + "/banking")["data"]
            call(root + f"/accounts/{account}/banking/commands", {"action": "link", "snapshot": bank["snapshot"], "connection_id": connection["id"], "remote_account_id": bank["accounts"][0]["id"], "generation": connection["generation"], "opening_balance_mode": "none"}, key="old-simplefin-link", headers=headers)
            bank = call(root + "/banking")["data"]
            refresh = {"action": "refresh", "generation": connection["generation"], "snapshot": bank["snapshot"], "start_date": "2026-11-01", "end_date": "2026-11-10", "accounts": [{"account_id": account, "revision": bank["links"][0]["revision"]}]}
            result = call(path, refresh, key="old-bank-refresh", headers=owned(1))
            assert len(call(root + "/transactions")["data"]) > 0
            receipts.append({"path": path, "body": refresh, "key": "old-bank-refresh", "headers": owned(1), "response": result})
            expense = "29290000-0000-4000-8000-000000000077"
            body = {"id": expense, "account_id": account, "date": "2026-11-08", "amount": -77, "notes": "Genuine24 owned expense"}
            result = call(root + "/transactions", body, key="old-owned-expense", headers=owned(2), status=201)
            receipts.append({"path": root + "/transactions", "body": body, "key": "old-owned-expense", "headers": owned(2), "status": 201, "response": result})
            patch = {"notes": "Genuine24 retained edit"}
            result = call(root + "/transactions/" + expense, patch, method="PATCH", key="old-owned-edit", headers=owned(3))
            receipts.append({"path": root + "/transactions/" + expense, "method": "PATCH", "body": patch, "key": "old-owned-edit", "headers": owned(3), "response": result})
            note_id = "genuine24-arbitrary-note"
            note_body = {"id": note_id, "note": "Notes24 retained through banking25", "snapshot": call(root + "/checkpoint")["data"]["snapshot"]}
            call(root + "/notes/commands", note_body, key="old-note", headers=owned(4))
            # Preserve a genuinely captured old request: its snapshot becomes stale
            # while the provider GET is held, so the old runtime retains its job.
            bank = call(root + "/banking")["data"]
            pending = {**refresh, "snapshot": bank["snapshot"]}
            pending_headers = owned(5)
            state["hold"] = True
            with ThreadPoolExecutor(max_workers=1) as pool:
                future = pool.submit(call, path, pending, key="old-captured-refresh", headers=pending_headers, status=409)
                assert observed.wait(5)
                note_body = {"id": note_id, "note": "Notes24 edit while bank GET held", "snapshot": call(root + "/checkpoint")["data"]["snapshot"]}
                call(root + "/notes/commands", note_body, key="old-note-during-fetch", headers=owned(6))
                release.set()
                assert future.result()["error"]["code"] != "operation_canceled"
            state["hold"] = False
            snapshot = {name: call(root + suffix, headers={"X-Actual-History-Owner": owner}) for name, suffix in [("budget", ""), ("accounts", "/accounts"), ("transactions", "/transactions"), ("payees", "/payees"), ("categories", "/categories"), ("banking", "/banking"), ("history", "/history"), ("checkpoint", "/checkpoint")]}
            snapshot["note"] = call(root + "/notes/read", {"id": note_id})
            point = call(root + "/backups", {"label": "Genuine accepted24 before GoCardless25"}, key="old-native-backup", headers=headers, status=201)["data"]
            native = call(root + "/backups/" + point["id"] + "/download", raw=True)
            with zipfile.ZipFile(io.BytesIO(native)) as archive:
                assert json.loads(archive.read("manifest.json"))["schema_version"] == 24
            outputs[".zip"].write_bytes(native)
        finally:
            release.set()
            process.terminate()
            process.wait(timeout=15)
        database = Path(directory) / "actualbudget.sqlite3"
        with sqlite3.connect(f"file:{database}?mode=ro&immutable=1", uri=True) as db:
            assert db.execute("pragma user_version").fetchone()[0] == 24
            assert db.execute("pragma integrity_check").fetchone()[0] == "ok"
            assert not db.execute("pragma foreign_key_check").fetchall()
            counts = {table: db.execute("SELECT count(*) FROM " + table).fetchone()[0] for table in ["bank_connections", "bank_credentials", "bank_remote_accounts", "bank_links", "bank_jobs", "bank_request_log", "bank_transaction_origins", "notes"]}
            assert all(counts.values()), counts
        shutil.copyfile(database, outputs[".sqlite3"])
    manifest = {"schema_version": 24, "accepted_commit": "278e51f", "binary_sha256": binary_sha, "budget_id": budget, "account_id": account, "owner": owner, "note_id": note_id, "snapshot": snapshot, "receipts": receipts, "captured_request": {"path": path, "body": pending, "key": "old-captured-refresh", "headers": pending_headers}, "source": {"path": "bank-sync-source/" + source.name, "sha256": sha(source.read_bytes())}, "readonly_counts": counts, "scope": "Genuine24 public setup, real controlled-provider delivery and stale captured request. Every banking table is nonempty; notes24/history/receipts/financial graph retained. SQLite is never rewritten. NativeZIP excludes private credentials/jobs; DB retains them. Only synthetic provider/credentials are used.", "artifacts": {k: {"sha256": sha(p.read_bytes()), "bytes": p.stat().st_size} for k, p in outputs.items() if k != ".json"}}
    outputs[".json"].write_text(json.dumps(manifest, indent=2) + "\n")
    print("PASS genuine24: every banking table nonempty, source graph/notes/history/receipts, real captured GET, native archive and readonly SQLite integrity")
finally:
    release.set()
    provider.shutdown()
    provider.server_close()
