# Compatibility policy

Support only the current release and the immediately previous patch. For the next patch, the deployed baseline is actualBudget 0.6.0 (schema37) and actualExpenses 0.4.0 (native API-v1 encrypted state).

Fresh installations use the materialized baseline schema. Existing schema37 databases receive the transaction-notes migration to schema38. Earlier databases and native archives are rejected; database rejection occurs before changing journal mode or stored data. Upgrade backups and current native restore remain available.

Original Actual protocol clients, CRDT outbox translation, earlier schema migrations, and retired account-editor recovery are removed. Android retains its current native cache/outbox, retry identities, edit clocks, and restore-generation protections.

Actual desktop ZIP exports remain supported file imports. The pinned Actual 26.10 source/financial fixtures verify imported data and calculations; they do not provide old app-version support. `actualExpenses/tools/compatibility` supplies the official API dependency used to generate those reference fixtures, not an Android protocol server.

Run `cargo test --manifest-path actualBudget/server/Cargo.toml` for the previous-patch upgrade/archive tests and current API suites. Build that server before Android `:core:test`; those tests need no original-Actual fixture server.
