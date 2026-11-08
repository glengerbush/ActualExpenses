#!/usr/bin/env python3
"""Deterministic external CLI transport for release-boundary tests; never uses network."""
import base64
import json
import os
from pathlib import Path
import sys

path = Path(os.environ["RELEASE_TRANSPORT_STATE"])
state = json.loads(path.read_text())
tool = Path(sys.argv[0]).name
args = sys.argv[1:]
state.setdefault("events", []).append([tool, *args])


def save():
    path.write_text(json.dumps(state))


def fail(message):
    save()
    print(message, file=sys.stderr)
    sys.exit(1)


if tool == "docker":
    if args[:2] == ["image", "inspect"]:
        print(json.dumps({"org.opencontainers.image.revision": state["commit"],
                          "org.opencontainers.image.version": state["version"],
                          "org.opencontainers.image.source": "https://github.com/glengerbush/ActualExpenses"}))
    elif args[:3] == ["buildx", "imagetools", "inspect"]:
        ref = args[3]
        if ref not in state.get("images", {}):
            fail("manifest unknown: not found")
        print(json.dumps({"digest": state["images"][ref]}))
    elif args[0] == "push":
        state.setdefault("images", {})[args[1]] = "sha256:" + "a" * 64
    elif args[:3] == ["buildx", "imagetools", "create"]:
        target = args[args.index("--tag") + 1]
        state.setdefault("images", {})[target] = args[-1].split("@", 1)[1]
    elif args[0] != "tag":
        fail("Unexpected Docker command")
elif tool == "cosign":
    if args[0] == "sign":
        claims = dict(args[i + 1].split("=", 1) for i, value in enumerate(args) if value == "-a")
        state.setdefault("signed", {})[args[-1]] = claims
    elif args[0] == "verify":
        if state.get("reject_signature"):
            fail("signature invalid")
        expected_identity = "https://github.com/glengerbush/ActualExpenses/.github/workflows/release.yml@refs/heads/main"
        if args[args.index("--certificate-identity") + 1] != expected_identity:
            fail("Wrong signer identity")
        if args[args.index("--certificate-oidc-issuer") + 1] != "https://token.actions.githubusercontent.com":
            fail("Wrong issuer")
        claims = state.get("signed", {}).get(args[-1])
        if claims is None:
            fail("image unsigned")
        for i, value in enumerate(args):
            if value == "-a":
                key, expected = args[i + 1].split("=", 1)
                if claims.get(key) != expected:
                    fail("Signature annotation mismatch")
        print(json.dumps([{"optional": claims}]))
    else:
        fail("Unexpected Cosign command")
elif tool == "apksigner":
    digest = state.get("apk_certificate", "bb420a6b8dfdbcfd849a29f7c831d6b72b824fd411648dd8d92e9c6f85d8064c")
    print("Signer #1 certificate SHA-256 digest: " + digest)
elif tool == "aapt":
    print(f"package: name='org.actualexpenses.android' versionCode='{state['code']}' versionName='{state['version']}'")
elif tool == "gh":
    if args[0] == "api":
        if "--paginate" in args:
            print(json.dumps([[state["release"]] if state.get("release") else []]))
        elif "/git/ref/" in args[1]:
            if not state.get("tag"):
                fail("gh: Not Found (HTTP 404)")
            print(json.dumps({"object": {"type": "commit", "sha": state["tag"]}}))
        else:
            fail("Unexpected GitHub API command")
    elif args[1] == "create":
        state["release"] = {"draft": True, "tag_name": args[2], "target_commitish": args[args.index("--target") + 1],
                            "prerelease": "--prerelease" in args, "assets": []}
    elif args[1] == "upload":
        if state.pop("interrupt_upload", False):
            fail("Upload interrupted")
        state["assets"] = {Path(file).name: base64.b64encode(Path(file).read_bytes()).decode()
                           for file in args[3:args.index("--repo")]}
        state["release"]["assets"] = [{"name": name} for name in state["assets"]]
    elif args[1] == "edit":
        state["release"]["draft"] = False
        state["tag"] = state["release"]["target_commitish"]
    elif args[1] == "download":
        name = args[args.index("--pattern") + 1]
        (Path(args[args.index("--dir") + 1]) / name).write_bytes(base64.b64decode(state["assets"][name]))
    else:
        fail("Unexpected GitHub command")
else:
    fail("Unknown external command")
save()
