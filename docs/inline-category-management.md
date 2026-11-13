# Category management in Budget

The Categories sidebar/command entry is removed. Budget's table header adds groups; each group row adds a category with its group and income/expense type inherited. Empty groups remain visible. Category settings in the selected budget row open metadata editing; group names save inline on blur/Enter, and their color pickers stay on the row. Escape restores the original name. All categories are visible.

The existing durable category editor handles retries, conflicts and draft recovery inside Budget. New records reserve their identity before saving, so the automatic color preview matches the server. Resetting a category shade immediately recomputes it from its group, ignoring any previous manual override.

Validation: inline-categories-browser checks empty groups, inherited context, new/edit automatic previews against saved API colors, draft reload and mobile overflow. The adapted categories browser checks unchanged retries, stale-tab ownership, offline boundaries, null ordering, retained notes, merges and keyboard interaction.

Groups and categories have drag handles, with touch support and Arrow Up/Down keyboard reordering. Categories can move between groups of the same income/expense type. The numeric order field is removed. Existing order values persist through the same guarded category API; tied imported orders are normalized when necessary. Unconfirmed name/order requests retain their original identity across reload and offer retry. A reorder affects every displayed month.

Sidebar navigation and reporting tabs now use browser history. Back/Forward restore the screen and budget; refreshing preserves the URL's destination. Selecting the current tab adds no duplicate history entry. The Budget view no longer has a Close button.

`budget-order-navigation-browser.mjs` covers mouse/keyboard/touch ordering, tied orders, cross-group moves, inline names/colors, retry after a committed response is lost, browser Back/Forward, report tabs, reload, and mobile width.
