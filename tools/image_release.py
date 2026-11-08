"""Sign a tested image digest before publishing immutable versions or moving aliases."""
import json
from pathlib import Path
import re
import subprocess

from artifacts import checksums
from github_release import publish_release
from release import run, semver


def remote_digest(reference):
    result = subprocess.run(["docker", "buildx", "imagetools", "inspect", reference,
                             "--format", "{{json .Manifest}}"], capture_output=True, text=True)
    if result.returncode:
        if re.search(r"not found|manifest unknown|no such manifest", result.stderr, re.I):
            return None
        raise RuntimeError(result.stderr.strip())
    digest = json.loads(result.stdout)["digest"]
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", digest):
        raise ValueError("Registry returned an invalid image digest")
    return digest


def verify_signature(reference, repository, *, commit=None, version=None):
    command = ["cosign", "verify", "--certificate-identity",
               f"https://github.com/{repository}/.github/workflows/release.yml@refs/heads/main",
               "--certificate-oidc-issuer", "https://token.actions.githubusercontent.com"]
    if commit is not None:
        command.extend(["-a", f"git_sha={commit}"])
    if version is not None:
        command.extend(["-a", f"version={version}"])
    signatures = json.loads(run(*command, reference))
    versions = [item["optional"]["version"] for item in signatures]
    return max(versions, key=semver)


def promote(reference, image, digest, *, immutable):
    existing = remote_digest(reference)
    if existing == digest:
        return
    if immutable and existing is not None:
        raise ValueError(f"Immutable image {reference} already exists with another digest")
    subprocess.run(["docker", "buildx", "imagetools", "create", "--prefer-index=false",
                    "--tag", reference, f"{image}@{digest}"], check=True)
    if remote_digest(reference) != digest:
        raise ValueError("Promoted image digest does not match the verified image")


def publish_image(release, local_image, directory):
    app = release["apps"]["actualBudget"]
    image, version, commit = app["image"], app["version"], release["commit"]
    reference = f"{image}:{app['image_tag']}"
    digest = remote_digest(reference)
    if digest is None:
        if not local_image:
            raise ValueError("--image must name the locally qualified image")
        labels = json.loads(run("docker", "image", "inspect", local_image, "--format", "{{json .Config.Labels}}"))
        expected = {"org.opencontainers.image.revision": commit,
                    "org.opencontainers.image.version": version,
                    "org.opencontainers.image.source": f"https://github.com/{release['repository']}"}
        if any(labels.get(key) != value for key, value in expected.items()):
            raise ValueError("Local image provenance does not match the release plan")
        # This staging tag is not a release alias. Only verified digests are promoted.
        staging = f"{image}:build-{commit}"
        subprocess.run(["docker", "tag", local_image, staging], check=True)
        subprocess.run(["docker", "push", staging], check=True)
        digest = remote_digest(staging)
        if digest is None:
            raise ValueError("Pushed staging image is missing")
        subprocess.run(["cosign", "sign", "--yes", "-a", f"git_sha={commit}",
                        "-a", f"version={version}", f"{image}@{digest}"], check=True)
    verify_signature(f"{image}@{digest}", release["repository"], commit=commit, version=version)
    promote(reference, image, digest, immutable=True)

    directory.mkdir(parents=True, exist_ok=True)
    artifact = directory / "image.json"
    artifact.write_text(json.dumps({"image": image, "version": version, "commit": commit,
                                    "tag": reference, "digest": digest,
                                    "immutable_reference": f"{image}@{digest}"}, indent=2, sort_keys=True) + "\n")
    checksums(directory, [artifact.name], write=True)
    checksums(directory, [artifact.name])
    candidate = release["channel"] == "candidate"
    notes = ("Candidate for isolated qualification; not approved for production.\n\n" if candidate else "")
    notes += f"Image: `{reference}`\n\nImmutable image: `{image}@{digest}`\n\nPublishing never updates Coolify automatically.\n"
    publish_release(release["repository"], commit, app["tag"], f"actualBudget {app['image_tag']}",
                    [artifact, directory / "SHA256SUMS"], prerelease=candidate, notes=notes, verify_published=True)
    if app["moving_alias"]:
        moving = f"{image}:{app['moving_alias']}"
        current = remote_digest(moving)
        if current is not None:
            current_version = verify_signature(f"{image}@{current}", release["repository"])
            if semver(current_version) > semver(version):
                print("Newer stable alias already exists; leaving it unchanged")
                return
        promote(moving, image, digest, immutable=False)
    print(f"Verified release image: {image}@{digest}")
