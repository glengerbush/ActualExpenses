# actualBudget implementation record

The user authorized implementation of all 53 approved tickets on 2026-11-08, using subagents, dependency-aware parallelism, review and commits to the current branch. Production cutover remains manual and separately authorized.

## Review baseline

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

- Ticket 01: implementing repository reorganization and pinned reference import; evidence pending.
- Other tickets: assigned; implementation waits for reviewed prerequisites.
- Container qualification is currently unavailable: the local Docker socket requires the docker group, and passwordless sudo is unavailable. Normal-process API/oracle tests can proceed. Do not equate them with a successful container rehearsal.
