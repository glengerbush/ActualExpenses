# actualBudget product
<!-- impeccable:product-schema 1 -->

## Platform
web

## Stack
Confirmed in ADR 0005: Svelte 5/TypeScript static frontend, Rust/Axum API, authoritative SQLite; one container. Routine interface choices delegated by the user.

## Users
Trusted household devices accessing a personal budget over LAN or tailnet, including mobile browsers.

## Product Purpose
Manage accounts and finances with Actual 26.10.0 financial behavior and a simpler, resource-conscious architecture. actualExpenses remains the focused Android expense companion.

## Operating Context
Plain HTTP is supported. No application login, identity, roles or sessions. Server persistence is authoritative. Full offline budgeting and desktop packaging are excluded.

Accounts are fully manual: no external bank connectors. Retain manual file imports,
internal rules/schedules and Android delivery. Previously linked account history survives migration.

## Capabilities and Constraints
Initial slice: select/create budgets, create/select on/off-budget accounts, exact opening balances and calendar dates, reopen persisted data. Future features follow the canonical rewrite tickets; do not present unfinished controls. Native clients share the versioned public API.

## Brand Commitments
Use the names actualBudget and actualExpenses. Keep financial language concrete and amounts exact.

## Evidence on Hand
Canonical Obsidian rewrite spec and ticket02; pinned Actual 26.10.0 reference source. No financial customer data or marketing claims are supplied.

## Product Principles
- Put the account and the current task first.
- Retain entered information when a request fails.
- Confirm persisted results rather than showing optimistic financial balances.
- Keep every working control accessible by keyboard and on mobile.

## Accessibility & Inclusion
Visible focus, explicit labels, useful errors, sensible reading order, reduced-motion support and responsive layouts are required by the specification.
