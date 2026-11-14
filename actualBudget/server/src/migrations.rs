use crate::error::Error;
use rusqlite::Connection;
use std::path::Path;
use uuid::Uuid;

const MIGRATIONS: &[&str] = &[
    include_str!("schema.sql"),
    include_str!("migrations/002_register.sql"),
    include_str!("preferences/schema.sql"),
    include_str!("migrations/004_concurrency.sql"),
    include_str!("imports/schema.sql"),
    include_str!("lifecycle/schema.sql"),
    include_str!("migrations/007_accounts.sql"),
    include_str!("categories/schema.sql"),
    include_str!("migrations/009_payees_tags.sql"),
    include_str!("splits/schema.sql"),
    include_str!("migrations/011_register_settings.sql"),
    include_str!("migrations/012_reconciliation.sql"),
    include_str!("migrations/013_transfers.sql"),
    include_str!("migrations/014_locations.sql"),
    include_str!("migrations/015_bulk.sql"),
    include_str!("migrations/016_budgeting.sql"),
    include_str!("migrations/017_rules.sql"),
    include_str!("migrations/018_android_upgrade.sql"),
    include_str!("migrations/019_schedules.sql"),
    include_str!("migrations/020_budget_automations.sql"),
    include_str!("migrations/021_transaction_imports.sql"),
    include_str!("migrations/022_reporting.sql"),
    include_str!("migrations/023_banking.sql"),
    include_str!("migrations/024_notes.sql"),
    include_str!("migrations/025_gocardless.sql"),
    include_str!("migrations/026_retire_external_banks.sql"),
    include_str!("migrations/027_rule_expressions.sql"),
    include_str!("migrations/028_complete_manual_expenses.sql"),
    include_str!("migrations/029_category_colors.sql"),
    include_str!("migrations/030_visible_categories.sql"),
    include_str!("migrations/031_tracking_only.sql"),
    include_str!("migrations/032_retire_notes.sql"),
    include_str!("migrations/033_weekly_backups.sql"),
    include_str!("migrations/034_retire_experiments.sql"),
    include_str!("migrations/035_note_month_guards.sql"),
    include_str!("migrations/036_retired_rule_notes.sql"),
    include_str!("migrations/037_single_budget.sql"),
];
pub(crate) const CURRENT_VERSION: usize = MIGRATIONS.len();
pub fn apply(db: &mut Connection, directory: &Path) -> Result<(), Error> {
    crate::android_upgrade::order::configure(db)?;
    let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 0 || version as usize > MIGRATIONS.len() {
        return Err(Error::internal(
            "Database version is newer than this server supports",
        ));
    }
    if version > 0 && (version as usize) < MIGRATIONS.len() {
        let backups = directory.join("backups");
        std::fs::create_dir_all(&backups).map_err(Error::internal)?;
        let backup = backups.join(format!("before-v{version}-{}.sqlite3", Uuid::new_v4()));
        // SQLite produces a consistent standalone copy, including committed WAL pages.
        db.execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])?;
    }
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        tx.execute_batch(sql)?;
        if index == 36 {
            crate::single_budget::migrate(&tx)?;
        }
        if index == 35 {
            crate::rules::normalize_retired_notes(&tx, None)?;
            crate::imports::refresh_rule_capabilities(&tx, None)?;
        }
        if index == 30 {
            crate::budgeting::normalize(&tx, None)?;
        }
        if index == 26 {
            crate::imports::refresh_rule_capabilities(&tx, None)?;
        }
        if index == 23 {
            crate::notes::configure(&tx)?;
            crate::notes::enrich(&tx, None)?;
        }
        if index == 21 {
            crate::reports::import::enrich(&tx, None)?;
        }
        if index == 20 {
            crate::transaction_imports::enrich(&tx, None)?;
        }
        if index == 19 {
            crate::budgeting::templates::enrich(&tx, None)?;
        }
        if index == 18 {
            crate::schedules::enrich(&tx, None)?;
        }
        if index == 17 {
            crate::android_upgrade::order::enrich(&tx, None)?;
        }
        if index == 16 {
            crate::rules::enrich_legacy_imports(&tx, None)?;
        }
        if index == 15 {
            crate::budgeting::enrich_legacy_imports(&tx, None)?;
        }
        if index == 13 {
            crate::payee_locations::enrich_legacy_imports(&tx, None)?;
        }
        if index == 10 {
            crate::register_settings::enrich_legacy_imports(&tx, None)?;
        }
        if index == 7 {
            crate::categories::enrich_legacy_imports(&tx, None)?;
        }
        if index == 5 {
            crate::lifecycle::backfill_operation_links(&tx)?;
        }
        if index == 8 {
            crate::imports::backfill_payees_tags(&tx)?;
        }
        tx.pragma_update(None, "user_version", (index + 1) as i64)?;
    }
    if version > 0 && (version as usize) < MIGRATIONS.len() {
        crate::lifecycle::automatic::upgrade_points(&tx, version)?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod note_month_guards {
    use rusqlite::Connection;

    #[test]
    fn forward_repair_preserves_arbitrary_ids_without_reviving_deleted_notes() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE notes(budget_id TEXT,id TEXT,note TEXT,tombstone INT);
            CREATE TABLE categories(budget_id TEXT,id TEXT);
            CREATE TABLE category_groups(budget_id TEXT,id TEXT);
            CREATE TABLE accounts(budget_id TEXT,id TEXT,notes TEXT);
            CREATE TRIGGER retired_domain_notes_insert AFTER INSERT ON notes WHEN NEW.id GLOB 'budget-[0-9][0-9][0-9][0-9]-[0-9][0-9]' BEGIN UPDATE notes SET note='',tombstone=1 WHERE id=NEW.id; END;
            CREATE TRIGGER retired_domain_notes_update AFTER UPDATE ON notes WHEN NEW.note<>'' AND NEW.id GLOB 'budget-[0-9][0-9][0-9][0-9]-[0-9][0-9]' BEGIN UPDATE notes SET note='',tombstone=1 WHERE id=NEW.id; END;
            INSERT INTO notes VALUES('b','budget-2026-13','previously deleted',0);").unwrap();
        db.execute_batch(include_str!("migrations/035_note_month_guards.sql"))
            .unwrap();
        let erased: (String, bool) = db
            .query_row(
                "SELECT note,tombstone FROM notes WHERE id='budget-2026-13'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(erased, (String::new(), true));
        for id in [
            "budget-2026-00",
            "budget-2026-14",
            "budget-0000-01",
            "budget-automation",
        ] {
            db.execute("INSERT INTO notes VALUES('b',?1,'retained',0)", [id])
                .unwrap();
            db.execute("UPDATE notes SET note='edited' WHERE id=?1", [id])
                .unwrap();
            let visible: String = db
                .query_row(
                    &format!(
                        "SELECT t.note {} WHERE t.id=?1 AND t.tombstone=0",
                        crate::notes::QUERY_FROM
                    ),
                    [id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(visible, "edited");
        }
        for id in ["budget-0001-01", "budget-2026-12", "budget-9999-12", "b"] {
            db.execute("INSERT INTO notes VALUES('b',?1,'retired',0)", [id])
                .unwrap();
            let erased: (String, bool) = db
                .query_row("SELECT note,tombstone FROM notes WHERE id=?1", [id], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .unwrap();
            assert_eq!(erased, (String::new(), true));
        }
    }
}
