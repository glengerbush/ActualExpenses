#!/usr/bin/env python3
"""Build with a stable local signing identity; keep all signing material outside Git."""
import json
import os
from pathlib import Path
import secrets
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
signing = Path(os.environ.get("ACTUALEXPENSES_SIGNING_DIR", "~/.local/share/actualexpenses/signing")).expanduser()
config = signing / "signing.json"
if not config.exists():
    if "--initialize-signing" not in sys.argv:
        sys.exit("No signing identity. Run tools/build-release.py --initialize-signing once; back up the resulting signing directory.")
    signing.mkdir(parents=True, exist_ok=True, mode=0o700)
    os.chmod(signing, 0o700)
    keystore = signing / "actualexpenses.p12"
    if keystore.exists():
        sys.exit("Existing keystore has no signing.json; recover its password instead of replacing it.")
    password = secrets.token_urlsafe(32)
    env = dict(os.environ, ACTUALEXPENSES_NEW_KEY_PASSWORD=password)
    java_home = os.environ.get("JAVA_HOME")
    keytool = str(Path(java_home) / "bin/keytool") if java_home else "keytool"
    subprocess.run([keytool, "-genkeypair", "-storetype", "PKCS12", "-keystore", str(keystore),
                    "-storepass:env", "ACTUALEXPENSES_NEW_KEY_PASSWORD", "-keypass:env", "ACTUALEXPENSES_NEW_KEY_PASSWORD",
                    "-alias", "actualexpenses", "-keyalg", "RSA", "-keysize", "3072", "-validity", "10000",
                    "-dname", "CN=ActualExpenses, OU=Local distribution"], env=env, check=True)
    os.chmod(keystore, 0o600)
    descriptor = os.open(config, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w") as output:
        json.dump({"keystore": str(keystore), "password": password, "alias": "actualexpenses"}, output)

identity = json.loads(config.read_text())
env = dict(os.environ, ACTUALEXPENSES_KEYSTORE=identity["keystore"],
           ACTUALEXPENSES_STORE_PASSWORD=identity["password"],
           ACTUALEXPENSES_KEY_PASSWORD=identity["password"], ACTUALEXPENSES_KEY_ALIAS=identity["alias"])
subprocess.run([str(root / "gradlew"), ":app:assembleRelease"], cwd=root, env=env, check=True)
print("Signed APK:", root / "app/build/outputs/apk/release/app-release.apk")
print("Back up the signing identity privately:", signing)
