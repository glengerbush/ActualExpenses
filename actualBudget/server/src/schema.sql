-- actualBudget 0.6.0 (schema 37): fresh-install baseline.

-- Historical migrations before the supported patch window are intentionally absent.

CREATE TABLE budgets (
    id TEXT PRIMARY KEY, name TEXT NOT NULL,
    budget_type TEXT NOT NULL CHECK(budget_type IN ('tracking','envelope'))
, deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0,1)));

CREATE TABLE accounts (
    budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL,
    name TEXT NOT NULL, notes TEXT NOT NULL DEFAULT '',
    off_budget INTEGER NOT NULL CHECK(off_budget IN (0,1)),
    closed INTEGER NOT NULL DEFAULT 0 CHECK(closed IN (0,1)), group_id TEXT, sort_order INTEGER NOT NULL DEFAULT 0 CHECK(typeof(sort_order)='integer'), tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN (0,1)), last_reconciled TEXT,
    PRIMARY KEY(budget_id,id)
);

CREATE TABLE payees (
    budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL,
    name TEXT NOT NULL, favorite INTEGER NOT NULL DEFAULT 0 CHECK(favorite IN (0,1)), tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN (0,1)), transfer_account_id TEXT, category_id TEXT, merged_into TEXT, learn_categories INTEGER NOT NULL DEFAULT 1 CHECK(learn_categories IN(0,1)), PRIMARY KEY(budget_id,id)
);

CREATE TABLE categories (
    budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL,
    name TEXT NOT NULL, is_income INTEGER NOT NULL CHECK(is_income IN (0,1)), group_id TEXT, hidden INTEGER NOT NULL DEFAULT 0 CHECK(hidden IN(0,1)), notes TEXT NOT NULL DEFAULT '', sort_order REAL DEFAULT 0, tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)), replacement_id TEXT, color TEXT,
    PRIMARY KEY(budget_id,id)
);

CREATE TABLE transactions (
    budget_id TEXT NOT NULL, id TEXT NOT NULL, account_id TEXT NOT NULL,
    amount INTEGER NOT NULL CHECK(amount BETWEEN -9007199254740991 AND 9007199254740991),
    date TEXT NOT NULL,
    cleared INTEGER NOT NULL DEFAULT 0 CHECK(cleared IN (0,1)),
    reconciled INTEGER NOT NULL DEFAULT 0 CHECK(reconciled IN (0,1)),
    starting_balance INTEGER NOT NULL DEFAULT 0 CHECK(starting_balance IN (0,1)),
    payee_id TEXT, category_id TEXT, notes TEXT NOT NULL DEFAULT '', tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN (0,1)), sort_order INTEGER NOT NULL DEFAULT 0, imported_payee TEXT, is_parent INTEGER NOT NULL DEFAULT 0 CHECK(is_parent IN (0,1)), parent_id TEXT, transfer_id TEXT,
    PRIMARY KEY(budget_id,id),
    FOREIGN KEY(budget_id,account_id) REFERENCES accounts(budget_id,id),
    FOREIGN KEY(budget_id,payee_id) REFERENCES payees(budget_id,id),
    FOREIGN KEY(budget_id,category_id) REFERENCES categories(budget_id,id)
);

CREATE TABLE operations (
    scope TEXT NOT NULL, operation_id TEXT NOT NULL, payload TEXT NOT NULL,
    response TEXT NOT NULL, PRIMARY KEY(scope,operation_id)
);

CREATE TABLE ledger_totals (
    budget_id TEXT NOT NULL,account_id TEXT NOT NULL,
    total INTEGER NOT NULL,cleared INTEGER NOT NULL,uncleared INTEGER NOT NULL,
    PRIMARY KEY(budget_id,account_id),
    FOREIGN KEY(budget_id,account_id) REFERENCES accounts(budget_id,id)
);

CREATE TABLE budget_preferences (
    budget_id TEXT PRIMARY KEY REFERENCES budgets(id),
    settings TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0)
);

