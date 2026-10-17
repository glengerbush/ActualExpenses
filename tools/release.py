#!/usr/bin/env python3
"""Version-bump detection and resumable GitHub publication. No third-party dependencies."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess

VERSION_FILE = "app/build.gradle.kts"


def run(*args):
    return subprocess.check_output(args, text=True).strip()


def version_at(ref):
    # A missing file is a first release; an unavailable commit is an error.
    run("git", "rev-parse", "--verify", f"{ref}^{{commit}}")
    if not run("git", "ls-tree", "--name-only", ref, "--", VERSION_FILE):
        return None
    source = run("git", "show", f"{ref}:{VERSION_FILE}")
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


def api(path):
    response = subprocess.run(["gh", "api", path], capture_output=True, text=True)
    if response.returncode:
        if "(HTTP 404)" in response.stderr:
            return None
        raise RuntimeError(f"GitHub request failed: {response.stderr.strip()}")
    return json.loads(response.stdout)


def publish(repository, sha, directory):
    version, _ = version_at(sha)
    tag = f"v{version}"
    assets = [directory / f"ActualExpenses-{version}.apk", directory / "SHA256SUMS"]
    if not all(path.is_file() for path in assets):
        raise ValueError("Signed APK and checksum must exist before publishing.")
    prefix = f"repos/{repository}"
    ref = api(f"{prefix}/git/ref/tags/{tag}")
    if ref is not None:
        obj = ref["object"]
        while obj["type"] == "tag":
            obj = api(f"{prefix}/git/tags/{obj['sha']}")["object"]
        if obj["type"] != "commit" or obj["sha"] != sha:
            raise ValueError(f"Tag {tag} already points elsewhere; refusing to replace it.")
    # REST's tag endpoint misses drafts whose tag has not been published yet.
    # Authenticated release listing includes drafts; paginate without a fixed limit.
    pages = json.loads(run("gh", "api", "--paginate", "--slurp", f"{prefix}/releases?per_page=100"))
    release = next((item for page in pages for item in page if item["tag_name"] == tag), None)
    if release is not None:
        if ref is None and release["target_commitish"] != sha:
            raise ValueError("Existing draft belongs to another commit.")
        if not release["draft"]:
            names = {asset["name"] for asset in release["assets"]}
            if not {asset.name for asset in assets} <= names:
                raise ValueError("Published release is missing assets; refusing to rewrite it.")
            print(f"{tag} is already published; leaving it unchanged.")
            return
    else:
        subprocess.run(["gh", "release", "create", tag, "--repo", repository,
                        "--target", sha, "--title", f"ActualExpenses {version}",
                        "--generate-notes", "--draft"], check=True)
    # Upload before publication. A failed run can safely replace draft assets.
    subprocess.run(["gh", "release", "upload", tag, *map(str, assets),
                    "--repo", repository, "--clobber"], check=True)
    subprocess.run(["gh", "release", "edit", tag, "--repo", repository, "--draft=false"], check=True)


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
