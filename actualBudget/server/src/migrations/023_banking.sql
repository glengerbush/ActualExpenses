-- Portable connection metadata; credentials deliberately absent from archive registry.
CREATE TABLE bank_connections (
 budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL,
 provider TEXT NOT NULL CHECK(provider='simplefin'), name TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'claim_pending' CHECK(state IN('claim_pending','claim_inflight','connected','claim_failed','credentials_missing')), generation TEXT NOT NULL,
 revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>=1),
 error_code TEXT, message TEXT, last_success TEXT,
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id)
);
-- Server custody only: never history/change payloads or ordinary native exports.
CREATE TABLE bank_credentials (
 budget_id TEXT NOT NULL, connection_id TEXT NOT NULL,
 generation TEXT NOT NULL, setup_token TEXT, access_url TEXT,
 PRIMARY KEY(budget_id,connection_id),
 FOREIGN KEY(budget_id,connection_id) REFERENCES bank_connections(budget_id,id) ON DELETE CASCADE
);
CREATE TABLE bank_remote_accounts (
 budget_id TEXT NOT NULL,id TEXT NOT NULL,connection_id TEXT NOT NULL,
 generation TEXT NOT NULL,remote_connection_id TEXT NOT NULL,remote_id TEXT NOT NULL,
 name TEXT NOT NULL,currency TEXT NOT NULL,balance TEXT NOT NULL,available_balance TEXT,
 balance_date INTEGER NOT NULL,diagnostics TEXT NOT NULL DEFAULT '[]',tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id),FOREIGN KEY(budget_id,connection_id) REFERENCES bank_connections(budget_id,id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX bank_remote_identity ON bank_remote_accounts(budget_id,connection_id,remote_connection_id,remote_id);
CREATE TABLE bank_links (
 budget_id TEXT NOT NULL,id TEXT NOT NULL,connection_id TEXT NOT NULL,remote_account_id TEXT NOT NULL,
 connection_generation TEXT NOT NULL,revision INTEGER NOT NULL DEFAULT 1,tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id),FOREIGN KEY(budget_id,id) REFERENCES accounts(budget_id,id),
 FOREIGN KEY(budget_id,connection_id) REFERENCES bank_connections(budget_id,id) ON DELETE CASCADE,
 FOREIGN KEY(budget_id,remote_account_id) REFERENCES bank_remote_accounts(budget_id,id)
);
CREATE UNIQUE INDEX bank_active_link ON bank_links(budget_id,remote_account_id) WHERE tombstone=0;
CREATE TABLE bank_jobs (
 budget_id TEXT NOT NULL,connection_id TEXT NOT NULL,id TEXT NOT NULL,
 generation TEXT NOT NULL,epoch TEXT NOT NULL,fingerprint TEXT NOT NULL,
 phase TEXT NOT NULL CHECK(phase IN('fetching','captured')),response TEXT,error_code TEXT,error_message TEXT,started_at INTEGER NOT NULL,
 PRIMARY KEY(budget_id,connection_id,id),FOREIGN KEY(budget_id,connection_id) REFERENCES bank_connections(budget_id,id) ON DELETE CASCADE
);
CREATE TABLE bank_request_log (
 budget_id TEXT NOT NULL,connection_id TEXT NOT NULL,started_at INTEGER NOT NULL,
 FOREIGN KEY(budget_id,connection_id) REFERENCES bank_connections(budget_id,id) ON DELETE CASCADE
);
CREATE INDEX bank_request_window ON bank_request_log(budget_id,connection_id,started_at);
