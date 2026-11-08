CREATE TABLE budgets (
    id TEXT PRIMARY KEY, name TEXT NOT NULL,
    budget_type TEXT NOT NULL CHECK(budget_type IN ('tracking','envelope'))
);
CREATE TABLE accounts (
    budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL,
    name TEXT NOT NULL, notes TEXT NOT NULL DEFAULT '',
    off_budget INTEGER NOT NULL CHECK(off_budget IN (0,1)),
    closed INTEGER NOT NULL DEFAULT 0 CHECK(closed IN (0,1)),
    PRIMARY KEY(budget_id,id)
);
CREATE TABLE payees (
    budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL,
    name TEXT NOT NULL, PRIMARY KEY(budget_id,id)
);
CREATE TABLE categories (
    budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL,
    name TEXT NOT NULL, is_income INTEGER NOT NULL CHECK(is_income IN (0,1)),
    PRIMARY KEY(budget_id,id)
);
CREATE TABLE transactions (
    budget_id TEXT NOT NULL, id TEXT NOT NULL, account_id TEXT NOT NULL,
    amount INTEGER NOT NULL CHECK(amount BETWEEN -9007199254740991 AND 9007199254740991),
    date TEXT NOT NULL,
    cleared INTEGER NOT NULL DEFAULT 0 CHECK(cleared IN (0,1)),
    reconciled INTEGER NOT NULL DEFAULT 0 CHECK(reconciled IN (0,1)),
    starting_balance INTEGER NOT NULL DEFAULT 0 CHECK(starting_balance IN (0,1)),
    payee_id TEXT, category_id TEXT,
    PRIMARY KEY(budget_id,id),
    FOREIGN KEY(budget_id,account_id) REFERENCES accounts(budget_id,id),
    FOREIGN KEY(budget_id,payee_id) REFERENCES payees(budget_id,id),
    FOREIGN KEY(budget_id,category_id) REFERENCES categories(budget_id,id)
);
CREATE INDEX transactions_account_date ON transactions(budget_id,account_id,date DESC,id);

-- Never expire blindly: queued client retries must retain exactly-once effects.
CREATE TABLE operations (
    scope TEXT NOT NULL, operation_id TEXT NOT NULL, payload TEXT NOT NULL,
    response TEXT NOT NULL, PRIMARY KEY(scope,operation_id)
);
