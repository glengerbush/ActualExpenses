"""Public genuine24 -> current upgrade: retained original settings receipt."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import urllib.error
import urllib.request
import uuid

parser = argparse.ArgumentParser()
parser.add_argument("--legacy-binary", type=Path, required=True)
args = parser.parse_args()
assert hashlib.sha256(args.legacy_binary.read_bytes()).hexdigest() == "e0d425e2c5c9809f673f98ce700987f8286be0957fb12f03d05331513249edd9"
current = Path(__file__).resolve().parents[1] / "server/target/debug/actualbudget-server"
budget = str(uuid.uuid4())
changes = {"date_format": "dd.MM.yyyy", "experimental": {"enableBanking": True, "akahuBankSync": True}}
with tempfile.TemporaryDirectory(prefix="manual-preference-upgrade-") as directory:
    original = None
    for binary in [args.legacy_binary, current]:
        env = {**os.environ, "ACTUALBUDGET_DATA_DIR": directory, "ACTUALBUDGET_WEB_DIR": directory,
               "ACTUALBUDGET_HOST": "127.0.0.1", "ACTUALBUDGET_PORT": "0"}
        env.pop("ACTUALBUDGET_PUBLIC_ORIGIN", None)
        process = subprocess.Popen([str(binary)], env=env, stdout=subprocess.PIPE, text=True)
        try:
            line = process.stdout.readline().strip()
            assert line.startswith("LISTENING "), line
            origin = "http://" + line.removeprefix("LISTENING ") + "/api/v1"
            def call(path, body=None, method=None, key=None, status=200):
                request = urllib.request.Request(origin + path, data=None if body is None else json.dumps(body).encode(),
                    method=method, headers={"Content-Type": "application/json", "Idempotency-Key": key or str(uuid.uuid4())})
                try:
                    response = urllib.request.urlopen(request, timeout=10)
                except urllib.error.HTTPError as error:
                    response = error
                with response:
                    value = json.load(response)
                    assert response.status == status, value
                    return value
            root = "/budgets/" + budget
            if original is None:
                call("/budgets", {"id": budget, "name": "Old settings receipt"}, status=201)
                original = call(root + "/preferences", changes, "PATCH", "original-settings")
            else:
                values = call(root + "/preferences")["data"]
                assert values["date_format"] == "dd.MM.yyyy"
                assert not {"enableBanking", "akahuBankSync"}.intersection(values["experimental"])
                assert call(root + "/preferences", changes, "PATCH", "original-settings") == original
                call(root + "/preferences", changes, "PATCH", "new-request", status=400)
                assert call(root + "/preferences")["data"] == values
        finally:
            process.terminate()
            process.wait(timeout=10)
print("PASS genuine24 settings receipt resolves exactly; retired flags stay absent and new connector writes fail")
