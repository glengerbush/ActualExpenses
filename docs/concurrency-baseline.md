# Independent concurrency baseline

`node actualBudget/tools/generate-concurrency.mjs --check` creates a disposable
loopback Actual 26.10.0 sync server and budget. It sends fixed historical message
envelopes through the real public sync endpoint, then lets the published Actual
API synchronize them. Final visible transactions come from a fresh public import
of the exported budget; a read-only inspection of that public export records
otherwise inaccessible hidden fields and per-cell clocks. No replacement code
calculates the expected results. Generation and an independent check both pass.

The committed [fixture](../actualBudget/tests/fixtures/concurrency.json) covers:

- Late independent fields surviving while an older same-field edit loses.
- Deterministic node ordering for equal physical time/counter.
- Logical counter ordering before node ordering.
- Repeated identical sync message delivery.
- A published tombstone remaining invisible after a newer ordinary field edit.
- An older independent tombstone arriving after a newer notes edit.

Two boundaries matter. Actual retains the later ordinary field inside a deleted
row; preserving visibility alone would miss that state. Conversely, raw Actual
accepts the older independent tombstone in the last case. The approved
replacement deliberately adds a pre-publication deletion guard and must reject
that action, retaining the newer work. The fixture labels this policy exception;
it must not be treated as blanket server-wins or an ordinary field-order change.

Rows seeded through public `runImport` have no per-field history. The fixture's
initial zero clock describes a deterministic floor for replacement setup, not
an invented original source clock. All observed changed-field clocks are exact.

The locked CRDT package used by Actual's API additionally supplies HLC cases for
same-millisecond sends, clock regression, five-minute drift boundary, and counter
overflow. Its physical clock is temporarily frozen only during these standalone
clock observations. A lower-level seam is necessary here because the server's
public API does not expose its internal physical-clock generator. Protocol
behavior and financial state still use running public endpoints/export artifacts.

Generator fixtures are reproducible despite native random budget/account IDs:
only fixed transaction IDs, field results and ordering appear in the output.
The pinned timestamp source hash is recorded, and the public API/server package
versions are asserted. No existing server URL, credential or budget is accepted;
all disposable server/client directories are removed on completion.

This is baseline evidence for ticket05. Replacement HTTP/browser parity and
resource qualification remain required before accepting that ticket.
