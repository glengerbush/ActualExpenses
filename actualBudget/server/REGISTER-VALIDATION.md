# Register validation — tickets 03 and shared 43 integration

Validated 2026-11-08, candidate based on accepted ticket02 `4600a70`.
No live budget, server deployment, release version or Android identity changed.

## Public behavior

`tests/http.rs` runs 16 real-process HTTP tests, including all nine ticket02
checks. `tests/preferences.rs` adds five ticket43 HTTP tests and
`tests/register_snapshot.rs` adds two concurrent-pagination checks. All 23 passed,
zero ignored/skipped. No assertion reads internal SQLite state. The schema1
SQLite file is an input fixture generated through the old public HTTP API.

New coverage: income/expense/refund; exact total/cleared/uncleared and account
running balances; explicit date cutoffs and future entries; dirty-field PATCH;
duplicate status reset; deletion/tombstones; reference creation/selection/clear;
atomic inline-reference rollback; safe-range overflow; restart and replay after
each fixture operation; and pre-upgrade backup durability.

Red-to-green observations were made at the HTTP seam: create 405→201,
edit/reference/calculator 404→successful observable results. Browser first failed
against the ticket02 build because Add transaction did not exist, then exercised
the implemented flow. The 200% text check found an overflowing register heading;
wrapping the action fixes that layout.

```sh
cargo test --locked --manifest-path actualBudget/server/Cargo.toml
cargo clippy --locked --manifest-path actualBudget/server/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path actualBudget/server/Cargo.toml --check
```

## Independent financial expectations

All three committed oracle generators passed `--check` against pinned Actual
26.10.0 (`2bebdbaae0b701120a8cad9d56aafd64fa04c577`):

```sh
node actualBudget/tools/generate-opening-balances.mjs --check
node actualBudget/tools/generate-register.mjs --check
node actualBudget/tools/generate-calculator.mjs --check
```

Opening balances cover seven on/off-budget, zero, signed and one-cent cases.
Register snapshots cover creation, edit, duplication and refund deletion, with
whole normalized rows, all-date totals, cutoff balances, and cleared state.
The generator waits for public query convergence after writes; random identities
are normalized and fixed IDs stabilize same-date ordering. Calculator cases
preserve Actual binary64 rounding, negative ties, exponent order, trailing
prefix behavior and formatted inputs. Temporary engine upload failures are
expected: this local oracle has no remote sync credentials and checks local
public results only.

## Upgrade and rollback

`python3 actualBudget/tools/qualify-register-upgrade.py` compiled the genuine
`4600a70` server in a disposable checkout. It created a -12345 account through
HTTP, reopened the data with the new server, replayed the original operation,
and wrote +345 for -12000. The generated pre-upgrade backup opened successfully
with the original old binary and returned -12345. All temporary data/builds were
removed. `tests/fixtures/server-v1.json` records provenance and SHA256 of the
artificial schema1 input fixture. The regular HTTP suite also checks upgrade,
restart, original outcome replay, and the independently restored backup.

## Resource behavior

`python3 actualBudget/tools/register-scaling.py` uses public HTTP against a
disposable debug server. Each phase adds 5,000 entries, first to an unrelated
account, then to the measured account. Exact final balance is asserted.
Observed median / p95 milliseconds on this shared development host:

| Operation | Initial | +5,000 unrelated | +5,000 same account |
| --- | ---: | ---: | ---: |
| Latest-date append | 2.784 / 3.140 | 2.428 / 2.777 | 2.816 / 3.844 |
| Newest page | 9.078 / 10.297 | 7.336 / 8.250 | 10.826 / 14.610 |
| Notes-only edit | 2.703 / 3.076 | 2.282 / 2.570 | 2.580 / 3.305 |

These are comparative development observations, not production latency promises.
Common appends, newest-page reads and metadata-only changes avoid scanning the
ledger. Backdated financial changes deliberately revalidate affected account
prefixes; explicit historical cutoffs and later offset pages need more rows.
Ticket51 owns broader production profiling.

## Browser and visual evidence

`actualBudget/web/tests/register.mjs` drives the built app and a real local server:
CRUD, calculator, inline references, cleared state, draft reload, lost create/edit/
duplicate/delete acknowledgements, stable retry keys, storage-write failure,
cross-tab pending protection, malformed saved-action recovery, currency/NBSP/
apostrophe normalization and notes-only edits preserving hidden fractional money.
Date filtering uses the same formatted keyboard/calendar control as transactions.
The final register browser and all five existing ticket02 browser groups passed.
Desktop 1440px, mobile 390px, and 390px with 200% root text were inspected; register
and form stay within the viewport. Sidebar navigation scrolls within its own region.

Screenshots are generated under ignored `actualBudget/web/test-results/`:
`register-desktop.png`, `register-mobile.png`, `register-form.png`,
`register-mobile-large-text.png`, and `register-form-large-text.png`.
Svelte diagnostics report no errors or warnings; both components' autofixer
reports are empty, and Impeccable detector reports no findings. The UI follows
the existing Operate surface, theme variables, visible labels and keyboard focus.

The parent separately validated published OpenAPI schemas against real responses
using `actualBudget/tools/contract-smoke.py`, including stable and invalidated
pagination snapshots. Pages use a consistent read transaction; a concurrent
write causes continuation to return `409 register_changed`, then the browser
reloads a coherent first page. Failed date-cutoff reads retain the confirmed
label, rows and totals. Pending transaction requests remain recoverable without
cached preferences, while financial displays stay hidden until formatting is known.

The final real container qualification at `0b66dc6` passed all creation, register,
date, recovery and preference browser suites, including 220 formatting and 120
date-input oracle cases, plain HTTP and replacement-container persistence/replay.
Image: `sha256:7fc8a8aa7b3dd0916291a3aaec3011ef33112b14a88cc907a8ec9045d431652d`,
124,191,107 bytes, user `10001:10001`. Disposable test resources were cleaned.
The final eight restored-date browser flows cover account/transaction direct
submission and exact pending replay across year changes. Independent Standards
and Spec reviews both report zero remaining findings. Tickets 03 and 43 accepted.