CREATE TABLE budget_sync (
 budget_id TEXT PRIMARY KEY REFERENCES budgets(id), epoch TEXT NOT NULL,
 sequence INTEGER NOT NULL DEFAULT 0 CHECK(typeof(sequence)='integer' AND sequence>=0),
 clock TEXT NOT NULL DEFAULT '1970-01-01T00:00:00.000Z-0000-0000000000000000', node TEXT NOT NULL
);

CREATE TABLE field_versions (
 budget_id TEXT NOT NULL REFERENCES budgets(id), entity TEXT NOT NULL, entity_id TEXT NOT NULL,
 field TEXT NOT NULL, clock TEXT NOT NULL, PRIMARY KEY(budget_id,entity,entity_id,field)
);

CREATE TABLE budget_changes (
 budget_id TEXT NOT NULL REFERENCES budgets(id), entity TEXT NOT NULL, entity_id TEXT NOT NULL,
 sequence INTEGER NOT NULL, PRIMARY KEY(budget_id,entity,entity_id)
);

CREATE TABLE imported_field_clocks (
    budget_id TEXT NOT NULL REFERENCES budgets(id),
    dataset TEXT NOT NULL, row_id TEXT NOT NULL, column_name TEXT NOT NULL,
    edit_order TEXT NOT NULL, serialized_value TEXT NOT NULL,
    PRIMARY KEY(budget_id,dataset,row_id,column_name)
);

CREATE TABLE budget_lifecycle (
 budget_id TEXT PRIMARY KEY REFERENCES budgets(id), generation INTEGER NOT NULL DEFAULT 0 CHECK(generation>=0),
 restored_at TEXT, auto_due INTEGER, last_auto INTEGER, backup_error TEXT
, auto_dirty INTEGER NOT NULL DEFAULT 0 CHECK(auto_dirty IN (0,1)));

CREATE TABLE budget_backups (
 id TEXT PRIMARY KEY, budget_id TEXT NOT NULL REFERENCES budgets(id), label TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('manual','automatic','before_restore','before_delete','upgrade')),
 created_at TEXT NOT NULL, archive_sha256 TEXT NOT NULL, archive BLOB NOT NULL, source_sequence INTEGER NOT NULL
);

CREATE TABLE native_restore_stages (
 id TEXT PRIMARY KEY, archive_sha256 TEXT NOT NULL UNIQUE, original BLOB NOT NULL,
 budget_id TEXT NOT NULL, name TEXT NOT NULL, created_at TEXT NOT NULL, report TEXT NOT NULL
);

CREATE TABLE operation_budget_links (
 budget_id TEXT NOT NULL REFERENCES budgets(id), scope TEXT NOT NULL, operation_id TEXT NOT NULL,
 PRIMARY KEY(budget_id,scope,operation_id),
 FOREIGN KEY(scope,operation_id) REFERENCES operations(scope,operation_id)
);

CREATE TABLE budget_provenance (
 budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL, kind TEXT NOT NULL,
 source_budget_id TEXT NOT NULL, metadata TEXT NOT NULL, archive_sha256 TEXT, original BLOB,
 PRIMARY KEY(budget_id,id)
);

CREATE TABLE account_groups (
 budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL, name TEXT NOT NULL,
 sort_order INTEGER NOT NULL DEFAULT 0 CHECK(typeof(sort_order)='integer'),
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN (0,1)), PRIMARY KEY(budget_id,id)
);

CREATE TABLE category_groups (
 budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL,
 name TEXT NOT NULL,is_income INTEGER NOT NULL DEFAULT 0 CHECK(is_income IN(0,1)),
 hidden INTEGER NOT NULL DEFAULT 0 CHECK(hidden IN(0,1)),notes TEXT NOT NULL DEFAULT '',
 sort_order REAL DEFAULT 0,tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)), color TEXT,
 PRIMARY KEY(budget_id,id)
);

CREATE TABLE tags (
 budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL, tag TEXT NOT NULL,
 color TEXT, description TEXT, hidden INTEGER NOT NULL DEFAULT 0 CHECK(hidden IN(0,1)),
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)), PRIMARY KEY(budget_id,id), UNIQUE(budget_id,tag)
);

