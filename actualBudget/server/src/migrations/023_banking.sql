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
