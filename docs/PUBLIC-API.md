# Remote automation API

actualBudget owns the engine and financial state. Call HTTP directly; do not start
`@actual-app/api`, download a working database or run a local calculation engine.
Use `/api/v1/info`, `/api/v1/openapi.json` and `/api/v1/query-capabilities` to discover
the installed version, access model, routes and exact supported query profiles.
IDs in URLs are native budget UUIDs, not names or source cloud IDs.

[SDK migration map](public-api-sdk-map.json) explicitly maps all67 functions in
pinned Actual26.10 `packages/api/methods.ts` and all18 declared query resources.
This is a semantic migration, not a drop-in implementation of its local SDK.
Native route bodies/field names are defined by the served schema. Existing import,
metadata, budgeting, rule, schedule, reporting and banking writers retain their
own validation and revision contracts.

| SDK/resource family | Remote interface |
| --- | --- |
| Load/list/download/export budgets | `GET /budgets`; explicit budget scope; retained imports and portable backup/download routes. No local sync cache/password handshake. |
| Accounts/groups, categories/groups, payees, tags | Existing typed resource CRUD/lifecycle/merge routes. Queries expose fixed profiles where advertised. |
| Transactions/import/merge | Register pages; bank-file upload→read-only preview→selected commit; graph-aware merge preview/commit. Plain entry preserves manual intent; rule-entry route is opt-in. |
| Query | `POST /budgets/{b}/query`, typed JSON below. `runQuery` is deprecated upstream; both SDK calls map here. |
| Notes | `POST /notes/read` with `{id}`; `POST /notes/commands` with `{id,note,snapshot}`. Arbitrary bounded IDs, domain aliases, missing/null vs explicit empty. |
| Planning/holds | `GET /budget?month=YYYY-MM&count=1..12`; `POST /budget` actions set/carryover/hold/reset_hold. Explicit windows replace the SDK cache-generated month-horizon array; query cells/transaction dates for existing data bounds. No client financial calculation. |
| Rules/schedules | Typed routes, preview/apply and explicit foreground posting/catch-up. GET/query never posts. Payee-associated rules use `GET /rules?payee_id=...`. |
| Preferences/saved filters | `/preferences`, `/register-settings`. Translate supported source preference IDs to native typed fields; budgetType uses budgeting method action. Retained/deferred source data is not silently made executable. |
| Reports/cards/pages | `/reporting`, `/reports/query`, drilldown and dedicated report routes. Metadata/calculations use their validated schemas, not arbitrary table access. |
| Cleanup pools / payee locations | `/budget/templates` pools / `/payee-locations`. Native metadata types and bounds remain authoritative. |
| Bank refresh | Explicit configured connection refresh via banking commands. Provider secrets use specialized routes; the example financial outbox excludes credential commands. |
| SDK callbacks/batching | Supported-format staged imports or explicit typed calls. No remote JS callback. Transaction batches are atomic; there is no cross-command rollback promise for arbitrary custom construction. |

## Typed read queries

POST `/api/v1/budgets/{b}/query` is read-only, including calculations. It needs no
idempotency key, HLC or history owner; receipt-only/cancel headers are rejected.
Translate SDK serialized state as follows:

| SDK QueryState | Version1 request |
| --- | --- |
| table / selectExpressions / filterExpressions | table / select / filter |
| groupExpressions / orderExpressions | group_by / order_by |
| tableOptions.splits | splits: inline, all, none or grouped |
| calculation / withDead / validateRefs / rawMode | calculation / include_deleted / validate_refs / raw |
| limit / offset | limit / offset; supply original snapshot for consistent continuations |

```json
{
  "version": 1,
  "table": "transactions",
  "splits": "inline",
  "select": ["id", "date", "amount", "payee.name", "category.group.name"],
  "filter": [{"account": "ACCOUNT_UUID"}, {"date": {"$gte": "2026-11-01"}}],
  "order_by": [{"date": "desc"}],
  "limit": 100,
  "snapshot": "ORIGINAL_CHECKPOINT"
}
```

