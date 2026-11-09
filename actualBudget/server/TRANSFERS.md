# Linked transfers

Ticket15, stories24/25/39/95. Provisional schema10 on base570806f; root must renumber after tickets12/14.

`POST /budgets/{budget}/transfers` accepts client-generated `id`, `counterpart_id`,
`account_id`, `to_account_id`, signed integer `amount`, calendar `date`, optional
`notes`, `cleared`, `category_id`. Both rows, transfer payees, field clocks, account
balances, change events and durable receipt commit together. Counterpart starts
uncleared. Same-budget-status pairs have no category; crossing transfers retain
an applicable source category. Card payments between on-budget accounts create
no categorized income. Ordinary transaction/payee APIs cannot manufacture a pair.

`POST /transfers/link` accepts two existing IDs. Opposite amounts, distinct open
accounts, unlinked, unreconciled non-opening entries required. Dates, amounts,
notes and cleared states remain; both categories clear, matching pinned Actual.

Existing transaction PATCH/DELETE/duplicate endpoints recognize transfer pairs.
Amount/notes edits share the maximum field clock across both sides. Date and
cleared stay local, unless `sync_transfer_date` explicitly links date edits.
That boolean is saved with the edit as a shared preference; omitted uses the
saved preference, absent historical preference means false. Actual source
`sync-transfer-date` converts without losing its raw source provenance.

Deleting either side tombstones both, comparing the original deletion clock with
all fields on both rows. Superseded deletion is a durable terminal receipt.
Duplicating either side creates two fresh IDs and retains independent dates,
with cleared/reconciled reset. Reconciled pairs require unlocking before edits.

Account close accepts either the existing empty body, or a transfer body with
pair IDs, `to_account_id`, `date`, optional category and `expected_balance`.
A changed balance/newer history rejects the whole closure. Balance transfer and
closure share one receipt. Forced account deletion preserves the opposite
account's transaction and amount, clearing its payee/link, matching pinned Actual.

New capability names: `transfers`, `transfer_cancellation`,
`transfer_receipt_lookup`. Transfer creation/link honor the original immutable
method/path/body/key/clock/epoch, read-only receipt lookup and explicit cancel
fences. Existing account/transaction capabilities continue to own their routes.
No metadata writes enter the expense outbox. Plain HTTP, no app login.

The native archive registry already covers transactions/payees; schema10 adds
nullable transfer references, reciprocal active-pair validation, and defaults for
schema6–9 archives. Genuine Actual transfers convert their source references and
HLCs. A fresh upload intent rechecks previously blocked conversion plans; exact
old upload keys retain their original receipt. Recent and retained companion
queries exclude transfers before activation. Native state also excludes transfer
rows and transfer payee identities after genuine incremental updates.

## Integration gates

- Ticket12: use `t.transfer_id IS NOT NULL` for transfer predicates; preserve
  `transfer_id` in its batch row hydrator. Both signed legs belong in all-account
  register totals; they cancel. Ordinary rows omit absent transfer metadata.
- Ticket14: split children can be transfer legs; parent may not. Parent amount,
  all children, counterpart, field clocks and receipt require one atomic boundary.
  Ordinary-side deletion must detach a child without deleting its allocation.
  Date sync to a child must include its parent/siblings. Split replacement,
  unsplit/recombine and parent deletion must explicitly maintain paired legs.
  This isolated base does not contain ticket14: combined implementation and tests
  remain a root integration gate, not accepted parity.
- Ticket13: wrap the existing transaction/receipt boundary with one undo group,
  retaining both legs, generated payees, relevant parent/account state and versions.
  Do not expose per-leg undo or silently omit transfers. Undo is not implemented
  in this base; acceptance waits for the combined undo contract.
- Ticket16: PATCH accepts `confirm_reconciled`; strip it before transfer field
  validation and call `reconciliation::protect` for both legs. Disclose both
  protected legs in the editor. DELETE retains explicit local unlock requirements
  on both legs; it does not accept a confirmation bypass. Current guards reject
  either locked leg.

Pinned combined oracle reveals a defect: editing a counterpart from600 to700
changes its split child from−600 to−700 while the parent remains−1000 and another
child−400. The rewrite's explicit exact-total requirement takes precedence: root
integration must adjust parent by the delta atomically, documenting the divergence.
