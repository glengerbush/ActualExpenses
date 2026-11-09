ALTER TABLE transactions ADD COLUMN transfer_id TEXT;
CREATE INDEX transactions_transfer ON transactions(budget_id,transfer_id);
