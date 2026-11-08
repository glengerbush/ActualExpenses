# Release automation qualification — ticket 48

Date: 2026-11-08. Baseline: `4600a70e9e6de62d3da6036ad1435dd3403a6bec`.
No application version was increased. No GitHub release, GHCR image or production
deployment was created. actualBudget's production-ready gate remains **false**.

## Public release CLI

Sixteen new tests exercise real temporary Git histories and the public release
commands. Explicit external CLI transports stand in for GitHub, Docker and Cosign;
no internal release functions are mocked. Cases cover independent app bumps,
whole-push detection, namespace introduction without a release, version consistency,
invalid/missing/unrelated bases, selected unchanged candidates, the production gate,
checksum tampering, original APK signer enforcement, signing before image promotion,
interrupted draft recovery, existing-version conflicts and latest anti-rollback.

The original twelve Android release tests remain unchanged and pass. Their legacy
CLI still recognizes the pre-relocation file layout and old `v*` tags. The production
workflow uses the root CLI and new per-application namespaces.

A separate temporary Git rehearsal copied the actual Android/Cargo/npm manifests,
then introduced only `VERSION` and the release policy. Both applications correctly
reported unchanged/no publication: Android 0.1.2/code 3 and web/server 0.1.0.

```sh
python3 -m unittest discover -s tools -p test_release.py -v
python3 -m unittest discover -s actualExpenses/tools -p test_release.py -v
```

Workflow syntax and expressions passed actionlint 1.7.12. Python compilation and
`git diff --check` passed. New GitHub Actions references use full commit hashes;
release-tag resolution was checked against each action's primary GitHub repository.

## Real APK verification

The existing signed 0.1.2 APK passed the new artifact verifier using Android SDK
35.0.0 `apksigner` and `aapt`: package `org.actualexpenses.android`, versionCode 3,
versionName 0.1.2, original certificate
`bb420a6b8dfdbcfd849a29f7c831d6b72b824fd411648dd8d92e9c6f85d8064c`.

An altered APK was rejected even after regenerating its checksum manifest, proving
the cryptographic signature check adds protection beyond matching checksums. A
debug-signed APK was also rejected because its certificate was different. Only
temporary artifact copies were modified; signing material was not accessed or changed.

## Real registry and container rehearsal

A separate source archive of the accepted baseline above supplied the production
Dockerfile/web/server. This avoids qualifying another worker's unfinished changes.
Docker built the image with version, source and commit labels, then pushed it only
to a fresh loopback registry running official Distribution 3.1.2. The registry used
temporary memory storage and an automatically assigned loopback port.

Cosign 3.1.4 generated a temporary key, signed the image digest, and verified its
signature plus commit/version annotations. Verification with the wrong commit
failed. Promoting a candidate tag using `imagetools create --prefer-index=false`
preserved the exact digest. The image was pulled from that registry and passed the
public HTTP, no-login, bundled-assets, exact-balance, container-replacement and
idempotent-retry smoke test with a new named volume.

- Digest: `sha256:312b799104c1b5fb9bfb8660b0ee88813529cdabacfd5db771f6503787d2531f`.
- Docker-reported size: 120,329,637 bytes; runtime user `10001:10001`.
- Registry image index: `sha256:38907510915c3f1604c7e83d6ab602ab3020d092f73bdaed8696773693ff768c`.
- Cosign binary SHA-256: `424e2c97d0b45fd7a6b608a87f090f35633f4c23ece5f092d48bba11aaf4bbfa`.
- actionlint archive SHA-256: `8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8`.

The registry, application containers, test image tags, data volumes and temporary
signing keys were removed. Explicit Docker name/reference checks found no remaining
test resources. Downloaded base images/build caches remain reusable.

This rehearsal used a self-managed temporary key with transparency-log upload
disabled. It proves image digest signing/verification and registry promotion, **not**
GitHub's OIDC identity issuance or a remote Actions/GHCR execution. The production
workflow uses keyless OIDC with the exact workflow identity/issuer pinned and no
verification bypass flags. Those remote steps run only upon an explicitly selected
candidate publication or an accepted stable version bump. Dry-run qualification is
the ticket's approved acceptance path; full migration/cutover remains separate.

The previously accepted [container/browser evidence](../actualBudget/tools/container-smoke.md)
also applies to the source baseline. Raw local rehearsal logs are ignored scratch
artifacts; the commands, outcomes and immutable identifiers above are the durable record.

## Acceptance and independent review

Implementation commit: `608ac8b`; fixed review diff: `a771d84...608ac8b`.
Root independently reran all 16 new and 12 legacy release tests successfully.
Separate Astra/high reviews reported **Standards: 0 findings; Spec: 0 findings**.
The approved dry-run qualification path meets ticket48's acceptance criteria.
Ticket48 is complete; this does not establish complete rewrite or production acceptance.
