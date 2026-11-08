# ActualExpenses

<!-- impeccable:product-schema 1 -->

## Platform

android

## Stack

Delegated by the user: Kotlin, Jetpack Compose, native Android storage and pending-only WorkManager delivery. Direct Actual synchronization; no extra service.

## Users

Trusted Actual Budget users recording an expense at purchase, usually in Spending. The user approved the complete product interview and spec before implementation.

## Product Purpose

Save an expense with minimal input, including when disconnected. Review and correct the latest ten eligible expenses without opening the full budgeting interface.

## Operating Context

An Android phone, at shops and on the move, reaching an existing HTTP Actual server through LAN or tailnet. One unencrypted budget per installation; accounts may be switched. Tracking budgeting, automatically cleared expenses.

## Capabilities and Constraints

Setup is server/password, budget, account. Remember the account and secure session. Positive amount is required; payee, category, date and notes are optional. Cached payees and categories work offline. Preserve pending changes and show their state. Restrict offline editing to unsynced offline-created expenses, retaining an edit begun online if connectivity fails. Exclude transfers, income, splits and reconciled entries. Retain a backdated addition below the recent list. No idle background refresh.

## Brand Commitments

Name: ActualExpenses. The user requires a very limited interface: return to account selection, add an expense, review/edit/delete recent expenses. No marketing, reports, balance dashboards or invented product claims.

## Evidence on Hand

Approved Obsidian spec, CONTEXT glossary and direct-integration ADR. No incumbent UI, illustrations, logos or product screenshots.

## Product Principles

- Save locally before promising success.
- Keep the expense task obvious and short.
- Preserve Actual's semantics and explain pending state.
- Use familiar Android controls and system accessibility behavior.
