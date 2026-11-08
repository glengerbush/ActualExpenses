CREATE TABLE budget_preferences (
    budget_id TEXT PRIMARY KEY REFERENCES budgets(id),
    settings TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0)
);
