-- Restore transaction notes without restoring budget or category notes.
DROP TRIGGER IF EXISTS retired_transactions_notes_insert;
DROP TRIGGER IF EXISTS retired_transactions_notes_update;
