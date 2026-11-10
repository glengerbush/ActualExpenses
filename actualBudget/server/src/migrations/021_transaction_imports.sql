CREATE TABLE transaction_import_files (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL,account_id TEXT NOT NULL,
 filename TEXT NOT NULL,sha256 TEXT NOT NULL,original BLOB NOT NULL,
 created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 PRIMARY KEY(budget_id,id),FOREIGN KEY(budget_id,account_id) REFERENCES accounts(budget_id,id)
);
CREATE INDEX transaction_import_file_digest ON transaction_import_files(budget_id,account_id,sha256,filename);
CREATE TABLE transaction_import_metadata (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL,
 imported_id TEXT,raw_synced_data TEXT,pending INTEGER NOT NULL DEFAULT 0 CHECK(pending IN(0,1)),
 import_type TEXT,tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id),FOREIGN KEY(budget_id,id) REFERENCES transactions(budget_id,id)
);
CREATE INDEX transaction_import_identity ON transaction_import_metadata(budget_id,imported_id);
-- Source preference IDs remain distinct cells; migration must not replace their
-- individual clocks with a maximum clock for one synthetic settings document.
CREATE TABLE transaction_import_settings (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL,account_id TEXT NOT NULL,
 option_key TEXT NOT NULL,value TEXT NOT NULL,tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id),FOREIGN KEY(budget_id,account_id) REFERENCES accounts(budget_id,id)
);
CREATE INDEX transaction_import_account_settings ON transaction_import_settings(budget_id,account_id,tombstone);