Expressions use fixed advertised functions/fields and named aliases, e.g.
`"select":[{"total":{"$sum":"$amount"}}],"calculation":true`. Grouped aggregate
ordering and scalar wrappers such as abs(sum(amount)) are supported. Aggregate/
window expressions cannot be group keys or row-filter transforms. Unknown
functions, tables, references and invalid placements return typed400, never SQL.
A string beginning `$` denotes a field in an expression; use `$literal` for data.
Literal text and membership values are bound parameters, including quotes/SQL-like
text. Fractional numeric literals are refused; money uses exact integer minor units.

Important wire differences are discoverable, explicit and source-tested:

- `category_groups` query rows are flat. Expand with a bounded categories query
  filtered by `group=ID`, or use the grouped management resource.
- Source reflect_budgets/zero_budgets map to `budget_cells` with method tracking/
  envelope and business key method/month/category. Months are ISO, carryover is
  boolean, goals are decoded JSON, and native IDs are opaque.
- Automation definitions, rules and raw bank metadata are structured JSON, not
  original serialized text order. JSON supports projection/count/null checks;
  string predicates/order refuse400. Original source bytes remain downloadable.
- Rule field aliases acct/description/imported_description become account/payee/
  imported_payee. Nested recurrence discriminators/options/values remain intact.
- Select lists are explicit. `*` expands only advertised fields; request `id`
  explicitly when translating SDK projections that implicitly appended it.
- Native private positions are not source storage ranks. Use default stable display
  ordering, management ordering/move commands or explicit advertised sort fields.
- Dates remain ISO even in raw mode. Raw affects boolean/structured-value output,
  not access to underlying storage columns. Boolean null predicates mean false,
  as upstream; membership retains upstream scalar stringification. Use `$month`/
  `$year` for reduced-granularity comparisons rather than a month literal on date.
- Category filter IDs stay literal after deletion/replacement; mutation reference
  resolution is separate. Notes queries expose logical live identities, including
  explicit empty text, rather than private inverse-history tombstones.

Default page100/max1000; offset≤100000; matched rows≤100000; selections≤32;
expression depth8/nodes256; at most500 caller-bound values, joins depth4. Cumulative
roots+expanded children≤10000 rows/100000 values and conservative4MiB output;
SQLite work2million steps, shared text work64MiB and the existing shared regex
allowance64MiB. Simplify/narrow on
bounded-work400. Responses contain data, matched_count, next_offset, snapshot,
epoch. Reuse the original snapshot for every continuation: intervening changes
return409. Without snapshot each page is a separate fresh read, not a stable
scan. Ungrouped aggregate cardinality is one; it never advertises row-count pages.

Account balance is a live-leaf SUM(amount) filtered by account and optional ISO
cutoff. Common-payee convenience can be caller orchestration: count transactions
by payee over the selected12-week calendar window, sort by negative count then
name, and resolve typed payee records. No learning/default mutation occurs. Exact
name lookup uses equality; retain UUIDs and handle ambiguous names explicitly.

## Immutable financial commands

POST `/transactions/batch` body:

```json
{
  "snapshot": "REVIEWED_CHECKPOINT",
  "max_graph_members": 100000,
  "operations": [
    {"action":"create","transaction":{"id":"NEW_UUID","account_id":"ACCOUNT_UUID","amount":-1234,"date":"2026-11-09","notes":"Expense","cleared":true}},
    {"action":"update","id":"OTHER_UUID","fields":{"notes":"Correction"}},
    {"action":"delete","id":"THIRD_UUID"}
  ]
}
```

