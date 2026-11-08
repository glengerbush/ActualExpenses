# actualBudget

The replacement web/server application uses a static Svelte 5 frontend, Rust/Axum and authoritative SQLite, with no application login. The first slice creates/selects budgets and accounts with exact opening balances. Other rewrite features, including limited offline expense capture, follow in later tickets; this is not yet a production cutover. See the [accepted architecture](../docs/adr/0005-server-authoritative-web-architecture.md) and [migration plan](../docs/fork-migration-plan.md).

## Run the first slice locally

Requires Node 24 and Rust 1.99.1 (or the Docker build). From the repository root:

```sh
npm ci --prefix actualBudget/web
npm run check --prefix actualBudget/web
npm run build --prefix actualBudget/web
cargo build --locked --manifest-path actualBudget/server/Cargo.toml
ACTUALBUDGET_DATA_DIR=/tmp/actualbudget-development ACTUALBUDGET_WEB_DIR="$PWD/actualBudget/web/dist" actualBudget/server/target/debug/actualbudget-server
```

Open http://127.0.0.1:5006. The chosen data directory persists across process restarts. Use a disposable directory for tests. `ACTUALBUDGET_HOST` defaults to loopback; the container binds all interfaces. `ACTUALBUDGET_PORT` defaults to 5006. No login or SharedArrayBuffer is required. Budget/account selection is remembered in browser storage; disabled browser storage does not affect server persistence.

`GET /api/v1/info` publishes capabilities; `GET /api/v1/openapi.json` publishes the machine-readable v1 contract. Monetary amounts are signed integer minor units bounded by JavaScript's exact integer range; the initial web form displays two decimal places. Currency/display preferences are a later slice.

## Verify the slice

```sh
cargo test --locked --manifest-path actualBudget/server/Cargo.toml
npm run test:browser --prefix actualBudget/web
uv run actualBudget/tools/contract-smoke.py
python3 actualBudget/tools/container-smoke.py
```

Build the Rust binary and web assets first. Browser tests automatically launch that binary on a free port with a fresh temporary data directory, then stop it and remove the directory. Chromium (`CHROMIUM_PATH` overrides `/usr/bin/chromium`) maps `actualbudget.test` to loopback while retaining an insecure origin, exercising real LAN-HTTP restrictions. `ACTUALBUDGET_TEST_URL` may explicitly target an already running **isolated test server**; never use a live budget server. Tests create budgets, verify real server responses, inject a lost committed response, and capture desktop/mobile/200%-text screenshots in ignored `web/test-results/`.

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
