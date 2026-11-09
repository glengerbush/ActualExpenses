ALTER TABLE transactions ADD COLUMN is_parent INTEGER NOT NULL DEFAULT 0 CHECK(is_parent IN (0,1));
ALTER TABLE transactions ADD COLUMN parent_id TEXT;
CREATE INDEX transactions_children ON transactions(budget_id,parent_id,tombstone,sort_order DESC,id);
