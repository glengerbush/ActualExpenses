# Register queries and saved views — ticket 12

Candidate `384c6bf`, based on `e689d37`, 2026-11-08. Integration onto `3bef1aa` is in progress; acceptance remains pending. Register settings are schema 11 after payees/tags (9) and splits (10). No deployment, live data or version change.

## Delivered behavior

Account/all-account registers have bounded search, stable snapshot pagination, deterministic sorting, cleared/reconciled visibility, exact filtered totals and full chronological running balances. Configurable ordered columns include separate payment/deposit, category group and running balance. Saved compound AND/OR filters and per-register view state persist through the public settings API and native archives. Keyboard row navigation, plain HTTP, desktop/mobile and enlarged text are covered.

Settings use document revision compare-and-swap and a required budget epoch. Each saved request retains its exact body/key/epoch through response loss, reload and retry, with a unique browser storage key. Stale tabs cannot overwrite newer settings. Metadata is online-only; no metadata enters the offline expense outbox. This endpoint deliberately uses revision CAS, not transaction edit clocks; there is no LocalClockError/reset-clock branch. Authoritative rejection can reload settings; uncertain delivery keeps the original request.

Source conversion activates recognized transaction filters, account/global column preferences, hidden reconciled, category-group, balance and extra-balance preferences. Source files remain intact. Baseline hidden-cleared controls the cleared column, not cleared transaction visibility. Empty account preference falls back to the global preference. Invalid/unsupported source settings report a blocking conversion requirement; they are never silently discarded.

## Pinned evidence and domain boundaries

`tools/generate-register-query.mjs` executes published `@actual-app/api@26.10.0` against disposable data and freezes identifiers/totals in `tests/fixtures/register-query.json`. Cases cover AND/OR, regex, approximate/outflow and ranged amounts, month dates, exact-case tags, wildcard `?`, literal `_` and escaped `%`. The wildcard test first failed publicly, then passed after matching pinned `packages/loot-core/src/platform/server/sqlite/unicodeLike.ts`.

Recurrence conditions compute the first 100 raw occurrences with interval, month-day/weekday patterns, count/date ending, preserving source weekend options. Pinned filter recurrence does not apply schedule posting weekend shifts. Saved-filter references are UI selection resolved to stored conditions (`FiltersMenu.tsx:693–707`), not another SQL operator.

No ordinary register operator is intentionally deferred. Domain activation remains:

- Ticket 14, spec story 23: integration adds split parent/children matching, non-duplicated parent totals and bounded batch hydration. The whole compound predicate must match one parent or one child, matching the pinned grouped executor; separate children cannot independently satisfy an AND filter. Final integration evidence is pending below.
- Ticket 15, stories 24–25: linked transfer identity. `transfer` currently evaluates false on ordinary transactions.
- Ticket 25, stories 26/54–57 (and rules ticket 19): imported payee metadata. Pinned AQL schema maps `imported_payee` to `imported_description` (`packages/loot-core/src/server/aql/schema/index.ts:380`); that field is not among baseline register FilterMenu fields. Current converter already blocks imported/bank transaction metadata. Its original source/filter payload remains retained; semantic activation belongs with that converter.

Regex uses bounded ECMAScript UTF-16 code-unit execution; see the correction and exact limits below. Invalid filters return errors rather than silently dropping conditions. Requests permit 20 conditions, 100 membership values, 500 search characters, 1,000 rows/page and an offset of at most 1,000,000. Filter text is bounded to 1,000 bytes. These limits are public contract constraints.

## Resource behavior

Normal newest-first unfiltered pages use cached account financial totals, indexed count/page IDs and batched hydration. They do not scan history to recompute financial totals. Later offsets stream preceding amounts to maintain running balances. Exact filtered totals/arbitrary-order running balances stream server-side history with i128 arithmetic, retaining only requested rows; client never downloads history for totals. This filtered path is O(history) CPU and bounded page memory, not a claim of constant-time arbitrary predicates. Indexed SQL limits IDs before hydration; complex predicates/regex still scan.

Disposable public-HTTP probe: `python3 actualBudget/tools/register-query-scaling.py --history 10000`, debug build, eight 100-row reads per query, shared host while other checks ran. Latest captured results (`/tmp/ticket12-scaling-batched.json`):

| Query | Median / max ms | Mean process CPU ms | Response bytes |
| --- | --- | --- | --- |
| Unfiltered | 67.08 / 85.68 | 63.75 | 102863 |
| Search | 257.21 / 454.02 | 282.50 | 102855 |
| Regex | 328.59 / 528.19 | 312.50 | 102855 |

Process RSS/high-water: 26,372 KiB. A 1,200-row public test requested seven rows and returned 7,412 bytes with exact 600-match totals. Measurements are local debug observations, not production latency/energy guarantees. The final wildcard fix does not affect those three measured predicates.

## Validation

