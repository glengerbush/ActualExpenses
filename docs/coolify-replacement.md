# Manual Coolify replacement — rehearsal pending

This operator checklist preserves the existing pull/restart workflow. It is not
an authorization to switch production. Tickets51/52 and the copied-storage
rehearsal in53 must pass first. No live configuration was inspected.

## Existing setup to verify

Historical settings recovered from the deployment conversation:

| Setting | Recorded value |
| --- | --- |
| Coolify resource | Docker Image |
| Current image | `actualbudget/actual-server:26.10.0` |
| Published/container port | `5006:5006` |
| Persistent storage | `actualbudget-data` mounted at `/data` |
| Domains | Empty |
| Access | Plain HTTP over trusted LAN/tailnet |
| Updates | Operator selects Pull latest and restart |

Use `http://YOUR_BUDGET_HOST:5006` wherever a budget address is needed. This is a
placeholder, not a host to discover or probe. The Coolify dashboard's address is
separate; use the bookmark you normally use to manage this resource.
`patina-core` is unrelated.

In your external browser, open that existing Coolify resource and record its
actual image, resolved digest, published port, volume source/mount, container
user, environment, restart policy and registry access. Export or screenshot its
configuration. Resolve any difference from the table before proceeding. Do not
create a new empty volume accidentally because a display name differs from the
underlying Docker volume name.

## Before switching

1. Choose an accepted actualBudget release. Verify its release checksums,
   `image.json` digest and provenance using [release verification](releases.md).
   Initially select the immutable digest, not an unverified moving alias.
   Verify registry pull access in Coolify. Publishing an image does not deploy it.
2. Pause budgeting writes on every browser and phone. While the old server still
   runs, sync each old client and confirm zero pending changes. Export each
   budget using the old web app and retain the ZIPs separately. Record budget
   names, account balances, row counts and the old image digest.
3. If a phone has undelivered changes, keep its app data/APK intact. Either drain
   those changes before the final exports or explicitly account for that phone
   in the tested migration/recovery procedure. Do not declare the pause complete
   while an unaccounted device can publish more old-server writes.
4. Stop the old server. Make an offline, complete backup of the resolved `/data`
   storage, including metadata/ownership, and preserve the old resource settings.
   Verify that the backup can be restored into a separate disposable volume.
   Copying only a live SQLite main file omits possible WAL data.
5. Keep the original stopped-volume backup and old image together. Rehearse the
   new image against a disposable copy, including UID/GID10001 write access,
   import/activation, browser/Android checks, restart and rollback. Record the
   commands and results here before promoting this checklist to ready.

Switching the image alone does **not** convert Actual's server files. The
replacement uses its own `/data/actualbudget.sqlite3` and staged import workflow.
Import the final exported budgets through the replacement's Import Actual flow;
review every conversion report and activate each accepted budget. Preserve the
original archive. Unknown/encrypted input remains a refusal to resolve, not
permission to discard data.

## First replacement and subsequent updates

Keep the verified port, mount and volume name. Configure the accepted replacement
image digest and restart manually. Confirm `/api/v1/info`, the web entry point,
and persistent storage are healthy. Complete the staged imports before inviting
clients back. Compare recorded balances, counts, relationships, reports and
preferences; verify the upgraded phone can select the imported budget and
reconcile any explicitly retained pending work exactly once. Restart once and
recheck the same budget to prove persistence.

Keep a dated cutover record of the source exports, volume backup, both image
digests, conversion reports and each device's reconciliation result. A failed
check keeps the write pause in place.

After successful migration and acceptance, use
`ghcr.io/glengerbush/actualbudget:latest` for the normal **Pull latest and restart**
workflow. Stable publishing moves that alias only after release checks; candidate
images do not. For every subsequent update, retain the previous image digest
and a matching stopped-data backup before allowing schema changes.

## Rollback without discarding newer work

Rollback restores a matching old image **and its own data backup**. Never run the
old image against a newer native database and assume it can downgrade it.

1. Pause all clients again. Preserve the replacement's complete stopped volume,
   native budget exports and pending client state. Keep this copy even if the
   replacement cannot currently start; it is the recovery source for newer work.
2. If no post-cutover writes occurred, restore the verified pre-cutover volume
   backup and old image/settings together. Confirm the recorded old balances
   before permitting writes.
3. If newer writes occurred, first inventory every addition, edit, deletion and
   metadata/budget change against the cutover snapshot. Native archives are not
   assumed importable by the old Actual app. Reconcile those differences through
   supported old-app operations, checking both row-level values and totals.
   A CSV of new purchases alone does not preserve deletions or budget changes.
   Keep the recovered old installation isolated until this comparison passes.
4. Do not clear or downgrade a migrated Android installation to make it connect
   to the old server. Preserve its pending work. Keep it paused while using the
   old web app, until the replacement is restored or its saved work has an
   explicitly verified recovery path.
5. Resume writes only after the reconciled state is verified. Retain both sides
   until any later forward migration is complete; do not merge two independently
   modified installations by replacing one database file.

## Rehearsal evidence still required

- Exact copied-fixture volume/mount/port and permission checks.
- Fresh build, signed APK/checksum verification and immutable-image identity.
- Old export → new import → restart with independent financial comparison.
- New post-switch addition/edit/deletion retained across rollback, with a verified
  reconciliation into the restored old installation.
- Matching old image/data restoration and a final operator handoff.

These are outstanding acceptance gates, not completed deployment claims.
