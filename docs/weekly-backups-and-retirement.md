# Settings and weekly backups

Settings → General controls the server-wide automatic backup schedule: Weekly (default) or Never. A dirty budget gets one deadline seven days after its first change since the previous automatic backup. Subsequent edits do not postpone it. Clean budgets create no snapshots and no background timer runs when everything is clean. Changes made while disabled are remembered for re-enabling.

Each automatic backup is a complete native budget archive, including transactions, categories, plans, rules and recovery receipts. The latest ten automatic snapshots are retained. Manual and safety recovery points are separate. Snapshots live in the same persistent server data volume; exporting a copy protects against losing that volume.

Experimental report engines (forecast, budget analysis, Sankey, formulas and Monte Carlo), rule expressions/templates, custom themes, currency display and experimental switches are removed. Ordinary reports, crossover reports, built-in themes and fixed rule actions remain. Old experimental definitions are retired during migration/import; financial data and original source archives remain available. Pre-upgrade SQLite backups are still created automatically.

Settings also contains Import and Recovery. The sidebar remains available; page-level Done/Close/Back controls are removed. Editor cancellation remains available.

Transaction creation uses an inline row. Budget feedback uses green five-second notifications and persistent, dismissible red errors. Month controls align centrally; the Budget heading has no subtitle or bottom margin.
