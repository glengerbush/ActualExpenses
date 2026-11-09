//! Transfer pairs are one financial operation and one receipt transaction.
use crate::{
    api, concurrency as order,
    error::Error,
    ledger,
    models::{MAX_MONEY, validate_date, validate_id},
    store::{self, Store},
};
use axum::{
    Json, Router,
    extract::{
        Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::{HeaderMap, StatusCode},
    routing::post,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Create {
    id: String,
    counterpart_id: String,
    account_id: String,
    to_account_id: String,
    amount: i64,
    date: String,
    #[serde(default)]
    cleared: bool,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    category_id: Option<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Link {
    id: String,
    counterpart_id: String,
}

pub fn router() -> Router<Store> {
    Router::new()
        .route("/api/v1/budgets/{budget}/transfers", post(create))
        .route("/api/v1/budgets/{budget}/transfers/link", post(link))
}
async fn create(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<Create>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let b = api::budget_id(path)?;
    let Json(input) = input.map_err(api::json_error)?;
    let body = serde_json::to_string(&input).map_err(Error::internal)?;
    write(store, b, "transfers", headers, body, move |db, b, clock| {
        create_pair(db, b, &input, clock)?;
        response(db, b, &input.id, clock, vec![], StatusCode::CREATED)
    })
    .await
}
async fn link(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<Link>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let b = api::budget_id(path)?;
    let Json(input) = input.map_err(api::json_error)?;
    let body = serde_json::to_string(&input).map_err(Error::internal)?;
    write(
        store,
        b,
        "transfers/link",
        headers,
        body,
        move |db, b, clock| {
            link_pair(db, b, &input, clock)?;
            response(db, b, &input.id, clock, vec![], StatusCode::OK)
        },
    )
    .await
}
async fn write<F>(
    store: Store,
    b: String,
    route: &'static str,
    headers: HeaderMap,
    body: String,
    op: F,
) -> Result<(StatusCode, Json<Value>), Error>
where
    F: FnOnce(&Connection, &str, &str) -> Result<Value, Error> + Send + 'static,
{
    let key = api::idempotency_key(&headers)?;
    let proposed = order::header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let cancel = order::cancel_header(&headers)?;
    let receipt = order::receipt_only_header(&headers)?;
    store
        .run(move |db| {
            let scope = format!("POST /budgets/{b}/{route}");
            if receipt {
                return order::receipt(
                    db,
                    &scope,
                    &key,
                    &body,
                    proposed.as_deref(),
                    StatusCode::OK,
                );
            }
            order::write(
                db,
                &scope,
                &key,
                &body,
                proposed.as_deref(),
                StatusCode::OK,
                |db| {
                    if cancel {
                        return order::canceled(db, &b);
                    }
                    store::require_budget(db, &b)?;
                    crate::lifecycle::require_epoch(db, &b, epoch.as_deref())?;
                    let clock = order::clock(db, &b, proposed.as_deref())?;
                    op(db, &b, &clock)
                },
            )
        })
        .await
}
fn account(db: &Connection, b: &str, id: &str) -> Result<bool, Error> {
    validate_id(id)?;
    db.query_row(
        "SELECT off_budget FROM accounts WHERE budget_id=?1 AND id=?2 AND closed=0 AND tombstone=0",
        params![b, id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| Error::invalid("Choose an open account in this budget"))
}
fn payee(db: &Connection, b: &str, account: &str) -> Result<String, Error> {
    if let Some(id)=db.query_row("SELECT id FROM payees WHERE budget_id=?1 AND transfer_account_id=?2 AND tombstone=0 ORDER BY id LIMIT 1",params![b,account],|r|r.get::<_,String>(0)).optional()?{return Ok(id)}
    let id = uuid::Uuid::new_v4().to_string();
    db.execute("INSERT INTO payees(budget_id,id,name,transfer_account_id) SELECT ?1,?2,name,id FROM accounts WHERE budget_id=?1 AND id=?3",params![b,id,account])?;
    Ok(id)
}
pub fn counterpart(db: &Connection, b: &str, id: &str) -> Result<Option<String>, Error> {
    Ok(db
        .query_row(
            "SELECT transfer_id FROM transactions WHERE budget_id=?1 AND id=?2",
            params![b, id],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten())
}
fn stamp(db: &Connection, b: &str, id: &str, fields: &[&str], clock: &str) -> Result<(), Error> {
    for f in fields {
        order::seed_field_version(db, b, "transactions", id, f, clock)?;
    }
    order::mark(db, b, "transactions", id)
}
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
    "transfer_id",
];
fn create_pair(db: &Connection, b: &str, input: &Create, clock: &str) -> Result<(), Error> {
    for id in [&input.id, &input.counterpart_id] {
        validate_id(id)?;
    }
    if input.id == input.counterpart_id || input.account_id == input.to_account_id {
        return Err(Error::invalid(
            "A transfer needs distinct entries and accounts",
        ));
    }
    validate_date(&input.date)?;
    if !(-MAX_MONEY..=MAX_MONEY).contains(&input.amount) || input.notes.chars().count() > 10000 {
        return Err(Error::invalid("Invalid transfer amount or notes"));
    }
    let from = account(db, b, &input.account_id)?;
    let to = account(db, b, &input.to_account_id)?;
    let category = crate::categories::resolve(db, b, input.category_id.clone())?;
    // Same-side transfers (including card payments) never create categorized income.
    let category = if from == to || from { None } else { category };
    let p1 = payee(db, b, &input.to_account_id)?;
    let p2 = payee(db, b, &input.account_id)?;
    for (id, acct, amount, cleared, payee, category, other) in [
        (
            &input.id,
            &input.account_id,
            input.amount,
            input.cleared,
            p1,
            category,
            &input.counterpart_id,
        ),
        (
            &input.counterpart_id,
            &input.to_account_id,
            -input.amount,
            false,
            p2,
            None,
            &input.id,
        ),
    ] {
        if db.query_row(
            "SELECT EXISTS(SELECT 1 FROM transactions WHERE budget_id=?1 AND id=?2)",
            params![b, id],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(Error::conflict(
                "Transfer transaction identity already exists",
            ));
        }
        db.execute("INSERT INTO transactions(budget_id,id,account_id,amount,date,cleared,notes,payee_id,category_id,transfer_id,sort_order) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,(SELECT COALESCE(MAX(sort_order),0)+1 FROM transactions WHERE budget_id=?1))",params![b,id,acct,amount,input.date,cleared,input.notes,payee,category,other])?;
        stamp(db, b, id, FIELDS, clock)?;
    }
    refresh(db, b, &input.id, &input.counterpart_id)
}
fn refresh(db: &Connection, b: &str, id: &str, other: &str) -> Result<(), Error> {
    for id in [id, other] {
        let acct: String = db.query_row(
            "SELECT account_id FROM transactions WHERE budget_id=?1 AND id=?2",
            params![b, id],
            |r| r.get(0),
        )?;
        ledger::refresh_account(db, b, &acct)?;
    }
    ledger::validate_budget_total(db, b)
}
fn link_pair(db: &Connection, b: &str, input: &Link, clock: &str) -> Result<(), Error> {
    validate_id(&input.id)?;
    validate_id(&input.counterpart_id)?;
    let a = ledger::get(db, b, &input.id)?;
    let z = ledger::get(db, b, &input.counterpart_id)?;
    if a["account_id"] == z["account_id"]
        || a["amount"].as_i64().unwrap() != -z["amount"].as_i64().unwrap()
        || !a["transfer_id"].is_null()
        || !z["transfer_id"].is_null()
    {
        return Err(Error::invalid(
            "Choose two opposite amounts in different accounts that are not linked",
        ));
    }
    for row in [&a, &z] {
        if row["reconciled"] == true || row["starting_balance"] == true {
            return Err(Error::conflict(
                "Opening balances and reconciled entries cannot be linked",
            ));
        }
        account(db, b, row["account_id"].as_str().unwrap())?;
        if newer(row, clock) {
            return Err(Error::conflict(
                "A newer edit prevents linking. Refresh both entries.",
            ));
        }
    }
    for (row, other) in [(&a, &z), (&z, &a)] {
        let id = row["id"].as_str().unwrap();
        let payee = payee(db, b, other["account_id"].as_str().unwrap())?;
        db.execute("UPDATE transactions SET transfer_id=?3,payee_id=?4,category_id=NULL WHERE budget_id=?1 AND id=?2",params![b,id,other["id"].as_str(),payee])?;
        stamp(
            db,
            b,
            id,
            &["transfer_id", "payee_id", "category_id"],
            clock,
        )?;
    }
    Ok(())
}
fn newer(row: &Value, clock: &str) -> bool {
    row["field_versions"].as_object().is_some_and(|o| {
        o.values()
            .any(|v| v.as_str().unwrap_or(order::ZERO) > clock)
    })
}
fn response(
    db: &Connection,
    b: &str,
    id: &str,
    clock: &str,
    conflicts: Vec<Value>,
    status: StatusCode,
) -> Result<Value, Error> {
    let mut row = ledger::get_any(db, b, id)?;
    let deleted: bool = db.query_row(
        "SELECT tombstone FROM transactions WHERE budget_id=?1 AND id=?2",
        params![b, id],
        |r| r.get(0),
    )?;
    if deleted {
        row["deleted"] = json!(true)
    }
    Ok(
        json!({"response_status":status.as_u16(),"response_body":{"data":row,"edit_clock":clock,"conflicts":conflicts,"cursor":order::cursor(db,b)?}}),
    )
}
fn refusal(db: &Connection, b: &str, id: &str, code: &str, message: &str) -> Result<Value, Error> {
    Ok(
        json!({"response_status":409,"response_body":{"error":{"code":code,"message":message},"data":ledger::get_any(db,b,id)?,"cursor":order::cursor(db,b)?}}),
    )
}
pub fn patch(
    db: &Connection,
    b: &str,
    id: &str,
    patch: Value,
    clock: &str,
) -> Result<Value, Error> {
    let other = counterpart(db, b, id)?.ok_or_else(Error::missing)?;
    let a = ledger::get_any(db, b, id)?;
    let z = ledger::get_any(db, b, &other)?;
    let mut fields = patch
        .as_object()
        .cloned()
        .ok_or_else(|| Error::invalid("Expected transfer fields"))?;
    let preference = fields.get("sync_transfer_date").cloned();
    let sync = match fields.remove("sync_transfer_date") {
        Some(v) => v
            .as_bool()
            .ok_or_else(|| Error::invalid("sync_transfer_date must be boolean"))?,
        None => crate::preferences::get_preferences(db, b)?["sync_transfer_date"] == true,
    };
    if fields.is_empty() {
        return Err(Error::invalid("Provide at least one transfer field"));
    }
    if a["reconciled"] == true || z["reconciled"] == true {
        return Err(Error::conflict(
            "Unlock both reconciled transfer entries before editing",
        ));
    }
    let mut conflicts = vec![];
    for (field, value) in fields {
        let shared = field == "amount" || field == "notes" || field == "date" && sync;
        if ![
            "amount",
            "notes",
            "date",
            "cleared",
            "category_id",
            "account_id",
        ]
        .contains(&field.as_str())
        {
            return Err(Error::invalid(format!(
                "Cannot edit transfer field {field}"
            )));
        }
        let mut winning = a["field_versions"][&field].as_str().unwrap_or(order::ZERO);
        if shared {
            winning = winning.max(z["field_versions"][&field].as_str().unwrap_or(order::ZERO));
        }
        if clock <= winning {
            conflicts.push(json!({"field":field,"winning_clock":winning,"submitted_clock":clock}));
            continue;
        }
        let sql = match field.as_str() {
            "amount" => rusqlite::types::Value::Integer(
                value
                    .as_i64()
                    .filter(|n| (-MAX_MONEY..=MAX_MONEY).contains(n))
                    .ok_or_else(|| Error::invalid("Invalid amount"))?,
            ),
            "notes" => rusqlite::types::Value::Text(
                value
                    .as_str()
                    .filter(|s| s.chars().count() <= 10000)
                    .ok_or_else(|| Error::invalid("Invalid notes"))?
                    .into(),
            ),
            "date" => {
                let date = value
                    .as_str()
                    .ok_or_else(|| Error::invalid("Invalid date"))?;
                validate_date(date)?;
                rusqlite::types::Value::Text(date.into())
            }
            "cleared" => rusqlite::types::Value::Integer(i64::from(
                value
                    .as_bool()
                    .ok_or_else(|| Error::invalid("Invalid cleared value"))?,
            )),
            "category_id" => {
                let category = if value.is_null() {
                    None
                } else {
                    Some(
                        value
                            .as_str()
                            .ok_or_else(|| Error::invalid("Invalid category"))?
                            .to_owned(),
                    )
                };
                let category = crate::categories::resolve(db, b, category)?;
                let off = account(db, b, a["account_id"].as_str().unwrap())?;
                let other_off = account(db, b, z["account_id"].as_str().unwrap())?;
                if category.is_some() && (off || off == other_off) {
                    return Err(Error::invalid(
                        "Only the on-budget side of a crossing transfer can have a category",
                    ));
                }
                category
                    .map(rusqlite::types::Value::Text)
                    .unwrap_or(rusqlite::types::Value::Null)
            }
            "account_id" => {
                let acct = value
                    .as_str()
                    .ok_or_else(|| Error::invalid("Invalid account"))?;
                account(db, b, acct)?;
                if value == z["account_id"] {
                    return Err(Error::invalid("Transfer accounts must differ"));
                }
                let p = payee(db, b, acct)?;
                db.execute(
                    "UPDATE transactions SET payee_id=?3 WHERE budget_id=?1 AND id=?2",
                    params![b, other, p],
                )?;
                stamp(db, b, &other, &["payee_id"], clock)?;
                rusqlite::types::Value::Text(acct.into())
            }
            _ => unreachable!(),
        };
        db.execute(
            &format!("UPDATE transactions SET {field}=?3 WHERE budget_id=?1 AND id=?2"),
            params![b, id, sql],
        )?;
        stamp(db, b, id, &[&field], clock)?;
        if shared {
            let opposite = if field == "amount" {
                rusqlite::types::Value::Integer(-value.as_i64().unwrap())
            } else {
                sql
            };
            db.execute(
                &format!("UPDATE transactions SET {field}=?3 WHERE budget_id=?1 AND id=?2"),
                params![b, other, opposite],
            )?;
            stamp(db, b, &other, &[&field], clock)?;
        }
    }
    if let Some(preference) = preference {
        crate::preferences::apply_changes(
            db,
            b,
            &serde_json::Map::from_iter([("sync_transfer_date".into(), preference)]),
        )?;
    }
    // Moving between on/off-budget accounts changes category eligibility, atomically.
    let from = ledger::get_any(db, b, id)?;
    let to = ledger::get_any(db, b, &other)?;
    let from_off: bool = db.query_row(
        "SELECT off_budget FROM accounts WHERE budget_id=?1 AND id=?2",
        params![b, from["account_id"].as_str()],
        |r| r.get(0),
    )?;
    let to_off: bool = db.query_row(
        "SELECT off_budget FROM accounts WHERE budget_id=?1 AND id=?2",
        params![b, to["account_id"].as_str()],
        |r| r.get(0),
    )?;
    for row in [&from, &to] {
        if from_off == to_off && !row["category_id"].is_null() {
            let rid = row["id"].as_str().unwrap();
            db.execute(
                "UPDATE transactions SET category_id=NULL WHERE budget_id=?1 AND id=?2",
                params![b, rid],
            )?;
            stamp(db, b, rid, &["category_id"], clock)?;
        }
    }
    for row in [&a, &z] {
        ledger::refresh_account(db, b, row["account_id"].as_str().unwrap())?;
    }
    refresh(db, b, id, &other)?;
    response(db, b, id, clock, conflicts, StatusCode::OK)
}
pub fn delete(db: &Connection, b: &str, id: &str, clock: &str) -> Result<Value, Error> {
    let other = counterpart(db, b, id)?.ok_or_else(Error::missing)?;
    let a = ledger::get_any(db, b, id)?;
    let z = ledger::get_any(db, b, &other)?;
    let hidden: bool = db.query_row(
        "SELECT tombstone FROM transactions WHERE budget_id=?1 AND id=?2",
        params![b, id],
        |r| r.get(0),
    )?;
    if hidden {
        let mut result = refusal(
            db,
            b,
            id,
            "transaction_deleted",
            "This transfer was already deleted.",
        )?;
        result["response_body"]["data"] =
            json!({"id":id,"deleted":true,"field_versions":order::versions(db,b,id)?});
        return Ok(result);
    }
    for row in [&a, &z] {
        if newer(row, clock) {
            return refusal(
                db,
                b,
                id,
                "deletion_superseded",
                "A newer edit kept the transfer. The pending deletion was canceled.",
            );
        }
        if row["reconciled"] == true {
            return Err(Error::conflict(
                "Unlock both reconciled transfer entries before deleting",
            ));
        }
    }
    for rid in [id, &other] {
        db.execute(
            "UPDATE transactions SET tombstone=1 WHERE budget_id=?1 AND id=?2",
            params![b, rid],
        )?;
        stamp(db, b, rid, &["tombstone"], clock)?;
    }
    refresh(db, b, id, &other)?;
    let mut result = response(db, b, id, clock, vec![], StatusCode::OK)?;
    result["response_body"]["data"] =
        json!({"id":id,"deleted":true,"field_versions":order::versions(db,b,id)?});
    Ok(result)
}
/// Validate imported/native state before making it visible. Hidden historic pairs
/// retain their references; every visible leg must have one reciprocal active leg.
pub fn validate(db: &Connection, b: &str) -> Result<(), Error> {
    let invalid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM transactions t LEFT JOIN transactions p ON p.budget_id=t.budget_id AND p.id=t.transfer_id LEFT JOIN payees y ON y.budget_id=t.budget_id AND y.id=t.payee_id WHERE t.budget_id=?1 AND t.tombstone=0 AND ((t.transfer_id IS NOT NULL AND (p.id IS NULL OR p.tombstone<>0 OR p.transfer_id IS NOT t.id OR p.amount<>-t.amount OR p.account_id=t.account_id OR y.transfer_account_id IS NOT p.account_id)) OR (t.transfer_id IS NULL AND y.transfer_account_id IS NOT NULL)))",[b],|r|r.get(0))?;
    if invalid {
        return Err(Error::invalid(
            "Transfer pairs must have reciprocal references, accounts and opposite amounts",
        ));
    }
    Ok(())
}

pub fn duplicate(
    db: &Connection,
    b: &str,
    source: &str,
    id: &str,
    clock: &str,
) -> Result<Value, Error> {
    let row = ledger::get(db, b, source)?;
    let other = ledger::get(
        db,
        b,
        row["transfer_id"].as_str().ok_or_else(Error::missing)?,
    )?;
    let peer = uuid::Uuid::new_v4().to_string();
    create_pair(
        db,
        b,
        &Create {
            id: id.into(),
            counterpart_id: peer.clone(),
            account_id: row["account_id"].as_str().unwrap().into(),
            to_account_id: other["account_id"].as_str().unwrap().into(),
            amount: row["amount"].as_i64().unwrap(),
            date: row["date"].as_str().unwrap().into(),
            cleared: false,
            notes: row["notes"].as_str().unwrap().into(),
            category_id: row["category_id"].as_str().map(str::to_owned),
        },
        clock,
    )?;
    db.execute("UPDATE transactions SET date=?3,category_id=?4,imported_payee=?5 WHERE budget_id=?1 AND id=?2",params![b,peer,other["date"].as_str(),other["category_id"].as_str(),other["imported_payee"].as_str()])?;
    db.execute(
        "UPDATE transactions SET imported_payee=?3 WHERE budget_id=?1 AND id=?2",
        params![b, id, row["imported_payee"].as_str()],
    )?;
    refresh(db, b, id, &peer)?;
    response(db, b, id, clock, vec![], StatusCode::CREATED)
}

pub fn close_account(
    db: &Connection,
    b: &str,
    id: &str,
    body: &Value,
    clock: &str,
) -> Result<Value, Error> {
    let fields = body
        .as_object()
        .ok_or_else(|| Error::invalid("Expected account transfer fields"))?;
    if fields.keys().any(|k| {
        ![
            "id",
            "counterpart_id",
            "to_account_id",
            "date",
            "category_id",
            "expected_balance",
        ]
        .contains(&k.as_str())
    }) {
        return Err(Error::invalid("Unknown account transfer field"));
    }
    let balance = ledger::cached_balances(db, b, id)?.total;
    if body["expected_balance"] != balance {
        return Err(Error::conflict(
            "The account balance changed. Review the balance before closing.",
        ));
    }
    let newest:Option<String>=db.query_row("SELECT MAX(clock) FROM field_versions WHERE budget_id=?1 AND ((entity='accounts' AND entity_id=?2) OR (entity='transactions' AND entity_id IN (SELECT id FROM transactions WHERE budget_id=?1 AND account_id=?2)))",params![b,id],|r|r.get(0))?;
    if newest.as_deref().is_some_and(|v| v > clock) {
        return Err(Error::conflict(
            "A newer edit prevents closing. Refresh the account.",
        ));
    }
    let input:Create=serde_json::from_value(json!({"id":body["id"],"counterpart_id":body["counterpart_id"],"account_id":id,"to_account_id":body["to_account_id"],"amount":-balance,"date":body["date"],"notes":"Closing account","category_id":body["category_id"]})).map_err(|_|Error::invalid("Provide transfer identities, destination and date"))?;
    if balance != 0 {
        create_pair(db, b, &input, clock)?;
    }
    let count: i64 = db.query_row(
        "SELECT COUNT(*) FROM transactions WHERE budget_id=?1 AND account_id=?2 AND tombstone=0",
        params![b, id],
        |r| r.get(0),
    )?;
    let field = if count == 0 { "tombstone" } else { "closed" };
    db.execute(
        &format!("UPDATE accounts SET {field}=1 WHERE budget_id=?1 AND id=?2"),
        params![b, id],
    )?;
    order::seed_field_version(db, b, "accounts", id, field, clock)?;
    order::mark(db, b, "accounts", id)?;
    Ok(
        json!({"response_status":200,"response_body":{"data":crate::accounts::get(db,b,id)?,"conflicts":[],"edit_clock":clock,"cursor":order::cursor(db,b)?}}),
    )
}
/// Pinned forced account deletion keeps the other account's amount, unlinked.
pub fn detach_for_account_deletion(
    db: &Connection,
    b: &str,
    id: &str,
    clock: &str,
) -> Result<(), Error> {
    if let Some(other) = counterpart(db, b, id)? {
        let row = ledger::get_any(db, b, &other)?;
        if newer(&row, clock) {
            return Err(Error::conflict(
                "A newer counterpart edit prevents account deletion",
            ));
        }
        db.execute(
            "UPDATE transactions SET transfer_id=NULL,payee_id=NULL WHERE budget_id=?1 AND id=?2",
            params![b, other],
        )?;
        stamp(db, b, &other, &["transfer_id", "payee_id"], clock)?;
        db.execute(
            "UPDATE transactions SET transfer_id=NULL,payee_id=NULL WHERE budget_id=?1 AND id=?2",
            params![b, id],
        )?;
        stamp(db, b, id, &["transfer_id"], clock)?;
    }
    Ok(())
}
