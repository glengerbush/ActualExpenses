ALTER TABLE payees ADD COLUMN learn_categories INTEGER NOT NULL DEFAULT 1 CHECK(learn_categories IN(0,1));
CREATE TABLE rules (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL,
 stage TEXT CHECK(stage IN('pre','post') OR stage IS NULL),
 conditions_op TEXT NOT NULL CHECK(conditions_op IN('and','or')),
 conditions TEXT NOT NULL,actions TEXT NOT NULL,
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id)
);
CREATE TABLE rule_settings (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL DEFAULT 'learning' CHECK(id='learning'),
 enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN(0,1)),
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone=0),PRIMARY KEY(budget_id,id)
);
INSERT INTO rule_settings(budget_id) SELECT id FROM budgets;
CREATE TRIGGER rules_budget_created AFTER INSERT ON budgets BEGIN
 INSERT INTO rule_settings(budget_id) VALUES(NEW.id);
END;
