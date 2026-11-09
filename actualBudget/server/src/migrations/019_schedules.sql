CREATE TABLE schedules (
 budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL, rule_id TEXT NOT NULL,
 name TEXT, posts_transaction INTEGER NOT NULL DEFAULT 0 CHECK(posts_transaction IN(0,1)),
 completed INTEGER NOT NULL DEFAULT 0 CHECK(completed IN(0,1)), next_date TEXT,
 custom_upcoming_length TEXT, tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id)
);
CREATE UNIQUE INDEX schedules_live_name ON schedules(budget_id,name) WHERE tombstone=0 AND name IS NOT NULL;
CREATE INDEX schedules_due ON schedules(budget_id,next_date) WHERE tombstone=0 AND completed=0;
CREATE TABLE schedule_occurrences (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL,schedule_id TEXT NOT NULL,
 occurrence_date TEXT NOT NULL,status TEXT NOT NULL CHECK(status IN('posted','skipped','matched')),
 transaction_id TEXT,tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id),UNIQUE(budget_id,schedule_id,occurrence_date)
);
CREATE TABLE schedule_links (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL,schedule_id TEXT NOT NULL,
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),PRIMARY KEY(budget_id,id)
);
CREATE INDEX schedule_links_schedule ON schedule_links(budget_id,schedule_id) WHERE tombstone=0;
CREATE TABLE schedule_settings (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL DEFAULT 'horizon' CHECK(id='horizon'),
 value TEXT NOT NULL DEFAULT '7',tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone=0),PRIMARY KEY(budget_id,id)
);
INSERT INTO schedule_settings(budget_id) SELECT id FROM budgets;
CREATE TRIGGER schedules_budget_created AFTER INSERT ON budgets BEGIN
 INSERT INTO schedule_settings(budget_id) VALUES(NEW.id);
END;
