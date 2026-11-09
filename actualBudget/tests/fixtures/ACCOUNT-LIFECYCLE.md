# Pinned account lifecycle observations

`account-lifecycle.json` records eight snapshots from the published
`@actual-app/api@26.10.0`, using only its public mutation/read methods in a
disposable local budget. The replacement server is not loaded. Account/group
IDs are normalized to aliases; transaction dates and financial values are not.

Regenerate with `node actualBudget/tools/generate-account-lifecycle.mjs`;
independently repeat and compare with `--check`. Both passed on 2026-11-08.
The pinned package logs an expected unsuccessful cloud upload when finishing
the local import; no server is configured or contacted.

Observed behavior for ticket 09:

- Renaming, grouping and account notes preserve balances/history.
- Nonzero balance closure without a destination fails and leaves history open.
- Zero balance closure with transactions retains the account and history;
  reopening restores its open status.
- Closing an empty account deletes it from the public account list.
- Explicit account deletion removes its visible history, verified with a
  subsequent public transaction read of the deleted account.
- Deleting an account group ungroups its accounts without deleting them.

This fixture does not qualify replacement behavior by itself. Ticket 09 must
exercise equivalent public HTTP/browser workflows. Opening balances already
have a separate import fixture; reorder UI, metadata migration and transfer
closure require their own applicable verification.
