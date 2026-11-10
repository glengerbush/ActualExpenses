# Navigation45 — stories78/79

Base: `4d7498c`. Scope: delivered web navigation, shared privacy preference, browser view memory and optional fork-local announcements. Implement/TDD, Impeccable and Svelte writer/core workflows used; existing Operate design retained. Root performs the consolidated Standards/Spec review; no subdelegation.

## Acceptance and public contract

- Preserve existing budget/account selection, account visibility, register queries/columns and draft recovery. Remember supported views per budget; draft recovery takes priority. Navigation cannot discard active forms.
- Register real delivered destinations only. Search by label/account, Ctrl/Cmd+K, arrows/Enter, Escape, `?` outside editing. Native dialog traps focus and restores the trigger; destination transitions use existing content focus.
- Mask displayed financial amounts at every viewport. Desktop decimal amount inputs use the pinned source's redaction-at-rest/reveal-on-hover-or-focus affordance. Values, parsers and stored arithmetic remain unchanged.
- Optional local release notice, dismissal and opt-out persist through the existing device preference record. Off by default; help remains available offline. No remote updater, update checks, telemetry or misleading upstream release announcements.

API: additive optional `privacy_mode:boolean` on existing `/api/v1/budgets/{budget}/preferences`; null resets false, omission means false. Existing revision/epoch/idempotent-write semantics retained. Historical preference JSON and receipts keep their original shape. Full `/api/v1/openapi.json` and preference fragment describe the addition; legacy transfer preference remains documented.

Conversion: `/api/v1/preferences/convert-actual` maps synced `isPrivacyEnabled` (boolean or true/false string) to shared privacy. Global `showNewsFeed` and `notifyWhenUpdateIsAvailable` map to optional device `show_release_notices`; either opt-out suppresses notices. Upstream seen dates/version keys remain unmapped for review, never mistaken for fork dismissals. Full budget import already delegates synced preference conversion here.

Device record stays v1: optional `show_release_notices`/`seen_release`, old records valid. Per-budget navigation v1 accepts only delivered remembered view IDs. Invalid/unavailable browser storage does not prevent navigation; device preference save errors are visible. Privacy write failures retain the confirmed setting and show retry guidance; disconnected privacy changes are disabled with an explanation.

## Integration and reference

`web/src/lib/navigation.ts`: `NavigationCommand` plus remembered-view helpers. App registers delivered callbacks/availability; future slices append real commands when delivered. NavigationTools owns search/help/privacy/local notices, not content routing. No new native tables or schema migration.

Pinned Actual26.10: `desktop-client/src/hooks/usePrivacyMode.ts`, `components/util/FinancialInput.tsx` (lines15–19,106), `loot-core/src/types/prefs.ts`. Font copied unchanged from the installed pinned `@actual-app/web` build (`redacted-script-latin-400-normal.DO_NTpDN.woff2`); SHA256 `b78e6e7daa990b9cb009e80e75ec9f7979e1985649d6d59226d75477358d9f04`, bundled SIL OFL in `web/public/licenses/redacted-script-OFL.txt` (Google Fonts primary license).

Navigation exposed a cache race: a newly mounted register could replace confirmed privacy/formatting with an older saved same-epoch revision. Public browser regression holds HTTP refresh after normal navigation and verifies amounts stay masked. Same-budget preference refresh also retains confirmed preferences until the server responds, so saving another setting cannot temporarily expose amounts from an older snapshot. Different epochs retain the existing restore behavior.

## Checks and evidence

- Running public API: `cargo test --test preferences --test budget_automation_upgrade`: 6 preference tests and 17 historical/native migration tests pass. Covers source conversion, published schema, validation/reset, restart persistence, receipt/legacy preference preservation and currency lineage.
- Earlier broad checkpoint additionally passed 10 account and 4 Android-upgrade public API tests; stopped on optional-field shape regression, fixed without changing historical fixtures. No native UI changes or device claims.
- Browser: navigation45 exercises privacy failure/retry/persistence, input redaction/focus/hover with exact unchanged value, stale-cache masking, remembered Settings, commands/focus, notice opt-in/dismissal, mobile and offline help. New privacy cache/refresh cases have explicit red evidence before fixes.
- Affected existing browser checks: insecure-HTTP keyboard creation; 220 pinned amount cases; currency/locales/week-start calendar; lost-response preference recovery; register recovery/settings; offline capture/replay/cache behavior. Existing tab-to-first-field behavior preserved by placing tools outside main. Current Settings remains safely activatable after remembered reopening.
- Svelte check: zero errors/warnings. Build passes (existing >500kB bundle advisory). Autofixer: no issues; pre-existing mutable Set/Map suggestions left unchanged. Impeccable detector: `[]`. Clippy all targets with warnings denied passes. `git diff --check` passes.
- Rendered desktop1280×900 and mobile390×844 inspected; no horizontal overflow, native focus/scrollable help retained. Test screenshots in worktree `actualBudget/web/artifacts/navigation-{desktop,mobile}.png`.

Limits: privacy is display masking, not access control; names/dates/notes/exports remain visible. Mobile editable fields follow upstream's reveal behavior. Navigation commands respect current editor guards; finish/close an editor before changing destination. No full offline budgeting, app login, native navigation rewrite or remote release feed. Broader integrated qualification remains with root.

## Consolidated review fix

Spec P2: global pending recovery previously formatted each amount using the active budget's privacy, which clears at the chooser and cannot represent mixed-budget rows. Recovery now masks every row initially, independent of active/cache preferences. Each row has an explicit Show/Hide amount control; reveal is local to that mounted view, preserves the row's captured number format and remains available for deleted/missing/offline budgets. Closing/reopening resets reveals. No retry/cancellation or outbox data changes.

Public regression reproduces private-budget → chooser → recovery disclosure with two different-budget pending rows; checks initial masking of both, independent per-row reveal/hide and reset on reopening while offline. Red observed `-12.34` exposed; focused navigation browser green after fix. Only changed-fix Svelte/check/build/format validation repeated; broader previous evidence remains valid.
