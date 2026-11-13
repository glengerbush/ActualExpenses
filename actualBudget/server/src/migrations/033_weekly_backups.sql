DROP TRIGGER lifecycle_budget_changed;
DROP TRIGGER lifecycle_budget_renamed;
ALTER TABLE backup_policy RENAME TO old_backup_policy;
CREATE TABLE backup_policy (id INTEGER PRIMARY KEY CHECK(id=1), interval_seconds INTEGER NOT NULL CHECK(interval_seconds BETWEEN 0 AND 604800));
INSERT INTO backup_policy VALUES(1,604800);
DROP TABLE old_backup_policy;
ALTER TABLE budget_lifecycle ADD COLUMN auto_dirty INTEGER NOT NULL DEFAULT 0 CHECK(auto_dirty IN (0,1));
UPDATE budget_lifecycle SET auto_dirty=1 WHERE auto_due IS NOT NULL;
UPDATE budget_lifecycle SET auto_due=unixepoch('now')+604800 WHERE auto_due IS NOT NULL;
CREATE TRIGGER lifecycle_budget_changed AFTER UPDATE OF sequence ON budget_sync WHEN NEW.sequence IS NOT OLD.sequence BEGIN
 UPDATE budget_lifecycle SET auto_dirty=1,auto_due=CASE WHEN (SELECT interval_seconds FROM backup_policy WHERE id=1)>0 THEN COALESCE(auto_due,unixepoch('now')+(SELECT interval_seconds FROM backup_policy WHERE id=1)) ELSE NULL END WHERE budget_id=NEW.budget_id;
END;
CREATE TRIGGER lifecycle_budget_renamed AFTER UPDATE OF name,budget_type ON budgets WHEN NEW.name IS NOT OLD.name OR NEW.budget_type IS NOT OLD.budget_type BEGIN
 UPDATE budget_lifecycle SET auto_dirty=1,auto_due=CASE WHEN (SELECT interval_seconds FROM backup_policy WHERE id=1)>0 THEN COALESCE(auto_due,unixepoch('now')+(SELECT interval_seconds FROM backup_policy WHERE id=1)) ELSE NULL END WHERE budget_id=NEW.id;
END;
