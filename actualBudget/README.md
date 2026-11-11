# actualBudget

The replacement web/server application uses a static Svelte 5 frontend, Rust/Axum and authoritative SQLite, with no application login. It includes tracking/envelope budgets, transactions, rules, schedules, manual imports, reports, preferences and limited offline expense capture. The49 retained rewrite tickets passed [final acceptance](../docs/final-parity-audit.md). Nothing has been deployed by that acceptance; use the [manual Coolify runbook](../docs/coolify-replacement.md) for a separately authorized replacement. See the [architecture](../docs/adr/0005-server-authoritative-web-architecture.md) and [107-story evidence ledger](../docs/parity-ledger.md).

## Run locally

Accounts are fully manual. File imports, rules/schedules, Android and the public
API remain; external bank connections and automatic bank sync are removed.

Requires Node 24 and Rust 1.99.1 (or the Docker build). From the repository root:

```sh
npm ci --prefix actualBudget/web
npm run check --prefix actualBudget/web
npm run build --prefix actualBudget/web
cargo build --locked --manifest-path actualBudget/server/Cargo.toml
ACTUALBUDGET_DATA_DIR=/tmp/actualbudget-development ACTUALBUDGET_WEB_DIR="$PWD/actualBudget/web/dist" actualBudget/server/target/debug/actualbudget-server
```

Open http://127.0.0.1:5006. The chosen data directory persists across process restarts. Use a disposable directory for tests. `ACTUALBUDGET_HOST` defaults to loopback; the container binds all interfaces. `ACTUALBUDGET_PORT` defaults to 5006. No login or SharedArrayBuffer is required. Budget/account selection and creation drafts are remembered in browser storage. Reading existing server data remains available when storage is disabled, but creation requires working browser storage so its request identity can survive reloads.

`GET /api/v1/info` publishes capabilities; `GET /api/v1/openapi.json` publishes the machine-readable v1 contract. Monetary amounts are signed integer minor units bounded by JavaScript's exact integer range; display and entry use the budget’s currency precision and formatting preferences.

## Recover creation drafts

Budget creation uses an origin-scoped versioned localStorage record; account creation uses a separate record for each budget UUID. Field changes are saved as entered, including incomplete values. Reloading restores the matching form. Submitted bodies, entity IDs and idempotency keys are persisted before POST and remain immutable until the outcome is acknowledged. A recovered pending creation requires an explicit **Retry creation**; it is never sent automatically or in the background.

Acknowledgement or an intentional cancellation of an unsubmitted draft removes its record. A confirmed validation rejection preserves editable fields. Unknown, malformed, mismatched-origin or mismatched-budget records are retained and reported instead of being delivered or overwritten. A storage error is visible and blocks a new POST; it does not erase the current fields. Clearing browser data manually removes local drafts, so avoid doing that while a creation is unconfirmed.

A draft's UUID also identifies its operation and resulting entity. Two tabs submitting the same saved draft therefore cannot create two entities. Different payloads using that identity receive a conflict rather than silently replacing the first creation; this does not provide collaborative form editing.

## Offline web capture

Previously opened budgets and bounded register views are cached with visible saved dates. A loaded page can record ordinary expenses while disconnected and edit/delete its own pending expenses. Confirmed balances exclude pending work. Durable queued requests resume on reconnect or foreground; interrupted edits retain their original identity and budget generation. Calculator expressions, new reference data, and beginning edits to confirmed transactions require a connection. Plain HTTP does not guarantee cold offline navigation or OS background execution. See [offline behavior and verification](../docs/offline-web-validation.md).

## Verify the runtime

```sh
cargo test --locked --manifest-path actualBudget/server/Cargo.toml
npm run test:browser --prefix actualBudget/web
uv run actualBudget/tools/contract-smoke.py
python3 actualBudget/tools/container-smoke.py
```