Capture once before delivery: URL/method, exact JSON bytes, Idempotency-Key,
X-Actual-Edit-Clock, X-Actual-Budget-Epoch and optional stable
X-Actual-History-Owner. Never change these on retry, refresh or restore. The server
uses existing financial writers. One command/receipt and, when owned, one undo
covers the entire batch. Any newer field, invalid reference, protected graph or
later failure rolls everything back; failed commands have no successful receipt.
This is stronger than upstream batchBudgetUpdates, which batches spreadsheet
refreshes without rollback.

Provide1–100 distinct targets, one operation per connected split/transfer graph.
Use existing split/transfer endpoints for a single combined graph intent. Resolve
new payee/category names through resource routes before capturing a batch.
Optional max_graph_members1..100000 defaults100000 and bounds discovery itself;
overflow is graph_limit400, not truncation. Response serialization is conservatively
bounded4MiB. Snapshot/generation, original clocks, reconciliation confirmations
and inverse-history graph guards remain active. Do not automatically split a
failed atomic intent into partial writes.

After uncertainty, send the same captured request with
`X-Actual-Receipt-Only:true`. A200 returns the exact stored outcome;404
operation_uncommitted reports no known receipt and leaves the intent retained.
Resending the original mutation is idempotent. Explicit cancellation sends that
same identity with `X-Actual-Cancel-If-Uncommitted:true`: if committed, its original
receipt wins; otherwise a durable409 operation_canceled fence prevents later
replay. A changed key/body/clock/epoch/owner is a different intent or409 identity
conflict, not a retry. An old receipt remains recoverable after restore even
though a new old-epoch mutation is refused.

## Runnable external caller

`actualBudget/tools/public-api-client.py` uses Python3 standard library only.
It reads remotely and persists a0600 financial outbox atomically before delivery.
The outbox stores original request bytes/origin/headers plus a corruption digest;
attempted state is persisted before network delivery. A new process reuses it
without refreshing clocks, epoch, body or server. Keep these financial-data files
private. The example intentionally queues only transaction batches/import commits
and retained file uploads; it never queues provider credentials.

```sh
python3 actualBudget/tools/public-api-client.py --server "$SERVER" read --path /budgets
python3 actualBudget/tools/public-api-client.py --server "$SERVER" --budget "$BUDGET" query --body query.json
python3 actualBudget/tools/public-api-client.py --server "$SERVER" --budget "$BUDGET" prepare --path /transactions/batch --body reviewed-batch.json --outbox pending.json --owner "$OWNER"
python3 actualBudget/tools/public-api-client.py send --outbox pending.json
python3 actualBudget/tools/public-api-client.py receipt --outbox pending.json
python3 actualBudget/tools/public-api-client.py cancel --outbox pending.json
```

Preparation refuses an existing outbox or stale supplied review snapshot. If no
snapshot was supplied, it captures the current checkpoint only while preparing a
new intent. Delivery refuses a replacement server/budget argument. Nonterminal
errors retain the request for review; no new purchase/clock is synthesized. A
failed atomic file replacement prevents delivery. A failed first preparation can
leave a visible reserved empty file; inspect/remove it explicitly before retrying
preparation. There is no background polling or automatic financial retry loop.

For CSV/OFX/QFX/QIF/CAMT: prepare-upload with account/file/outbox; send it; retain
the returned stage ID. Use preview with `/transaction-imports/ID/preview` and a
mapping/options/snapshot JSON file. Review returned rows, then prepare the commit
with explicit selected choices and send its new outbox. Uploads retain original
bytes and do not change financial rows; their route has no receipt-only/cancel
probe, so resend the original retained upload key for acknowledgment. Financial
commit has the full immutable receipt/cancellation contract. No imported ID is
invented from a content hash, and no equal-looking purchase is silently discarded.

Running witnesses: check-public-caller-http.mjs spawns the real Python process,
loses a batch ACK only after verified upstream200, proves the committed source
ledger before retry, compares exact bytes/key/HLC/epoch/owner across processes,
then uploads/reviews/imports a genuine independently captured manual-match case.
Validation/provenance and remaining qualification are recorded in
[public-query-validation.md](public-query-validation.md).
