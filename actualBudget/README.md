# actualBudget

The replacement web/server application uses a static Svelte 5 frontend, Rust/Axum and authoritative SQLite, with no application login. It includes one tracking budget, transactions, rules, schedules, manual imports, reports, preferences and limited offline expense capture. The 49 retained rewrite tickets passed [final acceptance](../docs/final-parity-audit.md). Nothing has been deployed by that acceptance; use the [manual Coolify runbook](../docs/coolify-replacement.md) for a separately authorized replacement. See the [architecture](../docs/adr/0005-server-authoritative-web-architecture.md) and [107-story evidence ledger](../docs/parity-ledger.md).

See [unified transaction history](../docs/adr/0006-unified-transaction-history.md) for the 2026-11-11 scope change and compatibility approach.

## Run locally

The app opens Transactions automatically in one budget, with no naming or budget chooser. Its transaction history has filters for useful views. New entries need no account setup. Transactions are fully manual. File imports, rules/schedules, Android and the public
API remain; external bank connections and automatic bank sync are removed.

Requires Node 24 and Rust 1.99.1 (or the Docker build). From the repository root:

```sh
npm ci --prefix actualBudget/web
npm run check --prefix actualBudget/web
npm run build --prefix actualBudget/web
cargo build --locked --manifest-path actualBudget/server/Cargo.toml
ACTUALBUDGET_DATA_DIR=/tmp/actualbudget-development ACTUALBUDGET_WEB_DIR="$PWD/actualBudget/web/dist" actualBudget/server/target/debug/actualbudget-server
```

Open http://127.0.0.1:5006. The chosen data directory persists across process restarts. Use a disposable directory for tests. `ACTUALBUDGET_HOST` defaults to loopback; the container binds all interfaces. `ACTUALBUDGET_PORT` defaults to 5006. No login or SharedArrayBuffer is required. The current view and transaction drafts are remembered in browser storage. Reading existing server data remains available when storage is disabled; financial entry requires writable storage so saved requests survive reloads.

`GET /api/v1/info` publishes capabilities; `GET /api/v1/openapi.json` publishes the machine-readable v1 contract. Monetary amounts are signed integer minor units bounded by JavaScript's exact integer range; display and entry use the budget’s currency precision and formatting preferences.

## One budget, imports and recovery

Existing active budgets merge automatically during upgrade. Transactions, categories, plans, recurrence, links and colors remain distinct; equal names or amounts do not merge records. The oldest active budget supplies the retained settings. Before migration, the server saves a complete SQLite copy and a recovery snapshot of each active source. See [single-budget behavior](../docs/single-budget.md) for the exact boundaries and recovery files.

Settings contains **General**, **Import**, **Backups** and **Recovery**. Archive imports initialize an empty budget or merge into the current one; transaction files always enter the current history. A native backup restore explicitly replaces current data, even when the archive came from a former budget. Normal naming, switching, duplication and budget deletion are removed.

Old device requests retain their original identity. Migration retires source budgets and changes the retained budget’s generation so stale requests cannot silently apply to merged data. Review **Settings → Recovery → Saved changes on this device** and any recovered operation shown in its original screen. Download saved details before removing an obsolete request or canceling work you still need; never change its budget ID to force replay. Keep browser storage until the outcome is confirmed. New financial values still wait for server confirmation.

## Offline web capture

The opened budget and bounded register views are cached with visible saved dates. A loaded page can record ordinary expenses while disconnected and edit/delete its own pending expenses. Confirmed balances exclude pending work. Durable queued requests resume on reconnect or foreground; interrupted edits retain their original identity and budget generation. Calculator expressions, new reference data, and beginning edits to confirmed transactions require a connection. Plain HTTP does not guarantee cold offline navigation or OS background execution. See [offline behavior and verification](../docs/offline-web-validation.md).

## iPhone Home Screen companion

In Safari, open this server, choose **Share → Add to Home Screen**, and enable **Open as Web App** if offered. Launch the saved icon. Only an installed iPhone web app uses the compact expense view; ordinary browser tabs, iPad and desktop retain the full app.

The companion opens the same budget automatically. Add expenses and edit/delete its ten most recent ordinary expenses. A newly added backdated expense stays available below the list. Payee search supports creating a payee inline. Accounts, budget editing, reports and recurrence controls are absent from this view.

Open the app online once to discover and cache the current budget. Offline additions (including new payees) remain editable until delivery; existing expenses need a connection to begin editing. An edit interrupted by disconnection is saved and retried, with the server resolving newer edits. Delivery resumes when the app reconnects or returns to the foreground; it does not poll while idle or depend on iOS background execution.

HTTP supports online use and offline capture while the page remains loaded. Reopening from scratch without a connection requires HTTPS trusted by the iPhone, because the offline shell uses a service worker. Do not clear website storage while unsent expenses remain.

Run `npm run test:iphone-pwa --prefix actualBudget/web` after building. Set `PWA_BROWSER=webkit` to exercise WebKit (install the Playwright WebKit browser first). These isolated tests cover platform routing, ten recent expenses, backdated retention, offline reload/new payees/edit delivery, conflict handling, draft isolation, idle network behavior and small-screen layouts. Browser emulation does not replace a physical iPhone installation check.

## Verify the runtime

```sh
cargo test --locked --manifest-path actualBudget/server/Cargo.toml
npm run test:browser --prefix actualBudget/web
uv run actualBudget/tools/contract-smoke.py
python3 actualBudget/tools/container-smoke.py
```

Build the Rust binary and web assets first. Browser tests automatically launch that binary on a free port with a fresh temporary data directory, then stop it and remove the directory. Chromium (`CHROMIUM_PATH` overrides `/usr/bin/chromium`) maps `actualbudget.test` to loopback while retaining an insecure origin, exercising real LAN-HTTP restrictions. `ACTUALBUDGET_TEST_URL` may explicitly target an already running **isolated test server**; never use a live budget server. Tests use isolated budget data, verify real server responses, exercise saved-request recovery and capture desktop/mobile/200%-text screenshots in ignored `web/test-results/`. The single-budget suites cover fresh startup and upgrade consolidation: `npm run test:single-budget --prefix actualBudget/web` and `(cd actualBudget/web && node tests/single-budget-migration.mjs)`.

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

Display preferences are available from Settings. Formatting persists in the budget's SQLite data; theme/language stay in this browser. Choose Light, Dark, or System appearance. Midnight is the only dark palette. All 15 language catalogs shipped by Actual 26.10.0 load on demand; new explanatory/recovery copy uses English fallback where the released catalog lacks a translation. See [preferences contract and provenance](web/docs/preferences-contract.md).

Preference changes and transaction entry require writable browser storage for recoverable drafts. A failed refresh can use validated cached formatting; without it, financial entry waits for preferences rather than guessing currency precision. Unconfirmed saved requests remain retryable after reload. `POST /api/v1/preferences/convert-actual` converts raw preference records with explicit warnings/unmapped values; it is not a full budget archive importer.