Build the Rust binary and web assets first. Browser tests automatically launch that binary on a free port with a fresh temporary data directory, then stop it and remove the directory. Chromium (`CHROMIUM_PATH` overrides `/usr/bin/chromium`) maps `actualbudget.test` to loopback while retaining an insecure origin, exercising real LAN-HTTP restrictions. `ACTUALBUDGET_TEST_URL` may explicitly target an already running **isolated test server**; never use a live budget server. Tests create budgets, verify real server responses, inject a lost committed response then reload/retry, verify draft/storage-failure and two-tab behavior, and capture desktop/mobile/200%-text screenshots in ignored `web/test-results/`.

The contract smoke uses a disposable server and a pinned JSON Schema validator managed by `uv` to check real API responses against the served contract. The container smoke builds the Dockerfile, uses a unique temporary volume, creates data through HTTP, recreates the container and checks persistence. It removes its test resources. It never uses the production volume. Build manually with `docker build -f actualBudget/Dockerfile -t actualbudget:local actualBudget`.

## Pinned behavior reference

`reference/actual-26.10.0` contains a curated source snapshot of Actual commit `2bebdbaae0b701120a8cad9d56aafd64fa04c577`, without upstream Git history. It is **source-only research material**, not the production runtime or a second buildable application. Its web source is named `desktop-client` upstream; that package is retained because it implements the browser UI.

The source selection keeps web, financial-engine, API/CLI, synchronization, shared components, relevant tests and written feature documentation. It excludes Electron/native-mobile packages and platform implementations, upstream workspace/build/release/install configuration, generated documentation assets, and upstream agent/editor configuration. Shared browser source may still describe how the baseline behaved in desktop environments; it is not wired into a desktop build. Retained source bytes are unchanged, with their original SHA-256 recorded in `manifest.json` and the original MIT notice in `LICENSE.txt`.

Reproduce or verify the snapshot from a clean checkout at that commit:

```sh
python3 actualBudget/tools/import-reference.py /path/to/pinned-actual-checkout --check
```

Without `--check`, the importer creates a new snapshot only; it refuses to overwrite an existing one. `manifest.json` records every retained path/hash and excluded path/reason, so exclusions and provenance can be audited independently.

## Runnable reference oracle

The [Android compatibility harness](../docs/compatibility.md) installs the **published** Actual API and sync-server version 26.10.0 with a lockfile. It creates fresh loopback-only server state and a disposable budget. Run it from `actualExpenses/tools/compatibility`; never point it at a live budget. This supplies an executable reference while the new runtime is built independently.

Each replacement slice must record baseline source/fixture evidence and observable results in the [parity ledger](../docs/parity-ledger.md). Passing the existing Android fixture does not establish feature parity for the new application.

Display preferences are available from Settings within a budget. Formatting and experimental opt-ins persist in the budget's SQLite data; theme/language stay in this browser. Light, dark, midnight and system appearance are supported, with all 15 language catalogs shipped by Actual 26.10.0 loaded on demand. New explanatory/recovery copy uses English fallback where the released catalog lacks a translation. See [preferences contract and provenance](web/docs/preferences-contract.md).

Preference changes and account creation require writable browser storage for recoverable drafts. A failed refresh can use validated cached formatting; without it, financial entry waits for preferences rather than guessing currency precision. Unconfirmed saved requests remain retryable after reload. `POST /api/v1/preferences/convert-actual` converts raw preference records with explicit warnings/unmapped values; it is not a full budget archive importer.

Custom themes are available in Settings. Install one of the 33 pinned catalog choices, paste supported CSS variables/embedded fonts, or import exported theme data/Actual global theme preferences. Appearance is saved in this browser’s IndexedDB before applying; it never changes another device. Unsupported overrides produce explicit warnings and retain their source for export. Installed themes load locally without downloading again. A readable **Use built-in theme** control remains available even with poor-contrast custom colors. See the [theme contract and validation](web/docs/custom-theme-contract.md).
