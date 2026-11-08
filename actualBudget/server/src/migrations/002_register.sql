ALTER TABLE transactions ADD COLUMN notes TEXT NOT NULL DEFAULT '';
ALTER TABLE transactions ADD COLUMN tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN (0,1));
ALTER TABLE transactions ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
UPDATE transactions SET sort_order=rowid;
CREATE INDEX transactions_register ON transactions(budget_id,tombstone,date,sort_order,id);
CREATE TABLE ledger_totals (
    budget_id TEXT NOT NULL,account_id TEXT NOT NULL,
    total INTEGER NOT NULL,cleared INTEGER NOT NULL,uncleared INTEGER NOT NULL,
    PRIMARY KEY(budget_id,account_id),
    FOREIGN KEY(budget_id,account_id) REFERENCES accounts(budget_id,id)
);
INSERT INTO ledger_totals(budget_id,account_id,total,cleared,uncleared)
SELECT a.budget_id,a.id,COALESCE(SUM(t.amount),0),COALESCE(SUM(CASE WHEN t.cleared=1 THEN t.amount ELSE 0 END),0),COALESCE(SUM(CASE WHEN t.cleared=0 THEN t.amount ELSE 0 END),0)
FROM accounts a LEFT JOIN transactions t ON t.budget_id=a.budget_id AND t.account_id=a.id AND t.tombstone=0 GROUP BY a.budget_id,a.id;
CREATE INDEX transactions_account_active_order ON transactions(budget_id,account_id,tombstone,date DESC,sort_order DESC,id DESC);
CREATE INDEX transactions_budget_order ON transactions(budget_id,sort_order DESC);
