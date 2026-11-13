# Goal templates retired

Removed goal templates, visual budget automations, cleanup pools and goal progress from the web UI; removed their settings flags, saved-draft routing, public routes, advertised capabilities and calculation engine. Ordinary budgets, rules and recurring transactions remain.

Legacy goal metadata and financial allocations remain importable/restorable. Only storage and historical migration helpers remain; templates cannot execute. Old goal settings are stripped from exposed preferences and new attempts to enable them are rejected.

The obsolete template execution tests are replaced by `web/tests/no-goals-browser.mjs`: imports a genuine Actual export with goals, verifies unchanged transactions and allocations, stages a native backup, checks all retired routes/settings, and exercises Budget/Settings with an obsolete browser draft on desktop and mobile.
