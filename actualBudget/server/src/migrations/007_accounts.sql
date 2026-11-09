CREATE TABLE account_groups (
 budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL, name TEXT NOT NULL,
 sort_order INTEGER NOT NULL DEFAULT 0 CHECK(typeof(sort_order)='integer'),
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN (0,1)), PRIMARY KEY(budget_id,id)
);
ALTER TABLE accounts ADD COLUMN group_id TEXT;
-- Budget-scoped group references are validated on writes and archive installation.

ALTER TABLE accounts ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0 CHECK(typeof(sort_order)='integer');
ALTER TABLE accounts ADD COLUMN tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN (0,1));
UPDATE accounts SET sort_order=(SELECT count(*) FROM accounts b WHERE b.budget_id=accounts.budget_id AND (b.name<accounts.name OR (b.name=accounts.name AND b.id<accounts.id)));
DROP TRIGGER change_accounts_update;
CREATE TRIGGER change_accounts_update AFTER UPDATE ON accounts WHEN NEW.name IS NOT OLD.name OR NEW.notes IS NOT OLD.notes OR NEW.off_budget IS NOT OLD.off_budget OR NEW.closed IS NOT OLD.closed OR NEW.group_id IS NOT OLD.group_id OR NEW.sort_order IS NOT OLD.sort_order OR NEW.tombstone IS NOT OLD.tombstone BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence) VALUES(NEW.budget_id,'accounts',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id)) ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
CREATE TRIGGER change_account_groups_insert AFTER INSERT ON account_groups BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence) VALUES(NEW.budget_id,'account_groups',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id)) ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
CREATE TRIGGER change_account_groups_update AFTER UPDATE ON account_groups BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence) VALUES(NEW.budget_id,'account_groups',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id)) ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
