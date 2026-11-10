-- Rebuild only the banking family inside the existing migration transaction.
-- Child copies prevent DROP-parent cascade from discarding any retained data.
CREATE TEMP TABLE bank25_copy_bank_connections AS SELECT * FROM bank_connections;
CREATE TEMP TABLE bank25_copy_bank_credentials AS SELECT * FROM bank_credentials;
CREATE TEMP TABLE bank25_copy_bank_remote_accounts AS SELECT * FROM bank_remote_accounts;
CREATE TEMP TABLE bank25_copy_bank_links AS SELECT * FROM bank_links;
CREATE TEMP TABLE bank25_copy_bank_jobs AS SELECT * FROM bank_jobs;
CREATE TEMP TABLE bank25_copy_bank_request_log AS SELECT * FROM bank_request_log;
CREATE TEMP TABLE bank25_copy_bank_transaction_origins AS SELECT * FROM bank_transaction_origins;
DROP TABLE bank_transaction_origins;
DROP TABLE bank_request_log;
DROP TABLE bank_jobs;
DROP TABLE bank_links;
DROP TABLE bank_remote_accounts;
DROP TABLE bank_credentials;
DROP TABLE bank_connections;
-- Portable connection metadata; credentials deliberately absent from archive registry.
CREATE TABLE bank_connections (
 budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL,
 provider TEXT NOT NULL CHECK(provider IN('simplefin','gocardless')), name TEXT NOT NULL,
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
 institution TEXT NOT NULL,name TEXT NOT NULL,currency TEXT NOT NULL,balance TEXT NOT NULL,available_balance TEXT,
 balance_date INTEGER NOT NULL,diagnostics TEXT NOT NULL DEFAULT '[]',tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id),FOREIGN KEY(budget_id,connection_id) REFERENCES bank_connections(budget_id,id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX bank_remote_identity ON bank_remote_accounts(budget_id,connection_id,remote_connection_id,remote_id);
CREATE TABLE bank_links (
 budget_id TEXT NOT NULL,id TEXT NOT NULL,connection_id TEXT NOT NULL,remote_account_id TEXT NOT NULL,
 connection_generation TEXT NOT NULL,revision INTEGER NOT NULL DEFAULT 1,
 initial_sync INTEGER NOT NULL DEFAULT 1 CHECK(initial_sync IN(0,1)),
 opening_mode TEXT NOT NULL DEFAULT 'auto' CHECK(opening_mode IN('auto','none','custom')),
 opening_amount INTEGER,opening_date TEXT,last_sync_end TEXT,
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
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

CREATE TABLE bank_transaction_origins (
 budget_id TEXT NOT NULL,id TEXT NOT NULL,connection_id TEXT NOT NULL,remote_account_id TEXT NOT NULL,
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id),FOREIGN KEY(budget_id,id) REFERENCES transactions(budget_id,id),
 FOREIGN KEY(budget_id,connection_id) REFERENCES bank_connections(budget_id,id) ON DELETE CASCADE,
 FOREIGN KEY(budget_id,remote_account_id) REFERENCES bank_remote_accounts(budget_id,id)
);

INSERT INTO bank_connections SELECT * FROM bank25_copy_bank_connections;
DROP TABLE bank25_copy_bank_connections;
INSERT INTO bank_credentials SELECT * FROM bank25_copy_bank_credentials;
DROP TABLE bank25_copy_bank_credentials;
INSERT INTO bank_remote_accounts SELECT * FROM bank25_copy_bank_remote_accounts;
DROP TABLE bank25_copy_bank_remote_accounts;
INSERT INTO bank_links SELECT * FROM bank25_copy_bank_links;
DROP TABLE bank25_copy_bank_links;
INSERT INTO bank_jobs SELECT * FROM bank25_copy_bank_jobs;
DROP TABLE bank25_copy_bank_jobs;
INSERT INTO bank_request_log SELECT * FROM bank25_copy_bank_request_log;
DROP TABLE bank25_copy_bank_request_log;
INSERT INTO bank_transaction_origins SELECT * FROM bank25_copy_bank_transaction_origins;
DROP TABLE bank25_copy_bank_transaction_origins;

-- Server-only consent custody. Never registered as portable or change-feed state.
CREATE TABLE bank_gocardless_credentials (
 budget_id TEXT NOT NULL,connection_id TEXT NOT NULL,generation TEXT NOT NULL,
 secret_id TEXT NOT NULL,secret_key TEXT NOT NULL,
 access_token TEXT,refresh_token TEXT,access_expires INTEGER,
 requisition_id TEXT,institution_id TEXT,country TEXT,authorization_url TEXT,
 consent_status TEXT,intent_id TEXT,intent_fingerprint TEXT,
 PRIMARY KEY(budget_id,connection_id),
 FOREIGN KEY(budget_id,connection_id) REFERENCES bank_connections(budget_id,id) ON DELETE CASCADE
);
-- New SimpleFIN one-use claims retain their original in-flight identity too.
-- Interrupted pre25 claims remain conservative and require replacement.
ALTER TABLE bank_credentials ADD COLUMN intent_id TEXT;
ALTER TABLE bank_credentials ADD COLUMN intent_fingerprint TEXT;
