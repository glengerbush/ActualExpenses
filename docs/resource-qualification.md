# Resource qualification — in progress

Ticket51 is **not accepted**. This note records the production browser comparison
and its limits. Opening, expense and report distributions are recorded below.
Large graph/history and saved-snapshot recovery observations are also recorded.
Native and released-baseline Android observations are also recorded. The
consolidated accessibility audit remains outstanding. Existing functional evidence is not relabeled as performance
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

Remaining51 gates: analysis-guard exhaustion/recovery, Android
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

## Web recovery of a saved 1001-child allocation

`measure-browser-restores.mjs` uses the genuine pinned `reconciliation-large.zip`
fixture: one101-child and one1001-child split,1104 records including parents.
Actual's direct `Load Backup` menu is Electron-only in the pinned source, so
this measures its supported web **Import file → Actual → Select file** path.
The replacement reviews its own portable ZIP and restores an existing budget.
These achieve equivalent financial recovery with different archive formats and
identity semantics; they are not identical internal operations.

[Summary](resource-results-229be58/restores/summary.json) and
[raw observations](resource-results-229be58/restores/observations.json.gz).
The source archive is85857 bytes; the native portable archive98016 bytes.
Both recover all1104 record projections (ID/account/date/amount/parent linkage,
parent/child flags and notes) to the independent pinned-engine hash. All40
per-sample reconciliations pass; the1001-child account balance remains−1001.
Native generation changes on every restore. Other migration fields retain their
separate accepted functional evidence; this performance projection is explicit.

Twenty fresh disk browser profiles per app; auth/file chooser setup excluded.
Measurements end at the visible1001-child account balance; source explicit sync
is included. Native archive review and activation are separate measured phases,
with confirmation typing excluded. Paired phase sums, not sums of quantiles,
give the native total below. Source/node oracle reads occur outside timing.
The restarted opening containers are reused, including their other fixture
budgets. Source imports create20 files; native restores one cloned budget20 times.
No equal server-state or isolated-host claim.

| Workflow | UI ms p50/p95 | Browser CPU ms p50/p95 | Server CPU ms p50/p95 |
| --- | ---: | ---: | ---: |
| Source web saved-file recovery |1847/1946|5530/6300|26/41|
| Native review + activation |1893/2544|970/1070|1404/1993|
| Native activation only |1798/2065|780/870|1401/1633|

Native validation reuses one retained stage for the same archive: first review
868ms, subsequent/repeated-review median99ms; first activation1945ms. This is
repeat-snapshot qualification, not20 cold validation runs. It resolves the
earlier1001-child debug deadline question with measured production observations;
the old30s fixture timeout is not a latency target.

Native summed median server TX/RX is2354877/117882 bytes, source5727/102261.
Native restores and then reads full split aggregates through HTTP; source keeps
the imported engine locally. This trades more server/network work for less
browser CPU. Observed activation browser PSS median427MiB versus source696MiB;
server cgroup medians92MiB versus84MiB. These states differ from the earlier
post-import opening run. Full phase memory/CPU/bytes/writes are retained; no
power or universal latency improvement claimed.

Two preparation attempts are excluded: the first waited for a file chooser
before pressing Select file; the second completed UI recovery but its native
checker requested2000 records despite the1000-row public page cap. The final
checker uses snapshot-bound paging. Neither required a product change.

## Large graph discovery, response and rollback

`check-resource-graphs.mjs` publicly clones the same source-derived fixture,
then repeats four operations20 times. A1001-member allowance refuses discovery
of the1002-member parent/children graph. A second batch deletes that entire
graph, then attempts an ordinary patch on the separate small split: that second
operation correctly fails. Snapshot/history, parent notes and the complete
1104-row financial projection remain unchanged. A subsequent supported sparse
child-note edit succeeds, followed by a complete split read. Both account
balances and all amounts/dates/identities/parent links remain unchanged.

[Summary](resource-results-229be58/graphs/summary.json) and
[raw observations](resource-results-229be58/graphs/observations.json.gz).

| Operation | HTTP ms p50/p95 | Server CPU ms p50/p95 | Server block writes bytes p50/p95 |
| --- | ---: | ---: | ---: |
| Bounded graph refusal |14/19|12/16|0/0|
| Rollback after full graph deletion |660/994|644/973|73728/90112|
| Sparse allocation-note edit |1049/1592|1021/1547|9957376/10117120|
| Complete split response |113/145|108/137|0/0|

