# Resource qualification — in progress

Ticket51 is **not accepted**. This note records the production browser comparison
and its limits. Opening, expense and report distributions are recorded below.
Restore distributions, large graph/history/guard
stress, Android resource observations and the consolidated accessibility audit
remain outstanding. Existing functional evidence is not relabeled as performance
evidence. Nothing here authorizes release or production cutover.

## Measured boundary

Application revision `229be58`; no product changes in this qualification slice.
Both apps run as production Docker containers on the same host, with separate
anonymous `/data` volumes and loopback-only published port5006. Neither uses live
budget data. The replacement image was built through `actualBudget/Dockerfile`;
the baseline is `actualbudget/actual-server:26.10.0`, digest
`sha256:24645e971da6bb1a1f953d6859e307a0b29be1e8b4130a35b3220c209b5b860c`.
The replacement image manifest digest is
`sha256:e400473e4eaa0b079edc496196e9996549bb5dde2e44113be3faf828ec51a22e`.
Results also record Docker's image config ID, distinct from the manifest digest.

Host: Intel i7-1370P,20 logical CPUs,66,972,958,720 bytes RAM,
Linux7.2.8-9-omarchy. Headless system Chromium through Playwright1.65.0;
browser version is recorded in the result. No CPU/memory limit, network shaping,
OS cache eviction or machine-wide isolation. Runs are sequential; no concurrent
build or other benchmark. These are instrumented development-host observations,
not forecasts for the user's phone, LAN/tailnet or Coolify server. No wattage or
battery-saving claim.

Each app receives the **same exported archive** for each case:

| Case | Public ledger rows | Dates | Contents |
| --- | ---: | --- | --- |
| empty |0|none|One manually created Checking account, zero opening balance|
| typical |48|2025-11-12–2027-01-05|Small, feature-rich migration fixture; not a population estimate of a typical household|
| large |10048|2017-01-01–2027-01-05|Same feature fixture plus10000 purchases spanning ten years|

Source financial rows follow Actual's default split representation: excluded
parent rows explain the49/10049 raw-storage counts in migration evidence.
`prepare-resource-budgets.mjs` uses the pinned official engine to import,
duplicate, rename and export fixtures; it changes the large purchase dates
through `transactions-batch-update`, never by editing a database. Export hashes,
source fixture hashes, source public-ledger hash/count/date range are retained.
`prepare-resource-browsers.mjs` imports those exact bytes into the replacement
and verifies account balances against the independent migration manifest.

## Opening protocol

Twenty independent browser profiles per app/case. Each profile lives on the
host's disk-backed Btrfs workspace, not RAM-backed `/tmp`. The baseline profile
receives authentication storage captured **before opening any budget**; the
replacement needs no credentials. Browser launch/auth setup are outside timing.
Both servers remain running, including allocator/cache state after import.

1. Cold: navigate to the chooser, select the budget and Checking account, await
   the ready register. Browser assets and financial caches initially absent;
   this does not mean a cold OS page cache or a restarted server.
2. Warm: reload the same persistent profile and await its ready register.
3. Unchanged: click Server Sync / Refresh transactions, await its successful
   response and the ready register. Preserve normal browser persistence.
4. On the last profile, observe35s without interaction.

UI-ready time is recorded separately from settled time. Counters continue until
observed requests finish and150ms of quiet elapses, so a trailing checkpoint
does not get charged to the next phase. This fixed quiet window is not included
in reported UI latency. Quantiles use the nearest-rank definition; p95 of20
observations is the19th sorted value, not a confidence interval.

Actual limits all requests, including static assets, to500/minute per IP. A
cold open needs about260 requests. Each fresh benchmark client has a distinct
`198.18.x.x` X-Forwarded-For identity, honored by the baseline's existing private
Docker proxy trust configuration. The limiter remains enabled; its allowance is
checked outside timing and runs pause if insufficient. Both apps receive the
same header. This measures individual-client cost, **not** repeated cold-open
throughput behind one NAT/proxy. Unexpected application429/other errors fail.

