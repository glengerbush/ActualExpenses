# Budget row editing and visible categories

Category rows display their color, name and amounts. Pointer or Enter/Space activation opens the existing durable budget form within that row. Allocation, copy, carryover, notes, category settings and transaction details remain accessible in the selected editor. Group creation/color controls remain on group headings.

The view starts with three months in a horizontal scrolling strip; month count remains adjustable. The strip stays horizontal on mobile without expanding the page width.

Hidden categories/groups are retired. Migration 30 makes existing categories visible and normalizes legacy writes/restores through compatibility columns. Category mutation APIs reject hiding; Budget, reports, forecasts, category learning, web selectors and Android no longer exclude hidden categories. Legacy source metadata remains readable, while current categories stay visible.

Validation: budget-row-browser covers pointer/keyboard editing, saved amounts, settings access, button-free rows, desktop/mobile horizontal layout, migration29→30, financial inclusion and rejected hiding. Inline group/category creation and category recovery browser tests pass, including offline drafts and concurrent-tab ownership. The Android category-choice integration test passes. Svelte and Rust builds pass.

Unchanged row editors dismiss on an outside click, including disabled toolbar controls. Fresh transaction, split and transfer row editors behave the same way. Drafts with changed values, recovered work, unresolved requests or an active save stay protected. Dismissal uses each editor’s existing ownership checks and does not move keyboard focus back to the row. Covered by `outside-editor-browser.mjs`.
