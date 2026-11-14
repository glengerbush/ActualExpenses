//! Keep historical feature oracles independent of the installation-wide merge
//! acceptance test. Changes apply only to disposable fixture copies.
pub fn keep_budget(path: &std::path::Path, budget: &str) {
    let db = rusqlite::Connection::open(path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF").unwrap();
    let triggers = db
        .prepare("SELECT name,sql FROM sqlite_master WHERE type='trigger'")
        .unwrap()
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for (name, _) in &triggers {
        db.execute_batch(&format!("DROP TRIGGER \"{name}\""))
            .unwrap();
    }
    let tables = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for table in tables {
        let scoped = db
            .prepare(&format!("PRAGMA table_info(\"{table}\")"))
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .any(|c| c.unwrap() == "budget_id");
        if scoped && table != "staged_imports" {
            db.execute(
                &format!("DELETE FROM \"{table}\" WHERE budget_id<>?1"),
                [budget],
            )
            .unwrap();
        }
    }
    db.execute("DELETE FROM budgets WHERE id<>?1", [budget])
        .unwrap();
    for (_, sql) in triggers {
        db.execute_batch(&sql).unwrap();
    }
}
