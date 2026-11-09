# Genuine pre-lifecycle server fixture

`server-v5.sqlite3` was created by the accepted server at commit
`8fe9b38444b98626f5e7670d228b2213ba3d376c`, built from `git archive` in an isolated
directory. All input went through its public HTTP API. The database was copied
after graceful shutdown; no SQL writes or schema-version downgrades created it.

SHA-256: `f80b28ca1a2ddbdc62cab4c6393b614fb076bf507bccc86ef309dcd2249f41b9`.
Size:290,816 bytes. The accompanying JSON records 14 requests and byte-exact
responses plus 11 final public snapshots:

- Two native budgets, an account/opening balance and ordinary payee/category.
- Identical idempotency keys deliberately reused across distinct resource and
  budget scopes, proving that key alone cannot identify receipt ownership.
- Shared preferences, clocked create/PATCH/duplicate/DELETE and a canceled
  operation whose late original delivery replays the cancellation fence.
- The existing independent Actual 26.10.0 ZIP staged and activated through the
  accepted importer, retaining its original archive and provenance.

The native account ends with its opening entry and one expense, total12,266
minor units. The imported budget retains the independent fixture's balances.
Server-generated clocks, cursor epochs and import-stage identities are captured
observations; regenerating intentionally changes those values and the checksum.

Generate: `python3 actualBudget/tools/generate-v5-receipts.py`.
Verify committed input: `python3 actualBudget/tools/generate-v5-receipts.py --check`.
Both passed. Check mode verifies the checksum, opens a temporary copy with the
same accepted binary, and checks every exact receipt and final snapshot without
rewriting the committed fixture. ZIP request bytes are represented as base64;
ordinary requests retain method/path/key/body/clock and cancellation intent.

Ticket08 must consume this input through the new server's public API to verify
schema6 migration, correct budget ownership and replay preservation. The fixture
itself is baseline evidence, not a claim that the lifecycle upgrade is complete.
