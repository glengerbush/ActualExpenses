# Schedules and matching qualification

Tickets21/22 cover spec stories48–53. Acceptance is **pending** final web22 review and integrated qualification. This record distinguishes completed evidence from remaining work; it is not a production-cutover approval.

## Independent reference evidence

All source observations use pinned Actual26.10.0 commit `2bebdbaae0b701120a8cad9d56aafd64fa04c577`, independently of replacement results.

- `generate-schedules.mjs` and `generate-schedule-financial.mjs` exercise recurrence, calendar boundaries, eight financial cases,36 horizon cases and six midpoint cases. The source's final/one-time skip behavior, split-child links and date horizons remain explicit expectations.
- `generate-schedules-overdue.mjs` records678 daily occurrences and exact−67800 balance. Genuine source import plus bounded400/278 continuation verifies persistent catch-up without duplicate payments. Eight barrier clients and restart retain one ledger and original receipts.
- `generate-schedules-generation.mjs` records four genuine synced source stages with original selector/date HLCs. Import uses the newest relevant source clock; equal-clock changes cannot overwrite imported generation state. No newer database was relabeled as an older fixture.
- `generate-schedule-matching.mjs` records12 matching sets, seven lifecycle cases,16 discovery histories, five conversion graphs and20 frozen horizon warnings. Matching includes split parents/children, transfer legs, reconciled rows and existing links where the source does. Genuine pre-operation ZIPs retain original IDs, bytes and digests.
- `generate-schedule-reassignment.mjs` records four ordinary/transfer and manual/automatic reassignment cases. The old payment survives under its new schedule; a distinct replacement occurrence may post for the old schedule, once.
- `generate-discovery-account-order.mjs` supplies a genuine equal-rank account witness: source name ordering wins over opposing UUID ordering. Imported account rank normalization preserves that winner; account-group ordering is unchanged.

Original fixture observations remain unchanged when additional genuine archives are exported. Source generators and archive provenance received independent review; qualification reuses that evidence.

## Server behavior and corrections

Schema19 persists schedules, linked rules, occurrence identities, releases and settings. Due probes, previews, discovery, matching and receipt lookup are read-only. Explicit catch-up has bounded work and an immutable original cutoff; there is no idle polling or background schedule engine.

Commands use the existing durable receipt, cancellation, epoch, history-owner and HLC contracts. Link/unlink and creation from selected history change the complete affected graph atomically. Conflicting explicit schedule intents on reciprocal transfer legs refuse without changing either leg. Historical payments survive unlink and reassignment; released occurrences receive a distinct later payment identity. Original receipts remain immutable.

Matching and discovery require a snapshot and bounded pagination. Discovery preserves source pattern/ranking behavior, including its current-weekday and eight-month quirks. The public tests compare full original ledgers before reading, reject stale/invalid continuations and verify restart stability.

Future conversion previews every affected row and protection state without creating references or reserving an ID. Commit checks future date, snapshot, locks and affected field clocks, then creates the schedule and removes the original graph atomically. New drafts reserve their original UUID with only final deleted state in the change feed; a delayed old Save refuses. Owned undo/redo works initially, but refuses after a new linked/scheduled payment would make restoration duplicate money.

Two intentional integrity corrections are explicit:

- The source future-conversion helper uses separate operations. Replacement conversion is atomic.
- The source split-transfer helper loses a reciprocal peer. Replacement preserves the complete transfer child/peer graph and ordinary sibling. Original source observations are retained. Public graph comparison caught an additional replacement bug in action ordering; deferring indexed transfer-payee assignment until allocations exist fixed it without changing the shared rule engine.

Link racing with automatic catch-up follows source behavior: link-first suppresses auto-posting; auto-post-first can retain both the existing and one generated payment after explicit fresh review. It never silently merges or deletes a payment. Barrier tests verify at-most-one generated occurrence, complete reciprocal graphs and exact receipts after abrupt restart; they do not claim both scheduler orderings were forced.

The existing eight-client catch-up test also races two activation retries of the genuine source archive: its original request key and a new key. Both acknowledge the already-activated import without reinstalling transactions. Exact source ledger−600, five new occurrences and all ten request receipts survive restart. This covers archive-import retries, not the later file/bank-import features. `/tmp/schedules-import-catchup-race-final.log`. The initial harness incorrectly expected409 for the new activation key; that expectation was corrected to the existing public201 acknowledgement, with all financial checks retained. No product change was needed.

## Browser and Android evidence

Web21 provides recurrence editing, posting/skipping/deletion, linked-rule navigation, horizon settings, globally date-ordered account previews and continuation disclosure. Foreground schedule recovery retains the exact request through lost acknowledgements, reload and epoch changes; retry/cancel is explicit and independent of expense capture. Frozen21 UI passed Standards/Spec and Impeccable review, including desktop,390px and200% light/dark renders. Evidence: `/tmp/schedules-web-render-final21b.log`, `/tmp/schedules-web-recovery-final21.log`, `/tmp/schedules-rules-browser-join.log`, `/tmp/schedules-forecast-sort-green.log`.

Android schedule requests run only during foreground work and stay outside the expense outbox/count/projection. Background work remains pending expense additions/edits/deletions. Known schedule refusals do not spin or block expense capture. [Native qualification](schedules-native-validation.md) records production-composable API36 phone, dark large-text and tablet renders, explicit retry/cancel and independent review. Those renders do not establish MainActivity system-bar, TalkBack or physical-device behavior.

Web22 matching/discovery/conversion and final integrated browser results remain pending.

## Integrated verification

- Initial complete serial HTTP run: `/tmp/schedules2122-full-http.log`. New schedule tests passed; older tests contained current-schema18 expectations and an obsolete unsupported-schedule-rule gate. Test-only `7534c33` updates current outputs to19 and retains the formula/template gate. Original fixtures, financial assertions and receipts are unchanged; independent review passed.
- Affected HTTP groups: `/tmp/schedules2122-qualification-fixes.log` —82 pass, zero failures/skips. Together with the initial run, all223 distinct HTTP cases have passing evidence; this is combined coverage, not one uninterrupted passing run.
- Full native API class:42 pass, zero failures/errors/skips; fresh debug APK and lint pass. `/tmp/schedules2122-native-qualification.log`; JUnit XML confirms the count. The genuine pre-upgrade schema17 binary was configured, including the app-first upgrade scenario. Unchanged legacy-protocol tests are not counted as rerun here.
- Final production-container/browser qualification and final UI22 review: pending. The reviewed `container-smoke.py --browser --keep-going` runs each package command once, records all failures and fails overall if any group fails; it does not retry or hide failures.

All work uses disposable budgets/containers/volumes. No production data, signing setup, version, release or deployment changed. Ticket51 still owns measured whole-system resource qualification; these functional bounds are not a universal latency or power claim.
