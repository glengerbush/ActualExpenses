# Ticket43 validation

Implemented against baseline4600a70; immutable Actual26.10.0 source unchanged. Shared API integration/migration registry coordinated with ticket03 owner; App/AccountView integration owned by root.

## Reproducible checks

From `actualBudget/web`:

```sh
npm run check
npm run format:check
npm run build
node tests/generate-preferences-fixture.mjs --check
node tests/preferences-browser.mjs
npm run test:browser
```

The combined browser command checks the reference fixture and runs creation, register, and preference suites, each with disposable server data unless an isolated test URL is explicitly supplied. From server: `cargo test --test preferences` (4 public HTTP tests). Runtime and web builds must exist before browser tests.

Observed green: Svelte check0errors/warnings; Prettier check; production build; 220 rendered monetary values compared to independently executed pinned Actual source; HTTP preference isolation/partial updates/default reset/invalid inputs/idempotency mismatch/replay/server replacement; released preference conversion with unmapped data warnings and credentials removed.

Browser coverage includes all built-in themes and system color changes; French catalog failed fetch→explicit retry→translated UI with error cleared; localized integer amount entry; Monday-first calendar keyboard selection; settings draft restoration; committed response lost→reload→same payload/key replay; storage failure→zero PATCH; original account pending replay after missing preference cache+failed GET; separate browser shared-vs-device preferences; no-cache financial availability gate; plain insecure HTTP; 320px+200% text/reduced motion in light/dark/midnight. No page errors.

Screenshots (ignored artifacts): `test-results/preferences-{light,dark,midnight}.png` and corresponding `-320-large-text.png`. Inspected desktop/midnight and mobile layouts; parent independently reviewed all themes. Labels, contrast and actions remain readable without document overflow. Horizontal account navigation remains contained.

Impeccable static detector: zero findings for Settings, DateInput, AccountForm, BudgetForm, styles. Svelte autofixer: zero issues across eight owned components/modules. One advisory on `Settings.svelte` treats the session's `update()` domain method as a state setter; retained intentionally because it encapsulates durable draft validation/persistence and performs ordinary reactive class property assignment. No effect-driven state synchronization.

## Scope/dependencies

Preference-record conversion is publicly available now. Ticket04 must call this exact converter when implementing archive import; whole archive import is not claimed. Custom theme data remains unmapped with warnings for ticket44. Language catalogs preserve shipped coverage, with natural English fallback for new copy (static coverage in translation-coverage.json). Full selected-format and ISO dates work; baseline abbreviated date shortcuts remain a documented parity gap for the date-entry owner. No login/encryption, offline background preference writes or production deployment added.
