# Manual-only account scope

User decision,2026-11-10: “you can remove all connections to external accounts.
this budgeting app will be fully manual!” This supersedes earlier provider parity
requirements in the canonical Obsidian spec and tickets.

Remove SimpleFIN, GoCardless, Pluggy, Enable Banking and Akahu integrations:
authorization, credentials, linking, provider balances, refresh, automatic bank
sync, operational configuration, UI and API capabilities. No external credential
revocation or production changes are requested.

Retain manual accounts, transaction entry, statement reconciliation, file
imports, historical imported identifiers, internal rules/schedules, the Android
companion and the application's public API. Previously linked accounts become
manual accounts without changing financial identities, amounts or relationships.
Upgrade/import removes operational credentials and reports the conversion;
original uploads and recovery artifacts remain unchanged.

Ticket29 now owns removal and its financial-preservation checks. Previously
accepted SimpleFIN28 is superseded;30–32 are canceled, not implemented. Historical
bank validation and frozen fixtures remain evidence, not an active feature
contract. Story IDs58–60 are explicit scope exceptions;61 retains manual import
preferences/identity;62 covers credential retirement. Stories13/100 no longer
require live provider balances/bank refresh. Final50–53 qualify the revised scope.

Removal is implemented and reviewed locally; see [retirement evidence](bank-retirement-validation.md).
The remaining rewrite tickets and whole-system qualification are still open.