Successful edit/read responses each contain about1.2MB; server cgroup memory
has median174MiB/p95228MiB during edits. Whole-graph journal protection and
immutable responses make even a sparse1001-child edit expensive. This run
measures that cost; it does not claim the100000-child ceiling has been stress
qualified. Rollback's nonzero physical writes do not mean financial commitment:
public state and history equality checks pass. Browser event arrays are not
instrumented in this Node caller; kernel network counters are retained.

The first graph harness incorrectly tried an ordinary batch patch on a split;
the API refused it before the intended second operation. The final harness uses
the supported delete/sparse-split paths and confirms the failure names the second
transaction. All80 final operations and their state assertions pass. Root review,
syntax/format and diff checks pass; no production code changed.

The remaining51 gate is the consolidated UI audit. Android resource/idle
comparisons and the due-schedule cross-check follow.

## Analysis work-limit recovery

The focused SQLite test `exhausted_read_releases_connection_for_later_work`
exercises the unchanged production32million VM-step guard with a finite recursive
read. It returns400 `analysis_work_limit`. The same connection then completes
the identical read without the scoped guard, returning the independently known
sum1…2000000 =2000001000000; a subsequent guarded read and ordinary write also
succeed. The original saved value remains42 and the later insert produces49.
This verifies interruption, hook cleanup and connection reuse without changing
the production limit, adding a configuration override or generating millions of
financial records.

This is explicitly a **lower-seam numeric resource-bound check**, not a claim
that an HTTP analysis request exhausted the limit. Public analysis results,
cache behavior and unchanged financial state remain covered by
`budget-analysis-api.mjs` and `budget-analysis-large-api.mjs`. The direct test
isolates the work bound and cleanup that realistic bounded financial fixtures
cannot adequately exercise. Command:

```sh
cargo test --manifest-path actualBudget/server/Cargo.toml \
  --bin actualbudget-server exhausted_read_releases_connection_for_later_work \
  -- --nocapture
```

Passed,1 test,4.61s in the debug test build. No latency benchmark or product
behavior change is claimed; the production image remains `229be58`.

## Android measurement boundary

`check-android-resources.py` drives opt-in `ResourceDeviceTest` on the disposable
API36 emulator. It selects fresh public duplicates of the same empty/typical/large
fixtures, using the current debug APK, real MainActivity/Compose controls,
encrypted state store and HTTP adapter. No physical or user device is involved.
The local forwarding proxy records request counts, body sizes and server-facing
response latency without recording request bodies or credentials.

Each case performs20 unchanged refreshes,20 background/foreground transitions
and20 acknowledged4.27 expenses, then35s foreground idle and35s background idle.
Every refresh/transition must preserve the encrypted cache byte-for-byte. Both
idle windows must preserve that cache and have zero supported UID RX/TX counter
growth. The host independently verifies all20 distinct purchases, cleared flags
and the exact8540-minor-unit balance change through the public API. This measures
warm foregrounding, not Android process-cold startup.

Counters distinguish action elapsed time, process CPU, end-of-window PSS, UID
network bytes, physical process I/O and syscall write bytes. PSS collection and
test activity contribute overhead; Compose instrumentation runs inside the
measured process. These are **instrumented emulator costs**, not phone response
times, production-build CPU estimates or a baseline Android comparison. Request
latency from the proxy is separate from the much longer UI-test action timings.
Measurement JSON is buffered until teardown so writing the report does not
create writes inside later observations. Idle physical writes may still occur
outside the unchanged financial cache; they are retained, not attributed to a
component without evidence. A single35s idle observation has no latency
distribution even if the generic summary repeats it in both quantile fields.

Earlier pilots were excluded: proxy connection cleanup used an unsupported
Python context manager; the cache probe initially named the old legacy file;
the UI probe initially checked pending-count before acknowledgement; and the
successful fourth device pilot exposed host-side external-storage retrieval
permissions. Final instrumentation waits for pending-count zero and reads the
native cache; host retrieval uses adb's permitted external-storage access.
No product correction is claimed for these harness failures.

The first large-case run also stopped when its newly backdated purchase was
outside the visible viewport. The corrected probe reveals the pinned row before
asserting its rendered presence; that reveal is included in large-case action
timing. Another large run completed all20 purchases but caught687 received and364
sent bytes at the idle boundary, consistent with its final due-schedule request;
the retained proxy trace contains no subsequent recurring request. Final large
qualification records a separate settlement phase, requiring two consecutive
quiet one-second UID-counter intervals, with a ten-second bound, before beginning
idle. Earlier small-case idle checks already passed without that settlement.
These differing measurement details are retained explicitly; the distributions
are not an isolated estimate of dataset size alone. Expense counters end at the
tested UI outcome, and a trailing schedule request can fall outside that window.
The earlier failed captures remain under `.scratch/qualification51-android-*`.

