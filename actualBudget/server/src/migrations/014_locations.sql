CREATE TABLE payee_locations (
 budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL,
 payee_id TEXT NOT NULL, latitude REAL NOT NULL CHECK(latitude BETWEEN -90 AND 90),
 longitude REAL NOT NULL CHECK(longitude BETWEEN -180 AND 180),
 created_at INTEGER NOT NULL CHECK(created_at BETWEEN 0 AND 9007199254740991),
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN (0,1)),
 PRIMARY KEY(budget_id,id), FOREIGN KEY(budget_id,payee_id) REFERENCES payees(budget_id,id)
);
CREATE INDEX payee_locations_payee ON payee_locations(budget_id,payee_id,tombstone);
CREATE TRIGGER change_locations_insert AFTER INSERT ON payee_locations BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence) VALUES(NEW.budget_id,'payee_locations',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id)) ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
CREATE TRIGGER change_locations_update AFTER UPDATE ON payee_locations BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence) VALUES(NEW.budget_id,'payee_locations',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id)) ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;
