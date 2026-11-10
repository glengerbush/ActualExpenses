ALTER TABLE budget_settings ADD COLUMN template_currency TEXT;
ALTER TABLE budget_settings ADD COLUMN template_currency_review INTEGER NOT NULL DEFAULT 0 CHECK(template_currency_review IN(0,1));
CREATE TABLE budget_automations (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL,
 goal_def TEXT,cleanup_def TEXT,template_settings TEXT NOT NULL DEFAULT '{"source":"notes"}',
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id),FOREIGN KEY(budget_id,id) REFERENCES categories(budget_id,id)
);
CREATE TABLE budget_cleanup_groups (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL,name TEXT NOT NULL,
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),PRIMARY KEY(budget_id,id)
);