Chromium DNS is restricted to loopback for both variants. Baseline optional
GitHub news/update requests therefore fail and are recorded. Internet-connected
costs are outside this LAN-only comparison. Loopback permits the baseline's
SharedArrayBuffer requirement; separate native plain-HTTP tests own operation
on an untrusted hostname without that capability.

## Counters and caveats

- Linux `/proc` CPU/read/write counters cover the owned browser process tree.
  Thread-specific child lists are traversed; Chromium can rewrite argv into one
  process-title string. PID plus start time prevents process-reuse mistakes.
- Sampling is250ms. Short-lived processes between samples can be missed. The
  sampler itself consumes host resources; it is not added to application CPU.
- Summed RSS double-counts shared pages and is explicitly labeled RSS. PSS is
  reported only when every sampled process permits reading `smaps_rollup`;
  missing coverage remains null, never an invented physical-memory total.
- Server CPU and memory use its dedicated cgroup v2. Memory includes anonymous
  pages, filesystem cache and kernel accounting; it is not just the Rust/Node
  heap. Post-import retention is part of this run and requires investigation.
- Kernel `eth0` counters in each isolated container's network namespace capture
  actual transfer bytes, including transport overhead. These are authoritative
  for LAN traffic. Browser request inventories are supplemental: Chromium omits
  the SharedWorker entry script's `requestfinished` event. Its exact path is
  retained as unobserved completion; all other unsettled requests fail the run.
- Cgroup I/O stays separated by block device. Physical and device-mapper layers
  may account the same write: do not sum them. Browser write counters include
  Chromium profile/cache writes and delayed flushes. Idle browser bytes alone do
  not prove application database rewrites; origin-specific follow-up is needed.
- Counters cover an observation window, not power draw. Browser shutdown and
  any flush after that window are excluded. Background CPU is reported even
  when HTTP request count is zero.

## Reproduction

Prerequisites: built production image, pinned baseline image, Docker access,
Linux cgroup v2/procfs, installed web/compatibility dependencies and Chromium.
Choose new disposable names and an unused workspace results directory. Bind
ports only to loopback; explicitly mount an anonymous volume on **both** images
(the upstream image does not declare one automatically).

```sh
docker run -d --name actualbudget-q51-baseline-example \
  -p 127.0.0.1::5006 -v /data actualbudget/actual-server:26.10.0
docker run -d --name actualbudget-q51-native-example \
  -p 127.0.0.1::5006 -v /data actualbudget:q51-229be58
docker port actualbudget-q51-baseline-example 5006
# Wait for baseline /health and replacement /api/v1/info to succeed.
# Replace BASELINE_PORT with the printed port; it is not a deployment hostname.
node actualBudget/tools/prepare-resource-budgets.mjs \
  http://127.0.0.1:BASELINE_PORT .scratch/resource-example
node actualBudget/tools/prepare-resource-browsers.mjs \
  .scratch/resource-example actualbudget-q51-baseline-example \
  actualbudget-q51-native-example
node actualBudget/tools/measure-browser-openings.mjs \
  .scratch/resource-example native .scratch/resource-example/native-openings.json 20
node actualBudget/tools/measure-browser-openings.mjs \
  .scratch/resource-example baseline .scratch/resource-example/baseline-openings.json 20
# Only after collecting results:
docker rm -fv actualbudget-q51-baseline-example actualbudget-q51-native-example
```

Preparation refuses existing output directories/populated servers. Measurement
refuses an existing result file. Authentication files contain **disposable lab**
credentials and are mode0600; do not publish them. Keep result files and exact
fixture hashes, not credential files. Docker inspection validates the named lab
containers, loopback ports, volume mount and matching cgroup.

If a later attempt fails, `--completed-from=PRIOR_RESULT_JSON` can retain only
whole profiles whose cold, warm and unchanged observations all completed. Variant,
image, hardware, sample count, method and archive hashes must match. The new
result records the original file's hash, retained profile counts and prior
errors. An incomplete profile restarts in full; results are never selected by
latency. Report failure counts alongside timings of successful starts.

