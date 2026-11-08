---
status: accepted
---

# Maintain an independent Actual fork

The user expects substantial divergence from Actual and chose not to maintain routine synchronization with upstream. Organize the combined repository for independent development rather than optimizing for recurring upstream merges. This avoids ongoing merge constraints but makes security fixes, dependency maintenance and client/server compatibility our responsibility; individual upstream fixes may still be adopted deliberately.

Start from an Actual 26.10.0 source snapshot without importing upstream Git history, while preserving ActualExpenses' existing history. Record the exact upstream commit and retain applicable license notices. This gives independent development a clear baseline without adding upstream history to the combined repository; historical upstream investigation remains available through the recorded source reference.

Keep the applications together in this repository under `actualExpenses/` (Android expense tracking) and `actualBudget/` (full desktop/web budgeting and sync server). Separate build tools preserve each application's build structure while shared root documentation and workflows coordinate delivery.