CREATE TABLE register_settings (
  budget_id TEXT PRIMARY KEY REFERENCES budgets(id) ON DELETE CASCADE,
  settings TEXT NOT NULL CHECK(json_valid(settings)),
  revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0)
);

CREATE TABLE payee_locations (
 budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL,
 payee_id TEXT NOT NULL, latitude REAL NOT NULL CHECK(latitude BETWEEN -90 AND 90),
 longitude REAL NOT NULL CHECK(longitude BETWEEN -180 AND 180),
 created_at INTEGER NOT NULL CHECK(created_at BETWEEN 0 AND 9007199254740991),
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN (0,1)),
 PRIMARY KEY(budget_id,id), FOREIGN KEY(budget_id,payee_id) REFERENCES payees(budget_id,id)
);

CREATE TABLE history_actions (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 budget_id TEXT NOT NULL REFERENCES budgets(id), owner TEXT NOT NULL, epoch TEXT NOT NULL,
 id TEXT NOT NULL UNIQUE, label TEXT NOT NULL, undone INTEGER NOT NULL DEFAULT 0 CHECK(undone IN(0,1)),
 changes TEXT NOT NULL
);

CREATE TABLE history_lineage (
 budget_id TEXT NOT NULL,owner TEXT NOT NULL,epoch TEXT NOT NULL,entity TEXT NOT NULL,entity_id TEXT NOT NULL,field TEXT NOT NULL,clock TEXT NOT NULL,lineage TEXT NOT NULL,
 PRIMARY KEY(budget_id,owner,epoch,entity,entity_id,field,clock)
);

CREATE TABLE budget_settings (
 budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL DEFAULT 'method',
 method TEXT NOT NULL CHECK(method IN ('tracking','envelope')), tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone=0), template_currency TEXT, template_currency_review INTEGER NOT NULL DEFAULT 0 CHECK(template_currency_review IN(0,1)),
 PRIMARY KEY(budget_id,id), CHECK(id='method')
);

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

CREATE TABLE android_upgrade_order (
 budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL,
 source_order REAL, display_key TEXT NOT NULL,
 PRIMARY KEY(budget_id,id)
);

CREATE TABLE schedules (
 budget_id TEXT NOT NULL REFERENCES budgets(id), id TEXT NOT NULL, rule_id TEXT NOT NULL,
 name TEXT, posts_transaction INTEGER NOT NULL DEFAULT 0 CHECK(posts_transaction IN(0,1)),
 completed INTEGER NOT NULL DEFAULT 0 CHECK(completed IN(0,1)), next_date TEXT,
 custom_upcoming_length TEXT, tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id)
);

CREATE TABLE schedule_occurrences (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL,schedule_id TEXT NOT NULL,
 occurrence_date TEXT NOT NULL,status TEXT NOT NULL CHECK(status IN('posted','skipped','matched','suppressed')),
 transaction_id TEXT,tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id),UNIQUE(budget_id,schedule_id,occurrence_date)
);

CREATE TABLE schedule_links (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL,schedule_id TEXT NOT NULL,
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),PRIMARY KEY(budget_id,id)
);

CREATE TABLE schedule_settings (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL DEFAULT 'horizon' CHECK(id='horizon'),
 value TEXT NOT NULL DEFAULT '7',tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone=0),PRIMARY KEY(budget_id,id)
);

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

CREATE TABLE transaction_import_files (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL,account_id TEXT NOT NULL,
 filename TEXT NOT NULL,sha256 TEXT NOT NULL,original BLOB NOT NULL,
 created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 PRIMARY KEY(budget_id,id),FOREIGN KEY(budget_id,account_id) REFERENCES accounts(budget_id,id)
);

CREATE TABLE transaction_import_metadata (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL,
 imported_id TEXT,raw_synced_data TEXT,pending INTEGER NOT NULL DEFAULT 0 CHECK(pending IN(0,1)),
 import_type TEXT,tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id),FOREIGN KEY(budget_id,id) REFERENCES transactions(budget_id,id)
);