Due-schedule behavior is cross-checked against the current52-test native HTTP
qualification in [complete migration](complete-migration-validation.md), rather
than rerunning an unchanged suite during measurements. The recorded tests include
`foreground schedules post once while pending delivery mode never runs schedules`,
`scheduled foreground lost ack retains separate original command through restart and background refresh`,
and `overdue scheduled work holds for review without spinning or blocking ordinary pending expenses`.
`ExpensesApplication` schedules only one-time work while expense changes are
pending; its worker calls `refresh(foreground=false)`. Foreground analysis of due
schedules remains enabled in these device runs. Earlier OS-denied/background
recovery and permitted-idle evidence remains explicitly in
[Android upgrade validation](android-upgrade-validation.md); these resource runs
do not claim a new OS-denial experiment. The browser idle observations above kept
automatic backups enabled; their functional recovery remains covered by08/50.

### Recorded native Android results

[Summary, APK identities, financial proofs and raw-file hashes](resource-results-229be58/android/summary.json).
Compressed observations and request traces beside it cover all three cases.
There are187 accepted observations:180 repeated workflow measurements, six idle
windows and the large-case settlement window. All60 purchases are independently
confirmed exactly once, cleared, with each account balance decreasing8540 minor
units and its uncleared balance unchanged. Failed pilots are not pooled into the
quantiles. Empty/typical use the first successful batch; large uses its final
corrected batch. Both instrumentation APK hashes are recorded; the product APK
is identical, SHA256 `d787863977c639ab9ee30f8742de7d9b1f1ec944a378b91fa8521eeb905f679f`.

[Final public readback](resource-results-229be58/android/exact-once-proof.json)
additionally checks one row per expected payee and60 distinct transaction IDs;
the maintained harness includes those same per-purchase assertions.

All cells below are p50/p95 milliseconds. UI values include instrumentation;
server values measure only the forwarding proxy's server-facing HTTP exchange.

| Case | Unchanged refresh | Warm foreground | Expense UI outcome | Server expense POST |
| --- | ---: | ---: | ---: | ---: |
| empty |2918/2934|3920/3949|7118/7460|18.2/22.8|
| typical |2924/2957|3927/3961|7128/7413|18.2/19.9|
| large |2928/2960|3900/3977|7143/7545|94.9/97.7|

Median unchanged-refresh UID traffic is3687 received/1321 sent bytes for all three
sizes. The largest companion response bodies are11419/11647/12874 bytes; the
large ledger is not downloaded in full by these workflows. Median expense PSS is
about116/118/117MiB; median measured process CPU is4118/4066/4260ms, including
Compose instrumentation. Full p50/p95 CPU, PSS, network and I/O counters are in
the summary. Do not interpret these UI/CPU numbers as actual phone latency or
compare them directly with the production browser measurements.

Every idle window has zero UID RX/TX growth and unchanged encrypted cache.
Foreground idle still records143360/86016/20480 physical process-write bytes;
background idle records zero in all three. Those other writes are not attributed
to a component. Final large settlement took3006ms and retained603 received/80
sent bytes separately. The final emulator scheduler dump reported zero running
background jobs for the app; this is a point-in-time observation, not an OS-wide
guarantee. Emulator network status reported zero configured min/max latency and
speed values; the installed product/test APK hashes match the recorded artifacts.
The owned emulator and proxy stopped after qualification. No user device changed.

Reproduce with built/installed debug and instrumentation APKs on a disposable
API36 emulator, using the browser fixture lab:

```sh
python3 actualBudget/tools/check-android-resources.py \
  .scratch/qualification51-final NEW_OUTPUT_DIRECTORY \
  --adb /path/to/android-sdk/platform-tools/adb --serial emulator-5556 --samples 20
```

Use `--case large` to select one fixture.
The harness refuses an existing output directory and accepts only emulator serials
and the local disposable server. Final compile, Python syntax, Rust formatting,
artifact-checksum and patch checks pass. These observations complete the native
measurement slice. The baseline comparison follows; consolidated UI audit and
52/53 acceptance remain open.

### Released Android baseline comparison

