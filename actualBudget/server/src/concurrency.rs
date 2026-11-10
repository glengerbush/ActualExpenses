//! Per-field ordering and incremental state convergence. All writes run inside
//! the same SQLite transaction as the financial mutation and durable receipt.
use crate::{
    error::Error,
    ledger::{self, NewTransaction},
    store,
};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{PathRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
    routing::get,
};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};
pub const ZERO: &str = "1970-01-01T00:00:00.000Z-0000-0000000000000000";
const FIELDS: &[&str] = &[
    "account_id",
    "amount",
    "date",
    "cleared",
    "notes",
    "payee_id",
    "category_id",
    "reconciled",
    "starting_balance",
    "tombstone",
];
fn now_ms() -> Result<i64, Error> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(Error::internal)?
            .as_millis(),
    )
    .map_err(Error::internal)
}
pub(crate) fn parse_clock(value: &str) -> Result<(i64, u32), Error> {
    let invalid = || {
        Error::new(
            StatusCode::BAD_REQUEST,
            "invalid_clock",
            "Edit clock must be a canonical Actual HLC timestamp",
        )
    };
    if value.len() != 46
        || !value.is_ascii()
        || &value[24..25] != "-"
        || &value[29..30] != "-"
        || !value[25..29]
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'A'..=b'F').contains(&c))
        || !value[30..].bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err(invalid());
    }
    let date = DateTime::parse_from_rfc3339(&value[..24]).map_err(|_| invalid())?;
    if date.timestamp_millis() < 0
        || date.to_rfc3339_opts(SecondsFormat::Millis, true) != value[..24]
    {
        return Err(invalid());
    }
    Ok((
        date.timestamp_millis(),
        u32::from_str_radix(&value[25..29], 16).map_err(|_| invalid())?,
    ))
}
pub fn validate_import_clock(value: &str) -> Result<(), Error> {
    parse_clock(value)?;
    Ok(())
}
pub fn import_clock_ahead(value: &str) -> Result<bool, Error> {
    Ok(parse_clock(value)?.0 - now_ms()? > 300_000)
}
pub fn cancel_header(headers: &HeaderMap) -> Result<bool, Error> {
    match headers.get("X-Actual-Cancel-If-Uncommitted") {
        None => Ok(false),
        Some(value) if value == "true" => Ok(true),
        Some(_) => Err(Error::invalid(
            "X-Actual-Cancel-If-Uncommitted must be true when supplied",
        )),
    }
}
pub fn receipt_only_header(headers: &HeaderMap) -> Result<bool, Error> {
    match headers.get("X-Actual-Receipt-Only") {
        None => Ok(false),
        Some(value) if value == "true" => Ok(true),
        Some(_) => Err(Error::invalid(
            "X-Actual-Receipt-Only must be true when supplied",
        )),
    }
}
pub fn header(headers: &HeaderMap) -> Result<Option<String>, Error> {
    headers
        .get("X-Actual-Edit-Clock")
        .map(|v| {
            v.to_str()
                .map(str::to_owned)
                .map_err(|_| Error::invalid("Invalid edit clock header"))
        })
        .transpose()
}
pub(crate) fn clock(
    db: &Connection,
    budget: &str,
    proposed: Option<&str>,
) -> Result<String, Error> {
    let now = now_ms()?;
    let (last, node): (String, String) = db.query_row(
        "SELECT clock,node FROM budget_sync WHERE budget_id=?1",
        [budget],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let (last_ms, last_counter) = parse_clock(&last)?;
    let incoming = proposed.map(parse_clock).transpose()?;
    if incoming.is_some_and(|(ms, _)| ms - now > 300_000) || last_ms - now > 300_000 {
        return Err(Error::new(
            StatusCode::BAD_REQUEST,
            "clock_skew",
            "A device clock is more than five minutes ahead. Correct its clock and retry the unchanged operation.",
        ));
    }
    let millis = now.max(last_ms).max(incoming.map_or(0, |v| v.0));
    let counter = match incoming {
        Some((remote, c)) if millis == last_ms && millis == remote => last_counter.max(c) + 1,
        Some((remote, c)) if millis == remote => c + 1,
        _ if millis == last_ms => last_counter + 1,
        _ => 0,
    };
    if counter > 65535 {
        return Err(Error::new(
            StatusCode::BAD_REQUEST,
            "clock_overflow",
            "Logical clock counter exhausted. Retry the unchanged operation after time advances.",
        ));
    }
    let date = DateTime::<Utc>::from_timestamp_millis(millis)
        .ok_or_else(|| Error::invalid("Clock outside supported range"))?
        .to_rfc3339_opts(SecondsFormat::Millis, true);
    let generated = format!("{date}-{counter:04X}-{node}");
    db.execute(
        "UPDATE budget_sync SET clock=?2 WHERE budget_id=?1",
        params![budget, generated],
    )?;
    Ok(proposed.map(str::to_owned).unwrap_or(generated))
}
/// Import exact source ordering; never replace missing clocks with migration time.
/// Unknown fields may be retained here too. This does not resurrect SQL tombstones.
pub fn seed_field_version(
    db: &Connection,
    budget: &str,
    entity: &str,
    id: &str,
    field: &str,
    clock: &str,
) -> Result<(), Error> {
    parse_clock(clock)?;
    db.prepare_cached("INSERT INTO field_versions(budget_id,entity,entity_id,field,clock) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(budget_id,entity,entity_id,field) DO UPDATE SET clock=excluded.clock WHERE excluded.clock>field_versions.clock")?.execute(params![budget,entity,id,field,clock])?;
    crate::notes::mirror_domain_clock(db, budget, entity, id, field, clock)?;
    db.prepare_cached("UPDATE budget_sync SET clock=?2 WHERE budget_id=?1 AND clock<?2")?
        .execute(params![budget, clock])?;
    Ok(())
}
pub fn stamp_new_transaction(db: &Connection, budget: &str, id: &str) -> Result<(), Error> {
    let assigned = clock(db, budget, None)?;
    for field in FIELDS {
        seed_field_version(db, budget, "transactions", id, field, &assigned)?;
    }
    mark(db, budget, "transactions", id)
}
pub fn versions(db: &Connection, budget: &str, id: &str) -> Result<Value, Error> {
    let mut values = serde_json::Map::new();
    for field in FIELDS {
        values.insert((*field).into(), json!(ZERO));
    }
    let mut q=db.prepare("SELECT field,clock FROM field_versions WHERE budget_id=?1 AND entity='transactions' AND entity_id=?2")?;
    for row in q.query_map(params![budget, id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (k, v) = row?;
        values.insert(k, json!(v));
    }
    Ok(Value::Object(values))
}
pub fn mark(db: &Connection, budget: &str, entity: &str, id: &str) -> Result<(), Error> {
    db.execute(
        "UPDATE budget_sync SET sequence=sequence+1 WHERE budget_id=?1",
        [budget],
    )?;
    db.execute("INSERT INTO budget_changes(budget_id,entity,entity_id,sequence) VALUES(?1,?2,?3,(SELECT sequence FROM budget_sync WHERE budget_id=?1)) ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence",params![budget,entity,id])?;
    Ok(())
}
pub fn cursor(db: &Connection, budget: &str) -> Result<String, Error> {
    let (epoch, sequence): (String, i64) = db.query_row(
        "SELECT epoch,sequence FROM budget_sync WHERE budget_id=?1",
        [budget],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(format!("c1{epoch}{sequence:016x}"))
}
fn deleted(db: &Connection, budget: &str, id: &str) -> Result<bool, Error> {
    db.query_row(
        "SELECT tombstone FROM transactions WHERE budget_id=?1 AND id=?2",
        params![budget, id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(Error::missing)
}
fn tombstone(db: &Connection, budget: &str, id: &str) -> Result<Value, Error> {
    Ok(json!({"id":id,"deleted":true,"field_versions":versions(db,budget,id)?}))
}
fn outcome(status: StatusCode, body: Value) -> Value {
    json!({"response_status":status.as_u16(),"response_body":body})
}
fn success(
    db: &Connection,
    budget: &str,
    data: Value,
    clock: &str,
    conflicts: Vec<Value>,
    status: StatusCode,
) -> Result<Value, Error> {
    Ok(outcome(
        status,
        json!({"data":data,"edit_clock":clock,"conflicts":conflicts,"cursor":cursor(db,budget)?}),
    ))
}
fn refusal(
    db: &Connection,
    budget: &str,
    data: Value,
    code: &str,
    message: &str,
) -> Result<Value, Error> {
    Ok(outcome(
        StatusCode::CONFLICT,
        json!({"error":{"code":code,"message":message},"data":data,"cursor":cursor(db,budget)?}),
    ))
}
pub enum Mutation {
    Create(Box<NewTransaction>),
    Patch(String, Value),
    ExpensePatch(String, Value),
    ExpenseDelete(String),
    Delete(String),
    Duplicate(String, String),
    Split(String, crate::splits::Change),
}
/// Called only inside a receipt transaction, after exact original-receipt lookup.
pub fn canceled(db: &Connection, budget: &str) -> Result<Value, Error> {
    crate::lifecycle::metadata(db, budget, true)?;
    Ok(outcome(
        StatusCode::CONFLICT,
        json!({"error":{"code":"operation_canceled","message":"This change was canceled before it was saved."}}),
    ))
}

pub fn execute(
    db: &Connection,
    budget: &str,
    proposed: Option<&str>,
    cancel_if_uncommitted: bool,
    expected_epoch: Option<&str>,
    mutation: Mutation,
) -> Result<Value, Error> {
    if cancel_if_uncommitted {
        return canceled(db, budget);
    }
    store::require_budget(db, budget)?;
    crate::lifecycle::require_epoch(db, budget, expected_epoch)?;
    let edit_clock = clock(db, budget, proposed)?;
    let mutation = match mutation {
        Mutation::ExpensePatch(ref id, _) | Mutation::ExpenseDelete(ref id) => {
            let row = ledger::get_any(db, budget, id)?;
            let code = if !row["transfer_id"].is_null() {
                Some("transfer_transaction")
            } else if row["is_parent"] == true || !row["parent_id"].is_null() {
                Some("split_transaction")
            } else {
                None
            };
            if let Some(code) = code {
                return refusal(
                    db,
                    budget,
                    row,
                    code,
                    "This expense is now part of a split or transfer. Your pending change is retained; discard it explicitly or review it in actualBudget.",
                );
            }
            match mutation {
                Mutation::ExpensePatch(id, fields) => Mutation::Patch(id, fields),
                Mutation::ExpenseDelete(id) => Mutation::Delete(id),
                _ => unreachable!(),
            }
        }
        other => other,
    };
    match mutation {
        Mutation::ExpensePatch(..) | Mutation::ExpenseDelete(..) => unreachable!(),
        Mutation::Split(id, input) => {
            let data = crate::splits::apply(db, budget, &id, input, &edit_clock)?;
            success(db, budget, data, &edit_clock, vec![], StatusCode::OK)
        }
        Mutation::Create(input) => {
            let learn = input.learn_categories == Some(true);
            let id = input.id.clone();
            let new_payee = input.new_payee.as_ref().map(|p| p.id.clone());
            ledger::create(db, budget, *input)?;
            if let Some(payee) = new_payee {
                seed_field_version(db, budget, "payees", &payee, "name", &edit_clock)?;
            }
            for field in FIELDS {
                seed_field_version(db, budget, "transactions", &id, field, &edit_clock)?;
            }
            mark(db, budget, "transactions", &id)?;
            if learn {
                crate::rules::learning::manual(db, budget, std::slice::from_ref(&id), &edit_clock)?;
            }
            success(
                db,
                budget,
                ledger::get(db, budget, &id)?,
                &edit_clock,
                vec![],
                StatusCode::CREATED,
            )
        }
        Mutation::Duplicate(source, id) => {
            if crate::transfers::counterpart(db, budget, &source)?.is_some() {
                return crate::transfers::duplicate(db, budget, &source, &id, &edit_clock);
            }
            let original = ledger::get(db, budget, &source)?;
            let duplicate = ledger::duplicate(db, budget, &source, &id)?;
            if let Some(children) = duplicate["children"].as_array() {
                for (index, child) in children.iter().enumerate() {
                    crate::transfers::composition::clone_child(
                        db,
                        budget,
                        &original["children"][index],
                        child["id"].as_str().unwrap(),
                        &edit_clock,
                    )?;
                }
                crate::splits::stamp_many(
                    db,
                    budget,
                    &children
                        .iter()
                        .map(|child| child["id"].as_str().unwrap())
                        .collect::<Vec<_>>(),
                    &edit_clock,
                )?;
            }
            for field in FIELDS {
                seed_field_version(db, budget, "transactions", &id, field, &edit_clock)?;
            }
            mark(db, budget, "transactions", &id)?;
            success(
                db,
                budget,
                ledger::get(db, budget, &id)?,
                &edit_clock,
                vec![],
                StatusCode::CREATED,
            )
        }
        Mutation::Patch(id, mut patch) => {
            let learn = match patch
                .as_object_mut()
                .and_then(|p| p.remove("learn_categories"))
            {
                None => false,
                Some(Value::Bool(v)) => v,
                _ => return Err(Error::invalid("learn_categories must be boolean")),
            };

            let confirmed = match patch
                .as_object_mut()
                .and_then(|p| p.remove("confirm_reconciled"))
            {
                None => false,
                Some(Value::Bool(v)) => v,
                _ => return Err(Error::invalid("confirm_reconciled must be boolean")),
            };
            let confirmed_version = match patch
                .as_object_mut()
                .and_then(|p| p.remove("reconciled_version"))
            {
                None => None,
                Some(Value::String(version)) if confirmed => Some(version),
                _ => {
                    return Err(Error::invalid(
                        "reconciled_version must be a string accompanying confirm_reconciled=true",
                    ));
                }
            };
            let transfer_proof = match patch
                .as_object_mut()
                .and_then(|p| p.remove("transfer_protection_token"))
            {
                None => None,
                Some(Value::String(token)) if confirmed => Some(token),
                _ => {
                    return Err(Error::invalid(
                        "transfer_protection_token requires confirm_reconciled=true",
                    ));
                }
            };
            let hidden = deleted(db, budget, &id)?;
            if !hidden && crate::transfers::counterpart(db, budget, &id)?.is_some() {
                return crate::transfers::patch(
                    db,
                    budget,
                    &id,
                    patch,
                    &edit_clock,
                    transfer_proof.as_deref(),
                );
            }
            if transfer_proof.is_some() {
                return Err(Error::invalid(
                    "Transfer protection proof requires an active transfer",
                ));
            }
            // Retired pair references are provenance, never propagation edges.
            // Keep valid local tombstone merges; a stale date-sync choice has no
            // effect on live rows or shared preferences.
            if hidden
                && let Some(sync) = patch
                    .as_object_mut()
                    .and_then(|p| p.remove("sync_transfer_date"))
                && !sync.is_boolean()
            {
                return Err(Error::invalid("sync_transfer_date must be boolean"));
            }
            let previous = if hidden {
                ledger::get_any(db, budget, &id)?
            } else {
                ledger::get(db, budget, &id)?
            };
            if !hidden && (previous["is_parent"] == true || !previous["parent_id"].is_null()) {
                return refusal(
                    db,
                    budget,
                    previous,
                    "split_transaction",
                    "This expense is now part of a split. Your pending edit is retained; cancel it or review the split in actualBudget.",
                );
            }
            ledger::validate_patch(&previous, &patch)?;
            let fields = patch.as_object().unwrap();
            let old = versions(db, budget, &id)?;
            let mut accepted = serde_json::Map::new();
            let mut conflicts = vec![];
            let mut changed_fields = vec![];
            for (field, value) in fields {
                let target = match field.as_str() {
                    "new_payee" => "payee_id",
                    "new_category" => "category_id",
                    _ => field,
                };
                let winning = old[target].as_str().unwrap_or(ZERO);
                if edit_clock.as_str() > winning {
                    accepted.insert(field.clone(), value.clone());
                    changed_fields.push(target.to_owned());
                } else {
                    conflicts.push(json!({"field":target,"submitted_clock":edit_clock,"winning_clock":winning}));
                }
            }
            if !accepted.is_empty() {
                let learn_category =
                    learn && accepted.get("category_id").is_some_and(|v| !v.is_null());
                let new_payee = accepted
                    .get("new_payee")
                    .and_then(|v| v["id"].as_str())
                    .map(str::to_owned);
                if hidden {
                    ledger::update_hidden(db, budget, &id, Value::Object(accepted))?;
                } else {
                    if let Some(version) = confirmed_version.as_deref() {
                        ledger::update_confirmed(
                            db,
                            budget,
                            &id,
                            Value::Object(accepted),
                            crate::reconciliation::Confirmation {
                                version,
                                clock: &edit_clock,
                            },
                        )?;
                    } else {
                        ledger::update(db, budget, &id, Value::Object(accepted))?;
                    }
                }
                if let Some(payee) = new_payee {
                    seed_field_version(db, budget, "payees", &payee, "name", &edit_clock)?;
                }
                for field in changed_fields {
                    seed_field_version(db, budget, "transactions", &id, &field, &edit_clock)?;
                }
                mark(db, budget, "transactions", &id)?;
                if learn_category {
                    crate::rules::learning::manual(
                        db,
                        budget,
                        std::slice::from_ref(&id),
                        &edit_clock,
                    )?;
                }
            }
            let mut result = if hidden {
                ledger::get_any(db, budget, &id)?
            } else {
                ledger::get(db, budget, &id)?
            };
            if hidden {
                result["deleted"] = json!(true);
            }
            success(db, budget, result, &edit_clock, conflicts, StatusCode::OK)
        }
        Mutation::Delete(id) => {
            if crate::transfers::counterpart(db, budget, &id)?.is_some() {
                return crate::transfers::delete(db, budget, &id, &edit_clock);
            }
            if deleted(db, budget, &id)? {
                return refusal(
                    db,
                    budget,
                    tombstone(db, budget, &id)?,
                    "transaction_deleted",
                    "This transaction was already deleted.",
                );
            }
            let known = versions(db, budget, &id)?;
            let newer_child:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM transactions c JOIN field_versions v ON v.budget_id=c.budget_id AND v.entity='transactions' AND v.entity_id=c.id WHERE c.budget_id=?1 AND c.parent_id=?2 AND c.tombstone=0 AND v.clock>?3)",params![budget,id,edit_clock],|r|r.get(0))?;
            let aggregate = ledger::get(db, budget, &id)?;
            let newer_peer = crate::transfers::composition::newer_related(
                db,
                budget,
                aggregate["children"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
                &edit_clock,
            )?;
            if newer_child
                || newer_peer
                || known.as_object().unwrap().values().any(|clock| {
                    clock
                        .as_str()
                        .is_some_and(|clock| clock > edit_clock.as_str())
                })
            {
                return refusal(
                    db,
                    budget,
                    ledger::get(db, budget, &id)?,
                    "deletion_superseded",
                    "A newer edit kept this transaction. The pending deletion was canceled.",
                );
            }
            let previous = ledger::get(db, budget, &id)?;
            if let Some(children) = previous["children"].as_array() {
                crate::transfers::composition::guard_children(
                    db,
                    budget,
                    children,
                    &edit_clock,
                    None,
                )?;
            }
            ledger::delete(db, budget, &id)?;
            if let Some(children) = previous["children"].as_array() {
                for child in children {
                    let cid = child["id"].as_str().unwrap();
                    crate::splits::remove(db, budget, cid, &edit_clock)?;
                }
            }
            seed_field_version(db, budget, "transactions", &id, "tombstone", &edit_clock)?;
            mark(db, budget, "transactions", &id)?;
            success(
                db,
                budget,
                tombstone(db, budget, &id)?,
                &edit_clock,
                vec![],
                StatusCode::OK,
            )
        }
    }
}
pub fn write(
    db: &mut Connection,
    scope: &str,
    key: &str,
    payload: &str,
    clock: Option<&str>,
    legacy_status: StatusCode,
    operation: impl FnOnce(&Connection) -> Result<Value, Error>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let payload = receipt_payload(payload, clock);
    let result = store::write(db, scope, key, &payload, operation)?;
    receipt_response(result, legacy_status)
}
pub(crate) fn receipt_payload(payload: &str, clock: Option<&str>) -> String {
    // Keep old headerless payload bytes so pre-upgrade pending receipts still replay.
    clock
        .map(|clock| json!({"payload":payload,"edit_clock":clock}).to_string())
        .unwrap_or_else(|| payload.to_owned())
}
pub fn receipt(
    db: &Connection,
    scope: &str,
    key: &str,
    payload: &str,
    clock: Option<&str>,
    legacy_status: StatusCode,
) -> Result<(StatusCode, Json<Value>), Error> {
    let result =
        store::receipt(db, scope, key, &receipt_payload(payload, clock))?.ok_or_else(|| {
            Error::new(
                StatusCode::NOT_FOUND,
                "operation_uncommitted",
                "No committed outcome exists for this request.",
            )
        })?;
    receipt_response(result, legacy_status)
}
fn receipt_response(
    result: Value,
    legacy_status: StatusCode,
) -> Result<(StatusCode, Json<Value>), Error> {
    if let (Some(status), Some(body)) = (
        result["response_status"].as_u64(),
        result.get("response_body"),
    ) {
        Ok((
            StatusCode::from_u16(status as u16).map_err(Error::internal)?,
            Json(body.clone()),
        ))
    } else {
        Ok((legacy_status, Json(json!({"data":result}))))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChangeQuery {
    cursor: String,
    #[serde(default = "limit")]
    limit: u32,
}
fn limit() -> u32 {
    100
}
pub fn check_snapshot(db: &Connection, budget: &str, expected: Option<&str>) -> Result<(), Error> {
    store::require_budget(db, budget)?;
    if let Some(expected) = expected
        && cursor(db, budget)? != expected
    {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "snapshot_changed",
            "Budget data changed while loading. Start a fresh snapshot.",
        ));
    }
    Ok(())
}
pub fn router() -> Router<store::Store> {
    Router::new().route("/api/v1/budgets/{budget}/changes", get(changes))
}
async fn changes(
    State(store): State<store::Store>,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<ChangeQuery>, QueryRejection>,
) -> Result<Json<Value>, Error> {
    let budget = crate::api::budget_id(path)?;
    let Query(query) = query.map_err(|_| Error::invalid("Invalid change query"))?;
    if query.limit == 0 || query.limit > 1000 {
        return Err(Error::invalid("limit must be1–1000"));
    }
    Ok(Json(
        store
            .run(move |db| {
                let read = db.transaction()?;
                read_changes(&read, &budget, &query.cursor, query.limit)
            })
            .await?,
    ))
}
pub fn read_changes(
    db: &Connection,
    budget: &str,
    after: &str,
    limit: u32,
) -> Result<Value, Error> {
    store::require_budget(db, budget)?;
    let checkpoint = cursor(db, budget)?;
    if after.len() != 50
        || !after.is_ascii()
        || after[..34] != checkpoint[..34]
        || !after[34..].bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "cursor_invalid",
            "This checkpoint belongs to another budget or snapshot. Reload the initial view.",
        ));
    }
    let sequence = i64::from_str_radix(&after[34..], 16)
        .map_err(|_| Error::invalid("Invalid change sequence"))?;
    let current = i64::from_str_radix(&checkpoint[34..], 16).map_err(Error::internal)?;
    if sequence > current {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "cursor_invalid",
            "The checkpoint is newer than this database. Reload the initial view.",
        ));
    }
    let mut q=db.prepare("SELECT entity,entity_id,sequence FROM budget_changes WHERE budget_id=?1 AND sequence>?2 ORDER BY sequence LIMIT ?3")?;
    let mut rows = q
        .query_map(params![budget, sequence, limit + 1], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let next = if more {
        format!("{}{:016x}", &checkpoint[..34], rows.last().unwrap().2)
    } else {
        checkpoint
    };
    let mut data = vec![];
    for (entity, id, _) in rows {
        let value = match entity.as_str() {
            "transactions" => {
                if deleted(db, budget, &id)? {
                    tombstone(db, budget, &id)?
                } else {
                    ledger::get(db, budget, &id)?
                }
            }
            "notes" => crate::notes::record(db, budget, &id)?,
            "accounts" => crate::accounts::get(db, budget, &id)?,
            "account_groups" => crate::accounts::get_group(db, budget, &id)?,
            "preferences" => crate::preferences::get_preferences(db, budget)?,
            "payees" | "tags" => crate::payees_tags::record(db, budget, &entity, &id)?,
            "payee_locations" => crate::payee_locations::record(db, budget, &id)?,
            "rules" => crate::rules::record(db, budget, &id)?,
            "schedules" | "schedule_occurrences" | "schedule_links" | "schedule_settings" => {
                crate::schedules::record_entity(db, budget, &entity, &id)?
            }
            "rule_settings" => {
                json!({"id":"learning","enabled":crate::rules::learning::enabled(db,budget)?,"deleted":false})
            }
            "transaction_import_metadata" | "transaction_import_settings" => {
                crate::transaction_imports::record(db, budget, &entity, &id)?
            }
            "budget_automations" | "budget_cleanup_groups" => {
                crate::budgeting::templates::record(db, budget, &entity, &id)?
            }
            "budget_cells" | "budget_months" | "budget_settings" => {
                crate::budgeting::record(db, budget, &entity, &id)?
            }
            "register_settings" => crate::register_settings::read_settings(db, budget)?,
            "categories" | "category_groups" => crate::categories::get(db, budget, &entity, &id)?,
            _ => return Err(Error::internal("Unknown change entity")),
        };
        data.push(json!({"entity":entity,"id":id,"data":value}));
    }
    Ok(json!({"data":data,"next_cursor":next,"has_more":more}))
}

pub(crate) fn decorate_many(
    db: &Connection,
    budget: &str,
    rows: &mut [Value],
) -> Result<(), Error> {
    if rows.is_empty() {
        return Ok(());
    }
    let positions = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row["id"].as_str().unwrap().to_string(), index))
        .collect::<std::collections::HashMap<_, _>>();
    let mut parameters = vec![budget.to_string()];
    parameters.extend(rows.iter().map(|r| r["id"].as_str().unwrap().to_string()));
    for row in rows.iter_mut() {
        row["field_versions"] = json!(
            FIELDS
                .iter()
                .map(|field| (field.to_string(), json!(ZERO)))
                .collect::<serde_json::Map<_, _>>()
        );
    }
    let placeholders = (2..=rows.len() + 1)
        .map(|n| format!("?{n}"))
        .collect::<Vec<_>>()
        .join(",");
    let mut statement=db.prepare(&format!("SELECT entity_id,field,clock FROM field_versions WHERE budget_id=?1 AND entity='transactions' AND entity_id IN ({placeholders})"))?;
    for row in statement.query_map(rusqlite::params_from_iter(parameters.iter()), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (id, field, clock) = row?;
        rows[positions[&id]]["field_versions"][field] = json!(clock);
    }
    crate::android_upgrade::order::decorate(db, budget, rows)?;
    crate::transaction_imports::decorate_many(db, budget, rows)?;
    Ok(())
}

/// Opt-in history; owner and captured epoch are part of immutable receipt identity.
// Keeps the existing receipt API plus its explicit history owner.
#[allow(clippy::too_many_arguments)]
pub fn write_owned(
    db: &mut Connection,
    scope: &str,
    key: &str,
    payload: &str,
    clock: Option<&str>,
    status: StatusCode,
    owner: Option<&crate::history::Owner>,
    operation: impl FnOnce(&Connection) -> Result<Value, Error>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let payload = crate::history::payload(payload, owner);
    write(db, scope, key, &payload, clock, status, |db| {
        if let Some(owner) = owner {
            let budget = scope
                .split('/')
                .nth(2)
                .ok_or_else(|| Error::internal("Missing history budget"))?;
            crate::history::record(db, budget, owner, scope, operation)
        } else {
            operation(db)
        }
    })
}
pub fn receipt_owned(
    db: &Connection,
    scope: &str,
    key: &str,
    payload: &str,
    clock: Option<&str>,
    status: StatusCode,
    owner: Option<&crate::history::Owner>,
) -> Result<(StatusCode, Json<Value>), Error> {
    receipt(
        db,
        scope,
        key,
        &crate::history::payload(payload, owner),
        clock,
        status,
    )
}
