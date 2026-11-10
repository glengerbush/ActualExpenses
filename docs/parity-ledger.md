# actualBudget parity ledger

Baseline: Actual **26.10.0**, commit `2bebdbaae0b701120a8cad9d56aafd64fa04c577`. Source provenance and unchanged-file hashes are in the [reference manifest](../actualBudget/reference/actual-26.10.0/manifest.json). The [canonical spec](</home/ggg/Documents/pileOfNotes/ActualExpenses/Issues/ActualBudget Web Rewrite Spec.md>) defines 107 stories. The [Obsidian ticket index](</home/ggg/Documents/pileOfNotes/ActualExpenses/ActualBudget Rewrite Tickets.md>) owns ticket status.

Each feature owner records a baseline source/function, independent fixture and expected public result, replacement test/evidence, and any explicit deviation. A ticket assignment below is not a claim of parity. Mark a story verified only once every assigned workflow is demonstrated. The approved seam is the running public API, exercised directly and through browser/Android workflows; lower seams are reserved for behavior that cannot adequately be tested there.

User scope change2026-11-10: all external account connections are removed;
see [manual-only scope](manual-only-scope.md). Original story IDs remain for
traceability; explicit exceptions are not implementation-completion claims.

## Baseline evidence families

| Family | Pinned source anchors | Fixture/observable result |
| --- | --- | --- |
| Budget files, backups and migration | Core budget-file/importer code; docs backup-restore and migration guides | Export/import/restore round trip; IDs, relationships and amounts |
| Accounts and reconciliation | Core account operations; web account/reconciliation workflows | Opening balances, closing, grouped accounts and locked entries |
| Transactions, payees and categories | Core transactions/importers and category/payee code; web register tests | Exact signed amounts, dates, split/transfer totals and merge effects |
| Tracking, envelope and automation | Core budget/spreadsheet/goal/cleanup code; budgeting guides | Month-boundary totals, income holds, rollover and allocation outcomes |
| Rules and schedules | Core rules/schedules and associated tests | Ordering, matching, recurring occurrences and generated transactions |
| File imports and bank providers | Core importers; legacy linked-account migration | Manual formats/deduplication retained; provider connections removed by user2026-11-10 |
| Reports, dashboards and formulas | Web reports and core queries/forecast/formula tests | Reference aggregate values, saved configurations and seeded forecasts |
| Settings and accessible workflows | Web settings, preferences, themes and browser tests | Persisted preference semantics and observable browser behavior |
| Android and offline expense capture | Released actualExpenses public expense-service integration and device tests | Interrupted request/restart cases with an independent Actual API oracle |
| Public API, deployment and resources | Published API and query behavior; release CLI/workflow | External-caller scenarios, artifacts, persistence and measured resource use |

## Story assignments

