"""Resumable GitHub release publication with immutable published artifacts."""
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


def run(*args):
    return subprocess.check_output(args, text=True).strip()


def api(path):
    response = subprocess.run(["gh", "api", path], capture_output=True, text=True)
    if response.returncode:
        if "(HTTP 404)" in response.stderr:
            return None
        raise RuntimeError(f"GitHub request failed: {response.stderr.strip()}")
    return json.loads(response.stdout)


def publish_release(repository, sha, tag, title, assets, *, prerelease=False, notes=None, verify_published=False):
    if not all(path.is_file() for path in assets):
        raise ValueError("All release artifacts must exist before publishing")
    prefix = f"repos/{repository}"
    ref = api(f"{prefix}/git/ref/tags/{tag}")
    if ref is not None:
        obj = ref["object"]
        while obj["type"] == "tag":
            obj = api(f"{prefix}/git/tags/{obj['sha']}")["object"]
        if obj["type"] != "commit" or obj["sha"] != sha:
            raise ValueError(f"Tag {tag} already points elsewhere; refusing to replace it")
    pages = json.loads(run("gh", "api", "--paginate", "--slurp", f"{prefix}/releases?per_page=100"))
    release = next((item for page in pages for item in page if item["tag_name"] == tag), None)
    if release is not None:
        if ref is None and release["target_commitish"] != sha:
            raise ValueError("Existing draft belongs to another commit")
        if not release["draft"]:
            if not {asset.name for asset in assets} <= {asset["name"] for asset in release["assets"]}:
                raise ValueError("Published release is missing assets; refusing to rewrite it")
            if verify_published:
                with tempfile.TemporaryDirectory(prefix="release-download-") as directory:
                    for asset in assets:
                        subprocess.run(["gh", "release", "download", tag, "--repo", repository,
                                        "--pattern", asset.name, "--dir", directory], check=True)
                        downloaded = Path(directory) / asset.name
                        if hashlib.sha256(downloaded.read_bytes()).digest() != hashlib.sha256(asset.read_bytes()).digest():
                            raise ValueError(f"Published {asset.name} differs; refusing to replace it")
            print(f"{tag} is already published; leaving it unchanged")
            return
    else:
        with tempfile.TemporaryDirectory(prefix="release-notes-") as directory:
            command = ["gh", "release", "create", tag, "--repo", repository,
                       "--target", sha, "--title", title, "--draft"]
            if notes is None:
                command.append("--generate-notes")
            else:
                path = Path(directory) / "notes.md"
                path.write_text(notes)
                command.extend(["--notes-file", str(path)])
            if prerelease:
                command.extend(["--prerelease", "--latest=false"])
            subprocess.run(command, check=True)
    subprocess.run(["gh", "release", "upload", tag, *map(str, assets),
                    "--repo", repository, "--clobber"], check=True)
    subprocess.run(["gh", "release", "edit", tag, "--repo", repository, "--draft=false"], check=True)
