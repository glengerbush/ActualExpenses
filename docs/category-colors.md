# Category colors and grouped budgets

Budget categories appear below collapsible group headings, with group totals and aligned money columns, following the original Actual Budget layout.

Each group has a color picker in Budget and Categories. Categories receive stable automatic shades; their color pickers allow overrides. Reset returns a category to its automatic shade. Changing a group color updates automatic shades and preserves overrides. Transactions use a soft category tint without changing amount formatting.

Schema29 stores optional group/category colors. Existing data receives automatic colors; pre29 native archives remain restorable. Color edits use existing concurrency, history and idempotent receipt handling, with saved retry/cancellation after connection loss.

Tag controls, tag API routes and tag conditions are retired. Existing hashtags remain plain note text. Legacy tag data remains only for archive/import and history compatibility; tag-dependent saved rules are not executed.

Validation: focused browser/API checks cover colors, override/reset, grouped Budget, tag retirement, backup staging, schema28 migration and mobile overflow. Payee management retains rename/merge, immutable retry, cross-tab recovery, clock correction and offline coverage.
