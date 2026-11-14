-- Several reviewed archives can target the same singleton. Preserve retained
-- originals and imported-source evidence while removing the old one-stage-per-budget constraint.
CREATE TABLE staged_imports_single (
 id TEXT PRIMARY KEY, archive_sha256 TEXT NOT NULL UNIQUE, original BLOB NOT NULL,
 budget_id TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN('ready','blocked','activated')),
 report TEXT NOT NULL, conversion TEXT NOT NULL
);
INSERT INTO staged_imports_single SELECT * FROM staged_imports;
CREATE TABLE imported_sources_single (
 budget_id TEXT PRIMARY KEY REFERENCES budgets(id),
 import_id TEXT NOT NULL UNIQUE REFERENCES staged_imports_single(id),
 source_local_id TEXT NOT NULL, source_cloud_id TEXT, source_group_id TEXT,
 source_metadata TEXT NOT NULL
);
INSERT INTO imported_sources_single SELECT * FROM imported_sources;
DROP TABLE imported_sources;
DROP TABLE staged_imports;
ALTER TABLE staged_imports_single RENAME TO staged_imports;
ALTER TABLE imported_sources_single RENAME TO imported_sources;
-- The Rust migration merges active ledgers, then enforces one active identity.
