ALTER TABLE payees ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0 CHECK(favorite IN (0,1));
ALTER TABLE payees ADD COLUMN tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN (0,1));
ALTER TABLE payees ADD COLUMN transfer_account_id TEXT;
ALTER TABLE payees ADD COLUMN category_id TEXT;
ALTER TABLE payees ADD COLUMN merged_into TEXT;
ALTER TABLE transactions ADD COLUMN imported_payee TEXT;
CREATE TABLE tags (
 budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL, tag TEXT NOT NULL,
 color TEXT, description TEXT, hidden INTEGER NOT NULL DEFAULT 0 CHECK(hidden IN(0,1)),
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)), PRIMARY KEY(budget_id,id), UNIQUE(budget_id,tag)
);
DROP TRIGGER change_payees_update;
CREATE TRIGGER change_payees_update AFTER UPDATE ON payees BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence) VALUES(NEW.budget_id,'payees',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id)) ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
CREATE TRIGGER change_tags_insert AFTER INSERT ON tags BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence) VALUES(NEW.budget_id,'tags',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id)) ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
CREATE TRIGGER change_tags_update AFTER UPDATE ON tags BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence) VALUES(NEW.budget_id,'tags',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id)) ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
