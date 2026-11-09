# Payee locations — ticket46

Accepted2026-11-09 through `4796e11`. [Final integrated qualification](1314151646-validation.md) resolves the historical integration/review gates recorded below.

Slice based on5a9efa1, committed as89560a8 and composed with1713c33.
Independent Standards/Spec and Impeccable finish/documentation reviews pass;
integrated batch qualification remains a gate. No release, deployment or
production data access.

Delivered: multiple saved locations; explicit Request/Save capture; local500m
nearest-ten suggestions and branch-specific forgetting; atomic optional new payee;
permission/HTTP fallback; online-only metadata with durable manual retry/cancel,
revision fences, edit-clock recovery and budget epochs. Expense outbox unchanged.
Server stores budget locations and tombstones, emits change pages, and preserves
them through import/backup/restore/duplicate. Android expense scope unchanged.

## Independent baseline

`actualBudget/tools/generate-payee-locations.mjs` runs @actual-app/api26.10.0's
`init().send` handlers. Source pin2bebdbaae0b701120a8cad9d56aafd64fa04c577.
Captures nearest-ten, inclusive500m, second-nearest branch after forgetting,
merged/deleted-payee exclusion. Original locations remain attached to removed
payees; no invented merge relocation. `--check` reproduces labeled outcomes.
Genuine export ZIP carries15 locations, including one tombstone; exports strip
clocks. Separate adversarial source variation verifies retained clocks exactly
and rejects an out-of-range coordinate before activation.

`generate-locations-schema11.py` uses a private immutable5a9efa1 executable.
SQLite/native ZIP are genuine outputs, with no schema rewriting. Tests verify
native accounts/receipts, default empty locations, old blocked source stage
refresh and unchanged original blocked upload receipt. Final integration assigns
locations schema14 after reconciliation12 and transfers13.

## Verification

- Full Rust suite:109 tests/18 suites passed; final focused suite5 tests passes
  after adding preserved source-clock/invalid-source coverage (110 total cases).
- Native API:23 tests pass, including location-only changes plus a queued ordinary
  expense across restart. Native cache contains no latitude/longitude. Projection
  regression only; no new Android UI or device-test claim.
- Served OpenAPI success/error/change-page contract passes. Rustfmt and clippy
  warnings-denied pass. Svelte check/build and autofixer pass.
- Focused real-browser suite passes against disposable server: pinned proximity,
  two branches/forget/merge/remove, no automatic geolocation, same-origin-only
  requests, fresh explicit capture, atomic new payee, lost committed response
  and unchanged reload retry, precommit cancellation, stale-tab revision refusal,
  restore-epoch refusal/cancel, offline ordinary expense, real insecure HTTP,
  denied permission, keyboard,390px/dark/200% text and1280px desktop.
  Chromium geolocation is emulated; permission-denial callback is forced. No
  physical position or actual household coordinates are involved.
- Full browser run passed initial/register/date/preferences/import groups, then
  failed existing `concurrency-browser.mjs:486`: expected at least3 transaction
  POSTs after lost-ack/reload, observed2. Assertion retained; cause unconfirmed.
  Root notes newer main has diagnostic improvements absent from this5a9efa1
  baseline. Isolated repeat passed that assertion, then failed at595 expecting
  the ordinary expense cancellation button (5s timeout). Both logs retained;
  cause unconfirmed and broader transaction concurrency remains an integration
  gate. Separate remainder passes receipt feedback, themes, offline, epochs,
  race/revert, lifecycle, accounts, categories and metadata clocks; stops at
  existing payees-tags Field1 exact-label selector timeout. Root confirms that
  selector is fixed in main8a941e9; unchanged in this private baseline. Final tail
  passes split, register query/settings/review/regex-budget and locations groups.
- Final focused browser run also proves two successive newly created payees can
  save locations in one expense draft; its regression failed against the old
  bundle, then passed after renewing the inline reference ID. Held-response
  regression also proves late acknowledgements preserve newer payee typing;
  transaction submission waits only for its own unresolved new payee identity.
  Payee resolution persists before clearing the location receipt/retrying views.

