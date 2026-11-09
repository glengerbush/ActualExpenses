# Schedules (tickets21–22, in progress)

The baseline is Actual26.10.0, commit `2bebdbaae0b701120a8cad9d56aafd64fa04c577`. `node actualBudget/tools/generate-schedules.mjs` executes that pinned installed API/engine in a disposable directory, with a frozen UTC calendar. `--check` executes it again and compares the normalized result. No replacement code or rewritten expected calculations enter the oracle.

The first slice records eleven calendar cases and seven persisted lifecycle snapshots: daily/weekly intervals, specific monthly days, last days and weekdays, mixed recurrence rules with separate count limits, leap years, inclusive end dates, weekend movement/collisions, manual posting, skipping, automatic catch-up, repeated runs and completion. It exports a genuine local source archive; the local-only import's attempted cloud upload is unauthorized as expected, without changing the exported data. Generated IDs are normalized by first appearance; values, dates, references and financial order remain exact.

Posting and matching implementation/qualification are not yet accepted. Subsequent slices must add financial transfer/rule/horizon and matching/discovery/conversion oracles and public API/browser/native checks. Catch-up is an explicit idempotent POST; GET and receipt-only probes remain read-only. Native background work remains pending mutation delivery, never schedule polling.