CREATE TABLE transaction_import_settings (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL,account_id TEXT NOT NULL,
 option_key TEXT NOT NULL,value TEXT NOT NULL,tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id),FOREIGN KEY(budget_id,account_id) REFERENCES accounts(budget_id,id)
);

CREATE TABLE reporting_documents (
 budget_id TEXT NOT NULL REFERENCES budgets(id),id TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN('page','widget','custom_report')),
 config TEXT NOT NULL CHECK(json_valid(config)),
 revision INTEGER NOT NULL CHECK(revision>=1),
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id)
);

CREATE TABLE notes (
 budget_id TEXT NOT NULL REFERENCES budgets(id),
 id TEXT NOT NULL,note TEXT NOT NULL DEFAULT '',
 tombstone INTEGER NOT NULL DEFAULT 0 CHECK(tombstone IN(0,1)),
 PRIMARY KEY(budget_id,id)
);

CREATE TABLE backup_policy (id INTEGER PRIMARY KEY CHECK(id=1), interval_seconds INTEGER NOT NULL CHECK(interval_seconds BETWEEN 0 AND 604800));

CREATE TABLE "staged_imports" (
 id TEXT PRIMARY KEY, archive_sha256 TEXT NOT NULL UNIQUE, original BLOB NOT NULL,
 budget_id TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN('ready','blocked','activated')),
 report TEXT NOT NULL, conversion TEXT NOT NULL
);

CREATE TABLE "imported_sources" (
 budget_id TEXT PRIMARY KEY REFERENCES budgets(id),
 import_id TEXT NOT NULL UNIQUE REFERENCES "staged_imports"(id),
 source_local_id TEXT NOT NULL, source_cloud_id TEXT, source_group_id TEXT,
 source_metadata TEXT NOT NULL
);

CREATE INDEX transactions_account_date ON transactions(budget_id,account_id,date DESC,id);

CREATE INDEX transactions_budget_order ON transactions(budget_id,sort_order DESC);

CREATE INDEX budget_changes_sequence ON budget_changes(budget_id,sequence);

CREATE INDEX budget_backups_budget_time ON budget_backups(budget_id,created_at DESC,id);

CREATE INDEX transactions_companion_expenses ON transactions(budget_id,account_id,date DESC,sort_order DESC,id ASC) WHERE tombstone=0 AND reconciled=0 AND amount<0;

CREATE INDEX category_order ON categories(budget_id,tombstone,group_id,sort_order);

CREATE INDEX transactions_category_history ON transactions(budget_id,category_id,tombstone,date DESC,sort_order DESC,id);

CREATE INDEX transactions_children ON transactions(budget_id,parent_id,tombstone,sort_order DESC,id);

CREATE INDEX transactions_account_active_order ON transactions(budget_id,account_id,tombstone,parent_id,date DESC,starting_balance ASC,sort_order DESC,id ASC);

CREATE INDEX transactions_register ON transactions(budget_id,tombstone,parent_id,date DESC,starting_balance ASC,sort_order DESC,id ASC);

CREATE INDEX transactions_transfer ON transactions(budget_id,transfer_id);

CREATE INDEX payee_locations_payee ON payee_locations(budget_id,payee_id,tombstone);

CREATE INDEX history_owner ON history_actions(budget_id,owner,epoch,sequence);

CREATE INDEX android_upgrade_source_order ON android_upgrade_order(budget_id,source_order);

CREATE UNIQUE INDEX schedules_live_name ON schedules(budget_id,name) WHERE tombstone=0 AND name IS NOT NULL;

CREATE INDEX schedules_due ON schedules(budget_id,next_date) WHERE tombstone=0 AND completed=0;

CREATE INDEX schedule_occurrences_payment ON schedule_occurrences(budget_id,transaction_id) WHERE tombstone=0;

CREATE INDEX schedule_links_schedule ON schedule_links(budget_id,schedule_id) WHERE tombstone=0;

CREATE INDEX transaction_import_file_digest ON transaction_import_files(budget_id,account_id,sha256,filename);

