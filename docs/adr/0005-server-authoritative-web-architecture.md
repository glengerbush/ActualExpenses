---
status: accepted
---

# Centralize financial operations behind a lightweight web application

Use a static Svelte 5/TypeScript frontend, Rust/Axum backend and authoritative SQLite storage for actualBudget. The user prioritizes resource efficiency and does not need full offline budgeting; clients retain cached views and a durable expense outbox instead of replicating the budgeting engine. Remove application login, identities and permissions for the trusted-device deployment, while preserving external bank-provider authorization. Future desktop work may reuse the web application through Tauri and remains deferred.

This requires porting the financial engine, not merely replacing React and Express. Preserve Actual 26.10.0's observable financial functionality, including useful experimental features, except the explicitly removed authentication, full offline operation, encryption and desktop support. Resource improvements require measurement rather than assumptions about framework choice.

The replacement API supersedes ADR 0001's direct Actual-protocol strategy for the new server. ADR 0001 still documents the released Android client's existing integration. Deliver a coordinated Android adapter and migration that preserve queued work, signing identity and the existing expense workflow; unchanged APK compatibility is not assumed. Preserve per-field newest-edit ordering and the agreed pending-deletion rule. See the [canonical specification](</home/ggg/Documents/pileOfNotes/ActualExpenses/Issues/ActualBudget Web Rewrite Spec.md>).
