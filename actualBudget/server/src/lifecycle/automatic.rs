//! Durable dirty deadlines; no timer runs while every budget is clean.
use super::make_point;
use crate::{error::Error, store::Store};
use rusqlite::{Connection, OptionalExtension, params};
use std::{sync::Arc, time::Duration};
use tokio::sync::Notify;

pub fn configure(db: &Connection) -> Result<(), Error> {
    let seconds: i64 = std::env::var("ACTUALBUDGET_BACKUP_INTERVAL_SECONDS")
        .unwrap_or_else(|_| "900".into())
        .parse()
        .map_err(Error::internal)?;
    if !(1..=86400).contains(&seconds) {
        return Err(Error::invalid("Backup interval must be 1–86400 seconds"));
    }
    db.execute(
        "UPDATE backup_policy SET interval_seconds=?1 WHERE id=1 AND interval_seconds<>?1",
        [seconds],
    )?;
    Ok(())
}
fn next(db: &Connection) -> Result<Option<i64>, Error> {
    Ok(db.query_row("SELECT MIN(auto_due) FROM budget_lifecycle l JOIN budgets b ON b.id=l.budget_id WHERE b.deleted=0",[],|r|r.get(0))?)
}
fn due(db: &mut Connection) -> Result<(), Error> {
    let budget:Option<String>=db.query_row("SELECT l.budget_id FROM budget_lifecycle l JOIN budgets b ON b.id=l.budget_id WHERE b.deleted=0 AND auto_due<=unixepoch('now') ORDER BY auto_due LIMIT 1",[],|r|r.get(0)).optional()?;
    let Some(budget) = budget else {
        return Ok(());
    };
    let result = (|| {
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        make_point(&tx, &budget, "automatic", "Automatic recovery point")?;
        // Actual retains at most three today, one per older day, ten automatic points overall.
        tx.execute("DELETE FROM budget_backups WHERE id IN (SELECT id FROM (SELECT id,created_at,ROW_NUMBER() OVER(PARTITION BY date(created_at,'localtime') ORDER BY created_at DESC,rowid DESC) AS rank FROM budget_backups WHERE budget_id=?1 AND kind='automatic') WHERE rank>CASE WHEN date(created_at,'localtime')=date('now','localtime') THEN 3 ELSE 1 END)",[&budget])?;
        tx.execute("DELETE FROM budget_backups WHERE id IN (SELECT id FROM budget_backups WHERE budget_id=?1 AND kind='automatic' ORDER BY created_at DESC,rowid DESC LIMIT -1 OFFSET 10)",[&budget])?;
        tx.execute("UPDATE budget_lifecycle SET auto_due=NULL,last_auto=unixepoch('now'),backup_error=NULL WHERE budget_id=?1",[&budget])?;
        tx.commit()?;
        Ok::<(), Error>(())
    })();
    if let Err(error) = result {
        // The failed transaction retained the previous points and dirty state. Back off even
        // if a full disk also prevents recording the visible failure status.
        let _=db.execute("UPDATE budget_lifecycle SET auto_due=unixepoch('now')+60,backup_error=?2 WHERE budget_id=?1",params![budget,error.message]);
        return Err(error);
    }
    Ok(())
}
pub fn start(store: Store, changed: Arc<Notify>) {
    tokio::spawn(async move {
        loop {
            let next = store.run(|db| next(db)).await;
            match next {
                Ok(None) => changed.notified().await,
                Ok(Some(deadline)) => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    if deadline > now {
                        tokio::select! {_=changed.notified()=>{},_=tokio::time::sleep(Duration::from_secs((deadline-now) as u64))=>{}}
                    } else if store.run(due).await.is_err() {
                        tokio::time::sleep(Duration::from_secs(60)).await;
                    }
                }
                Err(_) => tokio::time::sleep(Duration::from_secs(60)).await,
            }
        }
    });
}

/// Native, budget-scoped copies of the last pre-upgrade state; the untouched standalone
/// SQLite recovery file remains alongside them for full-server disaster recovery.
pub fn upgrade_points(db: &Connection, previous_schema: i64) -> Result<(), Error> {
    let mut q = db.prepare("SELECT id FROM budgets WHERE deleted=0 ORDER BY id")?;
    let ids = q
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in ids {
        make_point(
            db,
            &id,
            "upgrade",
            &format!("Before upgrading database version {previous_schema}"),
        )?;
    }
    Ok(())
}
