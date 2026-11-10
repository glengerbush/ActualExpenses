# actualBudget implementation record

The user authorized implementation of all 53 approved tickets on 2026-11-08, using subagents, dependency-aware parallelism, review and commits to the current branch. Production cutover remains manual and separately authorized.

## Review baseline

- Credit preference (user-approved 2026-11-09): Standard speed only; never enable Fast, priority or Ultrafast tiers. Use cheaper models for routine implementation and review; reserve Astra/high for difficult financial, migration or conflict work. Normally run one implementer and one reviewer. Finish the current integration batch before opening more work; use compact task handoffs, focused checks during development and one full suite/container qualification after integration. Review standards and spec together where practical; reuse valid evidence instead of repeating unchanged reviews.
- Account creation and setup use the user's external browser, never the in-app browser.

- Branch: `main`.
- Pre-implementation commit: `2b25b070ddee0c6bca5f2e95b11d82ddac269012`.
- Review standards and spec separately. Review each ticket against its own start commit and acceptance scope, then review the complete replacement against this baseline.
- Canonical [specification](</home/ggg/Documents/pileOfNotes/ActualExpenses/Issues/ActualBudget Web Rewrite Spec.md>) and [ticket index](</home/ggg/Documents/pileOfNotes/ActualExpenses/ActualBudget Rewrite Tickets.md>) are in Obsidian. Record evidence before completion.

## Assigned queues

Assignments reserve ownership, not permission to bypass blocking edges. Root coordinates shared-file changes and commits; workers do not commit independently.

| Worker | Tickets |
| --- | --- |
| foundation | 01, 03, 05, 08, 09, 10, 13, 14, 15, 16, 17, 18, 19, 21, 22, 23, 24, 49 |
| web_and_reports | 02, 07, 11, 12, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 51 |
| android_and_imports | 04, 06, 20, 25, 26, 27, 28, 29, 30, 31, 32, 47, 48, 50, 52, 53 |

## Verification contract

- Use the running public API through representative browser/Android workflows, with pinned Actual 26.10.0 fixtures as the independent financial and migration oracle. The user approved this seam.
- Use red/green cycles where applicable, focused tests and regular typechecking; run the full suite at final integration. Existing relocation checks must remain green.
- Every UI implementation uses the relevant Impeccable skills; Svelte work also follows the Svelte skills.
- Review both documented standards and spec behavior before marking a ticket complete. No placeholder capability counts as parity.
- Preserve pending work, per-field original ordering and replay identity through Android migration, including evidence to reconcile uncertain legacy acknowledgements.

## Current progress

- Ticket 01: complete in `d5a49e0`; independent Standards and Spec reviews found no actionable findings. [Evidence](foundation-validation.md): 16 real-server tests with no skips, 12 release tests, signed/debug builds, lint and 3,088 verified reference files.
- Ticket 02: complete in `6f3661d` and `8d09012`; [evidence](open-budget-validation.md) includes 9 HTTP tests, 7 pinned Actual cases, browser/container restart and replay checks, and resolved independent review findings.
- Ticket 48: complete in `608ac8b`; [evidence](release-validation.md) records 28 passing release tests, real signed-APK verification, isolated registry signature/promotion/persistence checks, and independent Standards/Spec reviews with zero findings. Production publication remains gated.
- Tickets 03 and 43: complete in `0256ca6`, `ca9040d` and `0b66dc6`; [register evidence](register-validation.md) and [preference evidence](../actualBudget/web/docs/preferences-validation.md) include 23 real HTTP tests, public OpenAPI validation, pinned financial/format/date oracles, full browser/container replacement checks, and zero remaining independent Standards/Spec findings.
- Tickets 04, 05 and 44: complete through `8fe9b38`; [import](import-validation.md), [concurrency](concurrency-validation.md) and [custom-theme evidence](../actualBudget/web/docs/custom-theme-contract.md) include 50 HTTP tests, independent pinned oracles, the full container/browser suite and zero remaining Standards/Spec findings.
- Tickets 06, 07 and 08: complete through `d77a46b`; [acceptance](060708-validation.md) records 60 HTTP tests, Android API26/API36 runs, full container/browser qualification, final scoped regressions, real ENOSPC/SIGKILL recovery and zero remaining independent Standards/Spec findings.
- Tickets09/10 complete through `c8de872`: [acceptance](0910-validation.md), prior79 HTTP tests plus3 metadata regressions,18 native tests, full and corrective production-container/browser runs; independent corrective reviews pass.
- Ticket11 complete through `d6a5005`: [acceptance](payees-tags-validation.md),92 HTTP and21 native tests, full production-container/browser qualification, APK/lint and independent Standards/Spec reviews pass.
- Ticket12 complete through `5a9efa1`: [register query acceptance](register-query-validation.md),105 HTTP and22 freshly executed native tests, pinned ordinary/split predicates, bounded regex and10k resource qualification, all container/browser suites and independent reviews. Initial recovery timeout remains unexplained after successful targeted diagnosis; failure-only evidence capture added, no product or timeout change.
- Tickets13–16 and46 complete through `4796e11`: [acceptance](1314151646-validation.md),164 HTTP cases,28 fresh native tests, all34 production browser groups across recorded runs, shared fixture isolation and independent code/UI reviews.
- Tickets17/18 complete through `82c2f37`: [acceptance](budgeting-validation.md),174 HTTP cases across composed/isolated runs,29 native tests, all35 browser groups in one production-container run and independent code/UI reviews.
- Ticket19 complete through `f486a7b`: [acceptance](rules-validation.md),186 HTTP cases,30 native tests with APK/lint, all36 production browser groups across recorded runs, pinned rules/learning/financial oracles and independent code/UI reviews.
- Ticket20 complete through `f4b9ca0`, integrated at `5124e9a`: [acceptance](android-upgrade-validation.md),190 HTTP cases and37 native cases across recorded runs, fresh APK/lint, all36 production browser groups plus five fixture checks, final container persistence/replay, signed debug upgrade/reinstall, background/idle and LAN/Tailscale-address checks. Independent code, harness and Impeccable reviews pass.
- Tickets21/22 complete through `87b9fdc`: [acceptance](schedules-validation.md),225 distinct HTTP cases and42 native API cases across recorded runs, APK/lint, all40 production browser groups plus five fixture checks across the documented initial/correction images, final container persistence/replay, native renders and independent Standards/Spec/Impeccable reviews. Twenty-six of53 complete;23 goals and24 budget automation form the next dependent batch. One implementer and one consolidated reviewer; Standard speed only.
- 2026-11-09 corrective evidence: two independent bulk-history findings (duplicate account-group names and opaque tag-name collision) reproduced through public HTTP, corrected in `f9e53f0`, and all13 history tests pass. Independent reviewer confirmed both fixes. Split protection's quadratic graph query was corrected in `936ec6b`; protected sparse transfer, native27 and focused browser suites pass. A1001-entry archive restore exceeded5s under concurrent work on both candidate and unchanged control; root's unchanged original workflow subsequently passed13.03s total, with minimized protected restore3.653s. No timeout/assertion changes; final suite runs without concurrent builds. Logs `/tmp/bulk13-{metadata-red,tag-red,metadata-green}.log`, `/tmp/restore-perf-{original,minimal,protected}.log`.
- Local Docker access restored on 2026-11-08: the user added `ggg` to the docker group; `newgrp docker` lets this existing session access Docker 29.9.2 (Linux x86_64). Tickets 02 and 48 exercised isolated test containers/volumes successfully; production storage was not used.
