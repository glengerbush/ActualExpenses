# actualBudget parity ledger

Baseline: Actual **26.10.0**, commit `2bebdbaae0b701120a8cad9d56aafd64fa04c577`. Source provenance and unchanged-file hashes are in the [reference manifest](../actualBudget/reference/actual-26.10.0/manifest.json). The [canonical spec](</home/ggg/Documents/pileOfNotes/ActualExpenses/Issues/ActualBudget Web Rewrite Spec.md>) defines 107 stories. The [Obsidian ticket index](</home/ggg/Documents/pileOfNotes/ActualExpenses/ActualBudget Rewrite Tickets.md>) owns ticket status.

Each feature owner records a baseline source/function, independent fixture and expected public result, replacement test/evidence, and any explicit deviation. A ticket assignment below is not a claim of parity. Mark a story verified only once every assigned workflow is demonstrated. The approved seam is the running public API, exercised directly and through browser/Android workflows; lower seams are reserved for behavior that cannot adequately be tested there.

## Baseline evidence families

| Family | Pinned source anchors | Fixture/observable result |
| --- | --- | --- |
| Budget files, backups and migration | Core budget-file/importer code; docs backup-restore and migration guides | Export/import/restore round trip; IDs, relationships and amounts |
| Accounts and reconciliation | Core account operations; web account/reconciliation workflows | Opening balances, closing, grouped accounts and locked entries |
| Transactions, payees and categories | Core transactions/importers and category/payee code; web register tests | Exact signed amounts, dates, split/transfer totals and merge effects |
| Tracking, envelope and automation | Core budget/spreadsheet/goal/cleanup code; budgeting guides | Month-boundary totals, income holds, rollover and allocation outcomes |
| Rules and schedules | Core rules/schedules and associated tests | Ordering, matching, recurring occurrences and generated transactions |
| File imports and bank providers | Core importers; sync-server provider integrations | Format-specific fixtures, deduplication and sanitized provider responses |
| Reports, dashboards and formulas | Web reports and core queries/forecast/formula tests | Reference aggregate values, saved configurations and seeded forecasts |
| Settings and accessible workflows | Web settings, preferences, themes and browser tests | Persisted preference semantics and observable browser behavior |
| Android and offline expense capture | Released actualExpenses public expense-service integration and device tests | Interrupted request/restart cases with an independent Actual API oracle |
| Public API, deployment and resources | Published API and query behavior; release CLI/workflow | External-caller scenarios, artifacts, persistence and measured resource use |

## Story assignments