## Excluded pilots

Early pilots discovered and corrected measurement defects before final runs:
incognito profiles, RAM-backed persistent profiles, missed browser PID due to
rewritten argv, SharedWorker missing completion, cross-phase checkpoint traffic,
baseline rate limiting and optional external news requests. A source-fixture
setup also initially used an unmounted writable container layer; both final
containers have real `/data` volumes. Pilot figures are not final measurements.

The recorded baseline run required two resumptions: a sync began during the
final process-counter sample, triggering the boundary assertion; the sampler
now drains and resamples late requests. The empty case had already completed
all20 profiles and remains valid. Later, typical profile12 displayed upstream's
“Fatal Error / There was a problem loading the app in this browser version.”
Its underlying stack was not captured before cleanup, so its cause remains
unknown. That attempt is reported separately, not converted into a successful
latency. Subsequent failures capture console/page errors and expanded error text.
Native completed all60 profiles without either failure. Successful timings must
not be interpreted as an unconditional startup reliability result.

Production build/check succeeded. Main JS is1110.94KB uncompressed,
312.76KB gzip; Vite's large-chunk advisory remains visible. No speculative bundle
change was made before measuring actual opening behavior.

## Recorded opening results

[Machine-readable summaries](resource-results-229be58/summary.json), compressed
[baseline observations](resource-results-229be58/baseline-openings.json.gz) and
[replacement observations](resource-results-229be58/native-openings.json.gz)
include all counters, request inventories, image IDs, sample coverage and hashes.
[Dataset provenance](resource-results-229be58/datasets.json) includes exact input
hashes and checked financial balances. [Excluded attempts](resource-results-229be58/excluded-attempts.json)
preserve both failure snapshots and the resumption chain. System Chromium was
155.0.8059.84. There are20 successful observations per cell below; baseline
failures described above are outside those latency distributions.

UI-ready observation in milliseconds, **p50 / p95**. This includes Playwright
actionability/readiness observation overhead, not just browser rendering time.
The baseline condition is visible Add New/account navigation; replacement is
visible register total/enabled Refresh transactions. Network settlement is
checked separately for both.

| Dataset | Action | Actual26.10 | Replacement |
| --- | --- | ---: | ---: |
| empty |cold|3843 /5062|828 /1200|
| empty |warm|2261 /2939|211 /264|
| empty |unchanged|89 /114|66 /75|
| typical |cold|3773 /5220|1044 /1147|
| typical |warm|2487 /3163|334 /393|
| typical |unchanged|152 /175|104 /116|
| large |cold|7872 /9621|1355 /1544|
| large |warm|2487 /2854|592 /648|
| large |unchanged|496 /753|184 /194|

Cold-open CPU time, milliseconds **p50 / p95**; multiple cores can consume more
CPU time than elapsed wall time.

| Dataset | Actual browser | Replacement browser | Actual server | Replacement server |
| --- | ---: | ---: | ---: | ---: |
| empty |13080 /17280|1790 /2550|530 /742|25 /31|
| typical |12960 /19550|2150 /2400|430 /778|34 /37|
| large |20590 /26240|2490 /2770|664 /839|131 /186|

Median bytes sent by the server, including transport overhead:

| Dataset | Actual cold | Replacement cold | Actual warm | Replacement warm | Actual unchanged | Replacement unchanged |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| empty |21830623|1202575|3094|9455|877|380|
| typical |21841223|1306535|3347|47908|454|380|
| large |23798834|1455945|2918|134558|1573|380|

Cold bytes and CPU are lower for the replacement in every case. Warm reloads
transfer **more** data: it fetches server-authoritative register/reference data,
whereas Actual reopens a full local database. The large replacement warm load
transferred about135KB, not the10048-row history. Unchanged replacement refresh
returned one95-byte changes body,380 server-side wire bytes, at every size.
This supports the unchanged-refresh boundary; it does not claim identical
costs for reload and explicit refresh.

Warm browser PSS and server cgroup peak memory, MiB **p50 / p95**:

