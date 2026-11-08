#!/usr/bin/env python3
"""Version-bump detection and resumable GitHub publication. No third-party dependencies."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools"))
from github_release import publish_release

VERSION_FILES = ("actualExpenses/app/build.gradle.kts", "app/build.gradle.kts")


def run(*args):
    return subprocess.check_output(args, text=True).strip()


def version_at(ref):
    # A missing file is a first release; an unavailable commit is an error.
    # Read from the repository root even when CI runs inside actualExpenses.
    git = ("git", "-C", run("git", "rev-parse", "--show-toplevel"))
    run(*git, "rev-parse", "--verify", f"{ref}^{{commit}}")
    version_file = next((path for path in VERSION_FILES
                         if run(*git, "ls-tree", "--name-only", ref, "--", path)), None)
    if version_file is None:
        return None
    source = run(*git, "show", f"{ref}:{version_file}")
    names = re.findall(r'^\s*versionName\s*=\s*"(\d+\.\d+\.\d+)"\s*$', source, re.M)
    codes = re.findall(r"^\s*versionCode\s*=\s*(\d+)\s*$", source, re.M)
    if len(names) != 1 or len(codes) != 1:
        raise ValueError("Use one literal versionName (major.minor.patch) and versionCode.")
    if any(len(part) > 1 and part.startswith("0") for part in names[0].split(".")):
        raise ValueError("Version components must not have leading zeros.")
    code = int(codes[0])
    if not 1 <= code <= 2_100_000_000:
        raise ValueError("versionCode must be between 1 and 2100000000.")
    return names[0], code


def plan(before, head):
    current = version_at(head)
    if current is None:
        raise ValueError("Current commit has no Android version.")
    previous = version_at(before) if before and set(before) != {"0"} else None
    if current == previous:
        return {"release": "false", "version": current[0], "tag": f"v{current[0]}"}
    if previous is not None:
        old_name, old_code = previous
        name, code = current
        if tuple(map(int, name.split("."))) <= tuple(map(int, old_name.split("."))) or code <= old_code:
            raise ValueError("A release must increase BOTH versionName and versionCode.")
    return {"release": "true", "version": current[0], "tag": f"v{current[0]}"}


def publish(repository, sha, directory):
    version, _ = version_at(sha)
    assets = [directory / f"ActualExpenses-{version}.apk", directory / "SHA256SUMS"]
    publish_release(repository, sha, f"v{version}", f"ActualExpenses {version}", assets)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    detect = commands.add_parser("plan")
    detect.add_argument("--before", default="")
    detect.add_argument("--head", default="HEAD")
    release = commands.add_parser("publish")
    release.add_argument("--repository", required=True)
    release.add_argument("--sha", required=True)
    release.add_argument("--directory", type=Path, default=Path("dist"))
    args = parser.parse_args()
    if args.command == "plan":
        result = plan(args.before, args.head)
        print(json.dumps(result))
        if os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a") as output:
                output.writelines(f"{key}={value}\n" for key, value in result.items())
    else:
        publish(args.repository, args.sha, args.directory)


if __name__ == "__main__":
    main()
