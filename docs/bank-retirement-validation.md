# External-bank retirement29 — WIP

User removed external bank/account connections from scope. Manual accounts,
transactions, file imports, Android and the server's own public API remain.
Source/provider witnesses and immutable23/24 fixtures remain historical evidence,
not active connector support or parity claims.

## Backend checkpoint

From reviewed3ec8dfb, schema26 drops all operational provider metadata, private
credentials, captured jobs and quota counters. Historical23–25 DDL remains only
for genuine database upgrade provenance. The entire provider runtime/router,
network normalizers/claim/consent code and operational capability are removed.
Reqwest/base64 remain test-only dependencies; the server no longer depends on
outbound HTTP transport. Manual file parsing/matching/rules retain one writer.

Legacy native archives discard retired connection domains and their field
clocks, reporting the retired configuration count. Original upload/recovery bytes
are retained unchanged. Actual imports make manual accounts with nonblocking
retirement disclosure; historical transaction import identifiers/raw fields stay
intact. Old history inverses skip retired provider-origin metadata while keeping
the original financial actions and ownership. No credentials are contacted.

Active custody is purged. Original immutable fixtures/uploads and pre-upgrade
recovery copies may retain original historical bytes; they are inert recovery
artifacts, not operational connections, and are intentionally not rewritten.

Public red `/tmp/bank-retirement-red.log` proves the old advertised banking
capability violates removal. Green `/tmp/bank-retirement-backend-green.log`
compares genuine24 DB and native archives against original full financial,
reference/status/import fields and clocks. All old owned undo actions drain and
redo to the exact original financial graph (fresh inverse clocks are expected).
Fixture SHA256 checks precede both modes. Retired GET/POST surfaces return404
without any checkpoint effect. Original fixture files are untouched.

Focused file-import selection/order5 cases and rule/import5 cases PASS
`/tmp/bank-retirement-file-import-{edges,rules}.log`. Build/bin Clippy PASS
`/tmp/bank-retirement-final-backend-build.log` and
`/tmp/bank-retirement-clippy.log`. Served maintained contracts PASS
`/tmp/bank-retirement-contract.log`; SDK static map PASS
`/tmp/bank-retirement-sdk-map.log`, with runBankSync explicitly removed.

Not complete: web surface/draft navigation cleanup, native representative manual
import/feed check, maintained command/CI/documentation cleanup, fresh Actual
source-import retirement disclosure and portable manual-only export proof.
No broad suite, container qualification, real provider or deployment run.