| Dataset | Actual browser PSS | Replacement browser PSS | Actual server | Replacement server |
| --- | ---: | ---: | ---: | ---: |
| empty |703 /712|429 /435|95 /96|272 /273|
| typical |748 /759|460 /466|96 /99|273 /274|
| large |860 /881|507 /513|163 /204|274 /275|

All warm PSS cells have20 complete observations. Baseline cold PSS has only
14/16/13 complete observations respectively; those partial quantiles are not
used for a physical-memory comparison. Native browser memory is lower; native
post-import server memory is higher. Do not add browser PSS to cgroup memory as
an exact whole-machine physical total, or conceal the server tradeoff.

Browser cold-write medians were44.1/44.5/79.6MB for Actual versus
3.25/3.40/3.24MB for the replacement. These include browser caches/profile state.
Every sampled server opening/refresh interval had zero block-device writes at
both recorded layers. The35s idle windows had zero observed HTTP requests and
zero server block writes for both apps. Browser idle CPU was1070/1300/1470ms
for Actual versus520/490/630ms for the replacement; server CPU was10.6/8.8/6.4ms
versus2.5/2.2/2.6ms. Idle network counters were **not zero**: TCP/bridge control
traffic is included. Idle browser writes were1.5–4.3MB, requiring the separate
origin-file check instead of a false claim that the browser performs no writes.

Default automatic backups remained enabled. Native recovery points for all
three budgets appeared at02:57:21UTC,15minutes after preparation; the large
archive was1754129 bytes. This occurred after native opening measurements and
during the baseline run, with both containers resident on the shared host.
That brief scheduled work is an additional comparison limitation, not an
application poll. Public backup metadata is retained with the memory witness.

Warm large-budget server CPU also increases:31/38ms p50/p95 versus12/17ms
for the baseline. The replacement executes financial reads on the server;
the baseline executes its cached engine in the browser, whose corresponding
warm CPU was7500/8830ms versus1140/1320ms. This is a measured placement tradeoff,
not evidence that every component uses fewer resources.

## Idle persistence and memory follow-up

Both disposable containers were restarted **after** completing the opening
comparison. `check-resource-idle.mjs` then opened the large budget in a new
disk-backed profile, allowed60s to settle and observed another60s. It captured
IndexedDB/localStorage state through Playwright and watched the profile's
IndexedDB, Local Storage and File System directories. Storage snapshot calls
are outside the file-event observation window. It also records unrelated
profile paths, so browser housekeeping is not hidden.

[Baseline witness](resource-results-229be58/baseline-idle.json) and
[replacement witness](resource-results-229be58/native-idle.json) both pass:
zero app-origin requests, zero observed writes in those origin-store directories,
unchanged origin values during steady idle. Initial settling had2/4 origin
events respectively, with unchanged logical values. Chromium continued changing
Preferences, quota bookkeeping, metrics/cache files and other profile state;
this explains why whole-browser write counters cannot be labeled “budget writes.”
This is a bounded foreground observation, not yet the full Android/background
or pending-mutation resource qualification.

The [pre-restart snapshot](resource-results-229be58/memory-before-restart.json)
showed210001920 anonymous bytes for native and224735232 for baseline. Native
then started at2052096 anonymous bytes, rising to7544832 after opening/querying
the large register and observing idle. Baseline started at241422336, peaked at
266424320 in the first observation and fell to77414400 during the second.
Native retained about200MiB after import/automatic backup but used about7.2MiB
in the post-restart witness. During native idle, a separate explicit public
100-row register read also succeeded (122259-byte body); the browser remained
untouched. These are memory observations, not a second latency distribution.

The replacement Store retains one SQLite connection, not a persistent
in-memory transaction graph. Import conversion materializes source/converted
JSON; its activation/backup path creates substantial transient allocation.
The observed drop after restart is consistent with retained allocation after
bulk work, but does not identify the allocator or prove leak absence. No unsafe
heap intervention or allocator swap was introduced. Repeat-import/restore and
large mutation stress remain open51 work. A restart is currently an effective
operational way to release that high-water memory after migration; ordinary
opening does not require it. Restart also resets cgroup file-cache charging, so
file-memory changes are not claimed as physical pages freed.

