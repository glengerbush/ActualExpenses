# Ticket 07 — cached views and durable expense delivery

Public browser tests run the real Rust binary and built static app at insecure `http://actualbudget.test`, using disposable server data and browser storage. No service worker, HTTPS, SharedArrayBuffer, or login is required.

## Observable behavior

Catalog, budget references/preferences, and bounded register rows/totals are cached in IndexedDB. They render before network responses, with their saved timestamp. Pending amounts stay separate from confirmed balances. Initial references use a checkpoint and guarded reads; later checks replay bounded changes. Unknown change kinds force a guarded bootstrap. Unchanged checkpoints cause no reference download, cache write, or fabricated freshness timestamp.

An expense is journaled before local success or network mutation. Offline ordinary amounts use captured formatting; calculator expressions require a connection. Confirmed rows cannot begin editing offline. Edits begun online can finish offline. Offline-created pending expenses can be edited/deleted.

Every attempted request retains body, entity ID, idempotency key, clock, and budget epoch. Atomic claims coordinate tabs; delivery is at least once, with one financial effect enforced by server receipts. Queue changes, reconnect, foreground and explicit retry cause delivery; empty queues cause no delivery requests. Claims recover after interrupted tabs. Blocked attempts stay visible; safe cancellation recovers a committed receipt or fences delayed originals. Restore conflicts never silently replace a captured epoch.

Storage failures prevent sending unjournaled requests. Existing creation/transaction drafts remain recoverable; pre-outbox pending records are adopted without inventing a clock or key. Plain HTTP supports loaded-page capture. Cold offline navigation and OS background execution are not promised; reopening resumes pending work after a stopped process.

## Verification

Build server/web, then from `actualBudget/web` run:

```
node tests/offline-browser.mjs
node tests/offline-epoch-browser.mjs
node tests/concurrency-browser.mjs
node tests/register.mjs
node tests/register-recovery.mjs
node tests/date-recovery-browser.mjs
```

Normal `npm run test:browser` composes these with prior suites and lifecycle/generation acceptance. Each suite owns an isolated server; explicit `ACTUALBUDGET_TEST_URL` overrides are only for disposable container tests.

Offline cases: capture/edit/delete, backdated pending rows, cached restart, lost committed response, concurrent-tab exact replay, quota before POST, interrupted online edit/newer remote winner, cache-before-response, unchanged checkpoint zero transfers/writes, unknown-change bootstrap, genuine legacy pending adoption, keyboard controls and 320px/200% text. Epoch cases: pre-restore queue, exact retry, cancellation fence, fresh replacement, draft opened before restore.

Ignored screenshots include `actualBudget/web/test-results/offline-pending-mobile-200.png` plus concurrency/register captures.

## Existing scenario mapping

- `browser.mjs`: creation/lost account acknowledgement, stored identities, two tabs and quota coverage remain. Failed bootstrap intercepts checkpoint GET; account POST interception stays unchanged.
- `register.mjs`: old form/action retry controls become the pending panel or automatic foreground receipt recovery. CRUD, totals, date/amount formatting, duplicate/delete identities, malformed legacy recovery and stale-tab protection remain.
- `register-recovery.mjs`: missing-preference recovery removes both caches; original pending requests still resolve while new financial entry is blocked. Snapshot pagination and failed-cutoff preservation remain.
- `date-recovery-browser.mjs`: transaction replay after a year change is automatic foreground delivery. Captured date/body/key checks remain; creation recovery stays manual.
- `concurrency-browser.mjs`: controlled faults attach to the context when any tab may claim delivery. Additional identical retries are allowed; exact request identities and final financial results remain asserted. Delayed rejection, hidden-row feedback, clock correction, lost cancellation requests/acknowledgements, cancel-before-commit versus receipt-after-commit, capability gating and latent category preservation remain.
