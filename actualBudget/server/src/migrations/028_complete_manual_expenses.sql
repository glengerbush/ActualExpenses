-- Manual entries have no settlement or reconciliation lifecycle. Keep legacy
-- columns as constant compatibility values for archives and older companions.
UPDATE transactions SET cleared=1,reconciled=0;
UPDATE ledger_totals SET cleared=total,uncleared=0;
UPDATE accounts SET last_reconciled=NULL;
CREATE TRIGGER manual_entries_insert AFTER INSERT ON transactions
WHEN NEW.cleared<>1 OR NEW.reconciled<>0
BEGIN
 UPDATE transactions SET cleared=1,reconciled=0 WHERE budget_id=NEW.budget_id AND id=NEW.id;
END;
CREATE TRIGGER manual_entries_update AFTER UPDATE OF cleared,reconciled ON transactions
WHEN NEW.cleared<>1 OR NEW.reconciled<>0
BEGIN
 UPDATE transactions SET cleared=1,reconciled=0 WHERE budget_id=NEW.budget_id AND id=NEW.id;
END;
CREATE TRIGGER manual_balances_insert AFTER INSERT ON ledger_totals
WHEN NEW.cleared<>NEW.total OR NEW.uncleared<>0
BEGIN
 UPDATE ledger_totals SET cleared=total,uncleared=0 WHERE budget_id=NEW.budget_id AND account_id=NEW.account_id;
END;
CREATE TRIGGER manual_balances_update AFTER UPDATE ON ledger_totals
WHEN NEW.cleared<>NEW.total OR NEW.uncleared<>0
BEGIN
 UPDATE ledger_totals SET cleared=total,uncleared=0 WHERE budget_id=NEW.budget_id AND account_id=NEW.account_id;
END;
