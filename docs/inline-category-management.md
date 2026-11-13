# Category management in Budget

The Categories sidebar/command entry is removed. Budget's table header adds groups; each group row adds a category with its group and income/expense type inherited. Empty groups remain visible. Category settings in the selected budget row open metadata editing; group pencils open group editing, including merge/delete. All categories are visible.

The existing durable category editor handles retries, conflicts and draft recovery inside Budget. New records reserve their identity before saving, so the automatic color preview matches the server. Resetting a category shade immediately recomputes it from its group, ignoring any previous manual override.

Validation: inline-categories-browser checks empty groups, inherited context, new/edit automatic previews against saved API colors, draft reload and mobile overflow. The adapted categories browser checks unchanged retries, stale-tab ownership, offline boundaries, null ordering, retained notes, merges and keyboard interaction.
