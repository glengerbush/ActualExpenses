# actualBudget

The full web/server application will use a static Svelte frontend, Rust/Axum and authoritative SQLite, with no application login and limited offline expense capture. See the [accepted architecture](../docs/adr/0005-server-authoritative-web-architecture.md) and [migration plan](../docs/fork-migration-plan.md).

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
