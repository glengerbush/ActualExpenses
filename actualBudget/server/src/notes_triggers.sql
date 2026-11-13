CREATE TRIGGER IF NOT EXISTS note_identity_accounts_insert AFTER INSERT ON accounts WHEN NEW.notes<>''
BEGIN INSERT INTO notes(budget_id,id,note) VALUES(NEW.budget_id,NEW.id,'') ON CONFLICT(budget_id,id) DO UPDATE SET note='',tombstone=0 WHERE notes.tombstone=1 OR notes.note=''; END;
CREATE TRIGGER IF NOT EXISTS note_identity_accounts_update AFTER UPDATE OF notes ON accounts WHEN NEW.notes IS NOT OLD.notes
BEGIN INSERT INTO notes(budget_id,id,note) VALUES(NEW.budget_id,NEW.id,'') ON CONFLICT(budget_id,id) DO UPDATE SET note='',tombstone=0 WHERE notes.tombstone=1 OR notes.note=''; END;
