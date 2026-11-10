CREATE TABLE reporting_documents (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN('page','widget','custom_report')),
 config TEXT NOT NULL CHECK(json_valid(config)),
 revision INTEGER NOT NULL CHECK(revision>=1),
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id)
);
