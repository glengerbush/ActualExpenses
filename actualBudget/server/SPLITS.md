# Split allocation editing

`POST /api/v1/budgets/{budget}/transactions/{id}/splits` edits one complete
financial aggregate atomically. Full replacement (`children`) remains supported.
Sparse replacement sends `changes: { upsert: [...], remove: [...] }` instead:

- `upsert` contains complete child values: ID, signed integer amount, optional
  payee/category, notes. A matching ID retains its position; a new ID appends in
  request order. Unmentioned children preserve their fields and positions.
- `remove` contains existing child IDs. Unknown, repeated, or overlapping
  upsert/remove IDs reject the whole operation.
- Sparse changes require `replace`, empty/absent `children` and `detach`.
  All replacement children are assembled before exact/even/proportional/auto
  allocation. Only the final complete sum must equal the parent total.
- Parent edits, allocation changes, reference validation, reconciliation proof,
  field versions, balance updates, and the receipt commit together. Selected
  detach and duplication operate on the same full aggregate.

The importer accepts 100000 source rows per table and a 64 MiB expanded archive.
Split editing therefore allows 100000 live children and a **64 MiB JSON request**
on this route only. Ordinary JSON routes retain their 64 KiB limit. Per-child
notes remain limited to 10000 characters. Oversized requests return 413; excess
child counts return 400. Neither imports nor allocations are truncated. Sparse
edits permit changing any part of an accepted aggregate without retransmitting
its unchanged notes. These bounds do not promise an arbitrary number of newly
entered note characters in one operation; reduce the changed set if it exceeds
64 MiB. Redistribution itself needs no repeated child list.

The browser renders 20 children at a time, provides previous/next and direct
split-number navigation, and retains edits across pages. New requests contain
only changed/new/removed children. Existing pending requests retain their exact
historical payload. Drafts remain in IndexedDB; storage failure keeps the normal
recovery state. Reads, drafts, and receipts still contain the full aggregate and
use memory proportional to its size; rendering pagination does not make these
constant-size. A browser storage quota can still require draft recovery.

Retries preserve snapshot, idempotency key, edit clock, epoch and captured
reconciliation token. They never rebase a protection proof. Cancellation records
an immutable outcome before any edit, or returns the already-committed receipt.
The new optional sparse field is omitted when absent from normalized payloads,
preserving preexisting native archive receipts.

Hydration uses the shared ledger batches; SQL statements use at most 501 bound
parameters. Version/feed writes are batched with a distinct sequence per member.
Membership checks and redistribution avoid quadratic scans, reference resolution
is cached per unique reference, and integrity checks use one joined scan with
128-bit sums. Prepared row writes preserve each child's exact data.

Evidence: public HTTP import/edit/replay/detach/duplicate/protection/cancellation/
restart/native-restore/epoch tests use pinned Actual 26.10.0 101/1001-child
archives. Executed pinned Actual allocation callbacks cover even and proportional
remainders at those sizes. The 1001-child complete replacement exceeds 64 KiB;
the browser's single-child sparse request is below 1 KiB and survives lost
acknowledgement plus reload. Browser coverage includes final-child access,
add/distribute, keyboard navigation, desktop and 390px mobile layout. The
100000-child ceiling is a resource contract, not a measured stress-test result.