CREATE INDEX transaction_import_identity ON transaction_import_metadata(budget_id,imported_id);

CREATE INDEX transaction_import_account_settings ON transaction_import_settings(budget_id,account_id,tombstone);

CREATE UNIQUE INDEX budgets_single_active ON budgets((1)) WHERE deleted=0;

CREATE TRIGGER budget_sync_created AFTER INSERT ON budgets BEGIN
 INSERT INTO budget_sync(budget_id,epoch,node) VALUES(NEW.id,lower(hex(randomblob(16))),lower(hex(randomblob(8))));
END;

CREATE TRIGGER change_transactions_insert AFTER INSERT ON transactions BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'transactions',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;

CREATE TRIGGER change_transactions_update AFTER UPDATE ON transactions WHEN NEW.account_id IS NOT OLD.account_id OR NEW.amount IS NOT OLD.amount OR NEW.date IS NOT OLD.date OR NEW.cleared IS NOT OLD.cleared OR NEW.reconciled IS NOT OLD.reconciled OR NEW.starting_balance IS NOT OLD.starting_balance OR NEW.payee_id IS NOT OLD.payee_id OR NEW.category_id IS NOT OLD.category_id OR NEW.notes IS NOT OLD.notes OR NEW.tombstone IS NOT OLD.tombstone OR NEW.sort_order IS NOT OLD.sort_order BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'transactions',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;

CREATE TRIGGER change_accounts_insert AFTER INSERT ON accounts BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'accounts',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;

CREATE TRIGGER change_payees_insert AFTER INSERT ON payees BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'payees',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;

CREATE TRIGGER change_categories_insert AFTER INSERT ON categories BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'categories',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;

CREATE TRIGGER change_categories_update AFTER UPDATE ON categories WHEN NEW.name IS NOT OLD.name OR NEW.is_income IS NOT OLD.is_income BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'categories',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;

CREATE TRIGGER change_ledger_totals_insert AFTER INSERT ON ledger_totals BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'accounts',NEW.account_id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;

CREATE TRIGGER change_ledger_totals_update AFTER UPDATE ON ledger_totals WHEN NEW.total IS NOT OLD.total OR NEW.cleared IS NOT OLD.cleared OR NEW.uncleared IS NOT OLD.uncleared BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'accounts',NEW.account_id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;

CREATE TRIGGER change_budget_preferences_insert AFTER INSERT ON budget_preferences BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'preferences',NEW.budget_id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;

CREATE TRIGGER change_budget_preferences_update AFTER UPDATE ON budget_preferences WHEN NEW.settings IS NOT OLD.settings OR NEW.revision IS NOT OLD.revision BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence)
 VALUES(NEW.budget_id,'preferences',NEW.budget_id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id))
 ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;

CREATE TRIGGER lifecycle_budget_created AFTER INSERT ON budgets BEGIN
 INSERT INTO budget_lifecycle(budget_id) VALUES(NEW.id);
END;

CREATE TRIGGER change_accounts_update AFTER UPDATE ON accounts WHEN NEW.name IS NOT OLD.name OR NEW.notes IS NOT OLD.notes OR NEW.off_budget IS NOT OLD.off_budget OR NEW.closed IS NOT OLD.closed OR NEW.group_id IS NOT OLD.group_id OR NEW.sort_order IS NOT OLD.sort_order OR NEW.tombstone IS NOT OLD.tombstone BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence) VALUES(NEW.budget_id,'accounts',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id)) ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;

CREATE TRIGGER change_account_groups_insert AFTER INSERT ON account_groups BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence) VALUES(NEW.budget_id,'account_groups',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id)) ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;

CREATE TRIGGER change_account_groups_update AFTER UPDATE ON account_groups BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence) VALUES(NEW.budget_id,'account_groups',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id)) ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;

CREATE TRIGGER change_payees_update AFTER UPDATE ON payees BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence) VALUES(NEW.budget_id,'payees',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id)) ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;

CREATE TRIGGER change_tags_insert AFTER INSERT ON tags BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence) VALUES(NEW.budget_id,'tags',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id)) ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;

