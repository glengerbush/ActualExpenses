//! Statement reconciliation and explicit protection changes share the receipt transaction.
use crate::{
    api::{idempotency_key, json_error},
    concurrency,
    error::Error,
    ledger,
    models::{MAX_MONEY, validate_date, validate_id},
    store::Store,
};
use axum::{
    Json, Router,
    extract::{
        OriginalUri, Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use rusqlite::{Connection, params};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub fn router() -> Router<Store> {
    Router::new()
        .route(
            "/api/v1/budgets/{budget}/accounts/{id}/reconciliation",
            get(read).post(mutate),
        )
        .route(
            "/api/v1/budgets/{budget}/transactions/{id}/unlock",
            post(mutate),
        )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Statement {
    statement_balance: Option<i64>,
}
fn path(p: Result<Path<(String, String)>, PathRejection>) -> Result<(String, String), Error> {
    let Path((b, id)) = p.map_err(|_| Error::invalid("Invalid reconciliation path"))?;
    validate_id(&b)?;
    validate_id(&id)?;
    Ok((b, id))
}
fn money(value: i128) -> Result<i64, Error> {
    if value.abs() > MAX_MONEY as i128 {
        Err(Error::invalid(
            "Reconciliation amount exceeds the exact integer range",
        ))
    } else {
        Ok(value as i64)
    }
}
pub fn preview(db: &Connection, b: &str, id: &str, target: Option<i64>) -> Result<Value, Error> {
    ledger::account_exists(db, b, id)?;
    let balances = ledger::cached_balances(db, b, id)?;
    let target = target.unwrap_or(balances.cleared);
    money(target as i128)?;
    let (locked,ready,uncleared):(i64,i64,i64)=db.query_row("SELECT COALESCE(SUM(reconciled=1),0),COALESCE(SUM(cleared=1 AND reconciled=0),0),COALESCE(SUM(cleared=0),0) FROM transactions WHERE budget_id=?1 AND account_id=?2 AND tombstone=0 AND parent_id IS NULL",params![b,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    Ok(
        json!({"account_id":id,"balances":balances,"statement_balance":target,"difference":money(target as i128-balances.cleared as i128)?,"locked_count":locked,"ready_count":ready,"uncleared_count":uncleared,"snapshot":concurrency::cursor(db,b)?,"last_reconciled":crate::accounts::get(db,b,id)?["last_reconciled"],"edit_clock":db.query_row("SELECT clock FROM budget_sync WHERE budget_id=?1",[b],|r|r.get::<_,String>(0))?}),
    )
}
async fn read(
    State(s): State<Store>,
    p: Result<Path<(String, String)>, PathRejection>,
    q: Result<Query<Statement>, QueryRejection>,
) -> Result<Json<Value>, Error> {
    let (b, id) = path(p)?;
    let Query(q) = q.map_err(|_| Error::invalid("Invalid statement balance"))?;
    Ok(Json(
        json!({"data":s.run(move|db|{let tx=db.transaction()?;preview(&tx,&b,&id,q.statement_balance)}).await?}),
    ))
}
async fn mutate(
    State(s): State<Store>,
    p: Result<Path<(String, String)>, PathRejection>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let (b, id) = path(p)?;
    let Json(body) = input.map_err(json_error)?;
    let key = idempotency_key(&headers)?;
    let owner = crate::history::owner_header(&headers)?;
    let proposed = concurrency::header(&headers)?;
    let cancel = concurrency::cancel_header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let unlock = uri.path().ends_with("/unlock");
    let scope = format!("POST {}", uri.path().trim_start_matches("/api/v1"));
    s.run(move |db| {
        concurrency::write_owned(
            db,
            &scope,
            &key,
            &body.to_string(),
            proposed.as_deref(),
            StatusCode::OK,
            owner.as_ref(),
            |db| {
                if cancel {
                    return concurrency::canceled(db, &b);
                }
                crate::lifecycle::require_epoch(db, &b, epoch.as_deref())?;
                let clock = concurrency::clock(db, &b, proposed.as_deref())?;
                if unlock {
                    return unlock_row(db, &b, &id, &body, &clock);
                }
                reconcile(db, &b, &id, &body, &clock, epoch.as_deref())
            },
        )
    })
    .await
}
fn fields(body: &Value, allowed: &[&str]) -> Result<(), Error> {
    let values = body
        .as_object()
        .ok_or_else(|| Error::invalid("Expected reconciliation object"))?;
    if values.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(Error::invalid("Unknown reconciliation field"));
    }
    Ok(())
}
fn required<'a>(body: &'a Value, key: &str) -> Result<&'a str, Error> {
    body[key]
        .as_str()
        .ok_or_else(|| Error::invalid(format!("{key} is required")))
}
fn reconcile(
    db: &Connection,
    b: &str,
    id: &str,
    body: &Value,
    clock: &str,
    epoch: Option<&str>,
) -> Result<Value, Error> {
    let action = required(body, "action")?;
    fields(
        body,
        if action == "adjust" {
            &[
                "action",
                "statement_balance",
                "snapshot",
                "date",
                "transaction_id",
            ]
        } else {
            &["action", "statement_balance", "snapshot", "date"]
        },
    )?;
    let date = required(body, "date")?;
    validate_date(date)?;
    let target = body["statement_balance"]
        .as_i64()
        .ok_or_else(|| Error::invalid("statement_balance must be an exact integer"))?;
    concurrency::check_snapshot(db, b, Some(required(body, "snapshot")?))?;
    let before = preview(db, b, id, Some(target))?;
    let difference = before["difference"].as_i64().unwrap();
    match action {
        "adjust" => {
            if difference == 0 {
                return Err(Error::conflict(
                    "The statement already matches; no adjustment is needed",
                ));
            }
            let tid = required(body, "transaction_id")?;
            validate_id(tid)?;
            let input = ledger::NewTransaction {
                id: tid.into(),
                account_id: id.into(),
                amount: difference,
                date: date.into(),
                cleared: true,
                notes: "Reconciliation balance adjustment".into(),
                payee_id: None,
                category_id: None,
                new_payee: None,
                new_category: None,
            };
            input.validate()?;
            // Reuse ordinary ordered creation; future rule execution belongs at this same seam.
            concurrency::execute(
                db,
                b,
                Some(clock),
                false,
                epoch,
                concurrency::Mutation::Create(Box::new(input)),
            )?;
        }
        "lock" => {
            if difference != 0 {
                return Err(Error::new(
                    StatusCode::CONFLICT,
                    "reconciliation_difference",
                    "The cleared balance must match the statement before locking",
                ));
            }
            let mut q=db.prepare("SELECT id FROM transactions WHERE budget_id=?1 AND account_id=?2 AND tombstone=0 AND cleared=1 AND reconciled=0")?;
            let ids = q
                .query_map(params![b, id], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            for tid in ids {
                let versions = concurrency::versions(db, b, &tid)?;
                if versions
                    .as_object()
                    .unwrap()
                    .values()
                    .any(|v| v.as_str().is_some_and(|v| v >= clock))
                {
                    return Err(Error::new(
                        StatusCode::CONFLICT,
                        "reconciliation_superseded",
                        "A newer transaction edit exists. Refresh before reconciling.",
                    ));
                }
                db.execute(
                    "UPDATE transactions SET reconciled=1 WHERE budget_id=?1 AND id=?2",
                    params![b, tid],
                )?;
                concurrency::seed_field_version(db, b, "transactions", &tid, "reconciled", clock)?;
                concurrency::mark(db, b, "transactions", &tid)?;
            }
            let previous = crate::accounts::get(db, b, id)?;
            if previous["field_versions"]["last_reconciled"]
                .as_str()
                .is_some_and(|v| v >= clock)
            {
                return Err(Error::conflict(
                    "A newer reconciliation exists. Refresh before reconciling.",
                ));
            }
            let when = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(Error::internal)?
                .as_millis()
                .to_string();
            db.execute(
                "UPDATE accounts SET last_reconciled=?3 WHERE budget_id=?1 AND id=?2",
                params![b, id, when],
            )?;
            concurrency::seed_field_version(db, b, "accounts", id, "last_reconciled", clock)?;
            concurrency::mark(db, b, "accounts", id)?;
        }
        _ => return Err(Error::invalid("action must be adjust or lock")),
    }
    preview(db, b, id, Some(target))
}
fn unlock_row(
    db: &Connection,
    b: &str,
    id: &str,
    body: &Value,
    clock: &str,
) -> Result<Value, Error> {
    fields(
        body,
        &[
            "reconciled_version",
            "reconciled_versions",
            "reconciliation_token",
        ],
    )?;
    let rows = members(db, b, id)?;
    if let Some(token) = body.get("reconciliation_token") {
        verify_token(
            b,
            &rows,
            token
                .as_str()
                .ok_or_else(|| Error::invalid("Invalid reconciliation token"))?,
            clock,
        )?;
    } else {
        let versions = if rows.len() == 1 && body.get("reconciled_versions").is_none() {
            BTreeMap::from([(
                id.to_owned(),
                required(body, "reconciled_version")?.to_owned(),
            )])
        } else {
            serde_json::from_value::<BTreeMap<String, String>>(body["reconciled_versions"].clone())
                .map_err(|_| Error::invalid("Supply reconciled_versions for every split member"))?
        };
        if versions.len() > 101 {
            return Err(Error::invalid("Too many reconciliation proofs"));
        }
        for row in &rows {
            let tid = row["id"].as_str().unwrap();
            let current = row["field_versions"]["reconciled"]
                .as_str()
                .unwrap_or(concurrency::ZERO);
            if versions.get(tid).map(String::as_str) != Some(current) || clock <= current {
                return Err(Error::new(
                    StatusCode::CONFLICT,
                    "reconciliation_changed",
                    "The protection changed. Refresh before unlocking the whole split.",
                ));
            }
        }
    }
    for row in rows {
        let tid = row["id"].as_str().unwrap();
        db.execute(
            "UPDATE transactions SET reconciled=0 WHERE budget_id=?1 AND id=?2",
            params![b, tid],
        )?;
        concurrency::seed_field_version(db, b, "transactions", tid, "reconciled", clock)?;
        concurrency::mark(db, b, "transactions", tid)?;
    }
    ledger::get(db, b, id)
}
/// A split's parent and every live allocation share protection, even when the
/// action originates from a child. Transfer callers expand both such sets.
pub fn members(db: &Connection, budget: &str, id: &str) -> Result<Vec<Value>, Error> {
    let row = ledger::get(db, budget, id)?;
    let parent = match row["parent_id"].as_str() {
        Some(parent) => ledger::get(db, budget, parent)?,
        None => row,
    };
    let mut rows = parent["children"].as_array().cloned().unwrap_or_default();
    rows.insert(0, parent);
    Ok(rows)
}
/// Fixed-size proof of the complete logical aggregate. Never hash presentation
/// order, caller-supplied membership, or unrelated mutable ledger fields.
pub fn protection_token(budget: &str, rows: &[Value]) -> String {
    let mut state: Vec<_> = rows
        .iter()
        .map(|row| {
            (
                row["id"].as_str().unwrap(),
                row["reconciled"].as_bool().unwrap_or(false),
                row["field_versions"]["reconciled"]
                    .as_str()
                    .unwrap_or(concurrency::ZERO),
            )
        })
        .collect();
    state.sort_unstable_by(|a, b| a.0.cmp(b.0));
    let payload = json!([
        "actualBudget.reconciliation.v1",
        budget,
        rows[0]["id"],
        state
    ]);
    format!("{:x}", Sha256::digest(payload.to_string().as_bytes()))
}
/// Called only after hydrating every current member inside the receipt transaction.
/// Fresh digest alone is insufficient: a delayed original clock must still refuse.
pub fn verify_token(budget: &str, rows: &[Value], token: &str, clock: &str) -> Result<(), Error> {
    if token.len() != 64
        || !token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::invalid("Invalid reconciliation token"));
    }
    if token != protection_token(budget, rows)
        || rows.iter().any(|row| {
            clock
                <= row["field_versions"]["reconciled"]
                    .as_str()
                    .unwrap_or(concurrency::ZERO)
        })
    {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "reconciliation_changed",
            "The split membership or protection changed. Refresh and confirm the current transaction.",
        ));
    }
    Ok(())
}
pub fn protect_members(
    rows: &[Value],
    versions: Option<&BTreeMap<String, String>>,
    clock: &str,
) -> Result<(), Error> {
    for row in rows {
        protect(
            row,
            versions
                .and_then(|versions| versions.get(row["id"].as_str().unwrap()))
                .map(|version| Confirmation { version, clock }),
        )?;
    }
    Ok(())
}
/// Proof captured when the user confirmed a protected edit. Aggregate callers
/// must construct one proof per affected protected row, never reuse another leg's.
#[derive(Clone, Copy)]
pub struct Confirmation<'a> {
    pub version: &'a str,
    pub clock: &'a str,
}
/// Shared guard for ordinary, split, transfer and future bulk mutations.
pub fn protect(row: &Value, confirmation: Option<Confirmation<'_>>) -> Result<(), Error> {
    if row["reconciled"] != true {
        return Ok(());
    }
    let Some(confirmation) = confirmation else {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "reconciled_protected",
            "This transaction is reconciled. Confirm the edit or explicitly unlock it first.",
        ));
    };
    let current = row["field_versions"]["reconciled"]
        .as_str()
        .unwrap_or(concurrency::ZERO);
    if confirmation.version != current || confirmation.clock <= current {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "reconciliation_changed",
            "Reconciliation protection changed after this confirmation. Refresh and confirm the current transaction before editing.",
        ));
    }
    Ok(())
}
pub fn source_timestamp(value: &Value) -> Result<Option<String>, Error> {
    if value.is_null() {
        return Ok(None);
    }
    let raw = value
        .as_str()
        .ok_or_else(|| Error::invalid("Invalid last reconciliation timestamp"))?;
    let ms = raw
        .parse::<i64>()
        .map_err(|_| Error::invalid("Invalid last reconciliation timestamp"))?;
    if ms < 0 || chrono::DateTime::from_timestamp_millis(ms).is_none() {
        return Err(Error::invalid("Invalid last reconciliation timestamp"));
    }
    Ok(Some(raw.to_owned()))
}
