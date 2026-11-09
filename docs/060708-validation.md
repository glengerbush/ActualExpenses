# Tickets 06, 07 and 08 acceptance

Accepted through `d77a46b`, from prior accepted `8fe9b38`. All three ticket criteria are met for the currently supported domains. Remaining feature and upgrade work retains its own tickets; no production release or cutover occurred.

## Delivered

- [06 Android API adapter](android-api-validation.md): no-login setup, budget/account selection, focused eligible expenses, retained pending/backdated additions, captured intent context and unchanged application/signing identity. Legacy queued state remains untouched for ticket20.
- [07 browser offline capture](offline-web-validation.md): dated cached views, durable scoped expense outbox, immutable retry, field-order conflict feedback, global recovery for unavailable budgets, and pending-only delivery. Cold offline navigation is not promised.
- [08 lifecycle and backups](budget-lifecycle-validation.md): rename/close/delete/duplicate, complete native archives for supported domains, staging/restore, automatic/manual/pre-upgrade recovery, exact-name confirmation and epoch fencing. Future domain additions must extend `actualBudget/server/LIFECYCLE.md` contracts.

## Evidence

| Boundary | Result |
| --- | --- |
| Running Rust HTTP API | 60 integration tests, zero skips; Clippy all targets with warnings denied; rustfmt passed |
| Served API contract | Expanded OpenAPI validation passed companion and lifecycle operations, cancellation, deleted/epoch failures and native archive staging/activation |
| Android | 25 core tests, zero skips; debug/instrumentation builds and lint; 11 device tests each on API26/API36; separate dark200% and tablet real-HTTP flows |
| Full production-container browser suite | Passed at `0563ff3`, including prior financial/date/import/concurrency/theme workflows, offline delivery, lifecycle and generation recovery |
| Final scoped container regression | Passed at `d77a46b`: seven pending reversion/reload cases, three original editor races, nine offline groups; container replacement retained exact balances and receipts |
| Storage/process faults | Real ENOSPC and SIGKILL during an in-flight restore passed; old/new whole state, unchanged backup hashes, exact retry and one restore generation |
| Frontend quality | Svelte check/autofixer, complete Prettier check; responsive/keyboard/error/recovery captures inspected |

Full-suite image: `sha256:7ed349ccf6467e7b338b24b469ab9d89dbd80ef78a978e0a9147a203b758c747`,126863026bytes. Final image: `sha256:e5a50b37d604225bdc51eeb5455c7598146e4b30d790517d363569fe8207ee7d`,126863296bytes. Both run as10001:10001. Backend fault qualification used the same unchanged backend; details and image are in the lifecycle record. All containers/volumes/images were disposable and removed.

Logs: `.scratch/060708-{native,clippy,fmt}-final.log`, `.scratch/060708-contract-expanded.log`, `.scratch/060708-container-fault-final.log`, `.scratch/060708-container-browser-repaired.log`, `.scratch/060708-container-reload-final.log`, `.scratch/060708-format-final.log`. Android log paths are in its validation record.

## Review

Independent Standards and Spec reviews pass at `d77a46b`: zero remaining confirmed findings. Review found and resolved original-delivery/edit races, reverted pending PATCH loss, dependency loss, and offline eligibility after editor reload. Public regressions cover each. Integrated qualification also found and resolved the missing preference-recovery action after replaying account creation without cached preferences.

Lifecycle Impeccable finish review initially requested confirmation focus and visible selected archive identity. Both were fixed, recaptured and scored `ship`. The documenter preserved the existing design system. Root inspected native setup, dark200% editor and tablet results independently.

Ticket20 still owns legacy Android upgrade/background qualification; tickets14/15 must extend companion eligibility when splits/transfers become supported; tickets49–53 retain repair, whole-feature, efficiency and deployment qualification. This acceptance makes no physical power-loss, physical-phone, complete feature-parity or live-deployment claim.
