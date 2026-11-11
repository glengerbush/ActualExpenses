# Final parity audit — in progress

Ticket52 remains open. Production application revision229be58; no product
changes, release or live deployment in this audit. Preserve accepted feature
evidence and distinguish it from final whole-system qualification.

## Inventory reconciliation

At9d7f99f, [the ledger](parity-ledger.md) contains all107 ordered unique story IDs
and172 existing evidence links. Stale placeholders for23/24,35–42,47 and50 now
link their accepted source/fixture/public-outcome evidence. Provider exceptions
remain explicit;51–53 gates remain open. This checks inventory integrity, not
every linked financial expectation afresh.

`node actualBudget/tools/check-public-api-map.mjs --static` passes:67 pinned SDK
functions and18 query resources explicitly mapped, with pinned source hash.
The existing [caller migration guide](PUBLIC-API.md) includes typed queries,
immutable batch commands, receipt lookup/cancellation and staged manual imports.
Its standard-library Python caller already has running public-contract evidence
in [ticket47](public-query-validation.md). Final composition review remains open.

## Anonymous production API witness

Read-only requests against the disposable release container used for opening
qualification, image manifest
`sha256:e400473e4eaa0b079edc496196e9996549bb5dde2e44113be3faf828ec51a22e`.
Requests supplied no credentials, cookies or authorization headers. Every
response lacked Set-Cookie and WWW-Authenticate.

| Request | Status |
| --- | ---: |
| GET /api/v1/info |200|
| GET /api/v1/openapi.json |200|
| GET /api/v1/budgets |200|
| POST /account/login |405|
| POST /account/bootstrap |405|
| POST /api/v1/auth/login |404|
| GET /api/v1/session |404|

The served119-path contract has no security schemes, global security requirement
or operation security requirements. Static runtime-source searches found no
password/OAuth/JWT/login implementation; `session` matches concern reconciliation,
and the frontend broad-pattern match was the word “reassigning” in a comment.
Pinned reference code and Android's retained old-server adapter are separate
from the replacement web/server runtime. This witness verifies the sampled
production API surface; it does not claim a fresh complete browser/Android suite.

Remaining: reconcile every story and retained experiment against its complete
source/fixture/outcome obligations, review caller migration/deviations, then run
the final consolidated acceptance checks. Resource51 and cutover53 remain open.
