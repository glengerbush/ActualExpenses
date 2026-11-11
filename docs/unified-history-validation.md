# Unified transaction history — 2026-11-11

Scope: [accepted decision](adr/0006-unified-transaction-history.md). Review baseline: `c2c3d35d1940a1de7199caae7e7c2029dc28e166`. This is a deliberate product change after the prior rewrite acceptance, not a renewed claim of exact upstream UI parity.

## Checked behavior

- New budget → transaction entry requires no account setup. All historical entries appear in one register; account columns/pickers and new transfer actions are removed from normal flows. Existing imported identities and financial classifications remain intact.
- Budget, categories, reports, settings and transaction history switch directly through the sidebar and commands. A partially entered expense survives navigation away and back.
- Register filters/saved views, exact amounts, localized entry, imports/CSV export, splits, schedules, reports, recovery, offline delivery and existing paired-transfer protections still work.
- Android selects a budget directly, without login/account selection. Its recent ten expenses span the budget. It edits/deletes the original historical entry, including a closed former destination, preserves interrupted edits, and retains a newly backdated expense after delivery and restart.
- Old browser caches acquire a durable automatic destination even when the server checkpoint is unchanged. Reopening while disconnected still enables expense entry. Epoch changes during preparation stop the operation rather than assigning a stale destination.

## Evidence

Compressed logs and SHA-256 hashes: [manifest](unified-history-results/manifest.json).

- Svelte check: zero errors/warnings; production and Docker builds succeeded. Autofixer reported no issues. Its two suggestions concern existing local `Set` use and dialog `bind:this`; these were reviewed and left intact.
- 27 affected browser groups passed across initial/corrected runs. Includes the new unified-history workflow and existing register, filters, navigation, preferences, dates, epochs, metadata recovery, schedules, imports, crossover, manual-only behavior, offline editing/cache/outbox, lifecycle/recovery, splits, reports, existing paired protection and bulk history. These are focused checks, not a rerun of all prior 107 parity stories.
- Public API/SDK mapping check passed. Rust companion tests cover existing scoped behavior plus budget-wide history, retained rows, repeat preparation, restart and stale-epoch rejection.
- All 54 native Android API cases passed across the full/corrected runs, including the separately supplied historical-server upgrade executable. Android lint passed. The first closure fixture correctly failed because its balance was nonzero; adding the fixture's balancing income made it a valid closed historical destination. No financial guard was weakened to make the fixture pass.
- Emulator API36: all 13 expense UI cases passed across initial/corrected runs, with an additional dark/1.3× font pass. One old recovery-heading assertion was corrected to the already-shipped text. Screenshots were inspected in light and dark modes. No real phone was used.

## Review and local preview

Review caught and corrected three integration gaps: original account context for Android edits/deletes, the upcoming-schedules panel formerly attached to an account view, and persistence of the automatic destination when upgrading an unchanged browser cache. Archived account-centric UI suites were removed from the active command list; corresponding compatibility API behavior remains tested.

Preview: `http://localhost:5006`, container `actualbudget-preview`, loopback port binding, existing volume `actualbudget-preview-c2c3d35-data:/data`. A stopped-container copy is at `.scratch/unified-preview-backup/`. Final runtime image: `sha256:54fa94dee5108b26e2fd64945a36663b4a59666947b9d65b1f15d2f07aba09a5`. HTTP readiness returned 200 and `/api/v1/info` reported `authentication: none` and `unified_ledger`.

Versions are unchanged. No push, release, real-device installation or remote deployment.

## Deployment follow-up — 2026-11-11

The preceding paragraph records the original local acceptance. The user subsequently
requested deployment. [Android0.2.0](https://github.com/glengerbush/ActualExpenses/releases/tag/actualExpenses-v0.2.0)
is published; its downloaded APK checksum, original certificate, package ID and
versionCode4 were verified. Server0.2.3 reached a successful hosted image build,
but [its container/browser job failed](https://github.com/glengerbush/ActualExpenses/actions/runs/43066753732/job/130683916198).
No server image has been published and Coolify remains unchanged. Detailed hosted
logs require GitHub sign-in; the latest failure log has been requested from the operator.

Earlier release blockers were corrected: optional Ubuntu font installation failed
on the runner's mirror DNS; missed formatting failed Prettier; identical source
transactions had nondeterministic UUID tie order. Chromium now installs without
the redundant apt step and must pass a real launch check. The reference comparator
keeps its financial fields and cross-snapshot identity mapping, using normalized
identity only as the final tie-break. The original failure reproduced on run5;
the correction passed20 consecutive genuine-source runs without fixture changes.

The full pre-container stage passed locally:459 Rust tests, formatting, Svelte,
production build, financial references, migration and served contracts. Those
checks also passed in the latest hosted run. A complete local container/browser
batch passed100/104 groups. Corrected reruns passed all four remaining groups:

- Concurrency selects the intended historical transaction in the combined history.
- Large split editing selects the imported1001-allocation transaction directly.
- Register settings save to the shared `all` view; its columns survive reload.
  Existing imported account filters now display “Historical account” instead of a
  blank field. That option appears only while editing an existing legacy condition;
  changing it to a normal field removes the historical option. Desktop/mobile
  screenshots were inspected and the no-overflow check passed.
- External API edits retain exact pinned source rows and per-account totals; the
  displayed total now correctly includes both sides of the historical transfer.

The clean0.2.3 image also passed container persistence/recreation and the initial
unified-history workflow locally with the same pinned Chromium1254 used in CI.
The later historical-filter label was checked through the rebuilt production web
bundle and public server API. All104 browser/API groups pass across the full and
corrected runs; this is not a claim that the still-unexplained hosted failure passed.
Final Svelte/autofixer and formatting checks passed. [Evidence manifest](deployment-2026-11-11-results/manifest.json)
records logs and the exact hashes of the five follow-up files. Live migration still
requires the [manual cutover procedure](coolify-replacement.md).
