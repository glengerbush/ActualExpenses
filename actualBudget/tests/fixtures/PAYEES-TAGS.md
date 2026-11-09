# Payee/tag evidence

`payees-tags.json` comes from disposable `@actual-app/api@26.10.0` public calls,
source commit `2bebdbaae0b701120a8cad9d56aafd64fa04c577`. Only generated IDs are
aliased. Monetary values, payee references, imported text, note text and tag
metadata are unchanged. `tag_renames` additionally executes the pinned pure
`shared/tags.ts` helper after TypeScript transpilation; it is not replacement code.
The public API does not expose favorite/hidden fields: those controls are checked
against pinned `server/payees/app.ts`, `server/tags/app.ts` and real replacement
HTTP/browser outcomes, not claimed as observable public-oracle fields.

`payees-tags-import.zip` is a genuine public `exportBudget()` archive. Its sibling
JSON identifies generated source records. It includes merged/removed payees,
an automatic transfer payee, imported payee text, note tokens and tag metadata.
No SQL creates or edits that source fixture.

`payees-schema7.sqlite3` and `payees-schema7-native.zip` come from an isolated
server built at `ae3911b`, using only public HTTP: import the existing pinned
fixture, add a native payee, edit an imported transaction's payee/notes/amount
with a fixed HLC, then export a native backup. The SQLite backup API consolidates
its WAL after shutdown; it never changes data. The JSON stores exact observed
records, balances and receipts. Tests prove both SQLite upgrade and old native
restore retain the native edits and byte-equivalent JSON receipt values while
backfilling transfer metadata. No generated migration clocks replace old clocks.

Regenerate with `tools/generate-payees-tags.mjs` (use `ACTUAL_ORACLE_PACKAGE` for
an alternate compatibility package path), and
`tools/generate-payees-upgrade.py --binary /path/to/isolated/ae3911b/server`.
The public oracle supports `--check`; never regenerate from replacement output.

The public HTTP source-chain stress test constructs a 140-link alias graph inside
an input ZIP derived from the pinned fixture, and separately a real cycle. This
is importer robustness evidence, not pinned parity: Actual public payee merges
flatten mappings and its AQL view joins the mapping once. A separate public HTTP
case executes 140 successive ordinary merges and resolves the original queued ID.
