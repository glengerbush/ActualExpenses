-- Immutable source ordering is separate from native ranks and owned history.
CREATE TABLE android_upgrade_order (
 budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL,
 source_order REAL, display_key TEXT NOT NULL,
 PRIMARY KEY(budget_id,id)
);
CREATE INDEX android_upgrade_source_order ON android_upgrade_order(budget_id,source_order);
