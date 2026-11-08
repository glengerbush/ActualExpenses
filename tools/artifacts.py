#!/usr/bin/env python3
"""Check release bytes and the preserved Android signing/package identity."""
import argparse
import hashlib
import os
from pathlib import Path
import re
import subprocess

ANDROID_CERTIFICATE = "bb420a6b8dfdbcfd849a29f7c831d6b72b824fd411648dd8d92e9c6f85d8064c"


def sha256(path):
    with path.open("rb") as contents:
        return hashlib.file_digest(contents, "sha256").hexdigest()


def checksums(directory, names, write=False):
    if not names or len(set(names)) != len(names) or any(not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]*", name) for name in names):
        raise ValueError("Expected unique artifact basenames")
    if any((directory / name).is_symlink() for name in names):
        raise ValueError("Release artifacts must be regular files")
    expected = "".join(f"{sha256(directory / name)}  {name}\n" for name in sorted(names))
    manifest = directory / "SHA256SUMS"
    if write:
        manifest.write_text(expected)
    elif manifest.read_text() != expected:
        raise ValueError("SHA256SUMS differs from the expected artifacts")


def verify_apk(directory, version, code, apksigner="apksigner", aapt="aapt"):
    name = f"ActualExpenses-{version}.apk"
    checksums(directory, [name])
    artifact = directory / name
    certificate = subprocess.check_output([apksigner, "verify", "--verbose", "--print-certs", str(artifact)], text=True)
    digests = re.findall(r"Signer #\d+ certificate SHA-256 digest: ([0-9a-f]{64})", certificate)
    if digests != [ANDROID_CERTIFICATE]:
        raise ValueError("APK signing identity differs from the original Android certificate")
    metadata = subprocess.check_output([aapt, "dump", "badging", str(artifact)], text=True)
    match = re.search(r"^package: name='([^']+)' versionCode='(\d+)' versionName='([^']+)'", metadata)
    if not match or match.groups() != ("org.actualexpenses.android", str(code), version):
        raise ValueError("APK package/version does not match the release plan")
    print(f"Verified APK checksum, original certificate, package and version: {name}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    checksum = commands.add_parser("checksums")
    checksum.add_argument("--directory", type=Path, required=True)
    checksum.add_argument("--write", action="store_true")
    checksum.add_argument("names", nargs="+")
    apk = commands.add_parser("apk")
    apk.add_argument("--directory", type=Path, required=True)
    apk.add_argument("--version", required=True)
    apk.add_argument("--code", type=int, required=True)
    apk.add_argument("--apksigner", default=os.environ.get("APKSIGNER", "apksigner"))
    apk.add_argument("--aapt", default=os.environ.get("AAPT", "aapt"))
    args = parser.parse_args()
    if args.command == "checksums":
        checksums(args.directory, args.names, args.write)
    else:
        verify_apk(args.directory, args.version, args.code, args.apksigner, args.aapt)


if __name__ == "__main__":
    main()
