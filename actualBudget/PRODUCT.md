# actualBudget product
<!-- impeccable:product-schema 1 -->

## Platform
web

## Stack
Confirmed in ADR 0005: Svelte 5/TypeScript static frontend, Rust/Axum API, authoritative SQLite; one container. Routine interface choices delegated by the user.

## Users
Trusted household devices accessing a personal budget over LAN or tailnet, including mobile browsers.

## Product Purpose
Manage accounts and finances with Actual 26.10.0 financial behavior and a simpler, resource-conscious architecture. actualExpenses is the focused Android expense companion. The installed iPhone web app uses the same focused expense workflow: budget selection, adding expenses, and editing/deleting the latest ten plus the most recent backdated addition. Normal browser access, iPad and other platforms retain the full budget interface.

## Operating Context
Plain HTTP is supported. No application login, identity, roles or sessions. Server persistence is authoritative. Full offline budgeting and desktop packaging are excluded. The iPhone companion supports cached expense entry and silent queued delivery; a service-worker shell enables offline reopening on HTTPS. Plain HTTP still supports online installed use and offline capture while the loaded page remains open. No background polling is added.

Accounts are fully manual: no external bank connectors. Retain manual file imports,
internal rules/schedules and Android delivery. Previously linked account history survives migration.

## Capabilities and Constraints
Initial slice: select/create budgets, create/select on/off-budget accounts, exact opening balances and calendar dates, reopen persisted data. Future features follow the canonical rewrite tickets; do not present unfinished controls. Native clients share the versioned public API.

## Brand Commitments
Use the names actualBudget and actualExpenses. Keep financial language concrete and amounts exact.

## Evidence on Hand
Canonical Obsidian rewrite spec and ticket02; pinned Actual 26.10.0 reference source. No financial customer data or marketing claims are supplied.

## Product Principles
- Tracking is the only budgeting method. No method switch, envelope calculations, budget/category/transaction notes, or bulk Clear allocations action. Old active envelope plans convert on migration/import; transactions remain unchanged.
- Manual entries are complete when saved. No cleared, uncleared, reconciled, or pending-expense lifecycle. Offline delivery is internal; show only actionable failures.
- Put the account and the current task first.
- Retain entered information when a request fails.
- Confirm persisted results rather than showing optimistic financial balances.
- Keep every working control accessible by keyboard and on mobile.

## Accessibility & Inclusion
Visible focus, explicit labels, useful errors, sensible reading order, reduced-motion support and responsive layouts are required by the specification.

- Category planned amounts save inline on blur or after one second. Pencil buttons open advanced edits. Compact month controls use abbreviated month/year and a This month reset.
