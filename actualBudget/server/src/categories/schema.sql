CREATE TABLE category_groups (
 budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL,
 name TEXT NOT NULL,is_income INTEGER NOT NULL DEFAULT 0 CHECK(is_income IN(0,1)),
 hidden INTEGER NOT NULL DEFAULT 0 CHECK(hidden IN(0,1)),notes TEXT NOT NULL DEFAULT '',
 sort_order REAL DEFAULT 0,tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id)
);
ALTER TABLE categories ADD COLUMN group_id TEXT;
ALTER TABLE categories ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0 CHECK(hidden IN(0,1));
ALTER TABLE categories ADD COLUMN notes TEXT NOT NULL DEFAULT '';
ALTER TABLE categories ADD COLUMN sort_order REAL DEFAULT 0;
ALTER TABLE categories ADD COLUMN tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1));
ALTER TABLE categories ADD COLUMN replacement_id TEXT;
CREATE INDEX category_order ON categories(budget_id,tombstone,group_id,sort_order);
CREATE INDEX transactions_category_history ON transactions(budget_id,category_id,tombstone,date DESC,sort_order DESC,id);