Root reviewed the new tools against51's public-workflow/resource requirements
and repository conventions. Review corrected thread/process discovery, durable
profile placement, late-request settlement, failure preservation, incomplete
sample resumption and browser-version/limiter validation. JavaScript syntax and
format checks pass; both20-sample comparisons and both steady-idle checks pass
with the recorded baseline exceptions. No production source changed, so existing
financial/native functional results remain valid; no broad suite rerun is claimed.

Remaining51 gates: restore latency distributions,
graph/rollback and analysis-guard stress, Android
resource observations, background behavior, due-schedule cross-check and final
keyboard/accessibility/theme/locale/Impeccable review.52/53 also remain open.

## Acknowledged expenses and cash-flow reports

[Summary](resource-results-229be58/workloads/summary.json),
[baseline raw](resource-results-229be58/workloads/baseline-workloads.json.gz),
[replacement raw](resource-results-229be58/workloads/native-workloads.json.gz),
[datasets](resource-results-229be58/workloads/datasets.json) and
[independent financial proof](resource-results-229be58/workloads/financial-proof.json).

Same images, hardware, Chromium155 and three financial histories as above,
in fresh disposable containers. The pinned engine creates one `Resource latency`
dashboard with one static October–November2026 cash-flow card and removes other
dashboard pages before exporting identical inputs for both apps. Browser profiles
live on disk, one fresh profile per case, retained across20 expense/report pairs.
The account is warm before measurement; form entry is outside each window.
Each expense is1234 minor units dated2026-11-10, with a unique note and cleared
false. Measurements include Save through visible changed account balance and
server acknowledgement. Actual's explicit Server Sync is included; this measures
server acknowledgement, not its earlier local-only save. The replacement waits
for the successful mutation response and register refresh. Then navigation to
Reports through the visible chart is measured, followed by an unmeasured return
to the account. First lazy chart loading is retained. Counter settlement follows
the shared opening protocol; no failed samples appear in these final runs.

`check-resource-workloads.mjs` independently downloads all source budgets after
timing. All60 acknowledged expenses per app match exactly once, every account
balance matches, and each complete cash-flow result matches the pinned engine.
The report oracle fixes calendar today to2026-11-10 in UTC while retaining real
synchronization clocks. All-date account sums deliberately include future-dated
fixture entries, as do the migration manifest and replacement register.

Latency milliseconds, p50/p95 (20 samples per cell):

| Case | Actual expense | Replacement expense | Actual report | Replacement report |
| --- | ---: | ---: | ---: | ---: |
| empty |432/594|228/263|238/276|127/156|
| typical |727/1014|310/433|245/401|131/167|
| large |787/1207|661/862|228/329|246/345|

Browser/server CPU milliseconds, medians:

| Case | Actual expense | Replacement expense | Actual report | Replacement report |
| --- | ---: | ---: | ---: | ---: |
| empty |760/9|430/23|470/0|240/6|
| typical |1240/8|580/27|470/<1|230/7|
| large |1480/7|860/145|450/<1|270/108|

The large replacement report is slightly slower despite lower browser CPU.
Its server performs the financial calculation instead of the source browser's
cached engine. Large expense server cost includes mutation, changes/checkpoint,
history, schedule and register requests. The raw trace shows two100-row register
responses of122259 bytes each after saving; this is repeated bounded-page work,
not a full10048-row history download. No claim that every resource improved.

Median server TX bytes, expense/report: Actual1022/0,1048/0,1026/0;
replacement32984/33670,105022/41169,256236/41563 for empty/typical/large.
More request/response traffic is a cost of the server-authoritative design.
Median browser PSS during expenses is782/847/905MiB for Actual and507/520/584MiB
for replacement, with all20 samples available. Median server cgroup memory is
89/92/119MiB versus271/272/295MiB, including the retained import allocation and
charged cache discussed above. Do not sum PSS and cgroup totals as exact physical
memory or infer power consumption.

