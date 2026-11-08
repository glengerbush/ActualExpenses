# Independent application releases

`actualExpenses` is the Android companion. `actualBudget` is the combined web/server
image. A version change releases only the affected application after qualification.
Publishing never calls Coolify, changes a deployment, or opens a production volume.

| Application | Version source | New GitHub release tags | Published artifact |
| --- | --- | --- | --- |
| actualExpenses | Android `versionName` and `versionCode` | `actualExpenses-vX.Y.Z` | Signed `ActualExpenses-X.Y.Z.apk` and `SHA256SUMS` |
| actualBudget | `actualBudget/VERSION` | `actualBudget-vX.Y.Z` | `ghcr.io/glengerbush/actualbudget:X.Y.Z`, immutable digest and `image.json`/`SHA256SUMS` |

Historical Android `v*` tags, downloads, application ID and signing identity remain
unchanged. New releases use the namespaces above. The old Android release CLI is
retained for compatibility tests; new publication uses the root `tools/` commands.

## Versions and acceptance

Versions use numeric `major.minor.patch`, without leading zeros. Android releases
must increase both `versionName` and `versionCode`. For actualBudget, update
`VERSION`, the server package version in `Cargo.toml`/`Cargo.lock`, and the frontend
version in `package.json`/`package-lock.json` together. The planner rejects a mismatch.
There is no version bump in the release-automation change itself.

`release-policy.json` records whether each application has passed production
acceptance. **actualBudget remains false until final rewrite acceptance.** A version
bump while false can qualify the image but cannot publish a stable version or move
`latest`. Changing this gate alone does not create a release. The coordinated
Android adapter must also be gated if it requires an unavailable replacement server.

The planner compares the complete push, not only its last commit. It rejects
missing/unknown/non-ancestor comparison commits, backward versions and partial
Android bumps. Introducing `VERSION` for an existing actualBudget version does not
count as a bump. Workflow-only changes do not republish current versions under new tags.

## Dry runs and candidates

Local planning reads Git history and makes no registry or GitHub requests:

```sh
python3 tools/release.py plan --before HEAD~1 --head HEAD
python3 tools/publish.py actualBudget --before HEAD --channel candidate
```

The second command is a dry run. Every publication command requires `--execute`
for remote writes; even then, the production gate and version checks still apply.
The explicit application argument selects an unchanged candidate. The general
planner defaults to changed applications only; use `--app actualBudget` to select
an unchanged actualBudget candidate without releasing Android.

GitHub Actions → **Qualify and release applications** → **Run workflow** defaults
to actualBudget, candidate channel and **dry_run=true**. It builds and verifies the
selected application, including signing the APK with the existing identity when
Android is selected, but creates no GitHub release and writes no registry image.
Android qualification artifacts are retained as workflow artifacts.

An intentionally published candidate uses GitHub tag
`actualBudget-vX.Y.Z-candidate.<12-character-commit>` (or the Android namespace),
is a GitHub prerelease, and uses image tag `X.Y.Z-candidate.<commit>`. Candidates
never move `latest`. No candidate or stable publication was run to implement this
workflow. Stable manual publication additionally requires an explicit comparison
commit containing the prior version. Automatic stable publication runs only after
a qualifying version bump reaches `main` and its application gate permits it.

## Checks and signing

Android qualification runs the existing pinned Actual 26.10.0 integration harness,
all non-skipped core tests, Android lint, and release APK build. It verifies the
checksum, APK signature, package ID and both version fields. The original certificate
SHA-256 is pinned as `bb420a6b8dfdbcfd849a29f7c831d6b72b824fd411648dd8d92e9c6f85d8064c`.
Existing `ACTUALEXPENSES_*` signing secrets are reused; no replacement key is generated.

actualBudget qualification checks Rust formatting/Clippy/tests, frontend checks and
build, pinned opening-balance/register/calculator oracles and the served API contract. It builds one
image, records source/version/revision labels, and runs persistent-volume replacement
and browser acceptance against that exact image. Publication never rebuilds it.

The image is first pushed under an internal `build-<full-commit>` staging tag. Cosign
signs its digest using GitHub Actions OIDC; no additional long-lived signing secret
is required. Verification pins the exact workflow identity and issuer, plus signed
commit/version annotations. Only then is the immutable version tag created. A
conflicting existing version is rejected. Interrupted GitHub uploads resume as
drafts; published artifacts are downloaded and compared before accepting a retry.

After a stable GitHub release is complete, `latest` moves to the same verified
digest. An older release retry cannot move it backwards. Jobs are serialized per
application. Only `latest` and internal build staging tags may move; version tags
are never overwritten by this workflow. Digest references remain content-addressed.

To verify a published image, replace the digest placeholder with the value in its
release's `image.json`:

```sh
cosign verify \
  --certificate-identity 'https://github.com/glengerbush/ActualExpenses/.github/workflows/release.yml@refs/heads/main' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com' \
  'ghcr.io/glengerbush/actualbudget@sha256:RELEASE_DIGEST'
```

Verify downloaded release files with `sha256sum --check SHA256SUMS`. APK signatures
can also be checked using Android SDK `apksigner verify --verbose --print-certs`.

## Manual Coolify updates

After migration and final acceptance, configure the image resource's moving tag as
`ghcr.io/glengerbush/actualbudget:latest`. Retain the recorded `5006` port and `/data`
mount only after verifying the live configuration and rehearsing the copied-volume
migration. Fresh test volumes use UID/GID `10001:10001`; existing data must be
checked for compatible ownership and a writable `/data` during that rehearsal.

GHCR package access must be configured once: public read access, or a read-only
registry credential in Coolify. **Pull latest and restart** then adopts the newest
accepted stable image. Candidate tags do not affect that action. Use an immutable
version/digest and its matching data backup for rollback; schema rollback and
post-cutover writes require the separate migration runbook.

## Reproduce automation checks

```sh
python3 -m unittest discover -s tools -p test_release.py -v
python3 -m unittest discover -s actualExpenses/tools -p test_release.py -v
python3 actualBudget/tools/container-smoke.py --browser
```

Release tests use real temporary Git histories and an explicit fake external
Docker/GitHub/Cosign transport. They test the public CLI without contacting services
or mocking internal functions. Real APK/registry rehearsal evidence and its limits
are recorded in [release validation](release-validation.md).

Primary references: [Sigstore CI signing](https://docs.sigstore.dev/quickstart/quickstart-ci/),
[Cosign digest signing](https://github.com/sigstore/cosign/blob/main/doc/cosign_sign.md),
[Docker registry digest inspection](https://docs.docker.com/reference/cli/docker/buildx/imagetools/inspect/).
