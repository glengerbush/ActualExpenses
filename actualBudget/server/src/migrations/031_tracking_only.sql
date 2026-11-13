-- Domain normalization is performed transactionally by budgeting::normalize.
DROP TRIGGER IF EXISTS note_identity_budget_months_insert;
DROP TRIGGER IF EXISTS note_identity_budget_months_update;
-- Historical undo must never reactivate the removed method.
DROP TRIGGER budgeting_method;
CREATE TRIGGER budgeting_method AFTER UPDATE OF method ON budget_settings BEGIN
 UPDATE budget_settings SET method='tracking' WHERE budget_id=NEW.budget_id AND method<>'tracking';
 UPDATE budgets SET budget_type='tracking' WHERE id=NEW.budget_id;
END;
