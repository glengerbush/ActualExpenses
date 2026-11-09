# Payee locations — ticket46

Private slice based on5a9efa1; independent root Standards/Spec and Impeccable
finish/documentation review, migration renumbering and integration remain gates.
No commit, release, deployment or production data access.

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
refresh and unchanged original blocked upload receipt. Schema12 is provisional;
root must renumber after other financial integrations.

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

- Private migration `012_locations.sql`; archive current/max12, pre12 defaults,
  migration hook index11, lifecycle restore report and OpenAPI enum must move
  together when root assigns the final number.
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

Complete focused location browser workflow, Svelte checking/build/autofixer and diff whitespace checks pass. Root inspected `desktop-recovery.png` and `desktop-recovered.png`; both are included in the final review packet. Logs `/tmp/locations46-desktop-{recovery-red,feedback-red,recovery-green}.log`. Independent code and Impeccable finish reviews remain pending.