Logs: `/tmp/locations46-{oracle,oracle-check,schema11,public-final,full-public,
native,contract,clippy,clippy-final,web,final-ui,autofixer,impeccable,browser,
browser-final,full-browser,browser-remainder,concurrency-repeat,new-payee-red,
late-payee-red,browser-tail}.log`.
Review captures: `docs/screenshots/payee-locations/`.
Full screenshots: `.scratch/evidence/locations46/` (mobile nearby, HTTP fallback,
denied dark/large text, desktop, restore pending, new-payee transaction guard;
panel crops for finish review).
No shipping raster assets. Existing DESIGN.md/token system retained. Mechanical
Impeccable detector reports no findings. Existing context config/surface-path
drift was reported to root and left untouched.

## Integration notes

- Final migration `014_locations.sql`; archive current/max14, pre14 location
  defaults, migration hook index13, lifecycle restore report and OpenAPI enum
  move together. Earlier reconciliation/transfer defaults retain their versions.
- Shared touchpoints: API route/cancellation/capability registry; changes union;
  source conversion/install; lifecycle clear/archive; TransactionForm composition;
  browser package script and native public-API test. No companion contract changes.
- Locations are online metadata. Pending location requests require manual retry
  or cancel; they never enter background expense delivery.
- The browser/OS owns geolocation acquisition. This app sends no position for
  lookup and uses no maps/geocoding/third-party location endpoint. Explicit saved
  coordinates belong to budget storage/backups and a temporary durable request.

Freeze: temporary dependency symlinks removed after checking their exact targets.
Independent review and integration gates above remain open.

## Root responsive recovery correction — 2026-11-09

Review found that widening the window hid a saved location request and its retry/cancel actions. A real-browser regression reproduced this after reload. Recovery, in-flight/error states and completion feedback now remain visible on desktop; fresh location discovery remains a mobile surface. The regression additionally waits until the request has fully settled before checking completion feedback, exposing and correcting a brief-success-only state.

Complete focused location browser workflow, Svelte checking/build/autofixer and diff whitespace checks pass. Root inspected `desktop-recovery.png` and `desktop-recovered.png`; both are included in the final review packet. Logs `/tmp/locations46-desktop-{recovery-red,feedback-red,recovery-green}.log`.

## Integrated evidence — 2026-11-09

- Independent Standards and Spec reviewers passed the composed working tree with no findings. Spec review independently checked genuine schema13 upgrade, atomic creation/forgetting, protected transfer refusal, original receipts and unchanged companion projection.
- 31 focused public HTTP cases pass: locations6, reconciliation8, transfers17. Native integration freshly executed27 cases, zero failures/errors/skips. No device-test claim.
- Location, payee/tag and reconciliation browser workflows pass. Svelte check/build, served OpenAPI, warnings-denied Clippy and Rustfmt pass. Logs `/tmp/locations46-integrated-{http,native,browser,payees-browser,reconciliation-browser,web-check,web-build,contract,clippy,fmt}.log`.
- `tools/generate-locations-schema13.py` generates genuine SQLite/native ZIP fixtures through immutable1713c33, recording executable SHA256. Upgrade/restore preserves protected transfer rows, locked account state and exact original receipts; previously blocked location upload receipts replay unchanged while stage inspection becomes ready. No database/version rewriting. Logs `/tmp/locations46-schema13-{fixture,upgrade}.log`.
- Full suite and production container qualification will run once after the financial integration batch; earlier baseline browser failures above are retained as historical evidence, not treated as passing.
- Impeccable finish disposition: ship; eight supplied captures and three detail crops inspected, no material fixes. Bounded documenter: no changes required after comparing DESIGN, styles and component patterns. Neutral surfaces/green actions, system type, 700px stacking, native44px controls and inline recovery remain consistent. Existing noncanonical DESIGN headings/token frontmatter were reported without unrelated repair. No shipping rasters.
