//! Durable dirty deadlines; no timer runs while every budget is clean.
use super::make_point;
use crate::{error::Error, store::Store};
use rusqlite::{Connection, OptionalExtension, params};
use std::{sync::Arc, time::Duration};
use tokio::sync::Notify;

const WEEK: i64 = 604800;
pub fn configure(db: &Connection) -> Result<(), Error> {
    // Explicit environment overrides remain useful for isolated tests. Otherwise preserve
    // the setting across restarts; the migration provides the weekly default.
    if let Ok(value) = std::env::var("ACTUALBUDGET_BACKUP_INTERVAL_SECONDS") {
        let seconds: i64 = value.parse().map_err(Error::internal)?;
        if !(0..=WEEK).contains(&seconds) {
            return Err(Error::invalid("Backup interval must be 0–604800 seconds"));
        }
        set_interval(db, seconds)?;
    }
    Ok(())
}
fn policy(db: &Connection) -> Result<serde_json::Value, Error> {
    let seconds: i64 = db.query_row(
        "SELECT interval_seconds FROM backup_policy WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    Ok(serde_json::json!({"interval_seconds":seconds,"retention_count":10,"format":"complete"}))
}
fn set_interval(db: &Connection, seconds: i64) -> Result<(), Error> {
    let previous: i64 = db.query_row(
        "SELECT interval_seconds FROM backup_policy WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    if seconds == previous {
        return Ok(());
    }
    db.execute(
        "UPDATE backup_policy SET interval_seconds=?1 WHERE id=1",
        [seconds],
    )?;
    if seconds == 0 {
        db.execute("UPDATE budget_lifecycle SET auto_due=NULL", [])?;
    } else {
        // A single deadline, never extended by subsequent edits. On re-enabling, capture
        // changes made while disabled too, including renames which don't change sequence.
        db.execute(
            "UPDATE budget_lifecycle SET auto_due=unixepoch('now')+?1 WHERE auto_dirty=1",
            [seconds],
        )?;
    }
    Ok(())
}
pub fn router() -> axum::Router<Store> {
    axum::Router::new().route(
        "/api/v1/backup-policy",
        axum::routing::get(read_policy).patch(update_policy),
    )
}
async fn read_policy(
    axum::extract::State(store): axum::extract::State<Store>,
) -> Result<axum::Json<serde_json::Value>, Error> {
    store
        .run(|db| Ok(axum::Json(serde_json::json!({"data":policy(db)?}))))
        .await
}
async fn update_policy(
    axum::extract::State(store): axum::extract::State<Store>,
    headers: axum::http::HeaderMap,
    body: Result<axum::Json<serde_json::Value>, axum::extract::rejection::JsonRejection>,
) -> Result<axum::Json<serde_json::Value>, Error> {
    let key = crate::api::idempotency_key(&headers)?;
    let axum::Json(body) = body.map_err(crate::api::json_error)?;
    let payload = serde_json::to_string(&body).map_err(Error::internal)?;
    store
        .run(move |db| {
            crate::store::write(db, "/backup-policy", &key, &payload, |db| {
                let object = body
                    .as_object()
                    .ok_or_else(|| Error::invalid("Invalid backup policy"))?;
                let seconds = body["interval_seconds"]
                    .as_i64()
                    .ok_or_else(|| Error::invalid("Choose weekly or never"))?;
                if object.len() != 1 || ![0, WEEK].contains(&seconds) {
                    return Err(Error::invalid("Choose weekly or never"));
                }
                set_interval(db, seconds)?;
                Ok(serde_json::json!({"data":policy(db)?}))
            })
            .map(axum::Json)
        })
        .await
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
        tx.execute("UPDATE budget_lifecycle SET auto_due=NULL,auto_dirty=0,last_auto=unixepoch('now'),backup_error=NULL WHERE budget_id=?1",[&budget])?;
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
