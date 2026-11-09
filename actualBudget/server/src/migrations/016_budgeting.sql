CREATE TABLE budget_settings (
 budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL DEFAULT 'method',
 method TEXT NOT NULL CHECK(method IN ('tracking','envelope')), tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone=0),
 PRIMARY KEY(budget_id,id), CHECK(id='method')
);
INSERT INTO budget_settings(budget_id,method) SELECT id,budget_type FROM budgets;
CREATE TRIGGER budgeting_created AFTER INSERT ON budgets BEGIN
 INSERT INTO budget_settings(budget_id,method) VALUES(NEW.id,NEW.budget_type);
END;
CREATE TRIGGER budgeting_method AFTER UPDATE OF method ON budget_settings BEGIN
 UPDATE budgets SET budget_type=NEW.method WHERE id=NEW.budget_id;
END;
CREATE TABLE budget_cells (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL,
 method TEXT NOT NULL CHECK(method IN ('tracking','envelope')),month TEXT NOT NULL,
 category_id TEXT NOT NULL,amount INTEGER NOT NULL DEFAULT 0 CHECK(typeof(amount)='integer' AND abs(amount)<=9007199254740991),
 carryover INTEGER NOT NULL DEFAULT 0 CHECK(carryover IN(0,1)),
 goal TEXT, long_goal TEXT,tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id),UNIQUE(budget_id,method,month,category_id),
 FOREIGN KEY(budget_id,category_id) REFERENCES categories(budget_id,id)
);
CREATE TABLE budget_months (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL,buffered INTEGER NOT NULL DEFAULT 0 CHECK(typeof(buffered)='integer' AND abs(buffered)<=9007199254740991),
 notes TEXT NOT NULL DEFAULT '',tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id)
);
