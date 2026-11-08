#!/usr/bin/env python3
"""Independent release plans. Planning never contacts GitHub or a registry."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import tomllib

PROJECT = Path(__file__).resolve().parents[1]
APPS = ("actualExpenses", "actualBudget")


def run(*args):
    return subprocess.check_output(args, text=True).strip()


def git(*args):
    return run("git", "-C", run("git", "rev-parse", "--show-toplevel"), *args)


def source(ref, path, required=True):
    files = git("ls-tree", "--name-only", ref, "--", path)
    if not files:
        if required:
            raise ValueError(f"Missing {path} at {ref}")
        return None
    return git("show", f"{ref}:{path}")


def semver(value):
    if not isinstance(value, str) or not re.fullmatch(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", value):
        raise ValueError("Version must be major.minor.patch without leading zeros")
    return tuple(map(int, value.split(".")))


def android_module():
    spec = importlib.util.spec_from_file_location("legacy_android_release", PROJECT / "actualExpenses/tools/release.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def budget_version(ref, validate=True):
    value = source(ref, "actualBudget/VERSION", required=False)
    cargo = source(ref, "actualBudget/server/Cargo.toml", required=False)
    if value is None and not validate:
        # Adding the release manifest to an existing app is not a version bump.
        return None if cargo is None else tomllib.loads(cargo)["package"]["version"]
    if value is None or cargo is None:
        raise ValueError("actualBudget requires VERSION and server/Cargo.toml")
    semver(value)
    cargo_version = tomllib.loads(cargo)["package"]["version"]
    lock = tomllib.loads(source(ref, "actualBudget/server/Cargo.lock"))
    server_versions = [p["version"] for p in lock["package"] if p["name"] == "actualbudget-server"]
    package = json.loads(source(ref, "actualBudget/web/package.json"))
    web_lock = json.loads(source(ref, "actualBudget/web/package-lock.json"))
    if server_versions != [value] or any(v != value for v in (
        cargo_version, package["version"], web_lock["version"], web_lock["packages"][""]["version"],
    )):
        raise ValueError("actualBudget VERSION, Cargo manifests and web manifests must share one version")
    return value


def plan(before, head, channel="stable", repository="glengerbush/ActualExpenses", selected_app=None):
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ValueError("Invalid GitHub repository")
    commit = git("rev-parse", "--verify", f"{head}^{{commit}}")
    if not before or set(before) == {"0"}:
        raise ValueError("An explicit existing --before commit is required; use HEAD for an unchanged dry run")
    base = git("rev-parse", "--verify", f"{before}^{{commit}}")
    git("merge-base", "--is-ancestor", base, commit)
    policy = json.loads(source(commit, "release-policy.json"))
    if set(policy) != set(APPS) or any(type(policy[app]) is not bool for app in APPS):
        raise ValueError("release-policy.json requires a production-ready boolean for each application")
    android = android_module()
    versions = {
        "actualExpenses": (android.version_at(base), android.version_at(commit)),
        "actualBudget": (budget_version(base, False), budget_version(commit)),
    }
    apps = {}
    for app, (old, new) in versions.items():
        if new is None:
            raise ValueError(f"Missing current {app} version")
        changed = old != new
        if app == "actualExpenses":
            version = new[0]
            if changed and old and (semver(new[0]) <= semver(old[0]) or new[1] <= old[1]):
                raise ValueError("Android release must increase BOTH versionName and versionCode")
        else:
            version = new
            if changed and old and semver(new) <= semver(old):
                raise ValueError("actualBudget version must increase")
        semver(version)
        suffix = f"-candidate.{commit[:12]}" if channel == "candidate" else ""
        allowed = channel == "candidate" or policy[app]
        selected = selected_app in (None, "both", app)
        apps[app] = {
            "version": version, "changed": changed,
            "qualify": selected and (changed or selected_app is not None),
            "publish": selected and allowed and (changed or (channel == "candidate" and selected_app is not None)),
            "tag": f"{app}-v{version}{suffix}",
            "blocked_reason": None if allowed else "Replacement has not passed final acceptance",
        }
        if app == "actualBudget":
            apps[app].update(image=f"ghcr.io/{repository.split('/')[0].lower()}/actualbudget",
                             image_tag=version + suffix, moving_alias="latest" if channel == "stable" else None)
    return {"commit": commit, "before": base, "repository": repository, "channel": channel, "apps": apps}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    detect = commands.add_parser("plan")
    detect.add_argument("--before", required=True)
    detect.add_argument("--head", default="HEAD")
    detect.add_argument("--channel", choices=("stable", "candidate"), default="stable")
    detect.add_argument("--repository", default="glengerbush/ActualExpenses")
    detect.add_argument("--app", choices=(*APPS, "both"))
    args = parser.parse_args()
    result = plan(args.before, args.head, args.channel, args.repository, args.app)
    print(json.dumps(result, indent=2))
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            for app, prefix in (("actualExpenses", "android"), ("actualBudget", "budget")):
                for key, value in result["apps"][app].items():
                    if isinstance(value, bool):
                        value = str(value).lower()
                    if value is not None:
                        output.write(f"{prefix}_{key}={value}\n")


if __name__ == "__main__":
    main()