- Full Rust suite: 77 public integration tests pass; `cargo clippy --all-targets -- -D warnings`, `cargo fmt`, build pass.
- Web `npm run check`, build and full format check pass; Svelte autofixer on the three changed components reported no issues/suggestions.
- Full existing browser sequence plus new query/settings tests passed. Initial full run hit the known old 5-second claim-release assertion; applied root's 65-second timeout (lease lasts 60 seconds), then concurrency and all later suites passed. No behavior change to that existing test.
- `register-query.mjs`: pinned fixture parity, recurrence, malformed requests, snapshot pages, exact totals, CAS/replay/epoch, native restore and source conversion.
- `register-settings-browser.mjs`: desktop/mobile, keyboard, 200% text, persisted state, lost commit response, exact retry identity, stale-tab rejection and offline boundary.
- `uv run actualBudget/tools/contract-smoke.py`: served OpenAPI validates real responses.
- Pinned generator `--check` and `git diff --check` pass.

Impeccable context and incumbent PRODUCT/DESIGN Operate identity retained. Detector ran once: `[]` (`/tmp/ticket12-detector.json`). Desktop and mobile renders visually inspected. Fresh independent finish/standards/spec review is pending root coordination; this document does not self-certify it.

Captures: `actualBudget/web/test-results/register-settings-desktop.png` (1440px), `actualBudget/web/test-results/register-settings-mobile.png` (390px). Browser tests also exercise enlarged text without horizontal overflow. Test artifacts are not source changes.

## Independent review corrections

All five reported findings have dedicated public regressions in `web/tests/register-review-browser.mjs` and the published oracle fixture:

- Rejection recovery retains the exact rejected request snapshot. Clearing it cannot erase another tab's later uncertain operation. Reload fetches the authoritative revision while preserving the local filter/column draft. The test commits B's replacement request, drops its response, reloads A's older rejection, verifies B's stored bytes survive, then retries B's same key/body with one resulting revision.
- Amount/range controls use the existing BigInt-based money parser/formatter. Real sequential typing of `70368744177664.01`, filtered results, persistence and reload retain `7036874417766401` minor units. Stable condition objects prevent focus/input loss when updating a field.
- View keys include all-account scope (`all`, `onbudget`, etc.). Switching scopes restores that scope's query, columns and cutoff; saving one cannot overwrite another.
- Account/payee/category/category-group conditions use identity selectors, including null and membership values. Labels disambiguate duplicate categories by group, and duplicate names within one group by ID. The pinned import fixture now includes an account-ID filter; browser tests edit it and author category/group/null selections. Category-group references load on opening the filter panel, avoiding another request during ordinary unchanged-register startup.
- The initial ASCII word/digit/boundary translation was later replaced by the ECMAScript engine described below. Published fixtures include accented words and Arabic digits. Baseline note-condition construction lowercases the pattern itself (`server/rules/condition.ts:141`); the oracle preserves that normalization, including its effect on uppercase shorthand escapes.

A full Rust rerun also exposed a pre-existing nondeterministic boundary in the new all-account running balance: valid final totals can have chronological intermediate values outside safe JSON integer range. The new public case uses distinct dates to fix ordering and tests both normal and filtered paths. Such a row now returns null plus `running_balance_error: exact_money_range`; the browser displays “Outside exact range”, never zero. Streaming arithmetic stays i128; valid final totals/pages remain available. The API contract documents this response.

Review red logs: `/tmp/ticket12-review-{recovery,amount,scope,identity,regex}-red.log`, `/tmp/ticket12-running-boundary-red.log`. Targeted green logs: `/tmp/ticket12-review-browser-green.log`, `/tmp/ticket12-review-query-green.log`, `/tmp/ticket12-running-browser-green.log`. Additional captures: `actualBudget/web/test-results/register-identity-{desktop,mobile}.png`.

Final correction checks: all 77 Rust cases pass across the 76-case run and the isolated long-category-restore case. Two combined attempts hit that unchanged test client's 5-second restore timeout under shared load; no test timeout or assertions were changed here. Logs: `/tmp/ticket12-rust-refreeze-remaining.log`, `/tmp/ticket12-category-restore-isolated.log`. Final full browser sequence passed; log is `/tmp/ticket12-browser-refreeze.log`; served contract `/tmp/ticket12-contract-refreeze.log`; pinned oracle `/tmp/ticket12-oracle-refreeze-check.log`. Latest three-component Svelte autofixer outputs remain empty. Detector was not rerun. Independent root re-review remains required.

## ECMAScript / UTF-16 correction

The partial fancy-regex syntax translation is removed. Pinned public Actual fixtures exposed BOM/NEL whitespace, astral dot/quantifier and escaped-surrogate differences. `register_regex.rs` now owns compilation and budgeted execution using vendored `jlens-regress` 0.31.0; both pattern and input are UTF-16 code units with no `u`/`v` flags. LIKE uses the same engine with ECMAScript `i` behavior; tags retain their baseline case sensitivity.