CREATE TRIGGER change_tags_update AFTER UPDATE ON tags BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence) VALUES(NEW.budget_id,'tags',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id)) ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;

CREATE TRIGGER change_locations_insert AFTER INSERT ON payee_locations BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence) VALUES(NEW.budget_id,'payee_locations',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id)) ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;

CREATE TRIGGER change_locations_update AFTER UPDATE ON payee_locations BEGIN
 UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=NEW.budget_id;
 INSERT INTO budget_changes(budget_id,entity,entity_id,sequence) VALUES(NEW.budget_id,'payee_locations',NEW.id,(SELECT sequence FROM budget_sync WHERE budget_id=NEW.budget_id)) ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence;
END;

CREATE TRIGGER budgeting_created AFTER INSERT ON budgets BEGIN
 INSERT INTO budget_settings(budget_id,method) VALUES(NEW.id,NEW.budget_type);
END;

CREATE TRIGGER rules_budget_created AFTER INSERT ON budgets BEGIN
 INSERT INTO rule_settings(budget_id) VALUES(NEW.id);
END;

CREATE TRIGGER schedules_budget_created AFTER INSERT ON budgets BEGIN
 INSERT INTO schedule_settings(budget_id) VALUES(NEW.id);
END;

CREATE TRIGGER manual_entries_insert AFTER INSERT ON transactions
WHEN NEW.cleared<>1 OR NEW.reconciled<>0
BEGIN
 UPDATE transactions SET cleared=1,reconciled=0 WHERE budget_id=NEW.budget_id AND id=NEW.id;
END;

CREATE TRIGGER manual_entries_update AFTER UPDATE OF cleared,reconciled ON transactions
WHEN NEW.cleared<>1 OR NEW.reconciled<>0
BEGIN
 UPDATE transactions SET cleared=1,reconciled=0 WHERE budget_id=NEW.budget_id AND id=NEW.id;
END;

CREATE TRIGGER manual_balances_insert AFTER INSERT ON ledger_totals
WHEN NEW.cleared<>NEW.total OR NEW.uncleared<>0
BEGIN
 UPDATE ledger_totals SET cleared=total,uncleared=0 WHERE budget_id=NEW.budget_id AND account_id=NEW.account_id;
END;

CREATE TRIGGER manual_balances_update AFTER UPDATE ON ledger_totals
WHEN NEW.cleared<>NEW.total OR NEW.uncleared<>0
BEGIN
 UPDATE ledger_totals SET cleared=total,uncleared=0 WHERE budget_id=NEW.budget_id AND account_id=NEW.account_id;
END;

CREATE TRIGGER visible_categories_insert AFTER INSERT ON categories
WHEN NEW.hidden<>0 BEGIN
 UPDATE categories SET hidden=0 WHERE budget_id=NEW.budget_id AND id=NEW.id;
END;

CREATE TRIGGER visible_categories_update AFTER UPDATE OF hidden ON categories
WHEN NEW.hidden<>0 BEGIN
 UPDATE categories SET hidden=0 WHERE budget_id=NEW.budget_id AND id=NEW.id;
END;

CREATE TRIGGER visible_category_groups_insert AFTER INSERT ON category_groups
WHEN NEW.hidden<>0 BEGIN
 UPDATE category_groups SET hidden=0 WHERE budget_id=NEW.budget_id AND id=NEW.id;
END;

CREATE TRIGGER visible_category_groups_update AFTER UPDATE OF hidden ON category_groups
WHEN NEW.hidden<>0 BEGIN
 UPDATE category_groups SET hidden=0 WHERE budget_id=NEW.budget_id AND id=NEW.id;
END;

CREATE TRIGGER budgeting_method AFTER UPDATE OF method ON budget_settings BEGIN
 UPDATE budget_settings SET method='tracking' WHERE budget_id=NEW.budget_id AND method<>'tracking';
 UPDATE budgets SET budget_type='tracking' WHERE id=NEW.budget_id;
END;

