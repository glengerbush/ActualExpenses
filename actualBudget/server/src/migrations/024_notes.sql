CREATE TABLE notes (
 budget_id TEXT NOT NULL REFERENCES budgets(id),
 id TEXT NOT NULL,note TEXT NOT NULL DEFAULT '',
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id)
);
