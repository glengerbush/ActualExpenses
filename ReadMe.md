# actualExpenses and actualBudget

Two applications sharing a budget:

- **[actualExpenses](actualExpenses/ReadMe.md)** — the released native Android expense companion. Build and run its tools from `actualExpenses/`; its application ID, signing identity and version remain unchanged.
- **[actualBudget](actualBudget/README.md)** — the web/server replacement, based on the recorded Actual 26.10.0 behavior. The approved architecture is Svelte, Rust/Axum and SQLite. Desktop delivery is deferred.

The Android [release workflow](.github/workflows/release.yml) remains at the repository root. Shared [design decisions](docs/adr/), [domain vocabulary](CONTEXT.md) and [issue-tracker instructions](docs/agents/issue-tracker.md) also remain here.

The independent fork does not track routine upstream merges. The pinned source-only reference under actualBudget supplies feature/migration evidence; the disposable published-server harness supplies a runnable independent oracle. Neither changes the deployed server.
