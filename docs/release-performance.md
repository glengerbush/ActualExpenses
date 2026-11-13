# actualBudget release performance

## Baseline and acceptance

The last successful sequential release, [43086429153](https://github.com/glengerbush/ActualExpenses/actions/runs/43086429153), took about 21 minutes: source qualification 573 seconds, production image build 179 seconds, browser qualification 470 seconds. The target is under five minutes with warm caches on the existing GitHub-hosted Ubuntu runners. Cold caches and runner queues remain variable.

## Qualification graph

After release planning, Rust/web checks, four source-check shards and the production image build run independently. Eight browser shards test that single image. Publication waits for every job to succeed and loads the same checksummed image artifact. It does not rebuild. Signing, verification and manual Coolify deployment are unchanged. Android's independent workflow is unchanged.

All 65 source/financial commands and the original 105 browser/API checks remain. The concurrency script runs its transaction and action recovery scenarios independently, producing 106 browser commands. Both retain the shared two-client checks; the real 60-second recovery leases and all financial assertions remain intact. Rust formatting, Clippy, all Rust tests, Svelte checking, formatting and the web build remain mandatory. Shards use recorded hosted durations; each command runs exactly once. A failed command does not prevent the remaining commands in that shard from running, but fails the shard and blocks publication.

Each browser shard proves real container-replacement persistence, then each command gets a fresh container and database volume. Stable imported fixture identities cannot leak between commands when their order changes. Recovery fault injection needs to stop and reopen copied databases; it uses the executable and web assets extracted from the very same image, rather than requiring a separately compiled debug server.

Compiler caches are keyed by toolchain, dependency lock and source content. Docker layer caching is supplemented by explicit Cargo cache-mount preservation. An exact Cargo image cache hit skips redundant extraction. Chromium and npm download caches are retained; locked installs and browser launch verification still run.

## Verification

- Release planning tests and shard partition/failure propagation tests pass; actionlint passes.
- The original 65 expanded source commands were compared against the old workflow without omissions.
- Recovery's copied-database browser scenario passes locally using the published image executable.
- Safari import recovery now uses a unique folder-wrapped archive and unique retained-operation keys, so earlier tests cannot contaminate its assertions. Original archive bytes and financial expectations are unchanged. It passed twice consecutively on a prepopulated shared test server.
- Split draft recovery waits for the existing visible saved confirmation before reloading, then explicitly asserts the restored notes and the final server value. Previously it could reload during the asynchronous draft write. Financial expectations are unchanged.
- Transaction-file mapping recovery similarly waits for the existing enabled Close control (all draft writes complete) before reloading, then asserts the retained Notes mapping. Svelte analysis confirmed the save boundaries; no application behavior changed.
- The container runner passed the register-query/import fixture command twice consecutively with fresh databases, followed by transaction-file browser recovery. This directly exercises isolation of stable archive identities.
- The first cold-cache run, [43368443771](https://github.com/glengerbush/ActualExpenses/actions/runs/43368443771), took 6m59s but failed recovery because the old test helper required a debug executable. All source checks and 104 other browser commands passed. This is a failed diagnostic run, not an accepted release benchmark.
- Intermediate warm runs exposed a formatting violation and the split-draft test race; publication remained blocked. All 105 browser checks passed in the first warm run, but its formatting failure and cancellation mean it is not a successful benchmark. The full concurrency command varied from 91 to 150 seconds because interrupted tabs legitimately retain delivery claims; separating its two recovery scenarios avoids putting both lease waits on one runner.

Timing artifacts are retained for seven days. To rebalance, update `tools/qualification-timings.json` using successful command timings and verify partition tests. Do not remove checks or weaken assertions to meet a timing target.
