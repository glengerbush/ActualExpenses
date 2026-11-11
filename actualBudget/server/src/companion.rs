//! Bounded expense view shared by companion clients. No full ledger replication.
use crate::{
    api::budget_id,
    concurrency,
    error::Error,
    ledger,
    models::validate_id,
    store::{self, Store},
};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{PathRejection, QueryRejection},
    },
    http::HeaderMap,
    routing::{get, post},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub fn router() -> Router<Store> {
    Router::new()
        .route("/api/v1/budgets/{budget}/companion", get(view))
        .route("/api/v1/budgets/{budget}/ledger", post(ensure_ledger))
}
// Accounts remain an internal compatibility detail for archives and old operations.
// Reuse an existing on-budget destination without rewriting any historical entries.
async fn ensure_ledger(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
) -> Result<Json<Value>, Error> {
    let budget = budget_id(path)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let value = store.run(move |db| {
        let tx = db.transaction()?;
        store::require_budget(&tx, &budget)?;
        crate::lifecycle::require_epoch(&tx, &budget, epoch.as_deref())?;
        let existing: Option<String> = tx.query_row(
            "SELECT id FROM accounts WHERE budget_id=?1 AND tombstone=0 AND closed=0 AND off_budget=0 ORDER BY sort_order,id LIMIT 1",
            [&budget], |row| row.get(0),
        ).optional()?;
        let account = match existing {
            Some(id) => crate::accounts::get(&tx, &budget, &id)?,
            None => store::create_account(&tx, &budget, crate::models::NewAccount {
                id: uuid::Uuid::new_v4().to_string(), name: "Transactions".into(),
                notes: String::new(), off_budget: false, opening_balance: 0,
                opening_date: "1970-01-01".into(),
            })?,
        };
        tx.commit()?;
        Ok(json!({"data": account}))
    }).await?;
    Ok(Json(value))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewQuery {
    account_id: Option<String>,
    #[serde(default)]
    retain: String,
    snapshot: Option<String>,
}
async fn view(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<ViewQuery>, QueryRejection>,
) -> Result<Json<Value>, Error> {
    let budget = budget_id(path)?;
    let Query(query) = query.map_err(|_| Error::invalid("Invalid companion query"))?;
    if let Some(account) = &query.account_id {
        validate_id(account)?;
    }
    if query.retain.len() > 3700
        || query
            .snapshot
            .as_ref()
            .is_some_and(|value| value.len() != 50 || !value.is_ascii())
    {
        return Err(Error::invalid("Invalid retained IDs or checkpoint"));
    }
    let retained = if query.retain.is_empty() {
        BTreeSet::new()
    } else {
        query
            .retain
            .split(',')
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
    };
    if retained.len() > 100 {
        return Err(Error::invalid(
            "At most 100 retained expenses may be requested",
        ));
    }
    for id in &retained {
        validate_id(id)?;
    }
    let result = store
        .run(move |db| {
            let read = db.transaction()?;
            store::require_budget(&read, &budget)?;
            let snapshot = concurrency::cursor(&read, &budget)?;
            concurrency::check_snapshot(&read, &budget, query.snapshot.as_deref())?;
            let exists: bool = read.query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE budget_id=?1 AND id=?2 AND closed=0 AND tombstone=0)",
                params![budget, query.account_id],
                |row| row.get(0),
            )?;
            if query.account_id.is_some() && !exists {
                return Err(Error::missing());
            }
            // Transfer exclusion precedes activation and applies to retained IDs too.
            let mut statement = read.prepare(&format!(
                "SELECT id FROM transactions t WHERE budget_id=?1 AND (?2 IS NULL OR account_id=?2)
             AND tombstone=0 AND reconciled=0 AND amount<0 AND transfer_id IS NULL AND is_parent=0 AND parent_id IS NULL
             ORDER BY date DESC,{} DESC,id ASC LIMIT 10",crate::android_upgrade::order::SQL
            ))?;
            let ids = statement
                .query_map(params![budget, query.account_id], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let recent = ids
                .iter()
                .map(|id| ledger::get(&read, &budget, id))
                .collect::<Result<Vec<_>, _>>()?;
            let mut extra = vec![];
            let mut unavailable = vec![];
            for id in retained {
                if ids.contains(&id) {
                    continue;
                }
                if eligible(&read, &budget, query.account_id.as_deref(), &id)? {
                    extra.push(ledger::get(&read, &budget, &id)?);
                } else {
                    unavailable.push(id);
                }
            }
            let epoch = crate::lifecycle::epoch(&read, &budget)?;
            Ok(json!({"data": {
                "recent": recent, "retained": extra, "unavailable": unavailable,
                "snapshot": snapshot, "epoch": epoch,
            }}))
        })
        .await?;
    Ok(Json(result))
}

fn eligible(db: &Connection, budget: &str, account: Option<&str>, id: &str) -> Result<bool, Error> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM transactions WHERE budget_id=?1 AND (?2 IS NULL OR account_id=?2)
         AND id=?3 AND tombstone=0 AND reconciled=0 AND amount<0 AND transfer_id IS NULL AND is_parent=0 AND parent_id IS NULL)",
        params![budget, account, id],
        |row| row.get(0),
    )?)
}
