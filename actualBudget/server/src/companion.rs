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
    routing::get,
};
use rusqlite::{Connection, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub fn router() -> Router<Store> {
    Router::new().route("/api/v1/budgets/{budget}/companion", get(view))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewQuery {
    account_id: String,
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
    validate_id(&query.account_id)?;
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
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE budget_id=?1 AND id=?2 AND closed=0)",
                params![budget, query.account_id],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(Error::missing());
            }
            // Only ordinary transactions can currently be activated/imported. Future
            // split/transfer support must extend this predicate before enabling those
            // records. The companion preserves its date/sort/ID ordering contract.
            let mut statement = read.prepare(
                "SELECT id FROM transactions WHERE budget_id=?1 AND account_id=?2
             AND tombstone=0 AND reconciled=0 AND amount<0
             ORDER BY date DESC,sort_order DESC,id ASC LIMIT 10",
            )?;
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
                if eligible(&read, &budget, &query.account_id, &id)? {
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

fn eligible(db: &Connection, budget: &str, account: &str, id: &str) -> Result<bool, Error> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM transactions WHERE budget_id=?1 AND account_id=?2
         AND id=?3 AND tombstone=0 AND reconciled=0 AND amount<0)",
        params![budget, account, id],
        |row| row.get(0),
    )?)
}
