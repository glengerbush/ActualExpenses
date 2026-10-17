#!/usr/bin/env python3
"""Upload the existing local signing identity as encrypted repository Actions secrets."""
import base64
import json
import os
from pathlib import Path
import subprocess

repository = "glengerbush/ActualExpenses"
subprocess.run(["gh", "auth", "status"], check=True)
directory = Path(os.environ.get("ACTUALEXPENSES_SIGNING_DIR", "~/.local/share/actualexpenses/signing")).expanduser()
identity = json.loads((directory / "signing.json").read_text())
values = {
    "ACTUALEXPENSES_KEYSTORE_BASE64": base64.b64encode(Path(identity["keystore"]).read_bytes()),
    "ACTUALEXPENSES_STORE_PASSWORD": identity["password"].encode(),
    "ACTUALEXPENSES_KEY_PASSWORD": identity["password"].encode(),
    "ACTUALEXPENSES_KEY_ALIAS": identity["alias"].encode(),
}
for name, value in values.items():
    # stdin avoids putting credentials in shell history, argv or temporary files.
    subprocess.run(["gh", "secret", "set", name, "--repo", repository], input=value, check=True)
    print(f"Configured {name}")
print("Release signing configured using the existing APK signing identity.")
