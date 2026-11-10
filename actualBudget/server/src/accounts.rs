//! Account organization and lifecycle. Every mutation shares its receipt, field clocks,
//! ledger effects and generation fence in one store transaction.
use crate::{
    api::{idempotency_key, json_error},
    concurrency,
    error::Error,
    models::{validate_id, validate_name},
    store::{self, Store},
};
use axum::{
    Json, Router,
    extract::{
        OriginalUri, Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, Method, StatusCode},
    routing::{get as route_get, post},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

pub fn router() -> Router<Store> {
    Router::new()
        .route(
            "/api/v1/budgets/{budget}/accounts/{id}",
            route_get(read).patch(mutate).delete(mutate),
        )
        .route("/api/v1/budgets/{budget}/accounts/{id}/close", post(mutate))
        .route(
            "/api/v1/budgets/{budget}/accounts/{id}/reopen",
            post(mutate),
        )
        .route("/api/v1/budgets/{budget}/accounts/{id}/move", post(mutate))
        .route(
            "/api/v1/budgets/{budget}/account-groups",
            route_get(groups).post(create_group),
        )
        .route(
            "/api/v1/budgets/{budget}/account-groups/{id}",
            route_get(read_group).patch(mutate).delete(mutate),
        )
        .route(
            "/api/v1/budgets/{budget}/account-groups/{id}/move",
            post(mutate),
        )
}
fn versions(db: &Connection, budget: &str, entity: &str, id: &str) -> Result<Value, Error> {
    let mut q = db.prepare(
        "SELECT field,clock FROM field_versions WHERE budget_id=?1 AND entity=?2 AND entity_id=?3",
    )?;
    let mut values = q
        .query_map(params![budget, entity, id], |r| {
            Ok((r.get::<_, String>(0)?, json!(r.get::<_, String>(1)?)))
        })?
        .collect::<Result<serde_json::Map<_, _>, _>>()?;
    if entity == "accounts" {
        crate::notes::decorate_versions(db, budget, id, &mut values)?;
    }
    Ok(Value::Object(values))
}
pub fn get(db: &Connection, budget: &str, id: &str) -> Result<Value, Error> {
    let mut value=db.query_row("SELECT a.name,a.notes,a.off_budget,a.closed,COALESCE(t.total,0),a.group_id,a.sort_order,a.tombstone,a.last_reconciled FROM accounts a LEFT JOIN ledger_totals t ON t.budget_id=a.budget_id AND t.account_id=a.id WHERE a.budget_id=?1 AND a.id=?2",params![budget,id],|r|Ok(json!({"id":id,"name":r.get::<_,String>(0)?,"notes":r.get::<_,String>(1)?,"off_budget":r.get::<_,bool>(2)?,"closed":r.get::<_,bool>(3)?,"balance":r.get::<_,i64>(4)?,"group_id":r.get::<_,Option<String>>(5)?,"sort_order":r.get::<_,i64>(6)?,"deleted":r.get::<_,bool>(7)?,"last_reconciled":r.get::<_,Option<String>>(8)?}))).optional()?.ok_or_else(Error::missing)?;
    value["field_versions"] = versions(db, budget, "accounts", id)?;
    Ok(value)
}
pub fn get_group(db: &Connection, budget: &str, id: &str) -> Result<Value, Error> {
    let mut value=db.query_row("SELECT name,sort_order,tombstone FROM account_groups WHERE budget_id=?1 AND id=?2",params![budget,id],|r|Ok(json!({"id":id,"name":r.get::<_,String>(0)?,"sort_order":r.get::<_,i64>(1)?,"deleted":r.get::<_,bool>(2)?}))).optional()?.ok_or_else(Error::missing)?;
    value["field_versions"] = versions(db, budget, "account_groups", id)?;
    Ok(value)
}
fn path(value: Result<Path<(String, String)>, PathRejection>) -> Result<(String, String), Error> {
    let Path((b, id)) = value.map_err(|_| Error::invalid("Invalid account path"))?;
    validate_id(&b)?;
    validate_id(&id)?;
    Ok((b, id))
}
async fn read(
    State(s): State<Store>,
    p: Result<Path<(String, String)>, PathRejection>,
) -> Result<Json<Value>, Error> {
    let (b, id) = path(p)?;
    Ok(Json(
        json!({"data":s.run(move|db|{store::require_budget(db,&b)?;get(db,&b,&id)}).await?}),
    ))
}
async fn read_group(
    State(s): State<Store>,
    p: Result<Path<(String, String)>, PathRejection>,
) -> Result<Json<Value>, Error> {
    let (b, id) = path(p)?;
    Ok(Json(
        json!({"data":s.run(move|db|{store::require_budget(db,&b)?;get_group(db,&b,&id)}).await?}),
    ))
}
async fn groups(
    State(s): State<Store>,
    p: Result<Path<String>, PathRejection>,
    q: Result<Query<crate::lifecycle::SnapshotQuery>, QueryRejection>,
) -> Result<Json<Value>, Error> {
    let b = crate::api::budget_id(p)?;
    let q = crate::lifecycle::snapshot_query(q)?;
    let values=s.run(move|db|{concurrency::check_snapshot(db,&b,q.snapshot.as_deref())?;let mut query=db.prepare("SELECT id FROM account_groups WHERE budget_id=?1 AND tombstone=0 ORDER BY sort_order,id")?;let ids=query.query_map([&b],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;ids.iter().map(|id|get_group(db,&b,id)).collect::<Result<Vec<_>,_>>()}).await?;
    Ok(Json(json!({"data":values})))
}
fn fields(body: &Value, allowed: &[&str]) -> Result<(), Error> {
    let fields = body
        .as_object()
        .ok_or_else(|| Error::invalid("Expected an object"))?;
    if fields.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(Error::invalid("Unknown account operation field"));
    }
    Ok(())
}
pub(crate) fn group_name(db: &Connection, b: &str, id: &str, name: &str) -> Result<(), Error> {
    validate_name(name)?;
    // Match pinned Actual's SQLite UPPER(name) and JavaScript uppercase lookup.
    let duplicate:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM account_groups WHERE budget_id=?1 AND id<>?2 AND tombstone=0 AND UPPER(name)=?3)",params![b,id,name.to_uppercase()],|r|r.get(0))?;
    if duplicate {
        return Err(Error::conflict(
            "An account group with this name already exists",
        ));
    }
    Ok(())
}
fn active(value: &Value) -> Result<(), Error> {
    if value["deleted"] == true {
        Err(Error::new(
            StatusCode::CONFLICT,
            "account_deleted",
            "This account or group was deleted. Refresh the current budget.",
        ))
    } else {
        Ok(())
    }
}
fn group_reference(db: &Connection, b: &str, value: &Value) -> Result<(), Error> {
    if value.is_null() {
        return Ok(());
    }
    let id = value
        .as_str()
        .ok_or_else(|| Error::invalid("group_id must be a UUID or null"))?;
    validate_id(id)?;
    active(&get_group(db, b, id)?)
}
async fn create_group(
    State(s): State<Store>,
    p: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let b = crate::api::budget_id(p)?;
    let Json(body) = input.map_err(json_error)?;
    fields(&body, &["id", "name"])?;
    let id = body["id"]
        .as_str()
        .ok_or_else(|| Error::invalid("id is required"))?
        .to_owned();
    validate_id(&id)?;
    let name = body["name"]
        .as_str()
        .ok_or_else(|| Error::invalid("name is required"))?
        .to_owned();
    let key = idempotency_key(&headers)?;
    let owner = crate::history::owner_header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let proposed = concurrency::header(&headers)?;
    let cancel = concurrency::cancel_header(&headers)?;
    s.run(move|db|concurrency::write_owned(db,&format!("POST /budgets/{b}/account-groups"),&key,&body.to_string(),proposed.as_deref(),StatusCode::CREATED,owner.as_ref(),|db|{if cancel { return concurrency::canceled(db, &b); } crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;group_name(db,&b,&id,&name)?;if db.query_row("SELECT EXISTS(SELECT 1 FROM account_groups WHERE budget_id=?1 AND id=?2)",params![b,id],|r|r.get::<_,bool>(0))?{return Err(Error::conflict("Group ID already exists"));}let clock=concurrency::clock(db,&b,proposed.as_deref())?;db.execute("INSERT INTO account_groups(budget_id,id,name,sort_order) VALUES(?1,?2,?3,(SELECT COALESCE(MAX(sort_order),-1)+1 FROM account_groups WHERE budget_id=?1))",params![b,id,name])?;for field in ["name","sort_order","tombstone"]{concurrency::seed_field_version(db,&b,"account_groups",&id,field,&clock)?;}get_group(db,&b,&id)})).await
}
async fn mutate(
    State(s): State<Store>,
    p: Result<Path<(String, String)>, PathRejection>,
    OriginalUri(uri): OriginalUri,
    method: Method,
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
    let entity = if uri.path().contains("/account-groups/") {
        "account_groups"
    } else {
        "accounts"
    };
    let action = if method == Method::DELETE {
        "delete"
    } else if method == Method::PATCH {
        "patch"
    } else {
        uri.path().rsplit('/').next().unwrap_or("")
    }
    .to_owned();
    let scope = format!("{} {}", method, uri.path().trim_start_matches("/api/v1"));
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
                execute(db, &b, entity, &id, &action, &body, proposed.as_deref())
            },
        )
    })
    .await
}
fn execute(
    db: &Connection,
    b: &str,
    entity: &str,
    id: &str,
    action: &str,
    body: &Value,
    proposed: Option<&str>,
) -> Result<Value, Error> {
    let group = entity == "account_groups";
    let previous = if group {
        get_group(db, b, id)?
    } else {
        get(db, b, id)?
    };
    active(&previous)?;
    // Actual account-close (including forced delete) ignores already-closed accounts.
    if !group && previous["closed"] == true && matches!(action, "close" | "delete") {
        fields(
            body,
            if action == "delete" {
                &["confirm_name"]
            } else {
                &[]
            },
        )?;
        if action == "delete" && body["confirm_name"] != previous["name"] {
            return Err(Error::invalid(
                "Type the current account name to confirm deletion.",
            ));
        }
        return Ok(
            json!({"response_status":200,"response_body":{"data":previous,"conflicts":[],"cursor":concurrency::cursor(db,b)?}}),
        );
    }
    let clock = concurrency::clock(db, b, proposed)?;
    if !group && action == "close" && body.get("to_account_id").is_some() {
        return crate::transfers::close_account(db, b, id, body, &clock);
    }
    let old = versions(db, b, entity, id)?;
    let mut conflicts = vec![];
    let mut changes = serde_json::Map::new();
    match action {
        "patch" => {
            fields(
                body,
                if group {
                    &["name"]
                } else {
                    &["name", "notes", "group_id"]
                },
            )?;
            if body.as_object().unwrap().is_empty() {
                return Err(Error::invalid("Supply a field to update"));
            }
            for (field, value) in body.as_object().unwrap() {
                match field.as_str() {
                    "name" => {
                        let name = value
                            .as_str()
                            .ok_or_else(|| Error::invalid("name must be text"))?;
                        if group {
                            group_name(db, b, id, name)?;
                        } else {
                            validate_name(name)?;
                        }
                    }
                    "notes" => {
                        if !value.as_str().is_some_and(|s| s.chars().count() <= 10000) {
                            return Err(Error::invalid(
                                "notes must be text with at most 10000 characters",
                            ));
                        }
                    }
                    "group_id" => group_reference(db, b, value)?,
                    _ => unreachable!(),
                }
                changes.insert(field.clone(), value.clone());
            }
        }
        "close" | "reopen" => {
            fields(body, &[])?;
            if action == "close" {
                if previous["balance"] != 0 {
                    return Err(Error::new(
                        StatusCode::CONFLICT,
                        "account_balance_nonzero",
                        "The balance must be zero before closing without a transfer.",
                    ));
                }
                let count:i64=db.query_row("SELECT COUNT(*) FROM transactions WHERE budget_id=?1 AND account_id=?2 AND tombstone=0",params![b,id],|r|r.get(0))?;
                if count == 0 {
                    changes.insert("tombstone".into(), json!(true));
                } else {
                    changes.insert("closed".into(), json!(true));
                }
            } else {
                changes.insert("closed".into(), json!(false));
            }
        }
        "delete" => {
            fields(body, if group { &[] } else { &["confirm_name"] })?;
            if !group && body["confirm_name"] != previous["name"] {
                return Err(Error::invalid(
                    "Type the current account name to confirm deletion of its transactions.",
                ));
            }
            changes.insert("tombstone".into(), json!(true));
        }
        "move" => {
            fields(
                body,
                if group {
                    &["before_id"]
                } else {
                    &["before_id", "group_id"]
                },
            )?;
            if !body.as_object().unwrap().contains_key("before_id") {
                return Err(Error::invalid(
                    "before_id is required; null moves to the end",
                ));
            }
            if !group && let Some(value) = body.get("group_id") {
                group_reference(db, b, value)?;
                changes.insert("group_id".into(), value.clone());
            }
            let mut q = db.prepare(&format!(
                "SELECT id FROM {entity} WHERE budget_id=?1 AND tombstone=0 ORDER BY sort_order,id"
            ))?;
            let mut ids = q
                .query_map([b], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            ids.retain(|v| v != id);
            let index = if body["before_id"].is_null() {
                ids.len()
            } else {
                let target = body["before_id"]
                    .as_str()
                    .ok_or_else(|| Error::invalid("before_id must be a UUID or null"))?;
                validate_id(target)?;
                ids.iter()
                    .position(|v| v == target)
                    .ok_or_else(|| Error::invalid("Move target is unavailable"))?
            };
            // Reordering is atomic. A stale order cannot partially reorder other accounts.
            for row in &ids {
                let v = versions(db, b, entity, row)?;
                if v["sort_order"].as_str().unwrap_or(concurrency::ZERO) >= clock.as_str() {
                    return Err(Error::conflict(
                        "A newer account order exists. Refresh before moving.",
                    ));
                }
            }
            if old["sort_order"].as_str().unwrap_or(concurrency::ZERO) >= clock.as_str() {
                return Err(Error::conflict(
                    "A newer account order exists. Refresh before moving.",
                ));
            }
            ids.insert(index, id.to_owned());
            for (i, row) in ids.iter().enumerate() {
                db.execute(
                    &format!("UPDATE {entity} SET sort_order=?3 WHERE budget_id=?1 AND id=?2"),
                    params![b, row, i as i64],
                )?;
                concurrency::seed_field_version(db, b, entity, row, "sort_order", &clock)?;
            }
        }
        _ => return Err(Error::invalid("Unsupported account action")),
    }
    if changes.contains_key("tombstone")
        && old
            .as_object()
            .unwrap()
            .values()
            .any(|v| v.as_str().unwrap_or(concurrency::ZERO) > clock.as_str())
    {
        return Err(Error::conflict(
            "A newer account edit prevents deletion. Refresh first.",
        ));
    }
    if changes.contains_key("tombstone") {
        let newer: bool = if group {
            db.query_row("SELECT EXISTS(SELECT 1 FROM accounts a JOIN field_versions f ON f.budget_id=a.budget_id AND f.entity='accounts' AND f.entity_id=a.id AND f.field='group_id' WHERE a.budget_id=?1 AND a.group_id=?2 AND f.clock>?3)",params![b,id,clock],|r|r.get(0))?
        } else {
            db.query_row("SELECT EXISTS(SELECT 1 FROM transactions t JOIN field_versions f ON f.budget_id=t.budget_id AND f.entity='transactions' AND f.entity_id=t.id WHERE t.budget_id=?1 AND t.account_id=?2 AND t.tombstone=0 AND f.clock>?3)",params![b,id,clock],|r|r.get(0))?
        };
        if newer {
            return Err(Error::conflict(
                "A newer account or transaction edit prevents deletion. Refresh first.",
            ));
        }
    }
    for (field, value) in changes {
        let winning = old[&field].as_str().unwrap_or(concurrency::ZERO);
        if clock.as_str() <= winning {
            conflicts.push(json!({"field":field,"winning_clock":winning,"submitted_clock":clock}));
            continue;
        }
        let sql_value = match &value {
            Value::Null => rusqlite::types::Value::Null,
            Value::String(v) => rusqlite::types::Value::Text(v.clone()),
            Value::Bool(v) => rusqlite::types::Value::Integer(i64::from(*v)),
            _ => return Err(Error::invalid("Invalid account value")),
        };
        db.execute(
            &format!("UPDATE {entity} SET {field}=?3 WHERE budget_id=?1 AND id=?2"),
            params![b, id, sql_value],
        )?;
        concurrency::seed_field_version(db, b, entity, id, &field, &clock)?;
        concurrency::mark(db, b, entity, id)?;
        if field == "tombstone" {
            if group {
                let mut q =
                    db.prepare("SELECT id FROM accounts WHERE budget_id=?1 AND group_id=?2")?;
                let members = q
                    .query_map(params![b, id], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                for member in members {
                    db.execute(
                        "UPDATE accounts SET group_id=NULL WHERE budget_id=?1 AND id=?2",
                        params![b, member],
                    )?;
                    concurrency::seed_field_version(
                        db, b, "accounts", &member, "group_id", &clock,
                    )?;
                }
            } else {
                let mut q=db.prepare("SELECT id FROM transactions WHERE budget_id=?1 AND account_id=?2 AND tombstone=0")?;
                let ids = q
                    .query_map(params![b, id], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                for tx in ids {
                    crate::reconciliation::protect(&crate::ledger::get(db, b, &tx)?, None)?;
                    crate::transfers::detach_for_account_deletion(db, b, &tx, &clock)?;
                    db.execute(
                        "UPDATE transactions SET tombstone=1 WHERE budget_id=?1 AND id=?2",
                        params![b, tx],
                    )?;
                    concurrency::seed_field_version(
                        db,
                        b,
                        "transactions",
                        &tx,
                        "tombstone",
                        &clock,
                    )?;
                }
                crate::ledger::refresh_account(db, b, id)?;
                crate::ledger::validate_budget_total(db, b)?;
            }
        }
    }
    let data = if group {
        get_group(db, b, id)?
    } else {
        get(db, b, id)?
    };
    Ok(
        json!({"response_status":200,"response_body":{"data":data,"conflicts":conflicts,"edit_clock":clock,"cursor":concurrency::cursor(db,b)?}}),
    )
}
