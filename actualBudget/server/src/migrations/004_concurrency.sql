CREATE TABLE budget_sync (
 budget_id TEXT PRIMARY KEY REFERENCES budgets(id), epoch TEXT NOT NULL,
 sequence INTEGER NOT NULL DEFAULT 0 CHECK(typeof(sequence)='integer' AND sequence>=0),
 clock TEXT NOT NULL DEFAULT '1970-01-01T00:00:00.000Z-0000-0000000000000000', node TEXT NOT NULL
);
INSERT INTO budget_sync(budget_id,epoch,node) SELECT id,lower(hex(randomblob(16))),lower(hex(randomblob(8))) FROM budgets;
CREATE TRIGGER budget_sync_created AFTER INSERT ON budgets BEGIN
 INSERT INTO budget_sync(budget_id,epoch,node) VALUES(NEW.id,lower(hex(randomblob(16))),lower(hex(randomblob(8))));
END;
CREATE TABLE field_versions (
 budget_id TEXT NOT NULL REFERENCES budgets(id), entity TEXT NOT NULL, entity_id TEXT NOT NULL,
 field TEXT NOT NULL, clock TEXT NOT NULL, PRIMARY KEY(budget_id,entity,entity_id,field)
);
-- Latest state per entity suffices for incremental convergence. Never remove tombstones
-- or this index while clients may retain a cursor; no edit timestamp is a checkpoint.
CREATE TABLE budget_changes (
 budget_id TEXT NOT NULL REFERENCES budgets(id), entity TEXT NOT NULL, entity_id TEXT NOT NULL,
 sequence INTEGER NOT NULL, PRIMARY KEY(budget_id,entity,entity_id)
);
CREATE INDEX budget_changes_sequence ON budget_changes(budget_id,sequence);
CREATE TRIGGER change_transactions_insert AFTER INSERT ON transactions BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'transactions',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
CREATE TRIGGER change_transactions_update AFTER UPDATE ON transactions WHEN NEW.account_id IS NOT OLD.account_id OR NEW.amount IS NOT OLD.amount OR NEW.date IS NOT OLD.date OR NEW.cleared IS NOT OLD.cleared OR NEW.reconciled IS NOT OLD.reconciled OR NEW.starting_balance IS NOT OLD.starting_balance OR NEW.payee_id IS NOT OLD.payee_id OR NEW.category_id IS NOT OLD.category_id OR NEW.notes IS NOT OLD.notes OR NEW.tombstone IS NOT OLD.tombstone OR NEW.sort_order IS NOT OLD.sort_order BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'transactions',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
CREATE TRIGGER change_accounts_insert AFTER INSERT ON accounts BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'accounts',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
CREATE TRIGGER change_accounts_update AFTER UPDATE ON accounts WHEN NEW.name IS NOT OLD.name OR NEW.notes IS NOT OLD.notes OR NEW.off_budget IS NOT OLD.off_budget OR NEW.closed IS NOT OLD.closed BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'accounts',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
CREATE TRIGGER change_payees_insert AFTER INSERT ON payees BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'payees',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
CREATE TRIGGER change_payees_update AFTER UPDATE ON payees WHEN NEW.name IS NOT OLD.name BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'payees',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
CREATE TRIGGER change_categories_insert AFTER INSERT ON categories BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'categories',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
CREATE TRIGGER change_categories_update AFTER UPDATE ON categories WHEN NEW.name IS NOT OLD.name OR NEW.is_income IS NOT OLD.is_income BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'categories',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
CREATE TRIGGER change_ledger_totals_insert AFTER INSERT ON ledger_totals BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'accounts',NEW.account_id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
CREATE TRIGGER change_ledger_totals_update AFTER UPDATE ON ledger_totals WHEN NEW.total IS NOT OLD.total OR NEW.cleared IS NOT OLD.cleared OR NEW.uncleared IS NOT OLD.uncleared BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'accounts',NEW.account_id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
CREATE TRIGGER change_budget_preferences_insert AFTER INSERT ON budget_preferences BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'preferences',NEW.budget_id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
CREATE TRIGGER change_budget_preferences_update AFTER UPDATE ON budget_preferences WHEN NEW.settings IS NOT OLD.settings OR NEW.revision IS NOT OLD.revision BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'preferences',NEW.budget_id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
