ALTER TABLE budgets ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0,1));
CREATE TABLE budget_lifecycle (
 budget_id TEXT PRIMARY KEY REFERENCES budgets(id), generation INTEGER NOT NULL DEFAULT 0 CHECK(generation>=0),
 restored_at TEXT, auto_due INTEGER, last_auto INTEGER, backup_error TEXT
);
INSERT INTO budget_lifecycle(budget_id) SELECT id FROM budgets;
CREATE TRIGGER lifecycle_budget_created AFTER INSERT ON budgets BEGIN
 INSERT INTO budget_lifecycle(budget_id) VALUES(NEW.id);
END;
CREATE TABLE backup_policy (id INTEGER PRIMARY KEY CHECK(id=1), interval_seconds INTEGER NOT NULL CHECK(interval_seconds BETWEEN 1 AND 86400));
INSERT INTO backup_policy VALUES(1,900);
CREATE TRIGGER lifecycle_budget_changed AFTER UPDATE OF sequence ON budget_sync WHEN NEW.sequence IS NOT OLD.sequence BEGIN
 UPDATE budget_lifecycle SET auto_due=COALESCE(auto_due,unixepoch('now')+(SELECT interval_seconds FROM backup_policy WHERE id=1)) WHERE budget_id=NEW.budget_id;
END;
CREATE TABLE budget_backups (
 id TEXT PRIMARY KEY, budget_id TEXT NOT NULL REFERENCES budgets(id), label TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('manual','automatic','before_restore','before_delete','upgrade')),
 created_at TEXT NOT NULL, archive_sha256 TEXT NOT NULL, archive BLOB NOT NULL, source_sequence INTEGER NOT NULL
);
CREATE INDEX budget_backups_budget_time ON budget_backups(budget_id,created_at DESC,id);
CREATE TABLE native_restore_stages (
 id TEXT PRIMARY KEY, archive_sha256 TEXT NOT NULL UNIQUE, original BLOB NOT NULL,
 budget_id TEXT NOT NULL, name TEXT NOT NULL, created_at TEXT NOT NULL, report TEXT NOT NULL
);
CREATE TABLE operation_budget_links (
 budget_id TEXT NOT NULL REFERENCES budgets(id), scope TEXT NOT NULL, operation_id TEXT NOT NULL,
 PRIMARY KEY(budget_id,scope,operation_id),
 FOREIGN KEY(scope,operation_id) REFERENCES operations(scope,operation_id)
);
CREATE INDEX transactions_companion_expenses ON transactions(budget_id,account_id,date DESC,sort_order DESC,id ASC) WHERE tombstone=0 AND reconciled=0 AND amount<0;
CREATE TABLE budget_provenance (
 budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL, kind TEXT NOT NULL,
 source_budget_id TEXT NOT NULL, metadata TEXT NOT NULL, archive_sha256 TEXT, original BLOB,
 PRIMARY KEY(budget_id,id)
);
CREATE TRIGGER lifecycle_budget_renamed AFTER UPDATE OF name,budget_type ON budgets WHEN NEW.name IS NOT OLD.name OR NEW.budget_type IS NOT OLD.budget_type BEGIN
 UPDATE budget_lifecycle SET auto_due=COALESCE(auto_due,unixepoch('now')+(SELECT interval_seconds FROM backup_policy WHERE id=1)) WHERE budget_id=NEW.id;
END;
