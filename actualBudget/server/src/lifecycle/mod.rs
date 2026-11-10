pub mod automatic;
use crate::{
    api::{budget_id, idempotency_key, json_error},
    error::Error,
    models::validate_name,
    store::{self, Store},
};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
    routing::get,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn epoch_header(headers: &HeaderMap) -> Result<Option<String>, Error> {
    headers
        .get("X-Actual-Budget-Epoch")
        .map(|v| {
            v.to_str()
                .map_err(|_| Error::invalid("Invalid budget epoch"))
                .and_then(|v| {
                    if v.len() != 32
                        || !v
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    {
                        return Err(Error::invalid("Invalid budget epoch"));
                    }
                    Ok(v.to_owned())
                })
        })
        .transpose()
}
pub fn metadata(db: &Connection, budget: &str, include_deleted: bool) -> Result<Value, Error> {
    let value=db.query_row("SELECT b.id,b.name,b.budget_type,b.deleted,s.epoch,l.generation,l.auto_due,l.last_auto,l.backup_error FROM budgets b JOIN budget_sync s ON s.budget_id=b.id JOIN budget_lifecycle l ON l.budget_id=b.id WHERE b.id=?1",[budget],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"budget_type":r.get::<_,String>(2)?,"deleted":r.get::<_,bool>(3)?,"epoch":r.get::<_,String>(4)?,"generation":r.get::<_,i64>(5)?,"auto_due":r.get::<_,Option<i64>>(6)?,"last_auto":r.get::<_,Option<i64>>(7)?,"backup_error":r.get::<_,Option<String>>(8)?}))).optional()?.ok_or_else(Error::missing)?;
    if !include_deleted && value["deleted"] == true {
        return Err(Error::new(
            StatusCode::GONE,
            "budget_deleted",
            "This budget was deleted. Saved work has not been discarded.",
        ));
    }
    Ok(value)
}
pub fn epoch(db: &Connection, budget: &str) -> Result<String, Error> {
    Ok(metadata(db, budget, false)?["epoch"]
        .as_str()
        .unwrap()
        .to_owned())
}
pub fn require_epoch(db: &Connection, budget: &str, expected: Option<&str>) -> Result<(), Error> {
    let current = metadata(db, budget, false)?;
    if expected.is_some_and(|epoch| current["epoch"] != epoch)
        || (expected.is_none() && current["generation"].as_i64().unwrap() > 0)
    {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "budget_restored",
            "This budget was restored. Keep the original pending work and review the current budget before making a new change.",
        ));
    }
    Ok(())
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SnapshotQuery {
    pub snapshot: Option<String>,
}
pub fn snapshot_query(
    query: Result<Query<SnapshotQuery>, QueryRejection>,
) -> Result<SnapshotQuery, Error> {
    query
        .map(|Query(q)| q)
        .map_err(|_| Error::invalid("Invalid snapshot query"))
}
pub fn router() -> Router<Store> {
    Router::new()
        .route(
            "/api/v1/budgets/{budget}",
            get(read).patch(rename).delete(delete_budget),
        )
        .route(
            "/api/v1/budgets/{budget}/backups",
            get(backups).post(backup),
        )
        .route(
            "/api/v1/budgets/{budget}/backups/{id}/download",
            get(download),
        )
        .route(
            "/api/v1/budgets/{budget}/backups/{id}/restore",
            axum::routing::post(restore_point),
        )
        .route("/api/v1/budgets/{budget}/checkpoint", get(checkpoint))
        .route(
            "/api/v1/budgets/{budget}/duplicate",
            axum::routing::post(duplicate_budget),
        )
        .route(
            "/api/v1/budgets/{budget}/restores/{id}/activate",
            axum::routing::post(activate_stage),
        )
        .route("/api/v1/restores/{id}", get(read_stage))
        .route("/api/v1/restores/{id}/original.zip", get(original_stage))
        .layer(axum::extract::DefaultBodyLimit::max(65536))
        .route(
            "/api/v1/restores",
            axum::routing::post(upload)
                .layer(axum::extract::DefaultBodyLimit::max(archive::MAX_ARCHIVE)),
        )
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct BudgetQuery {
    #[serde(default)]
    pub include_deleted: bool,
}
pub fn list(db: &Connection, include_deleted: bool) -> Result<Value, Error> {
    let mut q = db.prepare("SELECT id FROM budgets WHERE deleted=0 OR ?1 ORDER BY name,id")?;
    let ids = q
        .query_map([include_deleted], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!(
        ids.into_iter()
            .map(|id| metadata(db, &id, include_deleted))
            .collect::<Result<Vec<_>, _>>()?
    ))
}
async fn read(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<BudgetQuery>, QueryRejection>,
) -> Result<Json<Value>, Error> {
    let budget = budget_id(path)?;
    let query = query.map_err(|_| Error::invalid("Invalid budget query"))?.0;
    Ok(Json(
        json!({"data":store.run(move|db|metadata(db,&budget,query.include_deleted)).await?}),
    ))
}
async fn checkpoint(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<Value>, Error> {
    let budget = budget_id(path)?;
    Ok(Json(
        json!({"data":store.run(move|db|{let value=metadata(db,&budget,false)?;Ok(json!({"snapshot":crate::concurrency::cursor(db,&budget)?,"epoch":value["epoch"],"generation":value["generation"]}))}).await?}),
    ))
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Rename {
    name: String,
}
async fn rename(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<Rename>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    let budget = budget_id(path)?;
    let Json(input) = input.map_err(json_error)?;
    validate_name(&input.name)?;
    let key = idempotency_key(&headers)?;
    let cancel = crate::concurrency::cancel_header(&headers)?;
    let epoch = epoch_header(&headers)?;
    let payload = serde_json::to_string(&input).map_err(Error::internal)?;
    let result = store
        .run(move |db| {
            cancellation::write(
                db,
                &format!("PATCH /budgets/{budget}"),
                &key,
                &payload,
                cancel,
                |db| {
                    require_epoch(db, &budget, epoch.as_deref())?;
                    db.execute(
                        "UPDATE budgets SET name=?2 WHERE id=?1",
                        params![budget, input.name],
                    )?;
                    metadata(db, &budget, false)
                },
            )
        })
        .await?;
    Ok(Json(json!({"data":result})))
}
pub(crate) mod archive;
pub(crate) mod cancellation;
fn receipt_budget(
    db: &Connection,
    scope: &str,
    payload: &str,
    response: &str,
) -> Result<Option<String>, Error> {
    let Some((method, path)) = scope.split_once(' ') else {
        return Ok(None);
    };
    if !matches!(method, "POST" | "PATCH" | "DELETE" | "PUT") {
        return Ok(None);
    }
    let budget = if let Some(rest) = path.strip_prefix("/budgets/") {
        rest.split('/').next().map(str::to_owned)
    } else if path == "/budgets" && method == "POST" {
        let input: Value = serde_json::from_str(payload).map_err(Error::internal)?;
        let output: Value = serde_json::from_str(response).map_err(Error::internal)?;
        if input["id"] != output["id"] {
            return Err(Error::conflict(
                "Budget creation receipt identity is inconsistent",
            ));
        }
        input["id"].as_str().map(str::to_owned)
    } else if matches!(
        path,
        "/imports/actual" | "/imports/ynab4" | "/imports/nynab"
    ) && method == "POST"
    {
        let result: Value = serde_json::from_str(response).map_err(Error::internal)?;
        if let Some(id) = result["id"].as_str() {
            let found: Option<String> = db
                .query_row(
                    "SELECT budget_id FROM staged_imports WHERE id=?1 AND status='activated'",
                    [id],
                    |r| r.get(0),
                )
                .optional()?;
            if found.as_deref() != result["budget_id"].as_str() && found.is_some() {
                return Err(Error::conflict("Import receipt identity is inconsistent"));
            }
            found
        } else {
            None
        }
    } else if path.starts_with("/imports/") && path.ends_with("/activate") && method == "POST" {
        let id = path
            .trim_start_matches("/imports/")
            .trim_end_matches("/activate");
        let result: Value = serde_json::from_str(response).map_err(Error::internal)?;
        let found: Option<String> = db
            .query_row(
                "SELECT budget_id FROM staged_imports WHERE id=?1 AND status='activated'",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        if found.is_some() && found.as_deref() != result["id"].as_str() {
            return Err(Error::conflict(
                "Import activation receipt identity is inconsistent",
            ));
        }
        found
    } else {
        None
    };
    if let Some(budget) = budget {
        crate::models::validate_id(&budget)?;
        if db.query_row(
            "SELECT EXISTS(SELECT 1 FROM budgets WHERE id=?1)",
            [&budget],
            |r| r.get::<_, bool>(0),
        )? {
            return Ok(Some(budget));
        }
    }
    Ok(None)
}
pub fn link_operation(
    db: &Connection,
    scope: &str,
    key: &str,
    payload: &str,
    response: &str,
) -> Result<(), Error> {
    if let Some(budget) = receipt_budget(db, scope, payload, response)? {
        db.execute("INSERT OR IGNORE INTO operation_budget_links(budget_id,scope,operation_id) VALUES(?1,?2,?3)",params![budget,scope,key])?;
        if scope.starts_with("POST /imports/") && scope.ends_with("/activate") {
            let mut query=db.prepare("SELECT scope,operation_id,payload,response FROM operations WHERE scope IN('POST /imports/actual','POST /imports/ynab4','POST /imports/nynab')")?;
            let rows = query
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for (scope, key, payload, response) in rows {
                if receipt_budget(db, &scope, &payload, &response)?.as_deref() == Some(&budget) {
                    db.execute("INSERT OR IGNORE INTO operation_budget_links(budget_id,scope,operation_id) VALUES(?1,?2,?3)",params![budget,scope,key])?;
                }
            }
        }
    }
    Ok(())
}
pub fn backfill_operation_links(db: &Connection) -> Result<(), Error> {
    let mut query =
        db.prepare("SELECT scope,operation_id,payload,response FROM operations ORDER BY rowid")?;
    let rows = query
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (scope, key, payload, response) in rows {
        link_operation(db, &scope, &key, &payload, &response)?;
    }
    Ok(())
}
fn receipts(db: &Connection, budget: &str) -> Result<Vec<archive::Receipt>, Error> {
    let mut query=db.prepare("SELECT o.scope,o.operation_id,o.payload,o.response FROM operations o JOIN operation_budget_links l ON l.scope=o.scope AND l.operation_id=o.operation_id WHERE l.budget_id=?1 ORDER BY o.scope,o.operation_id")?;
    Ok(query
        .query_map([budget], |r| {
            Ok(archive::Receipt {
                scope: r.get(0)?,
                operation_id: r.get(1)?,
                payload: r.get(2)?,
                response: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?)
}
fn verify_receipt_budget(
    db: &Connection,
    budget: &str,
    receipt: &archive::Receipt,
) -> Result<(), Error> {
    if receipt.operation_id.is_empty()
        || receipt.operation_id.len() > 128
        || !receipt.operation_id.bytes().all(|b| b.is_ascii_graphic())
        || receipt_budget(db, &receipt.scope, &receipt.payload, &receipt.response)?.as_deref()
            != Some(budget)
    {
        return Err(Error::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_budget_archive",
            "An operation receipt does not belong to the archived budget",
        ));
    }
    Ok(())
}
fn confirm_name(db: &Connection, budget: &str, input: &Value) -> Result<Value, Error> {
    let current = metadata(db, budget, true)?;
    if input.as_object().is_none_or(|o| o.len() != 1) || input["confirm_name"] != current["name"] {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "budget_confirmation_mismatch",
            "Enter the affected budget's current name to confirm this action",
        ));
    }
    Ok(current)
}
fn point(db: &Connection, id: &str) -> Result<Value, Error> {
    db.query_row("SELECT id,budget_id,label,kind,created_at,archive_sha256,source_sequence,length(archive) FROM budget_backups WHERE id=?1",[id],|r|Ok(json!({"id":r.get::<_,String>(0)?,"budget_id":r.get::<_,String>(1)?,"label":r.get::<_,String>(2)?,"kind":r.get::<_,String>(3)?,"created_at":r.get::<_,String>(4)?,"archive_sha256":r.get::<_,String>(5)?,"source_sequence":r.get::<_,i64>(6)?,"bytes":r.get::<_,i64>(7)?}))).optional()?.ok_or_else(Error::missing)
}
pub(crate) fn make_point(
    db: &Connection,
    budget: &str,
    kind: &str,
    label: &str,
) -> Result<Value, Error> {
    let bytes = archive::capture(db, budget)?;
    let id = uuid::Uuid::new_v4().to_string();
    let sequence: i64 = db.query_row(
        "SELECT sequence FROM budget_sync WHERE budget_id=?1",
        [budget],
        |r| r.get(0),
    )?;
    db.execute("INSERT INTO budget_backups(id,budget_id,label,kind,created_at,archive_sha256,archive,source_sequence) VALUES(?1,?2,?3,?4,strftime('%Y-%m-%dT%H:%M:%fZ','now'),?5,?6,?7)",params![id,budget,label,kind,archive::hash(&bytes),bytes,sequence])?;
    point(db, &id)
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct NewPoint {
    #[serde(default)]
    label: String,
}
async fn backup(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<NewPoint>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let budget = budget_id(path)?;
    let Json(input) = input.map_err(json_error)?;
    if input.label.chars().count() > 120 {
        return Err(Error::invalid(
            "Backup label must contain at most 120 characters",
        ));
    }
    let key = idempotency_key(&headers)?;
    let cancel = crate::concurrency::cancel_header(&headers)?;
    let epoch = epoch_header(&headers)?;
    let payload = serde_json::to_string(&input).map_err(Error::internal)?;
    let result = store
        .run(move |db| {
            cancellation::write(
                db,
                &format!("POST /budgets/{budget}/backups"),
                &key,
                &payload,
                cancel,
                |db| {
                    require_epoch(db, &budget, epoch.as_deref())?;
                    make_point(db, &budget, "manual", &input.label)
                },
            )
        })
        .await?;
    Ok((StatusCode::CREATED, Json(json!({"data":result}))))
}
async fn backups(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<Value>, Error> {
    let budget = budget_id(path)?;
    let result = store
        .run(move |db| {
            metadata(db, &budget, true)?;
            let mut query = db.prepare(
                "SELECT id FROM budget_backups WHERE budget_id=?1 ORDER BY created_at DESC,id",
            )?;
            let ids = query
                .query_map([&budget], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            ids.into_iter()
                .map(|id| point(db, &id))
                .collect::<Result<Vec<_>, _>>()
                .map(|v| json!(v))
        })
        .await?;
    Ok(Json(json!({"data":result})))
}
fn point_path(
    path: Result<Path<(String, String)>, PathRejection>,
) -> Result<(String, String), Error> {
    let Path((budget, id)) = path.map_err(|_| Error::invalid("Invalid backup path"))?;
    crate::models::validate_id(&budget)?;
    crate::models::validate_id(&id)?;
    Ok((budget, id))
}
async fn download(
    State(store): State<Store>,
    path: Result<Path<(String, String)>, PathRejection>,
) -> Result<impl axum::response::IntoResponse, Error> {
    let (budget, id) = point_path(path)?;
    let bytes = store
        .run(move |db| {
            db.query_row(
                "SELECT archive FROM budget_backups WHERE budget_id=?1 AND id=?2",
                params![budget, id],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .ok_or_else(Error::missing)
        })
        .await?;
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "application/zip"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=actualBudget.zip",
            ),
            (axum::http::header::CACHE_CONTROL, "no-store"),
        ],
        bytes,
    ))
}
fn remove_active_data(db: &Connection, budget: &str) -> Result<(), Error> {
    db.execute("DELETE FROM bank_connections WHERE budget_id=?1", [budget])?;
    for table in [
        "budget_provenance",
        "imported_field_clocks",
        "imported_sources",
        "field_versions",
        "budget_preferences",
        "register_settings",
        "reporting_documents",
        "rules",
        "rule_settings",
        "schedules",
        "schedule_occurrences",
        "schedule_links",
        "schedule_settings",
        "budget_automations",
        "budget_cleanup_groups",
        "budget_cells",
        "budget_months",
        "budget_settings",
        "ledger_totals",
        "android_upgrade_order",
        "transaction_import_metadata",
        "transaction_import_settings",
        "transaction_import_files",
        "transactions",
        "accounts",
        "account_groups",
        "payee_locations",
        "payees",
        "tags",
        "categories",
        "category_groups",
        "budget_changes",
    ] {
        db.execute(&format!("DELETE FROM {table} WHERE budget_id=?1"), [budget])?;
    }
    Ok(())
}
fn install_restore(
    db: &Connection,
    budget: &str,
    validated: &archive::Validated,
) -> Result<Value, Error> {
    if validated.state.budget["id"] != budget {
        return Err(Error::invalid("Backup belongs to a different budget"));
    }
    let current = metadata(db, budget, true)?;
    let generation = current["generation"]
        .as_i64()
        .unwrap()
        .checked_add(1)
        .ok_or_else(|| Error::conflict("Budget generation exhausted"))?;
    let recovery = if current["deleted"] == false {
        Some(make_point(
            db,
            budget,
            "before_restore",
            "Before restoring a backup",
        )?)
    } else {
        None
    };
    // Receipt history is monotonic: restoring financial state never rewinds exactly-once evidence.
    for receipt in &validated.state.receipts {
        let prior: Option<(String, String)> = db
            .query_row(
                "SELECT payload,response FROM operations WHERE scope=?1 AND operation_id=?2",
                params![receipt.scope, receipt.operation_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if prior.as_ref().is_some_and(|(payload, response)| {
            payload != &receipt.payload || response != &receipt.response
        }) {
            return Err(Error::new(
                StatusCode::CONFLICT,
                "restore_receipt_conflict",
                "The archive contains an operation identity with a different committed outcome",
            ));
        }
    }
    remove_active_data(db, budget)?;
    db.execute(
        "UPDATE budgets SET name=?2,budget_type=?3,deleted=0 WHERE id=?1",
        params![
            budget,
            validated.state.budget["name"].as_str(),
            validated.state.budget["budget_type"].as_str()
        ],
    )?;
    archive::install_tables(db, validated)?;
    archive::validate_financial_state(db, budget)?;
    for receipt in &validated.state.receipts {
        db.execute("INSERT OR IGNORE INTO operations(scope,operation_id,payload,response) VALUES(?1,?2,?3,?4)",params![receipt.scope,receipt.operation_id,receipt.payload,receipt.response])?;
        link_operation(
            db,
            &receipt.scope,
            &receipt.operation_id,
            &receipt.payload,
            &receipt.response,
        )?;
    }
    let epoch = uuid::Uuid::new_v4().simple().to_string();
    db.execute(
        "UPDATE budget_sync SET epoch=?2,clock=MAX(clock,?3) WHERE budget_id=?1",
        params![budget, epoch, validated.state.clock],
    )?;
    db.execute("UPDATE budget_lifecycle SET generation=?2,restored_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE budget_id=?1",params![budget,generation])?;
    Ok(
        json!({"budget":metadata(db,budget,false)?,"recovery_point_id":recovery.map(|v|v["id"].clone())}),
    )
}
async fn restore_point(
    State(store): State<Store>,
    path: Result<Path<(String, String)>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    let (budget, id) = point_path(path)?;
    let Json(input) = input.map_err(json_error)?;
    let key = idempotency_key(&headers)?;
    let cancel = crate::concurrency::cancel_header(&headers)?;
    let expected = epoch_header(&headers)?;
    let payload = input.to_string();
    let result = store
        .run(move |db| {
            cancellation::write(
                db,
                &format!("POST /budgets/{budget}/backups/{id}/restore"),
                &key,
                &payload,
                cancel,
                |db| {
                    let bytes: Vec<u8> = db
                        .query_row(
                            "SELECT archive FROM budget_backups WHERE id=?1 AND budget_id=?2",
                            params![id, budget],
                            |r| r.get(0),
                        )
                        .optional()?
                        .ok_or_else(Error::missing)?;
                    let validated = archive::validate(&bytes)?;
                    let current = confirm_name(db, &budget, &input)?;
                    if current["deleted"] == false {
                        require_epoch(db, &budget, expected.as_deref())?;
                    } else if expected.is_some_and(|e| current["epoch"] != e) {
                        return Err(Error::conflict("Deleted budget identity changed"));
                    }
                    install_restore(db, &budget, &validated)
                },
            )
        })
        .await?;
    Ok(Json(json!({"data":result})))
}
async fn delete_budget(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    let budget = budget_id(path)?;
    let Json(input) = input.map_err(json_error)?;
    let key = idempotency_key(&headers)?;
    let cancel = crate::concurrency::cancel_header(&headers)?;
    let epoch = epoch_header(&headers)?;
    let payload = input.to_string();
    let result=store.run(move|db|cancellation::write(db,&format!("DELETE /budgets/{budget}"),&key,&payload,cancel,|db|{require_epoch(db,&budget,epoch.as_deref())?;let info=confirm_name(db,&budget,&input)?;let recovery=make_point(db,&budget,"before_delete","Before deleting this budget")?;remove_active_data(db,&budget)?;db.execute("UPDATE budgets SET deleted=1 WHERE id=?1",[&budget])?;db.execute("UPDATE budget_lifecycle SET auto_due=NULL WHERE budget_id=?1",[&budget])?;Ok(json!({"id":budget,"name":info["name"],"deleted":true,"recovery_point_id":recovery["id"]}))})).await?;
    Ok(Json(json!({"data":result})))
}
fn stage_report(db: &Connection, id: &str) -> Result<Value, Error> {
    db.query_row("SELECT id,budget_id,name,archive_sha256,created_at,report FROM native_restore_stages WHERE id=?1",[id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?))).optional()?.ok_or_else(Error::missing).and_then(|(id,budget,name,sha,created,report)|Ok(json!({"id":id,"budget_id":budget,"name":name,"archive_sha256":sha,"created_at":created,"status":"ready","report":serde_json::from_str::<Value>(&report).map_err(Error::internal)?})))
}
async fn upload(
    State(store): State<Store>,
    headers: HeaderMap,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let key = idempotency_key(&headers)?;
    let bytes = body.map_err(|_| {
        Error::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "archive_too_large",
            "Native budget archives must be at most 64 MiB",
        )
    })?;
    if !headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == "application/zip" || v == "application/octet-stream")
    {
        return Err(Error::invalid("Upload an actualBudget ZIP archive"));
    }
    let sha = archive::hash(&bytes);
    let result=store.run(move|db| store::write(db,"POST /restores",&key,&sha,|db| {
        if let Some(id)=db.query_row("SELECT id FROM native_restore_stages WHERE archive_sha256=?1",[&sha],|r|r.get::<_,String>(0)).optional()? { return stage_report(db,&id); }
        let validated=archive::validate(&bytes)?;
        let budget=validated.state.budget["id"].as_str().unwrap().to_owned();
        let name=validated.state.budget["name"].as_str().unwrap().to_owned();
        let report=json!({"format_version":1,"schema_version":22,"domains":validated.state.tables.iter().map(|(name,table)|(name.clone(),json!(table.rows.len()))).collect::<serde_json::Map<_,_>>(),"receipt_count":validated.state.receipts.len(),"original_preserved":true});
        let id=uuid::Uuid::new_v4().to_string();
        db.execute("INSERT INTO native_restore_stages(id,archive_sha256,original,budget_id,name,created_at,report) VALUES(?1,?2,?3,?4,?5,strftime('%Y-%m-%dT%H:%M:%fZ','now'),?6)",params![id,sha,bytes.as_ref(),budget,name,report.to_string()])?;
        stage_report(db,&id)
    })).await?;
    Ok((StatusCode::CREATED, Json(json!({"data":result}))))
}
async fn read_stage(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<Value>, Error> {
    let id = budget_id(path)?;
    Ok(Json(
        json!({"data":store.run(move|db|stage_report(db,&id)).await?}),
    ))
}
async fn original_stage(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
) -> Result<impl axum::response::IntoResponse, Error> {
    let id = budget_id(path)?;
    let bytes = store
        .run(move |db| {
            db.query_row(
                "SELECT original FROM native_restore_stages WHERE id=?1",
                [id],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .ok_or_else(Error::missing)
        })
        .await?;
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "application/zip"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=original-actualBudget.zip",
            ),
            (axum::http::header::CACHE_CONTROL, "no-store"),
        ],
        bytes,
    ))
}
async fn activate_stage(
    State(store): State<Store>,
    path: Result<Path<(String, String)>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    let (budget, id) = point_path(path)?;
    let Json(input) = input.map_err(json_error)?;
    let key = idempotency_key(&headers)?;
    let cancel = crate::concurrency::cancel_header(&headers)?;
    let expected = epoch_header(&headers)?;
    let payload = input.to_string();
    let result = store
        .run(move |db| {
            cancellation::write(
                db,
                &format!("POST /budgets/{budget}/restores/{id}/activate"),
                &key,
                &payload,
                cancel,
                |db| {
                    let bytes: Vec<u8> = db
                .query_row(
                    "SELECT original FROM native_restore_stages WHERE id=?1 AND budget_id=?2",
                    params![id, budget],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(Error::missing)?;
                    let validated = archive::validate(&bytes)?;
                    let exists = db.query_row(
                        "SELECT EXISTS(SELECT 1 FROM budgets WHERE id=?1)",
                        [&budget],
                        |r| r.get::<_, bool>(0),
                    )?;
                    if !exists {
                        if input.as_object().is_none_or(|v| v.len() != 1)
                            || input["confirm_name"] != validated.state.budget["name"]
                        {
                            return Err(Error::new(
                                StatusCode::CONFLICT,
                                "budget_confirmation_mismatch",
                                "Confirm the archived budget name before restoring",
                            ));
                        }
                        db.execute(
                            "INSERT INTO budgets(id,name,budget_type,deleted) VALUES(?1,?2,?3,1)",
                            params![
                                budget,
                                validated.state.budget["name"].as_str(),
                                validated.state.budget["budget_type"].as_str()
                            ],
                        )?;
                    } else {
                        let current = confirm_name(db, &budget, &input)?;
                        if current["deleted"] == false {
                            require_epoch(db, &budget, expected.as_deref())?;
                        } else if expected.is_some_and(|epoch| current["epoch"] != epoch) {
                            return Err(Error::conflict("Deleted budget identity changed"));
                        }
                    }
                    install_restore(db, &budget, &validated)
                },
            )
        })
        .await?;
    Ok(Json(json!({"data":result})))
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct DuplicateBudget {
    id: String,
    name: String,
}
async fn duplicate_budget(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<DuplicateBudget>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let budget = budget_id(path)?;
    let Json(input) = input.map_err(json_error)?;
    crate::models::validate_id(&input.id)?;
    validate_name(&input.name)?;
    let key = idempotency_key(&headers)?;
    let cancel = crate::concurrency::cancel_header(&headers)?;
    let expected = epoch_header(&headers)?;
    let payload = serde_json::to_string(&input).map_err(Error::internal)?;
    let result=store.run(move|db|cancellation::write(db,&format!("POST /budgets/{budget}/duplicate"),&key,&payload,cancel,|db|{
  require_epoch(db,&budget,expected.as_deref())?;
  if db.query_row("SELECT EXISTS(SELECT 1 FROM budgets WHERE id=?1)",[&input.id],|r|r.get::<_,bool>(0))?{return Err(Error::conflict("Budget identity already exists"));}
  let mut copy=archive::validate(&archive::capture(db,&budget)?)?;let original_name=copy.state.budget["name"].clone();copy.state.budget["id"]=json!(input.id);copy.state.budget["name"]=json!(input.name);copy.state.receipts.clear();
  // Clone provenance is evidence, never an active legacy/cloud identity mapping.
  let mut provenance=vec![];
  {let mut query=db.prepare("SELECT s.original,s.archive_sha256,i.source_metadata,s.report FROM imported_sources i JOIN staged_imports s ON s.id=i.import_id WHERE i.budget_id=?1")?;for row in query.query_map([&budget],|r|Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?)))?{provenance.push(row?);}}
  copy.state.tables.get_mut("staged_imports").unwrap().rows.clear();copy.state.tables.get_mut("imported_sources").unwrap().rows.clear();
  for table in copy.state.tables.values_mut(){let index=table.columns.iter().position(|c|c=="budget_id").unwrap();for row in &mut table.rows{row[index]=json!(input.id);}}
  db.execute("INSERT INTO budgets(id,name,budget_type) VALUES(?1,?2,?3)",params![input.id,input.name,copy.state.budget["budget_type"].as_str()])?;archive::install_tables(db,&copy)?;
  db.execute("INSERT INTO budget_provenance(budget_id,id,kind,source_budget_id,metadata) VALUES(?1,?2,'duplicate',?3,?4)",params![input.id,uuid::Uuid::new_v4().to_string(),budget,json!({"source_name":original_name}).to_string()])?;
  for (bytes,sha,metadata,report) in provenance{let report:Value=serde_json::from_str(&report).map_err(Error::internal)?;let kind=match report["source_format"].as_str(){Some("ynab4")=>"ynab4-import",Some("nynab")=>"nynab-import",_=>"actual-import"};db.execute("INSERT INTO budget_provenance(budget_id,id,kind,source_budget_id,metadata,archive_sha256,original) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![input.id,uuid::Uuid::new_v4().to_string(),kind,budget,metadata,sha,bytes])?;}
  archive::validate_financial_state(db,&input.id)?;db.execute("UPDATE budget_sync SET clock=MAX(clock,?2) WHERE budget_id=?1",params![input.id,copy.state.clock])?;metadata(db,&input.id,false)
 })).await?;
    Ok((StatusCode::CREATED, Json(json!({"data":result}))))
}