The shared instrumentation also ran against released0.1.2 source `2b25b07`,
which already includes the user's unchanged-history speed correction. Its tracked
production files are unchanged. Built debug APK SHA256:
`b24b43072ab4367bfa2c7f47681142de8057a4d61b9f5ee42c6081346acf48de`.
The shared probe handles the old login/cache filename and scrolls virtualized
choosers. Measurement uses the same host, emulator image/geometry and selected
financial datasets. Genuine source copies are verified against the original
ledger hashes before measurement. The independent pinned26.10 API then verifies
one cleared−427 transaction per expected payee,60 distinct IDs,−8540 total/cleared
delta per account and unchanged uncleared balances.

[Baseline summary, provenance, proofs and compressed raw observations](resource-results-229be58/android-baseline/summary.json)
retain189 successful observations:180 repeated workflows, six35s idle windows
and three separately recorded settlement windows. Refresh/foreground cache
hashes are unchanged for all120 measurements. All six idle windows have zero
UID RX/TX growth and unchanged encrypted cache. These are successful bounded
observations, not a claim that the old app never does background work.

UI observation milliseconds **p50 / p95**, including debug instrumentation:

| Case | Action | Released0.1.2 | Replacement |
| --- | --- | ---: | ---: |
| empty | unchanged refresh |2927/2962|2918/2934|
| typical | unchanged refresh |2919/2949|2924/2957|
| large | unchanged refresh |4865/5303|2928/2960|
| empty | warm foreground |3915/3943|3920/3949|
| typical | warm foreground |3926/3961|3927/3961|
| large | warm foreground |5382/5826|3900/3977|
| empty | expense |5146/5526|7118/7460|
| typical | expense |5161/5523|7128/7413|
| large | expense |38429/39611|7143/7545|

Expense costs, **p50**. The linked summaries retain p95 and every raw counter.

| Case | CPU ms old/new | End PSS KiB old/new | UID received bytes old/new | Physical write bytes old/new |
| --- | ---: | ---: | ---: | ---: |
| empty |3304/4118|117094/118616|83768/20550|294912/204800|
| typical |3501/4066|121142/120968|80638/21140|479232/229376|
| large |56979/4260|281567/119593|3843758/22464|36286464/212992|

The replacement improves the large-budget case substantially, while small-case
expense UI time and CPU increase. Request traces explain an important component:
the new path makes seven requests per new-payee expense versus five in the old
path, including bounded association/companion refresh and foreground schedule
checking. Both traces show roughly one-second inter-request gaps although the
proxy's server exchanges are usually milliseconds. This is an instrumented
emulator/transport observation; attributing that entire gap to production server
processing would be incorrect. No speculative product optimization was made to
hide the measured tradeoff. Process-cold Android opening and phone battery draw
are not measured.

The selected ledgers are identical, but the **server catalogs differ** after
earlier disposable restore tests. The old app fetches the entire file list every
refresh:9497-byte bodies for empty/typical and9832 for the final large run. The
replacement fetches its catalog once at selection. Old unchanged-refresh UID
RX is12067/11644/12391 bytes versus3687 for new. These whole-request ratios must
not be sold as predictions for a one-budget installation; the catalog component
is explicitly retained in the raw traces. Large expense response/history costs
remain visible separately from that small catalog body.

Old large foreground idle consumes31475ms process CPU and679936 physical write
bytes during its35s post-work window, despite zero network/cache changes. Its
background idle consumes38ms CPU and zero physical writes. This records residual
post-work cost without assigning it to GC, rendering or another component that
was not separately profiled. Empty/typical foreground idle use71/75ms CPU and
20480 write bytes; all old background windows have zero physical writes.
Zero network traffic is not a claim of zero CPU or disk activity.

Excluded attempts: source-package lookup and chooser-visibility setup corrections,
one-sample empty pilot, and the initial large run interrupted by the host's900s
watchdog after20 refreshes,20 foreground transitions and16 expenses. Its56 raw
observations are retained separately and excluded from quantiles; no financial
or idle success is claimed for that interrupted run. Only the complete large
case was repeated with a3600s watchdog. Host timeouts now preserve partial logs
and proxy events. Final source verification, artifact/hash/credential checks,
syntax/format checks and root review pass. Owned emulator/proxy stopped.

To reproduce the old side, build/install the unchanged2b25b07 debug APK and a
test APK containing the shared `ResourceDeviceTest`; pass
`--baseline-source /path/to/2b25b07-checkout` to the driver. It refuses changed
tracked source, creates new disposable source copies, passes only lab credentials
through private app files and removes that file afterward. Never publish the
lab configuration containing its password/token.