Server block writes per expense have medians167936 bytes for Actual in allcases,
versus65536/69632/65536 for replacement. Reports have zero observed server block
writes. Device-mapper and underlying-device counters mirror the same writes;
do not sum them. Browser write counters include profile housekeeping as before;
full p50/p95 CPU, memory, bytes and writes remain in the summary/raw evidence.

Excluded preparation attempts are not application failures: exploratory selectors
used a menuitem where Actual exposes a button; an initial replacement run waited
for the newly saved row to be visible even when future-dated fixture entries put
it beyond the first100 rows. That committed save and all samples from that run
were discarded; a fresh container received the original archive. The final
protocol checks changed balance plus acknowledgement on both apps. Oracle setup
was corrected to use the source sync group identity, UTC/fixed calendar and
all-date sums (the SDK balance convenience function defaults to today's cutoff).
These corrections did not change product code or financial expectations. Earlier
attempts remain in the disposable lab, outside the published distributions.

Root review against51 found no additional blocking issue in this measurement
slice. Shared process/counter helpers preserve the opening protocol; sample
fences refuse accidentally repeating mutating runs. Final archives/hashes,
240 observations, financial proof and credential exclusion were checked.
Syntax/format checks pass. Ticket51 remains open for the gates listed above.

## Repeated owned history on the large budget

`check-resource-history.mjs` imports the identical10048-row opening archive into
a fresh release container, makes40 owned note edits, then executes1000 undo/redo
cycles through HTTP. Every inverse checks its resulting note. Finally all20
retained actions undo and redo sequentially, with no further available action at
either end and unchanged source account balances. All checks pass. This stresses
the replacement's durable history; it is not a source-browser history benchmark.

[Summary](resource-results-229be58/history/summary.json),
[raw observations](resource-results-229be58/history/observations.json.gz),
[stopped-copy storage diagnostics](resource-results-229be58/history/storage-diagnostics.json).

| Command | Samples | HTTP ms p50/p95 | Server CPU ms p50/p95 | Server block writes bytes p50/p95 |
| --- | ---: | ---: | ---: | ---: |
| Ordinary owned note edit |40|54/65|48/58|98304/98304|
| Undo |1000|341/456|329/442|49152/86016|
| Redo |1000|342/455|329/444|49152/86016|

First/last20 undo CPU medians318/329ms, redo323/335ms. Individual spikes did
not establish a sustained growth regression. Inverse operations run structural
validation beyond the changed note, so their cost exceeds ordinary edits. This
run does not identify exact shares of that cost. Server cgroup memory medians
stay near276MiB including post-import allocation/cache; no allocator change.
Counters cover request through complete response; untimed public reads verify
each result. Node caller counters are not browser costs. Raw empty browser event
arrays are uninstrumented, not evidence of zero HTTP. Three diagnostic requests
(two API reads and one static fallback read) occurred during the first100 cycles;
their samples remain in the observations.

At every snapshot, combined database/WAL/SHM logical file lengths remain
77855904 bytes. A read-only copy after stopping the disposable container passes
SQLite integrity_check and contains20 history actions and2040 inverse lineage
rows. Lineage table/index consume561152/524288 bytes; history actions45056 bytes.
The database still has965 free4096-byte pages, so stable file lengths do **not**
prove bounded history storage. Lineage remains retained beyond the20 actions:
this run consumes about1.04MiB of lineage table/index space for1020 cycles,
including the final20-action walk. Larger lifetime workloads can grow storage.
No pruning was added: measured latency/memory stayed stable, while dropping
aliases without proving sequential ownership could break safe undo. All existing
remote-edit/relock protections remain unchanged; this slice does not rerun them.

Root reviewed protocol, disposable-target guards, stack assertions and explicit
measurement limits. JavaScript syntax, format and diff checks pass. Production
code remains229be58. Large graph/rollback/restore and analysis-guard stress still
require their separate checks; this result closes only the repeated-history and
large-budget ordinary-note-write observation gap.
