# Manual entry completion

Expenses are complete when saved. There is no clearing, reconciliation, or pending-expense lifecycle. The web register has one balance, no status column or visibility filters, and no clearing controls in entry, split, transfer, import, bulk-edit, graph, or rule forms. Previous status-based register views are normalized so they cannot hide transactions.

Android retains durable offline entry and on-demand background delivery. Ordinary saves and deletes carry no sync badge; only failures need attention. Web device saves appear in the same transaction history while delivery runs. Recovery tooling retains unsent operations across restored or missing budgets; it does not create another class of expense.

Schema 28 automatically backs up the preceding SQLite database, removes existing reconciliation locks, and makes legacy status fields constant (`cleared=true`, `reconciled=false`). These columns remain solely for archive/API compatibility. Insert/update guards enforce the same behavior for imports, restores, older companions and API callers. Amounts, dates, identities, financial totals, original archives, and durable delivery records are preserved. Reconciliation and unlock endpoints are retired.

Source-import validation still checks exact account totals; settlement partitions are intentionally outside the new financial contract. Prior status-specific tests must assert the new completion model rather than Actual's settlement behavior.
