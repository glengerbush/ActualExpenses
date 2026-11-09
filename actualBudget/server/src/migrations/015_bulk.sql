-- History belongs to one client engine and budget generation, never portable state.
CREATE TABLE history_actions (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 budget_id TEXT NOT NULL REFERENCES budgets(id), owner TEXT NOT NULL, epoch TEXT NOT NULL,
 id TEXT NOT NULL UNIQUE, label TEXT NOT NULL, undone INTEGER NOT NULL DEFAULT 0 CHECK(undone IN(0,1)),
 changes TEXT NOT NULL
);
CREATE INDEX history_owner ON history_actions(budget_id,owner,epoch,sequence);
-- A history-generated clock aliases the original field generation it restores.
-- Remote clocks never acquire aliases; equal values cannot hide intervening edits.
CREATE TABLE history_lineage (
 budget_id TEXT NOT NULL,owner TEXT NOT NULL,epoch TEXT NOT NULL,entity TEXT NOT NULL,entity_id TEXT NOT NULL,field TEXT NOT NULL,clock TEXT NOT NULL,lineage TEXT NOT NULL,
 PRIMARY KEY(budget_id,owner,epoch,entity,entity_id,field,clock)
);