| Story | Tickets | Reference family | Replacement evidence |
| --- | --- | --- | --- |
| 1 | 02, 52 | Budget files, backups and migration | [02: fresh HTTP web/API entry without login](open-budget-validation.md); final audit pending |
| 2 | 02, 52 | Budget files, backups and migration | [02: no app credentials, sessions or identity flows in replacement](open-budget-validation.md); Android transition/final audit pending |
| 3 | 02, 08 | Budget files, backups and migration | [02: create/select](open-budget-validation.md); [08: rename/close/delete/duplicate with independent identity](budget-lifecycle-validation.md) |
| 4 | 02, 08, 43 | Budget files, backups and migration | [02: remembered selection/drafts](open-budget-validation.md); [43: preferences](../actualBudget/web/docs/preferences-validation.md); [08: budget management](budget-lifecycle-validation.md) |
| 5 | 04, 50 | Budget files, backups and migration | [04: staged ordinary-ledger import with retained source/IDs](import-validation.md); complete migration qualification pending 50 |
| 6 | 08, 50 | Budget files, backups and migration | [08: portable native archives and staged restore](budget-lifecycle-validation.md); complete migration qualification pending50 |
| 7 | 27 | Budget files, backups and migration | [27: genuine YNAB4/nYNAB financial cases, sparse carry, source differences and recovery](ynab-import-validation.md); [integrated acceptance](ynab-custom-integration-validation.md) |
| 8 | 08 | Budget files, backups and migration | [08: automatic/manual/pre-upgrade recovery and fault-safe restore](budget-lifecycle-validation.md) |
| 9 | 04, 08, 50 | Budget files, backups and migration | [04: retained/rejected source validation](import-validation.md); [08: checksummed archive validation and pre-action recovery](budget-lifecycle-validation.md); complete migration qualification pending50 |
| 10 | 02, 09 | Accounts and reconciliation | [02: on/off-budget creation and opening balances](open-budget-validation.md); [09: organization, closure and notes](0910-validation.md) |
| 11 | 09 | Accounts and reconciliation | [09: grouping/order, closed visibility, deletion and native recovery](0910-validation.md) |
| 12 | 03, 09, 12 | Accounts and reconciliation | [03: exact account and running balances](register-validation.md); [09: organization](0910-validation.md); [12: accepted register customization and filtered totals](register-query-validation.md) |
| 13 | 16 | Accounts and reconciliation | [16: statement comparison, exact adjustments and protected edits](reconciliation-validation.md) |
| 14 | 16 | Accounts and reconciliation | [16: lock/unlock and retained reconcile dates](reconciliation-validation.md) |
| 15 | 16 | Accounts and reconciliation | [16: credit/debt signs and off-budget revaluation](reconciliation-validation.md) |
| 16 | 09 | Accounts and reconciliation | [09: account organization/deletion](0910-validation.md); [15: transfer-based closure](transfers-validation.md) |
| 17 | 03, 13 | Transactions, payees and categories | [03: single CRUD](register-validation.md); [13: atomic bulk edits/duplicate/delete and guarded undo](bulk-undo-validation.md) |
| 18 | 03, 52 | Transactions, payees and categories | [03: exact financial values, dates and calculator baseline](register-validation.md); whole-feature audit pending 52 |
| 19 | 03, 13 | Transactions, payees and categories | [03: ordinary edits](register-validation.md); [13: bulk fields, literal/regex notes and immutable replay](bulk-undo-validation.md) |
| 20 | 13 | Transactions, payees and categories | [13: owner-scoped20-group undo/redo, connected history and newer-edit conflict refusal](bulk-undo-validation.md) |
| 21 | 12 | Transactions, payees and categories | [12: accepted search, sorting, compound filters and saved views](register-query-validation.md) |
| 22 | 12, 16 | Transactions, payees and categories | [12: visibility and balances](register-query-validation.md); [16: reconciliation and protection](reconciliation-validation.md) |
| 23 | 14 | Transactions, payees and categories | [14: exact splits, recombination, sparse1001-child workflows and guarded history](splits-validation.md); [integrated acceptance](1314151646-validation.md) |
| 24 | 15 | Transactions, payees and categories | [15: reciprocal transfer edits/deletion and imports](transfers-validation.md) |
| 25 | 15 | Transactions, payees and categories | [15: account closure, duplicate/link handling and history](transfers-validation.md) |
| 26 | 25 | Transactions, payees and categories | [25: source-backed matching and manual merge graphs, protection and one-action undo](transaction-import-validation.md) |
| 27 | 03, 11 | Transactions, payees and categories | [11: payee management, aliases, retained references and Android choices](payees-tags-validation.md) |
| 28 | 11, 19 | Transactions, payees and categories | [11: payee management and metadata](payees-tags-validation.md); [19: associated-rule navigation and learning controls](rules-validation.md) |
| 29 | 46 | Transactions, payees and categories | [46: explicit locations, baseline proximity, migration and HTTP fallback](payee-locations-validation.md) |
| 30 | 11 | Transactions, payees and categories | [11: exact tag discovery, management, visibility, filtering and migration](payees-tags-validation.md) |
| 31 | 03, 10 | Transactions, payees and categories | [10: groups/order/hiding, notes, history-preserving merge/delete and legacy migration](0910-validation.md) |
| 32 | 03, 10 | Transactions, payees and categories | [10: signed refunds/reimbursements, uncategorized drilldowns and exact reassignment](0910-validation.md) |
| 33 | 17, 18 | Tracking, envelope and automation | [17/18: independent method allocations and retained draft intent](budgeting-validation.md) |
| 34 | 18 | Tracking, envelope and automation | [18: availability, allocations, spending and balances](budgeting-validation.md) |
| 35 | 18 | Tracking, envelope and automation | [18: moves, overspending, held income and signed carry](budgeting-validation.md) |
| 36 | 17 | Tracking, envelope and automation | [17: planned income/expenses and projected/final savings](budgeting-validation.md) |
| 37 | 17 | Tracking, envelope and automation | [17: optional tracking carry without automatic envelope carry](budgeting-validation.md) |
| 38 | 17, 18 | Tracking, envelope and automation | [17/18: multi-month navigation, copying and year boundaries](budgeting-validation.md) |
| 39 | 15, 18 | Tracking, envelope and automation | [15: transfer categorization](transfers-validation.md); [18: credit debt/refunds, split leaves and on/off-budget accounting](budgeting-validation.md) |
| 40 | 10, 17, 18 | Tracking, envelope and automation | [10: category history/notes](0910-validation.md); [17/18: monthly details/notes and exact actual drilldowns](budgeting-validation.md) |
| 41 | 23 | Tracking, envelope and automation | Pending feature acceptance |
| 42 | 23, 24 | Tracking, envelope and automation | Pending feature acceptance |
| 43 | 24 | Tracking, envelope and automation | Pending feature acceptance |
| 44 | 19 | Rules and schedules | [19: pinned ordering/conditions/actions, atomic entry/application and retained imported rules; explicit invariant exceptions](rules-validation.md) |
| 45 | 19 | Rules and schedules | [19: complete change previews, bounded account/ID selection, protected graphs and immutable retry](rules-validation.md) |
| 46 | 19 | Rules and schedules | [19: payee renaming, opt-in learning, global/per-payee controls and replay without retraining](rules-validation.md) |
| 47 | 42 | Rules and schedules | Pending feature acceptance |
| 48 | 21 | Rules and schedules | [21: income/expense/transfer schedules, calendar previews and durable posting](schedules-validation.md) |
| 49 | 21 | Rules and schedules | [21: pinned recurrence, month-day/end/weekend and horizon behavior](schedules-validation.md) |
| 50 | 22 | Rules and schedules | [22: exact/approximate/ranged matching, atomic links and concurrent catch-up/import retries](schedules-validation.md) |
| 51 | 21 | Rules and schedules | [21: manual/automatic posting, skip/completion, bounded catch-up and foreground-only recovery](schedules-validation.md) |
| 52 | 22 | Rules and schedules | [22: source discovery, existing-history creation and atomic future conversion](schedules-validation.md) |
| 53 | 21, 22 | Rules and schedules | [21/22: horizon settings/warnings, ordered account previews and coherent linked rules](schedules-validation.md) |
| 54 | 25, 26 | File imports and bank providers | [25/26: CSV/TSV, OFX/QFX, QIF and CAMT parsers plus committed public/browser flows](transaction-import-validation.md) |
| 55 | 25 | File imports and bank providers | [25: retained mapping/options, exact rounding, preview and durable failure recovery](transaction-import-validation.md) |
| 56 | 25, 26 | File imports and bank providers | [25/26: IDs, source-ordered matching, status/deleted choices, rules once and repeat imports](transaction-import-validation.md) |
| 57 | 08, 25 | File imports and bank providers | [08: native budget export](budget-lifecycle-validation.md); [25: source-backed CSV bytes/cells and canonical filtered export](transaction-import-validation.md) |
| 58 | 29 retirement | File imports and bank providers | Explicit user exception2026-11-10: SimpleFIN/GoCardless/Pluggy removed; [retirement verified](bank-retirement-validation.md) |
| 59 | 29 retirement | File imports and bank providers | Explicit user exception2026-11-10: Enable Banking/Akahu removed; [retirement verified](bank-retirement-validation.md) |
| 60 | 29 retirement | File imports and bank providers | Explicit user exception2026-11-10: external connection/refresh workflows removed; [retirement verified](bank-retirement-validation.md) |
| 61 | 25, 26, 29 retirement | File imports and bank providers | Revised: retain manual-import preferences and historical identifiers; remove provider balances/mappings; [retirement verified](bank-retirement-validation.md) |
| 62 | 29 retirement | File imports and bank providers | Revised: remove operational credentials/configuration, preserve original recovery artifacts; [retirement verified](bank-retirement-validation.md) |
| 63 | 33 | Reports, dashboards and formulas | [33: named dashboards, persistent layout/settings, source clocks and native round trips](reports-validation.md) |
| 64 | 33 | Reports, dashboards and formulas | [33: source-backed cash flow/net worth/spending, bounded aggregates and snapshot drilldowns](reports-validation.md) |
| 65 | 33 | Reports, dashboards and formulas | [33: text, summary/calendar widgets and accessible exact-value tables](reports-validation.md) |
| 66 | 34 | Reports, dashboards and formulas | [34: saved custom reports, five renderers and exact projections](reports-validation.md); [integrated acceptance](ynab-custom-integration-validation.md) |
| 67 | 33, 34 | Reports, dashboards and formulas | [33/34: dashboard/custom dates, conditions and account/category inclusion](reports-validation.md); [integrated acceptance](ynab-custom-integration-validation.md) |
| 68 | 34 | Reports, dashboards and formulas | [34: modes, nested grouping, bounded exact details and saved state](reports-validation.md); [integrated acceptance](ynab-custom-integration-validation.md) |
| 69 | 38 | Reports, dashboards and formulas | [38: exact source crossover projections, editable assumptions, saved settings, accessible chart/table and recovery](crossover-validation.md) |
| 70 | 35, 36, 37 | Reports, dashboards and formulas | Pending feature acceptance |
| 71 | 39, 40 | Reports, dashboards and formulas | Pending feature acceptance |
| 72 | 41 | Reports, dashboards and formulas | Pending feature acceptance |
| 73 | 33, 35, 36, 37, 38, 39, 40, 41 | Reports, dashboards and formulas | [33: recorded-entry labels and bounds](reports-validation.md); [35: forecasts](forecast-validation.md); [36: budget analysis](budget-analysis-validation.md); [37: Sankey](sankey-validation.md); [38: crossover assumptions and data view](crossover-validation.md);39–41 pending |
| 74 | 43, 51 | Settings and accessible workflows | [43: accessible responsive preference controls](../actualBudget/web/docs/preferences-validation.md); whole-app qualification pending 51 |
| 75 | 43, 51 | Settings and accessible workflows | [43: number/date/currency/decimal/week-start controls](../actualBudget/web/docs/preferences-validation.md); whole-app qualification pending 51 |
| 76 | 43, 44 | Settings and accessible workflows | [43: built-in appearance](../actualBudget/web/docs/preferences-validation.md); [44: catalog/custom themes, source preservation and recovery](../actualBudget/web/docs/custom-theme-contract.md) |
| 77 | 43 | Settings and accessible workflows | [43: released language catalogs, device scope and retry](../actualBudget/web/docs/preferences-validation.md) |
| 78 | 12, 45 | Settings and accessible workflows | [12: accepted remembered register views](register-query-validation.md); [45: privacy, navigation, command search, shortcuts and help](navigation-validation.md) |
| 79 | 45 | Settings and accessible workflows | [45: optional fork-local notices, dismissal and opt-out without idle network checks](navigation-validation.md) |
| 80 | 35, 36, 37, 39, 41, 42, 43 | Settings and accessible workflows | [43: preserved opt-in flags and currency gating](../actualBudget/web/docs/preferences-validation.md); remaining retained experimental features pending assigned tickets; provider toggles explicitly removed2026-11-10 |
| 81 | 49 | Settings and accessible workflows | [49: copied-data repair preview, guarded backup/apply and safe cache/connection recovery](recovery-validation.md) |
| 82 | 03, 43, 49, 51 | Settings and accessible workflows | [03: retained forms and durable retry](register-validation.md); [43: settings/storage/connection recovery](../actualBudget/web/docs/preferences-validation.md); [49: retained recovery intent/cancellation](recovery-validation.md); whole-app qualification pending51 |
| 83 | 07, 20 | Android and offline expense capture | [07: dated cache before network and unchanged-checkpoint efficiency](offline-web-validation.md); [20: bounded native cache and no legacy reread after atomic handoff](android-upgrade-validation.md) |
| 84 | 07, 20 | Android and offline expense capture | [07: durable ordinary offline capture using cached references](offline-web-validation.md); [20: migrated/native offline capture with original references](android-upgrade-validation.md) |
| 85 | 07, 20 | Android and offline expense capture | [07: unsent editing/deletion, editor reload and replacement races](offline-web-validation.md); [20: offline correction and deletion restrictions](android-upgrade-validation.md) |
| 86 | 07, 20 | Android and offline expense capture | [07: interrupted online edits retained](offline-web-validation.md); [20: interrupted native edits and immutable delivery](android-upgrade-validation.md) |
| 87 | 05, 07, 20, 49 | Android and offline expense capture | [05: exact receipts](concurrency-validation.md); [07: durable browser outbox](offline-web-validation.md); [20: original message identities, atomic handoff, lost ACK and reinstall](android-upgrade-validation.md); [49: cache repair retains pending mutations and cancellation phase](recovery-validation.md) |
| 88 | 05, 07, 20 | Android and offline expense capture | [05: per-field HLC](concurrency-validation.md); [07: offline merge](offline-web-validation.md); [20: original field clocks and visible newer-edit feedback](android-upgrade-validation.md) |
| 89 | 05, 07, 20 | Android and offline expense capture | [05: deletion ordering](concurrency-validation.md); [07: queued deletion recovery](offline-web-validation.md); [20: newer-edit deletion cancellation and published tombstones](android-upgrade-validation.md) |
| 90 | 06 | Android and offline expense capture | [06: no-login server/budget/account selection](android-api-validation.md) |
| 91 | 06 | Android and offline expense capture | [06: remembered account and bounded latest-ten selection](android-api-validation.md) |
| 92 | 06 | Android and offline expense capture | [06: pending/latest-backdated retention](android-api-validation.md) |
| 93 | 06 | Android and offline expense capture | [06: amount-only, optional fields and automatic clearing](android-api-validation.md) |
| 94 | 19, 20 | Android and offline expense capture | [19: simple associations and incremental invalidation](rules-validation.md); [20: cached associations and retained manual overrides/clears across upgrade](android-upgrade-validation.md) |
| 95 | 06, 14, 15 | Android and offline expense capture | [14/15: split/transfer exclusion and real native incremental eligibility](1314151646-validation.md) |
| 96 | 20 | Android and offline expense capture | [20: pending-only delivery, OS-denied background and foreground recovery](android-upgrade-validation.md) |
| 97 | 20 | Android and offline expense capture | [20: allowed-idle zero HTTP/cache writes/jobs; LAN/Tailscale-address equivalence with emulator limits](android-upgrade-validation.md) |
| 98 | 20, 50 | Android and offline expense capture | [20: both upgrade orders, exact source mapping, genuine released state and signed debug reinstall](android-upgrade-validation.md); full replacement qualification50 pending |
| 99 | 47, 52 | Public API, deployment and resources | Pending feature acceptance |
| 100 | 34, 47, 52 | Public API, deployment and resources | [34: bounded custom-report API/inspection](ynab-custom-integration-validation.md); [47: public query/import/export/batch caller](public-query-validation.md); bank refresh explicitly removed2026-11-10; final parity52 pending |
| 101 | 05, 51 | Public API, deployment and resources | [05: independent change cursor, 95-byte unchanged refresh and measured scaling](concurrency-validation.md); whole-system qualification pending 51 |
| 102 | 02, 48, 53 | Public API, deployment and resources | [02: container persistence/replay](open-budget-validation.md); [48: signed image promotion and isolated registry rehearsal](release-validation.md); production cutover qualification pending 53 |
| 103 | 01, 48, 53 | Public API, deployment and resources | [01: independent application layout](foundation-validation.md); [48: independent APK/container versions and release automation](release-validation.md); final deployment qualification pending 53 |
| 104 | 01, 48, 53 | Public API, deployment and resources | [01: original Android identity preserved](foundation-validation.md); [48: original APK signer and container signature verified](release-validation.md); final deployment qualification pending 53 |
| 105 | 48, 53 | Public API, deployment and resources | [48: manual image pull/restart release path, no automatic deployment](release-validation.md); full cutover rehearsal pending 53 |
| 106 | 04, 08, 50, 53 | Public API, deployment and resources | [04: staged conversion](import-validation.md); [08: archives/recovery/rollback faults](budget-lifecycle-validation.md); complete migration/cutover qualification pending50/53 |
| 107 | 51 | Public API, deployment and resources | Pending feature acceptance |

## Foundation evidence

Ticket 01 establishes the pinned source and executable oracle without changing financial behavior. Its validation is recorded in [foundation validation](foundation-validation.md). Existing Android compatibility does not establish that the replacement web/server is complete.
