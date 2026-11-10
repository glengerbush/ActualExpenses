CREATE TRIGGER IF NOT EXISTS note_identity_accounts_insert AFTER INSERT ON accounts WHEN NEW.notes<>''
BEGIN INSERT INTO notes(budget_id,id,note) VALUES(NEW.budget_id,NEW.id,'') ON CONFLICT(budget_id,id) DO UPDATE SET note='',tombstone=0 WHERE notes.tombstone=1 OR notes.note=''; END;
CREATE TRIGGER IF NOT EXISTS note_identity_accounts_update AFTER UPDATE OF notes ON accounts WHEN NEW.notes IS NOT OLD.notes
BEGIN INSERT INTO notes(budget_id,id,note) VALUES(NEW.budget_id,NEW.id,'') ON CONFLICT(budget_id,id) DO UPDATE SET note='',tombstone=0 WHERE notes.tombstone=1 OR notes.note=''; END;
CREATE TRIGGER IF NOT EXISTS note_identity_categories_insert AFTER INSERT ON categories WHEN NEW.notes<>''
BEGIN INSERT INTO notes(budget_id,id,note) VALUES(NEW.budget_id,NEW.id,'') ON CONFLICT(budget_id,id) DO UPDATE SET note='',tombstone=0 WHERE notes.tombstone=1 OR notes.note=''; END;
CREATE TRIGGER IF NOT EXISTS note_identity_categories_update AFTER UPDATE OF notes ON categories WHEN NEW.notes IS NOT OLD.notes
BEGIN INSERT INTO notes(budget_id,id,note) VALUES(NEW.budget_id,NEW.id,'') ON CONFLICT(budget_id,id) DO UPDATE SET note='',tombstone=0 WHERE notes.tombstone=1 OR notes.note=''; END;
CREATE TRIGGER IF NOT EXISTS note_identity_category_groups_insert AFTER INSERT ON category_groups WHEN NEW.notes<>''
BEGIN INSERT INTO notes(budget_id,id,note) VALUES(NEW.budget_id,NEW.id,'') ON CONFLICT(budget_id,id) DO UPDATE SET note='',tombstone=0 WHERE notes.tombstone=1 OR notes.note=''; END;
CREATE TRIGGER IF NOT EXISTS note_identity_category_groups_update AFTER UPDATE OF notes ON category_groups WHEN NEW.notes IS NOT OLD.notes
BEGIN INSERT INTO notes(budget_id,id,note) VALUES(NEW.budget_id,NEW.id,'') ON CONFLICT(budget_id,id) DO UPDATE SET note='',tombstone=0 WHERE notes.tombstone=1 OR notes.note=''; END;
CREATE TRIGGER IF NOT EXISTS note_identity_budget_months_insert AFTER INSERT ON budget_months WHEN NEW.notes<>''
BEGIN INSERT INTO notes(budget_id,id,note) VALUES(NEW.budget_id,'budget-'||NEW.id,'') ON CONFLICT(budget_id,id) DO UPDATE SET note='',tombstone=0 WHERE notes.tombstone=1 OR notes.note=''; END;
CREATE TRIGGER IF NOT EXISTS note_identity_budget_months_update AFTER UPDATE OF notes ON budget_months WHEN NEW.notes IS NOT OLD.notes
BEGIN INSERT INTO notes(budget_id,id,note) VALUES(NEW.budget_id,'budget-'||NEW.id,'') ON CONFLICT(budget_id,id) DO UPDATE SET note='',tombstone=0 WHERE notes.tombstone=1 OR notes.note=''; END;
