//! One-time native CRDT handoff. Message clocks are data, never a batch clock.
//! Receipts retain the original operation and all original messages indefinitely.
use crate::{
    api::{budget_id, idempotency_key, json_error},
    concurrency::{self, Mutation},
    error::Error,
    ledger,
    models::validate_id,
    store::Store,
};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub fn router() -> Router<Store> {
    Router::new()
        .route("/api/v1/android-upgrade/sources", get(sources))
        .route(
            "/api/v1/budgets/{budget}/android-upgrade/operations",
            post(operation),
        )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    file_id: String,
    group_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Handoff {
    source: Source,
    epoch: String,
    operation: LegacyOperation,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LegacyOperation {
    id: String,
    transaction_id: String,
    creates: bool,
    deletes: bool,
    created_offline: bool,
    attempted: bool,
    changes: Vec<Message>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Message {
    timestamp: String,
    dataset: String,
    row: String,
    column: String,
    value: Value,
}
fn matches_source(db: &Connection, budget: &str, source: &Source) -> Result<bool, Error> {
    // The released client stores cloud file ID + sync group, never the archive's local filename.
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM imported_sources WHERE budget_id=?1 AND source_cloud_id=?2 AND source_group_id=?3)",params![budget,source.file_id,source.group_id],|r|r.get(0))?)
}
async fn sources(
    State(store): State<Store>,
    query: Result<Query<Source>, QueryRejection>,
) -> Result<Json<Value>, Error> {
    let Query(source) =
        query.map_err(|_| Error::invalid("Supply original file_id and group_id"))?;
    validate_id(&source.file_id)?;
    validate_id(&source.group_id)?;
    let result=store.run(move|db|{let mut q=db.prepare("SELECT b.id,b.name,s.source_local_id FROM imported_sources s JOIN budgets b ON b.id=s.budget_id WHERE source_cloud_id=?1 AND source_group_id=?2 ORDER BY b.id")?;
 let rows=q.query_map(params![source.file_id,source.group_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?.collect::<Result<Vec<_>,_>>()?;
 let mut out=vec![];for (id,name,local) in rows{let metadata=crate::lifecycle::metadata(db,&id,false)?;out.push(json!({"id":id,"name":name,"source_local_id":local,"epoch":metadata["epoch"]}));}Ok(json!({"data":out}))}).await?;
    Ok(Json(result))
}
async fn operation(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let budget = budget_id(path)?;
    let Json(body) = input.map_err(json_error)?;
    let key = idempotency_key(&headers)?;
    let handoff: Handoff = serde_json::from_value(body.clone())
        .map_err(|_| Error::invalid("Invalid Android upgrade operation"))?;
    let receipt_only = concurrency::receipt_only_header(&headers)?;
    let cancel = concurrency::cancel_header(&headers)?;
    let payload = body.to_string();
    let scope = format!("POST /budgets/{budget}/android-upgrade/operations");
    store
        .run(move |db| {
            if receipt_only {
                return concurrency::receipt(db, &scope, &key, &payload, None, StatusCode::OK);
            }
            concurrency::write(db, &scope, &key, &payload, None, StatusCode::OK, |db| {
                if cancel {
                    return concurrency::canceled(db, &budget);
                }
                apply(db, &budget, &key, handoff)
            })
        })
        .await
}
fn blocked(message: impl Into<String>) -> Error {
    Error::new(
        StatusCode::CONFLICT,
        "android_upgrade_requires_review",
        message,
    )
}
fn native_field(message: &Message) -> Result<Option<(&str, Value)>, Error> {
    let field = match message.column.as_str() {
        "acct" => "account_id",
        "description" => "payee_id",
        "category" => "category_id",
        "amount" => "amount",
        "notes" => "notes",
        "date" => "date",
        "cleared" => "cleared",
        "tombstone" => "tombstone",
        "reconciled" | "isParent" | "isChild" => {
            if message.value != 0 {
                return Err(blocked(
                    "Only ordinary unreconciled expenses can be migrated",
                ));
            }
            return Ok(None);
        }
        "parent_id" | "transferred_id" => {
            if !message.value.is_null() {
                return Err(blocked("Review split or transfer changes in actualBudget"));
            }
            return Ok(None);
        }
        "sort_order" => {
            if !message.value.is_number() {
                return Err(Error::invalid("Invalid legacy sort order"));
            }
            return Ok(None);
        }
        _ => {
            return Err(blocked(
                "This saved field cannot be migrated safely; original work is retained",
            ));
        }
    };
    let value = match field {
        "date" => {
            let n = message
                .value
                .as_i64()
                .ok_or_else(|| Error::invalid("Invalid legacy date"))?;
            let date = format!("{:04}-{:02}-{:02}", n / 10000, n / 100 % 100, n % 100);
            crate::models::validate_date(&date)?;
            json!(date)
        }
        "notes" => {
            if message.value.is_null() {
                json!("")
            } else {
                message.value.clone()
            }
        }
        "cleared" | "tombstone" => match message.value.as_i64() {
            Some(0) => json!(false),
            Some(1) => json!(true),
            _ => return Err(Error::invalid("Legacy status must be 0 or 1")),
        },
        _ => message.value.clone(),
    };
    Ok(Some((field, value)))
}
fn apply(db: &Connection, budget: &str, key: &str, handoff: Handoff) -> Result<Value, Error> {
    let op = handoff.operation;
    validate_id(&op.id)?;
    validate_id(&op.transaction_id)?;
    if op.id != key {
        return Err(Error::invalid(
            "Use the original operation ID as Idempotency-Key",
        ));
    }
    if !matches_source(db, budget, &handoff.source)? {
        return Err(blocked(
            "No matching imported source budget and sync group. Import the original archive, then select its exact identity.",
        ));
    }
    crate::lifecycle::require_epoch(db, budget, Some(&handoff.epoch))?;
    if op.changes.is_empty() || op.changes.len() > 100 {
        return Err(Error::invalid(
            "A legacy operation needs 1–100 original messages",
        ));
    }
    // Flags are retained in the receipt. Attempted is uncertainty, never acknowledgement.
    let _ = (op.created_offline, op.attempted);
    let mut timestamps = BTreeSet::new();
    let mut cells = BTreeSet::new();
    for m in &op.changes {
        validate_id(&m.row)?;
        concurrency::validate_import_clock(&m.timestamp)?;
        if concurrency::import_clock_ahead(&m.timestamp)? {
            return Err(blocked(
                "Correct the device clock and retry the unchanged saved work",
            ));
        }
        if !timestamps.insert(&m.timestamp) || !cells.insert((&m.dataset, &m.row, &m.column)) {
            return Err(Error::invalid("Repeated message identity or field"));
        }
        if m.dataset == "transactions" {
            if m.row != op.transaction_id {
                return Err(Error::invalid("Operation contains another transaction"));
            }
            native_field(m)?;
        } else if !matches!(
            (m.dataset.as_str(), m.column.as_str()),
            ("payees", "name" | "tombstone" | "transfer_acct") | ("payee_mapping", "targetId")
        ) {
            return Err(blocked(
                "Unsupported legacy metadata is retained for review",
            ));
        }
    }
    if op.deletes
        && (op.creates
            || op.changes.len() != 1
            || op.changes[0].column != "tombstone"
            || op.changes[0].value != 1)
    {
        return Err(Error::invalid("Invalid legacy deletion"));
    }
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM transactions WHERE budget_id=?1 AND id=?2)",
        params![budget, op.transaction_id],
        |r| r.get(0),
    )?;
    if !exists && !op.creates {
        return Err(blocked(
            "The original expense is absent from this import. Import the matching source snapshot; do not recreate an uncertain edit.",
        ));
    }
    let mut conflicts = vec![];
    // Published tombstones cannot be resurrected, even by a newer ordinary field.
    if exists
        && db.query_row(
            "SELECT tombstone FROM transactions WHERE budget_id=?1 AND id=?2",
            params![budget, op.transaction_id],
            |r| r.get::<_, bool>(0),
        )?
    {
        return Ok(
            json!({"data":{"id":op.transaction_id,"deleted":true},"outcome":"published_deletion","conflicts":[],"cursor":concurrency::cursor(db,budget)?}),
        );
    }
    // Prepare original inline payees before transaction references, inside the same receipt transaction.
    let mut names = BTreeMap::new();
    for m in &op.changes {
        if m.dataset == "payees" && m.column == "name" {
            crate::models::validate_name(
                m.value
                    .as_str()
                    .ok_or_else(|| Error::invalid("Invalid payee name"))?,
            )?;
            names.insert(&m.row, m);
        }
    }
    for m in &op.changes {
        if m.dataset == "transactions" {
            continue;
        }
        match (m.dataset.as_str(), m.column.as_str()) {
            ("payees", "name") => {
                let current: Option<(String, bool)> = db
                    .query_row(
                        "SELECT name,tombstone FROM payees WHERE budget_id=?1 AND id=?2",
                        params![budget, m.row],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                if current.is_none() {
                    db.execute(
                        "INSERT INTO payees(budget_id,id,name) VALUES(?1,?2,?3)",
                        params![budget, m.row, m.value.as_str()],
                    )?;
                    concurrency::seed_field_version(
                        db,
                        budget,
                        "payees",
                        &m.row,
                        "name",
                        &m.timestamp,
                    )?;
                    concurrency::mark(db, budget, "payees", &m.row)?;
                } else {
                    let winning:String=db.query_row("SELECT COALESCE((SELECT clock FROM field_versions WHERE budget_id=?1 AND entity='payees' AND entity_id=?2 AND field='name'),?3)",params![budget,m.row,concurrency::ZERO],|r|r.get(0))?;
                    if m.timestamp > winning && !current.as_ref().unwrap().1 {
                        db.execute(
                            "UPDATE payees SET name=?3 WHERE budget_id=?1 AND id=?2",
                            params![budget, m.row, m.value.as_str()],
                        )?;
                        concurrency::seed_field_version(
                            db,
                            budget,
                            "payees",
                            &m.row,
                            "name",
                            &m.timestamp,
                        )?;
                        concurrency::mark(db, budget, "payees", &m.row)?;
                    }
                }
            }
            ("payees", "tombstone") if m.value == 0 => {}
            ("payees", "transfer_acct") if m.value.is_null() => {}
            ("payee_mapping", "targetId") if m.value == m.row => {}
            _ => {
                return Err(blocked(
                    "The saved payee change needs review; original messages are retained",
                ));
            }
        }
    }
    if !exists {
        let mut fields = serde_json::Map::new();
        for m in &op.changes {
            if m.dataset == "transactions" {
                if let Some((field, value)) = native_field(m)? {
                    fields.insert(field.into(), value);
                }
            }
        }
        for required in ["account_id", "amount", "date"] {
            if !fields.contains_key(required) {
                return Err(blocked(
                    "The saved creation is incomplete. Keep the original app data for recovery",
                ));
            }
        }
        if fields["amount"].as_i64().is_none_or(|n| n >= 0) {
            return Err(blocked("Only ordinary expenses can be migrated here"));
        }
        fields.remove("tombstone");
        fields.insert("id".into(), json!(op.transaction_id));
        let input: ledger::NewTransaction = serde_json::from_value(Value::Object(fields))
            .map_err(|_| Error::invalid("Invalid saved expense"))?;
        input.validate()?;
        ledger::create(db, budget, input)?;
        for m in &op.changes {
            if m.dataset == "transactions" {
                if let Some((field, _)) = native_field(m)? {
                    concurrency::seed_field_version(
                        db,
                        budget,
                        "transactions",
                        &op.transaction_id,
                        field,
                        &m.timestamp,
                    )?;
                }
            }
        }
        concurrency::mark(db, budget, "transactions", &op.transaction_id)?;
    } else {
        for m in &op.changes {
            if m.dataset != "transactions" {
                continue;
            }
            let Some((field, value)) = native_field(m)? else {
                continue;
            };
            if field == "tombstone" && value == false {
                continue;
            }
            // The new ledger owns rank; source sort_order is deliberately not a native field.
            let mutation = if field == "tombstone" {
                Mutation::ExpenseDelete(op.transaction_id.clone())
            } else {
                Mutation::ExpensePatch(op.transaction_id.clone(), json!({field:value}))
            };
            let result = concurrency::execute(
                db,
                budget,
                Some(&m.timestamp),
                false,
                Some(&handoff.epoch),
                mutation,
            )?;
            let body = &result["response_body"];
            if result["response_status"] != 200 {
                if body["error"]["code"] == "deletion_superseded" {
                    conflicts.push(json!({"field":"tombstone","outcome":"deletion_superseded"}));
                } else {
                    return Err(blocked(
                        body["error"]["message"]
                            .as_str()
                            .unwrap_or("The expense needs review in actualBudget"),
                    ));
                }
            }
            if let Some(items) = body["conflicts"].as_array() {
                conflicts.extend(items.iter().cloned());
            }
        }
    }
    Ok(
        json!({"data":{"id":op.transaction_id},"outcome":"reconciled","conflicts":conflicts,"cursor":concurrency::cursor(db,budget)?}),
    )
}
