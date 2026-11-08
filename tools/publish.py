#!/usr/bin/env python3
"""Publication boundary: dry-run by default; --execute is required for remote writes."""
import argparse
import json
import os
from pathlib import Path

from artifacts import verify_apk
from github_release import publish_release
from image_release import publish_image
from release import APPS, android_module, plan


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app", choices=APPS)
    parser.add_argument("--before", required=True)
    parser.add_argument("--head", default="HEAD")
    parser.add_argument("--channel", choices=("stable", "candidate"), default="stable")
    parser.add_argument("--repository", default="glengerbush/ActualExpenses")
    parser.add_argument("--directory", type=Path, default=Path("dist"))
    parser.add_argument("--image")
    parser.add_argument("--apksigner", default=os.environ.get("APKSIGNER", "apksigner"))
    parser.add_argument("--aapt", default=os.environ.get("AAPT", "aapt"))
    parser.add_argument("--execute", action="store_true")
    args = parser.parse_args()
    release = plan(args.before, args.head, args.channel, args.repository, args.app)
    app = release["apps"][args.app]
    if not args.execute:
        print(json.dumps({"dry_run": True, "would_publish": app["publish"], **release}, indent=2))
        return
    if not app["publish"]:
        raise ValueError(app["blocked_reason"] or "No version bump; unchanged stable releases are not published")
    if args.app == "actualBudget":
        publish_image(release, args.image, args.directory)
    else:
        version, code = android_module().version_at(release["commit"])
        verify_apk(args.directory, version, code, args.apksigner, args.aapt)
        candidate = args.channel == "candidate"
        assets = [args.directory / f"ActualExpenses-{version}.apk", args.directory / "SHA256SUMS"]
        notes = "Candidate for isolated Android qualification.\n" if candidate else None
        publish_release(args.repository, release["commit"], app["tag"], f"actualExpenses {version}",
                        assets, prerelease=candidate, notes=notes, verify_published=True)


if __name__ == "__main__":
    main()
