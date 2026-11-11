---
status: accepted
date: 2026-11-11
---

# One transaction history per budget

The user replaced account organization with a single transaction history and useful filtered views. Android uses the same budget scope, without password or account selection. Web navigation switches directly between views, including Budget; a view does not lock the sidebar. Durable transaction drafts survive switching views.

## Compatibility

Preserve stored account IDs, historical transactions, transfer pairs, off-budget classifications, balances, imported report definitions and pending operations. These remain internal compatibility data; do not merge, delete or rewrite financial records to change the UI.

`POST /api/v1/budgets/{budget}/ledger` prepares an internal destination for new transactions. It reuses the first open on-budget destination by existing order, or creates one with zero balance. It is naturally idempotent, serialized in a SQLite transaction and guarded by the budget epoch. No schema migration is needed. The server advertises `unified_ledger`; new clients use this capability.

The companion endpoint accepts an omitted `account_id`, returning the latest ten eligible expenses across the budget plus explicitly retained older entries. Existing account-filtered API requests still work. Android retains the original account reference when editing or deleting an old expense, preserves its durable outbox and keeps the newest backdated addition visible after delivery/restart. Historical closed destinations remain editable in unified mode; deleted destinations still block pending writes rather than resurrecting data.

## Product behavior

- Web opens Transaction history; filters, saved views, imports, schedules, reports and budgeting remain. Upcoming scheduled entries appear beneath the shared register. CSV export covers the entire history.
- Account creation, account navigation, account picking, new transfers between accounts and per-account reconciliation are no longer normal UI workflows. Recovery of old pending account operations and editing existing imported transfers remain available.
- Android setup is server address → budget → expenses. The existing native API already required no password; the UI now also removes account selection. New expense destination is automatic.
- Sidebar and command navigation remain available within views. Browser and Android retain their existing limited offline behavior and conflict handling.

This explicitly supersedes the account-organization portions of the prior Actual 26.10 parity scope. Financial compatibility APIs and their tests remain. The seven old browser suites dedicated to account setup/navigation, per-account reconciliation and creating/linking transfers are retained as historical source but removed from the active browser command list; `unified-history-browser.mjs` covers the replacement workflow. Mixed suites now exercise transaction history and retain their applicable financial, recovery, formatting and navigation checks.

No version bump, push, public release or remote deployment is part of this change. The local preview uses its existing isolated persistent volume, with a stopped-container data copy taken before replacement.