CREATE TRIGGER retired_categories_notes_insert AFTER INSERT ON categories
WHEN NEW.notes<>'' BEGIN UPDATE categories SET notes='' WHERE budget_id=NEW.budget_id AND id=NEW.id; END;

CREATE TRIGGER retired_categories_notes_update AFTER UPDATE OF notes ON categories
WHEN NEW.notes<>'' BEGIN UPDATE categories SET notes='' WHERE budget_id=NEW.budget_id AND id=NEW.id; END;

CREATE TRIGGER retired_category_groups_notes_insert AFTER INSERT ON category_groups
WHEN NEW.notes<>'' BEGIN UPDATE category_groups SET notes='' WHERE budget_id=NEW.budget_id AND id=NEW.id; END;

CREATE TRIGGER retired_category_groups_notes_update AFTER UPDATE OF notes ON category_groups
WHEN NEW.notes<>'' BEGIN UPDATE category_groups SET notes='' WHERE budget_id=NEW.budget_id AND id=NEW.id; END;

CREATE TRIGGER retired_transactions_notes_insert AFTER INSERT ON transactions
WHEN NEW.notes<>'' BEGIN UPDATE transactions SET notes='' WHERE budget_id=NEW.budget_id AND id=NEW.id; END;

CREATE TRIGGER retired_transactions_notes_update AFTER UPDATE OF notes ON transactions
WHEN NEW.notes<>'' BEGIN UPDATE transactions SET notes='' WHERE budget_id=NEW.budget_id AND id=NEW.id; END;

CREATE TRIGGER retired_budget_months_notes_insert AFTER INSERT ON budget_months
WHEN NEW.notes<>'' BEGIN UPDATE budget_months SET notes='' WHERE budget_id=NEW.budget_id AND id=NEW.id; END;

CREATE TRIGGER retired_budget_months_notes_update AFTER UPDATE OF notes ON budget_months
WHEN NEW.notes<>'' BEGIN UPDATE budget_months SET notes='' WHERE budget_id=NEW.budget_id AND id=NEW.id; END;

CREATE TRIGGER lifecycle_budget_changed AFTER UPDATE OF sequence ON budget_sync WHEN NEW.sequence IS NOT OLD.sequence BEGIN
 UPDATE budget_lifecycle SET auto_dirty=1,auto_due=CASE WHEN (SELECT interval_seconds FROM backup_policy WHERE id=1)>0 THEN COALESCE(auto_due,unixepoch('now')+(SELECT interval_seconds FROM backup_policy WHERE id=1)) ELSE NULL END WHERE budget_id=NEW.budget_id;
END;

CREATE TRIGGER lifecycle_budget_renamed AFTER UPDATE OF name,budget_type ON budgets WHEN NEW.name IS NOT OLD.name OR NEW.budget_type IS NOT OLD.budget_type BEGIN
 UPDATE budget_lifecycle SET auto_dirty=1,auto_due=CASE WHEN (SELECT interval_seconds FROM backup_policy WHERE id=1)>0 THEN COALESCE(auto_due,unixepoch('now')+(SELECT interval_seconds FROM backup_policy WHERE id=1)) ELSE NULL END WHERE budget_id=NEW.id;
END;

CREATE TRIGGER retired_reports_insert AFTER INSERT ON reporting_documents WHEN NEW.tombstone=0 AND NEW.kind='widget' AND json_extract(NEW.config,'$.type') IN ('formula-card','monte-carlo-card','sankey-card','budget-analysis-card','balance-forecast-card') BEGIN UPDATE reporting_documents SET tombstone=1 WHERE budget_id=NEW.budget_id AND id=NEW.id; END;

CREATE TRIGGER retired_rules_insert AFTER INSERT ON rules WHEN NEW.tombstone=0 AND EXISTS(SELECT 1 FROM json_each(NEW.actions) a WHERE json_extract(a.value,'$.options.formula') IS NOT NULL OR json_extract(a.value,'$.options.template') IS NOT NULL OR json_extract(a.value,'$.options.method')='formula') BEGIN UPDATE rules SET tombstone=1 WHERE budget_id=NEW.budget_id AND id=NEW.id; END;

