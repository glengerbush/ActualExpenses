CREATE TABLE register_settings (
  budget_id TEXT PRIMARY KEY REFERENCES budgets(id) ON DELETE CASCADE,
  settings TEXT NOT NULL CHECK(json_valid(settings)),
  revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0)
);
-- Match the canonical opening/creation/identity ordering, including same-day
-- imported rows, without sorting the whole account for each first page.
DROP INDEX transactions_account_active_order;
CREATE INDEX transactions_account_active_order ON transactions(budget_id,account_id,tombstone,parent_id,date DESC,starting_balance ASC,sort_order DESC,id ASC);
DROP INDEX transactions_register;
CREATE INDEX transactions_register ON transactions(budget_id,tombstone,parent_id,date DESC,starting_balance ASC,sort_order DESC,id ASC);
