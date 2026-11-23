use crate::error::Error;
use rusqlite::Connection;
use std::path::Path;
use uuid::Uuid;

// The deployed 0.6.0 patch is the only supported upgrade/restore predecessor.
pub(crate) const MIN_SUPPORTED_VERSION: usize = 37;
pub(crate) const CURRENT_VERSION: usize = 38;

pub fn supported_version(db: &Connection) -> Result<i64, Error> {
    let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version != 0 && !(MIN_SUPPORTED_VERSION as i64..=CURRENT_VERSION as i64).contains(&version) {
        return Err(Error::internal(
            "Unsupported database version. Only the current and immediately previous patch are supported; the database was not changed.",
        ));
    }
    // An unversioned populated database is not a fresh installation.
    if version == 0
        && db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name NOT LIKE 'sqlite_%')",
            [],
            |r| r.get::<_, bool>(0),
        )?
    {
        return Err(Error::internal(
            "Unsupported unversioned database; the database was not changed.",
        ));
    }
    Ok(version)
}

pub fn apply(db: &mut Connection, directory: &Path) -> Result<(), Error> {
    let version = supported_version(db)?;
    crate::transaction_order::configure(db)?;
    if version == CURRENT_VERSION as i64 {
        return Ok(());
    }
    if version > 0 {
        let backups = directory.join("backups");
        std::fs::create_dir_all(&backups).map_err(Error::internal)?;
        let backup = backups.join(format!("before-v{version}-{}.sqlite3", Uuid::new_v4()));
        db.execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])?;
    }
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if version == 0 {
        tx.execute_batch(include_str!("schema.sql"))?;
    }
    tx.execute_batch(include_str!("migrations/038_transaction_notes.sql"))?;
    tx.pragma_update(None, "user_version", CURRENT_VERSION as i64)?;
    if version > 0 {
        crate::lifecycle::automatic::upgrade_points(&tx, version)?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        lifecycle::archive,
        models::{NewAccount, NewBudget},
        store,
    };
    use std::io::{Cursor, Read, Write};
    use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

    const B: &str = "80000000-0000-4000-8000-000000000001";
    const A: &str = "80000000-0000-4000-8000-000000000002";

    // Materialize the deployed schema, not a current database with a spoofed version.
    fn predecessor(path: &Path) -> Connection {
        let db = Connection::open(path).unwrap();
        crate::transaction_order::configure(&db).unwrap();
        db.execute_batch(include_str!("schema.sql")).unwrap();
        db.pragma_update(None, "user_version", MIN_SUPPORTED_VERSION as i64)
            .unwrap();
        crate::notes::configure(&db).unwrap();
        crate::history::configure(&db).unwrap();
        store::create_budget(
            &db,
            NewBudget {
                id: B.into(),
                name: "Household".into(),
                budget_type: "tracking".into(),
            },
        )
        .unwrap();
        store::create_account(
            &db,
            B,
            NewAccount {
                id: A.into(),
                name: "History".into(),
                notes: String::new(),
                off_budget: false,
                opening_balance: 12345,
                opening_date: "2026-11-01".into(),
            },
        )
        .unwrap();
        db.execute("INSERT INTO operations(scope,operation_id,payload,response) VALUES('test','receipt','{}','{\"exact\":true}')", []).unwrap();
        db
    }
    fn archive_version(bytes: Vec<u8>, version: usize) -> Vec<u8> {
        let mut source = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut target = ZipWriter::new(Cursor::new(Vec::new()));
        for i in 0..source.len() {
            let mut file = source.by_index(i).unwrap();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            if file.name() == "manifest.json" {
                let mut manifest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                manifest["schema_version"] = version.into();
                bytes = serde_json::to_vec(&manifest).unwrap();
            }
            target
                .start_file(file.name(), SimpleFileOptions::default())
                .unwrap();
            target.write_all(&bytes).unwrap();
        }
        target.finish().unwrap().into_inner()
    }
    #[test]
    fn previous_patch_preserves_money_receipts_archive_and_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("actualbudget.sqlite3");
        let mut db = predecessor(&path);
        let before = store::accounts(&db, B).unwrap();
        let portable = archive_version(archive::capture(&db, B).unwrap(), MIN_SUPPORTED_VERSION);
        let validated = archive::validate(&portable).unwrap();
        let mut destination = Connection::open_in_memory().unwrap();
        apply(&mut destination, directory.path()).unwrap();
        destination
            .execute(
                "INSERT INTO budgets(id,name,budget_type) VALUES(?1,'Household','tracking')",
                [B],
            )
            .unwrap();
        archive::install_tables(&destination, &validated).unwrap();
        archive::validate_financial_state(&destination, B).unwrap();
        assert_eq!(store::accounts(&destination, B).unwrap(), before);
        assert!(archive::validate(&archive_version(portable, MIN_SUPPORTED_VERSION - 1)).is_err());
        apply(&mut db, directory.path()).unwrap();
        assert_eq!(store::accounts(&db, B).unwrap(), before);
        assert_eq!(
            db.query_row(
                "SELECT response FROM operations WHERE scope='test' AND operation_id='receipt'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "{\"exact\":true}"
        );
        db.execute(
            "UPDATE transactions SET notes='New patch notes' WHERE budget_id=?1",
            [B],
        )
        .unwrap();
        let backups: Vec<_> = std::fs::read_dir(directory.path().join("backups"))
            .unwrap()
            .collect();
        assert_eq!(backups.len(), 1);
        let saved = Connection::open(backups[0].as_ref().unwrap().path()).unwrap();
        assert_eq!(
            supported_version(&saved).unwrap(),
            MIN_SUPPORTED_VERSION as i64
        );
        assert_eq!(store::accounts(&saved, B).unwrap(), before);
        drop(db);
        let mut reopened = Connection::open(path).unwrap();
        apply(&mut reopened, directory.path()).unwrap();
        assert_eq!(store::accounts(&reopened, B).unwrap(), before);
        assert_eq!(
            std::fs::read_dir(directory.path().join("backups"))
                .unwrap()
                .count(),
            1
        );
    }
    #[test]
    fn unsupported_databases_are_rejected_without_mutation() {
        for version in [0, 1, 36, 39] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("actualbudget.sqlite3");
            let db = Connection::open(&path).unwrap();
            db.execute_batch(
                "CREATE TABLE sentinel(value TEXT); INSERT INTO sentinel VALUES('preserve');",
            )
            .unwrap();
            db.pragma_update(None, "user_version", version).unwrap();
            drop(db);
            let bytes = std::fs::read(&path).unwrap();
            assert!(store::Store::open(directory.path()).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            assert!(!directory.path().join("backups").exists());
        }
    }
}
