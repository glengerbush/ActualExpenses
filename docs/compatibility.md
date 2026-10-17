# Actual compatibility fixture

The compatibility reference is Actual **26.10.0**. The harness installs the published sync server and official Node API as test tools. The Android application neither embeds Node nor needs this harness at runtime.

## Run

Requires Node 22+ and npm. From the repository:

```sh
cd tools/compatibility
npm ci
npm start
```

Wait for `FIXTURE_READY`. The process creates a fresh budget, isolated server data, and an independent official API client under a new system temporary directory. It binds random ports on **127.0.0.1 only**. It never takes a production server address. Stop with Ctrl+C; the temporary data remains for diagnosis.

The ignored `.fixture/fixture.json` contains disposable credentials and endpoints. Set `ACTUAL_FIXTURE` to an absolute path to override its location. Do not commit or publish this file. It contains:

- `serverUrl`: direct-client endpoint through the fault proxy.
- `directServerUrl`: Actual server endpoint used by the independent official client.
- `oracleUrl`: loopback test control and independently observed results.
- `password`, `token`, `budgetId`, `groupId`: disposable authentication and budget identifiers.
- `accountId`, `payeeId`, `categoryId`, `categoryGroupId`: seeded Spending account, Fixture shop payee, Groceries category, Everyday group.
- `root`: disposable directory, including `server.log`.

In another terminal, validate the fixture:

```sh
cd tools/compatibility
npm run smoke
```

Run JVM integration tests with `ACTUAL_FIXTURE` pointing to this file. Test startup must load the current file; restarting the fixture changes ports and identifiers. One fixture supports serial tests; independent parallel suites should start separate fixture processes with different fixture paths.

## Independent-client contract

These are **test harness** endpoints, not endpoints exposed by Actual or required by the app. They invoke the official `@actual-app/api`; assertions can therefore observe records without trusting the Android client's serializer, merge logic, or database representation.

| Request | Behavior |
| --- | --- |
| `GET /state` | Sync; return `accounts`, `payees`, `categories`, `rules`, Spending `transactions`, and triggered `faultEvents`. |
| `POST /transactions` | Body `{transactions: [...], accountId?: ...}` using official API transaction fields. Assign UUIDs where absent, add, sync, return `{ids: [...]}`. |
| `PATCH /transactions/:id` | Body contains official API update fields; sync before and after update. |
| `DELETE /transactions/:id` | Sync before and after deletion. |
| `POST /fault` | Arm `{mode: "before-send" or "after-accept", onlyWrites: true}`. Default targets the next mutation exchange; `onlyWrites:false` also permits a pull. |

`before-send` terminates the connection without forwarding the selected request. `after-accept` forwards it, drains the server response, then drops the client connection before acknowledgement. Always verify the server outcome through `/state`: dropping a response alone does not establish that a mutation was valid. The fault proxy identifies mutation requests using field 1 of the pinned Protobuf request format. Fault events record trigger mode and whether the request carried changes.

Oracle requests are serialized because the official API has one active budget. To model competing devices, create or edit via the oracle between direct-client operations. Pending-delete protection must be tested before publication separately from the race after publication. The harness does not invent a server compare-and-set capability.

## Evidence

The fixture smoke test passed on 2026-10-16 using Node 24.15.0:

- Real published server bootstrapped and password authentication succeeded.
- Official API created and uploaded a disposable budget, account, payee, category, and simple category rule.
- Independent-client reads verified an exact `-1001` expense, date, payee/category, and cleared state.
- Independent-client update to `-1002` and deletion converged through the server.

This smoke result verifies the harness; it is **not** a claim that all Android acceptance checks passed. JVM and device test results must be reported separately. Docker was unavailable to this user, so the harness uses the published npm server without requiring Docker privileges.

## Pinned schema observations

Read-only inspection of the disposable official-client snapshot established these compatibility facts; the test oracle itself uses public API reads:

- Latest migration: `1788468782000`; 60 migration rows in the fresh budget.
- Transaction storage uses `acct` for account, `description` for payee, integer `YYYYMMDD` date, `isParent`/`isChild` for splits, and `transferred_id` for a transfer. The public API translates these names.
- A simple payee-category rule uses raw condition `field: "description"`, `op: "is"`; its action sets `field: "category"`. Conditions/actions are JSON text and `conditions_op` is `and`.
- Synced preferences are rows containing `id` and `value`; `defaultCurrencyCode` determines monetary precision. In the pinned currency table, IRR, JPY, and KRW use zero decimal places; other supported codes use two. Missing/unrecognized codes use the default of two. `hideFraction` changes display, not storage precision.
- Same-field ordering and tombstone behavior belong to Actual's native synchronization. A later ordinary field edit does not automatically reverse an earlier published tombstone.

## Current protocol limits

After a complete pull, the direct client saves the latest received timestamp and complete Merkle tree atomically with the applied rows. With no pending changes, it first requests messages after that timestamp. An empty response with an identical tree completes sync without replaying history or rewriting storage. If messages arrive or the tree differs, it pulls from the epoch so delayed mutations with older timestamps are not missed. Pending changes always use the complete-history acknowledgement and conflict path. Old installations establish their checkpoint on their first successful sync after updating.

Changed budgets still require the group's full message log; divergent-history reconciliation is not implemented. Large or long-lived budgets may need further incremental synchronization before they perform acceptably on a phone.

The client rejects responses larger than 128 MiB, decompressed archives larger than 256 MiB, and outgoing batches larger than 19 MiB. These limits retain pending work; they do not make every budget below the limits fit Android's heap. Snapshot data, decoded rows, and history are currently held in memory. The disposable fixture demonstrates correctness on a small budget, not large-budget memory or performance qualification.

References: [sync endpoints](https://github.com/actualbudget/actual/blob/v26.10.0/packages/sync-server/src/app-sync.ts), [API transaction contract](https://github.com/actualbudget/actual/blob/v26.10.0/packages/api/methods.ts), [currency table](https://github.com/actualbudget/actual/blob/v26.10.0/packages/loot-core/src/shared/currencies.ts), [currency formatting](https://github.com/actualbudget/actual/blob/v26.10.0/packages/desktop-client/src/hooks/useFormat.ts), [native sync merge](https://github.com/actualbudget/actual/blob/v26.10.0/packages/loot-core/src/server/sync/index.ts).