CREATE TRIGGER retired_rule_schedules_insert AFTER INSERT ON schedules WHEN NEW.tombstone=0 AND EXISTS(SELECT 1 FROM rules r WHERE r.budget_id=NEW.budget_id AND r.id=NEW.rule_id AND EXISTS(SELECT 1 FROM json_each(r.actions) a WHERE json_extract(a.value,'$.options.formula') IS NOT NULL OR json_extract(a.value,'$.options.template') IS NOT NULL OR json_extract(a.value,'$.options.method')='formula')) BEGIN UPDATE schedules SET tombstone=1 WHERE budget_id=NEW.budget_id AND id=NEW.id; END;

CREATE TRIGGER retired_reports_update AFTER UPDATE ON reporting_documents WHEN NEW.tombstone=0 AND NEW.kind='widget' AND json_extract(NEW.config,'$.type') IN ('formula-card','monte-carlo-card','sankey-card','budget-analysis-card','balance-forecast-card') BEGIN UPDATE reporting_documents SET tombstone=1 WHERE budget_id=NEW.budget_id AND id=NEW.id; END;

CREATE TRIGGER retired_rules_update AFTER UPDATE ON rules WHEN NEW.tombstone=0 AND EXISTS(SELECT 1 FROM json_each(NEW.actions) a WHERE json_extract(a.value,'$.options.formula') IS NOT NULL OR json_extract(a.value,'$.options.template') IS NOT NULL OR json_extract(a.value,'$.options.method')='formula') BEGIN UPDATE rules SET tombstone=1 WHERE budget_id=NEW.budget_id AND id=NEW.id; END;

CREATE TRIGGER retired_rule_schedules_update AFTER UPDATE ON schedules WHEN NEW.tombstone=0 AND EXISTS(SELECT 1 FROM rules r WHERE r.budget_id=NEW.budget_id AND r.id=NEW.rule_id AND EXISTS(SELECT 1 FROM json_each(r.actions) a WHERE json_extract(a.value,'$.options.formula') IS NOT NULL OR json_extract(a.value,'$.options.template') IS NOT NULL OR json_extract(a.value,'$.options.method')='formula')) BEGIN UPDATE schedules SET tombstone=1 WHERE budget_id=NEW.budget_id AND id=NEW.id; END;

CREATE TRIGGER retired_domain_notes_insert AFTER INSERT ON notes WHEN (NEW.note<>'' OR NEW.tombstone=0) AND (NEW.id=NEW.budget_id OR (NEW.id GLOB 'budget-[0-9][0-9][0-9][0-9]-[0-9][0-9]' AND substr(NEW.id,8,4) BETWEEN '0001' AND '9999' AND substr(NEW.id,13,2) BETWEEN '01' AND '12') OR EXISTS(SELECT 1 FROM categories c WHERE c.budget_id=NEW.budget_id AND c.id=NEW.id) OR EXISTS(SELECT 1 FROM category_groups g WHERE g.budget_id=NEW.budget_id AND g.id=NEW.id)) BEGIN UPDATE notes SET note='',tombstone=1 WHERE budget_id=NEW.budget_id AND id=NEW.id; END;

CREATE TRIGGER retired_domain_notes_update AFTER UPDATE ON notes WHEN (NEW.note<>'' OR NEW.tombstone=0) AND (NEW.id=NEW.budget_id OR (NEW.id GLOB 'budget-[0-9][0-9][0-9][0-9]-[0-9][0-9]' AND substr(NEW.id,8,4) BETWEEN '0001' AND '9999' AND substr(NEW.id,13,2) BETWEEN '01' AND '12') OR EXISTS(SELECT 1 FROM categories c WHERE c.budget_id=NEW.budget_id AND c.id=NEW.id) OR EXISTS(SELECT 1 FROM category_groups g WHERE g.budget_id=NEW.budget_id AND g.id=NEW.id)) BEGIN UPDATE notes SET note='',tombstone=1 WHERE budget_id=NEW.budget_id AND id=NEW.id; END;

INSERT INTO "backup_policy" VALUES(1,604800);
