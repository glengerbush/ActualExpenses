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
];
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
