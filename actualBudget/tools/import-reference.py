#!/usr/bin/env python3
"""Reproduce the unchanged source-only Actual reference from a pinned checkout."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

COMMIT = "2bebdbaae0b701120a8cad9d56aafd64fa04c577"
PACKAGES = {
    "api", "cli", "component-library", "crdt", "desktop-client",
    "loot-core", "plugins-service", "sync-server", "vite-plugin-peggy",
}
DESTINATION = Path(__file__).resolve().parents[1] / "reference" / "actual-26.10.0"


def exclusion(path):
    parts = path.parts
    if path.as_posix() == "LICENSE.txt":
        return None
    if parts[0] != "packages" or len(parts) < 3:
        return "upstream workspace, deployment, tooling or repository metadata"
    if parts[1] == "docs":
        if len(parts) > 3 and parts[2] == "docs" and path.suffix in {".md", ".mdx"} and "desktop" not in path.name and "electron" not in path.name:
            return None
        return "documentation application/assets or desktop-only documentation"
    if parts[1] not in PACKAGES:
        return "desktop/native-mobile package or unrelated upstream tooling"
    if any(part.startswith(".") for part in parts[2:]) or path.name in {"AGENTS.md", "CLAUDE.md"}:
        return "upstream agent/editor/package configuration"
    if "electron" in path.name or ".desktop." in path.name:
        return "desktop-only platform implementation"
    if "bin" in parts[2:] or "scripts" in parts[2:] or "build" in parts[2:]:
        return "upstream package build/install tooling"
    if path.name == "package.json" or ".config." in path.name or path.name.startswith("tsconfig"):
        return "upstream executable package/workspace configuration"
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("checkout", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    source = args.checkout.resolve()
    git = ["git", "-C", str(source)]
    head = subprocess.check_output([*git, "rev-parse", "HEAD"], text=True).strip()
    if head != COMMIT:
        raise SystemExit(f"Expected pinned commit {COMMIT}; found {head}")
    if subprocess.check_output([*git, "status", "--porcelain", "--untracked-files=no"], text=True).strip():
        raise SystemExit("Reference checkout contains tracked changes; use a clean pinned checkout.")
    paths = sorted(subprocess.check_output([*git, "ls-files", "-z"], text=True).rstrip("\0").split("\0"))
    manifest = {"repository": "https://github.com/actualbudget/actual", "version": "26.10.0", "commit": COMMIT,
                "kind": "source-only reference; not a production workspace", "files": {}, "excluded": {}}
    retained = []
    for name in paths:
        reason = exclusion(Path(name))
        if reason:
            manifest["excluded"][name] = reason
            continue
        path = source / name
        if path.is_symlink():
            raise SystemExit(f"Unexpected source symlink: {name}")
        manifest["files"][name] = hashlib.sha256(path.read_bytes()).hexdigest()
        retained.append(name)
    serialized = json.dumps(manifest, indent=2, sort_keys=True) + "\n"
    if args.check:
        if (DESTINATION / "manifest.json").read_text() != serialized:
            raise SystemExit("Reference manifest differs from pinned source selection.")
        actual = {str(path.relative_to(DESTINATION)) for path in DESTINATION.rglob("*") if path.is_file()}
        if actual != set(retained) | {"manifest.json"}:
            raise SystemExit("Reference contains missing or additional files.")
        for name in retained:
            if hashlib.sha256((DESTINATION / name).read_bytes()).hexdigest() != manifest["files"][name]:
                raise SystemExit(f"Reference source differs: {name}")
        print(f"Verified {len(retained)} unchanged files at {COMMIT}.")
        return
    if DESTINATION.exists():
        raise SystemExit("Reference already exists; use --check rather than overwrite it.")
    DESTINATION.mkdir(parents=True)
    for name in retained:
        destination = DESTINATION / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source / name, destination)
    (DESTINATION / "manifest.json").write_text(serialized)
    print(f"Imported {len(retained)} unchanged files; excluded {len(manifest['excluded'])} documented paths.")


if __name__ == "__main__":
    main()