| Story | Tickets | Reference family | Replacement evidence |
| --- | --- | --- | --- |
| 1 | 02, 52 | Budget files, backups and migration | [02: fresh HTTP web/API entry without login](open-budget-validation.md); final audit pending |
| 2 | 02, 52 | Budget files, backups and migration | [02: no app credentials, sessions or identity flows in replacement](open-budget-validation.md); Android transition/final audit pending |
| 3 | 02, 08 | Budget files, backups and migration | [02: create/select persistent budgets](open-budget-validation.md); rename/close/delete pending 08 |
| 4 | 02, 08, 43 | Budget files, backups and migration | [02: remembered budget/account and draft recovery](open-budget-validation.md); broader preferences pending |
| 5 | 04, 50 | Budget files, backups and migration | Pending feature acceptance |
| 6 | 08, 50 | Budget files, backups and migration | Pending feature acceptance |
| 7 | 27 | Budget files, backups and migration | Pending feature acceptance |
| 8 | 08 | Budget files, backups and migration | Pending feature acceptance |
| 9 | 04, 08, 50 | Budget files, backups and migration | Pending feature acceptance |
| 10 | 02, 09 | Accounts and reconciliation | [02: on/off-budget creation, notes and seven independent opening-balance cases](open-budget-validation.md); account management pending 09 |
| 11 | 09 | Accounts and reconciliation | Pending feature acceptance |
| 12 | 03, 09, 12 | Accounts and reconciliation | Pending feature acceptance |
| 13 | 16 | Accounts and reconciliation | Pending feature acceptance |
| 14 | 16 | Accounts and reconciliation | Pending feature acceptance |
| 15 | 16 | Accounts and reconciliation | Pending feature acceptance |
| 16 | 09 | Accounts and reconciliation | Pending feature acceptance |
| 17 | 03, 13 | Transactions, payees and categories | Pending feature acceptance |
| 18 | 03, 52 | Transactions, payees and categories | Pending feature acceptance |
| 19 | 03, 13 | Transactions, payees and categories | Pending feature acceptance |
| 20 | 13 | Transactions, payees and categories | Pending feature acceptance |
| 21 | 12 | Transactions, payees and categories | Pending feature acceptance |
| 22 | 12, 16 | Transactions, payees and categories | Pending feature acceptance |
| 23 | 14 | Transactions, payees and categories | Pending feature acceptance |
| 24 | 15 | Transactions, payees and categories | Pending feature acceptance |
| 25 | 15 | Transactions, payees and categories | Pending feature acceptance |
| 26 | 25 | Transactions, payees and categories | Pending feature acceptance |
| 27 | 03, 11 | Transactions, payees and categories | Pending feature acceptance |
| 28 | 11, 19 | Transactions, payees and categories | Pending feature acceptance |
| 29 | 46 | Transactions, payees and categories | Pending feature acceptance |
| 30 | 11 | Transactions, payees and categories | Pending feature acceptance |
| 31 | 03, 10 | Transactions, payees and categories | Pending feature acceptance |
| 32 | 03, 10 | Transactions, payees and categories | Pending feature acceptance |
| 33 | 17, 18 | Tracking, envelope and automation | Pending feature acceptance |
| 34 | 18 | Tracking, envelope and automation | Pending feature acceptance |
| 35 | 18 | Tracking, envelope and automation | Pending feature acceptance |
| 36 | 17 | Tracking, envelope and automation | Pending feature acceptance |
| 37 | 17 | Tracking, envelope and automation | Pending feature acceptance |
| 38 | 17, 18 | Tracking, envelope and automation | Pending feature acceptance |
| 39 | 15, 18 | Tracking, envelope and automation | Pending feature acceptance |
| 40 | 10, 17, 18 | Tracking, envelope and automation | Pending feature acceptance |
| 41 | 23 | Tracking, envelope and automation | Pending feature acceptance |
| 42 | 23, 24 | Tracking, envelope and automation | Pending feature acceptance |
| 43 | 24 | Tracking, envelope and automation | Pending feature acceptance |
| 44 | 19 | Rules and schedules | Pending feature acceptance |
| 45 | 19 | Rules and schedules | Pending feature acceptance |
| 46 | 19 | Rules and schedules | Pending feature acceptance |
| 47 | 42 | Rules and schedules | Pending feature acceptance |
| 48 | 21 | Rules and schedules | Pending feature acceptance |
| 49 | 21 | Rules and schedules | Pending feature acceptance |
| 50 | 22 | Rules and schedules | Pending feature acceptance |
| 51 | 21 | Rules and schedules | Pending feature acceptance |
| 52 | 22 | Rules and schedules | Pending feature acceptance |
| 53 | 21, 22 | Rules and schedules | Pending feature acceptance |
| 54 | 25, 26 | File imports and bank providers | Pending feature acceptance |
| 55 | 25 | File imports and bank providers | Pending feature acceptance |
| 56 | 25, 26 | File imports and bank providers | Pending feature acceptance |
| 57 | 08, 25 | File imports and bank providers | Pending feature acceptance |
| 58 | 28, 29, 30 | File imports and bank providers | Pending feature acceptance |
| 59 | 31, 32 | File imports and bank providers | Pending feature acceptance |
| 60 | 28, 29, 30, 31, 32 | File imports and bank providers | Pending feature acceptance |
| 61 | 28, 29, 30, 31, 32 | File imports and bank providers | Pending feature acceptance |
| 62 | 28, 29, 30, 31, 32 | File imports and bank providers | Pending feature acceptance |
| 63 | 33 | Reports, dashboards and formulas | Pending feature acceptance |
| 64 | 33 | Reports, dashboards and formulas | Pending feature acceptance |
| 65 | 33 | Reports, dashboards and formulas | Pending feature acceptance |
| 66 | 34 | Reports, dashboards and formulas | Pending feature acceptance |
| 67 | 33, 34 | Reports, dashboards and formulas | Pending feature acceptance |
| 68 | 34 | Reports, dashboards and formulas | Pending feature acceptance |
| 69 | 38 | Reports, dashboards and formulas | Pending feature acceptance |
| 70 | 35, 36, 37 | Reports, dashboards and formulas | Pending feature acceptance |
| 71 | 39, 40 | Reports, dashboards and formulas | Pending feature acceptance |
| 72 | 41 | Reports, dashboards and formulas | Pending feature acceptance |
| 73 | 33, 35, 36, 37, 38, 39, 40, 41 | Reports, dashboards and formulas | Pending feature acceptance |
| 74 | 43, 51 | Settings and accessible workflows | Pending feature acceptance |
| 75 | 43, 51 | Settings and accessible workflows | Pending feature acceptance |
| 76 | 43, 44 | Settings and accessible workflows | Pending feature acceptance |
| 77 | 43 | Settings and accessible workflows | Pending feature acceptance |
| 78 | 12, 45 | Settings and accessible workflows | Pending feature acceptance |
| 79 | 45 | Settings and accessible workflows | Pending feature acceptance |
| 80 | 31, 32, 35, 36, 37, 39, 41, 42, 43 | Settings and accessible workflows | Pending feature acceptance |
| 81 | 49 | Settings and accessible workflows | Pending feature acceptance |
| 82 | 03, 43, 49, 51 | Settings and accessible workflows | Pending feature acceptance |
| 83 | 07, 20 | Android and offline expense capture | Pending feature acceptance |
| 84 | 07, 20 | Android and offline expense capture | Pending feature acceptance |
| 85 | 07, 20 | Android and offline expense capture | Pending feature acceptance |
| 86 | 07, 20 | Android and offline expense capture | Pending feature acceptance |
| 87 | 05, 07, 20, 49 | Android and offline expense capture | Pending feature acceptance |
| 88 | 05, 07, 20 | Android and offline expense capture | Pending feature acceptance |
| 89 | 05, 07, 20 | Android and offline expense capture | Pending feature acceptance |
| 90 | 06 | Android and offline expense capture | Pending feature acceptance |
| 91 | 06 | Android and offline expense capture | Pending feature acceptance |
| 92 | 06 | Android and offline expense capture | Pending feature acceptance |
| 93 | 06 | Android and offline expense capture | Pending feature acceptance |
| 94 | 19, 20 | Android and offline expense capture | Pending feature acceptance |
| 95 | 06 | Android and offline expense capture | Pending feature acceptance |
| 96 | 20 | Android and offline expense capture | Pending feature acceptance |
| 97 | 20 | Android and offline expense capture | Pending feature acceptance |
| 98 | 20, 50 | Android and offline expense capture | Pending feature acceptance |
| 99 | 47, 52 | Public API, deployment and resources | Pending feature acceptance |
| 100 | 34, 47, 52 | Public API, deployment and resources | Pending feature acceptance |
| 101 | 05, 51 | Public API, deployment and resources | Pending feature acceptance |
| 102 | 02, 48, 53 | Public API, deployment and resources | [02: real single-container web/API with SQLite volume replacement/replay](open-budget-validation.md); release/cutover qualification pending |
| 103 | 01, 48, 53 | Public API, deployment and resources | Pending feature acceptance |
| 104 | 01, 48, 53 | Public API, deployment and resources | Pending feature acceptance |
| 105 | 48, 53 | Public API, deployment and resources | Pending feature acceptance |
| 106 | 04, 08, 50, 53 | Public API, deployment and resources | Pending feature acceptance |
| 107 | 51 | Public API, deployment and resources | Pending feature acceptance |

## Foundation evidence

Ticket 01 establishes the pinned source and executable oracle without changing financial behavior. Its validation is recorded in [foundation validation](foundation-validation.md). Existing Android compatibility does not establish that the replacement web/server is complete.
