CREATE TABLE staged_imports (
    id TEXT PRIMARY KEY,
    archive_sha256 TEXT NOT NULL UNIQUE,
    original BLOB NOT NULL,
    budget_id TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL CHECK(status IN ('ready','blocked','activated')),
    report TEXT NOT NULL,
    conversion TEXT NOT NULL
);
CREATE TABLE imported_sources (
    budget_id TEXT PRIMARY KEY REFERENCES budgets(id),
    import_id TEXT NOT NULL UNIQUE REFERENCES staged_imports(id),
    source_local_id TEXT NOT NULL,
    source_cloud_id TEXT,
    source_group_id TEXT,
    source_metadata TEXT NOT NULL
);
CREATE TABLE imported_field_clocks (
    budget_id TEXT NOT NULL REFERENCES budgets(id),
    dataset TEXT NOT NULL, row_id TEXT NOT NULL, column_name TEXT NOT NULL,
    edit_order TEXT NOT NULL, serialized_value TEXT NOT NULL,
    PRIMARY KEY(budget_id,dataset,row_id,column_name)
);