Primary-source review: [upstream fork README](https://docs.rs/crate/jlens-regress/0.31.0/source/README.md), [interrupt implementation](https://docs.rs/crate/jlens-regress/0.31.0/source/src/interrupt.rs), [engine API](https://docs.rs/crate/jlens-regress/0.31.0/source/src/api.rs). The published interruptible API accepts UTF-8 only; stock UCS2 calls cannot be interrupted. Root approved a narrow adapter rather than an unbounded call or a Node runtime. The vendored code additionally fixes escaped surrogate coalescing behind an opt-in `Flags.ucs2`, leaving upstream UTF-8 compilation unchanged. `actualBudget/server/vendor/jlens-regress/ACTUALBUDGET.md` records archive checksum, exact upstream revision, licenses and the four-file `actualbudget.patch`. UCS2 execution additionally counts every VM instruction and precharges backreference comparison units; the UTF16 path reuses the existing start-anchor optimization. Poll interval and Unicode tables are unchanged. Features enabled: std, utf16, prohibit-unsafe; PikeVM/default features are disabled. Minimum declared Rust version is now 1.88; the pinned container already uses 1.99.1.

Deterministic limits:

- Compiled pattern: at most 2,048 UTF-16 units, 64 nested groups, 256 structural syntax tokens. Existing public condition text limit remains 1,000 bytes. Escaped punctuation/class contents do not count as groups. `no_opt=true` disables AST/loop expansion; the emitter uses an explicit stack and one loop instruction rather than expanding counted repetition. An enormous count such as `a{999999999}` is tested publicly without allocation proportional to the count.
- Input per invocation: at most 20,000 UTF-16 units (the existing 10,000-character notes maximum fits).
- A match stops at its tenth 1,024-step engine interruption poll (10,240 counted VM instructions, backtracking/loop steps and precharged backreference comparison units). Poll steps are not individual CPU instructions: work between them is bounded by the capped pattern/input.
- One register request shares 67,108,864 work units across all patterns and SQL passes. Each invocation charges 1,024 + pattern units + text units before matching, including matches too cheap to poll. Additional polls charge 1,024. This intentionally bounds filtered work on large histories; narrow account/date scope or simplify the query after rejection.
- Syntax, compilation and execution failures return explicit HTTP 400 `invalid_regex`, `regex_complexity_limit`, or `regex_work_limit`. SQLite erases typed scalar-function errors, so request-local failure state preserves the reason at the API boundary. No partial rows or totals are returned. Confirmed browser rows and the editable draft survive; a corrected query works immediately.

Pinned fixtures now include BOM/NEL/line separators, literal emoji, surrogate escapes, astral character classes, quantifiers, lookbehind, backreferences, Annex-B octal/identity escapes and case-insensitive LIKE. Public budget tests cover catastrophic backtracking, compilation bounds, 350 rows of repeated cheap matches exhausting the shared budget, no partial totals, and a valid query after failure. Logs: `/tmp/ticket12-utf16-red.log`, `/tmp/ticket12-utf16-instruction-green.log`, `/tmp/ticket12-regex-instruction-budget-green.log`, `/tmp/ticket12-regex-recovery-browser-final.log`. This supersedes the earlier fancy-regex limitation/translation notes and earlier regex performance measurements.

Independent review found long literal near-misses undercounted by upstream failure/loop polling: ten 10,000-character rows with a 999-character near-miss took 27.5 seconds. The additional VM-dispatch and backreference comparison accounting fixes that gap. The public regression now repeats that exact ten-row query, requires explicit rejection within three seconds, then verifies an ordinary pattern succeeds. This timing guard detects unbounded execution; deterministic accounting remains the primary limit.


The 10,000-row ECMAScript probe completed **before final VM-dispatch/comparison accounting**: median/max/CPU ms were unfiltered 108.14/131.14/105.0, search 284.97/482.47/320.0, regex 265.81/346.26/270.0; RSS/HWM 21,916 KiB, page responses 102,863/102,855 bytes (`/tmp/ticket12-ecma-scaling.json`). These are historical local debug observations, not final-patch performance claims. Root requested final performance qualification after integration rather than another private 10k reseed.

Final resource correction validation: new-build public ten-row long-prefix rejection and recovery, pathological backtracking, compilation limits and request-wide cheap-match exhaustion pass in `/tmp/ticket12-regex-instruction-budget-green.log`; full pinned/public register suite passes in `/tmp/ticket12-utf16-instruction-green.log`. Final Clippy log `/tmp/ticket12-ecma-clippy-final.log`. MSRV advancement enabled nine mechanical let-chain lints, fixed in api (1), categories (4), concurrency (1), ledger (1), register_settings (2); preserve newer domain edits when integrating these syntax-only changes.

Independent engine corrective review passed `384c6bf`: the previous long-prefix reproducer now returns explicit rejection in 7.3 ms; a backreference probe rejects in 4.7 ms and subsequent valid requests work. Vendor provenance, bounds and patch were checked. Independent Spec review found nullable identity `ne` incorrectly excluded null values; root integration must correct it and verify pinned category/payee/group cases before acceptance.
