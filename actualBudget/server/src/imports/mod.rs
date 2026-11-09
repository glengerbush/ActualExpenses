use crate::{
    api::{idempotency_key, json_error},
    error::Error,
    models::validate_id,
    store::{self, Store},
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{
        DefaultBodyLimit, Path, State,
        rejection::{BytesRejection, JsonRejection, PathRejection},
    },
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;
pub(crate) mod conversion;
const MAX_ARCHIVE: usize = 16 * 1024 * 1024;

pub fn router() -> Router<Store> {
    Router::new()
        .route(
            "/api/v1/imports/actual",
            post(upload).layer(DefaultBodyLimit::max(MAX_ARCHIVE)),
        )
        .route("/api/v1/imports", get(list))
        .route("/api/v1/imports/{id}", get(read))
        .route("/api/v1/imports/{id}/original.zip", get(original))
        .route("/api/v1/imports/{id}/activate", post(activate))
}
fn data(value: Value) -> Json<Value> {
    Json(json!({"data":value}))
}
fn read_import(db: &Connection, id: &str) -> Result<Value, Error> {
    db.query_row("SELECT id,budget_id,status,report FROM staged_imports WHERE id=?1",[id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?)))
        .optional()?.ok_or_else(Error::missing).and_then(|(id,budget_id,status,report)|Ok(json!({"id":id,"budget_id":budget_id,"status":status,"report":serde_json::from_str::<Value>(&report).map_err(Error::internal)?})))
}
async fn list(State(store): State<Store>) -> Result<Json<Value>, Error> {
    Ok(data(
        store
            .run(|db| {
                let mut stmt =
                    db.prepare("SELECT id FROM staged_imports ORDER BY rowid DESC LIMIT 100")?;
                let ids = stmt
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                ids.into_iter()
                    .map(|id| read_import(db, &id))
                    .collect::<Result<Vec<_>, _>>()
                    .map(|rows| json!(rows))
            })
            .await?,
    ))
}
fn import_id(path: Result<Path<String>, PathRejection>) -> Result<String, Error> {
    let Path(id) = path.map_err(|_| Error::invalid("Invalid import path"))?;
    validate_id(&id)?;
    Ok(id)
}
async fn read(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<Value>, Error> {
    let id = import_id(path)?;
    Ok(data(store.run(move |db| read_import(db, &id)).await?))
}
async fn original(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
) -> Result<impl IntoResponse, Error> {
    let id = import_id(path)?;
    let bytes = store
        .run(move |db| {
            db.query_row(
                "SELECT original FROM staged_imports WHERE id=?1",
                [id],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .ok_or_else(Error::missing)
        })
        .await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=actual-original.zip",
            ),
            (header::CACHE_CONTROL, "no-store"),
        ],
        bytes,
    ))
}
async fn upload(
    State(store): State<Store>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let key = idempotency_key(&headers)?;
    let bytes = body.map_err(|_| {
        Error::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "archive_too_large",
            "Archive must be at most 16 MiB",
        )
    })?;
    if !headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == "application/zip" || v == "application/octet-stream")
    {
        return Err(Error::invalid("Upload an Actual ZIP archive"));
    }
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let result=store.run(move|db|{
        store::write(db,"POST /imports/actual",&key,&hash,|db|{
            if let Some(id)=db.query_row("SELECT id FROM staged_imports WHERE archive_sha256=?1",[&hash],|r|r.get::<_,String>(0)).optional()? {
                let (status,plan):(String,String)=db.query_row("SELECT status,conversion FROM staged_imports WHERE id=?1",[&id],|r|Ok((r.get(0)?,r.get(1)?)))?;
                let plan:Value=serde_json::from_str(&plan).map_err(Error::internal)?;
                // A new upload intent explicitly reviews a stage predating newer converters.
                // The original upload key still replays its exact original receipt first.
                if status!="activated" && (plan.get("category_groups").is_none() || plan["transfer_version"] != 1 || plan["budgeting_version"] != 1 || plan["rules_version"] != 1 || plan["accounts"].as_array().is_some_and(|rows|rows.iter().any(|row|row.get("last_reconciled").is_none()))) {
                    let (mut report,converted)=conversion::inspect(&bytes)?;
                    report["archive_sha256"]=json!(hash);
                    let status=if report["requirements"].as_array().unwrap().iter().any(|r|r["blocking"]==true){"blocked"}else{"ready"};
                    db.execute("UPDATE staged_imports SET status=?2,report=?3,conversion=?4 WHERE id=?1",params![id,status,report.to_string(),converted.to_string()])?;
                }
                return read_import(db,&id);
            }
            let (mut report,converted)=conversion::inspect(&bytes)?;
            report["archive_sha256"]=json!(hash);
            let id=Uuid::new_v4().to_string();
            let budget=Uuid::new_v4().to_string();
            let status=if report["requirements"].as_array().unwrap().iter().any(|r|r["blocking"]==true){"blocked"}else{"ready"};
            db.execute("INSERT INTO staged_imports(id,archive_sha256,original,budget_id,status,report,conversion) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,hash,bytes.as_ref(),budget,status,report.to_string(),converted.to_string()])?;
            read_import(db,&id)
        })
    }).await?;
    Ok((StatusCode::CREATED, data(result)))
}
async fn activate(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let id = import_id(path)?;
    let Json(input) = input.map_err(json_error)?;
    if input != json!({}) {
        return Err(Error::invalid("Activation requires an empty object"));
    }
    let key = idempotency_key(&headers)?;
    let result=store.run(move|db|store::write(db,&format!("POST /imports/{id}/activate"),&key,"{}",|db|{
        let stage=read_import(db,&id)?;
        let budget=stage["budget_id"].as_str().unwrap();
        if stage["status"]=="blocked" {return Err(Error::new(StatusCode::CONFLICT,"import_requires_conversion","Import contains unsupported data; review its conversion requirements"));}
        let converted:String=db.query_row("SELECT conversion FROM staged_imports WHERE id=?1",[&id],|r|r.get(0))?;
        let mut converted:Value=serde_json::from_str(&converted).map_err(Error::internal)?;
        if stage["status"]!="activated" && (converted.get("category_groups").is_none() || converted.get("account_groups").is_none() || converted.get("register_settings").is_none() || converted["budgeting_version"] != 1 || converted["rules_version"] != 1) {
            let original:Vec<u8>=db.query_row("SELECT original FROM staged_imports WHERE id=?1",[&id],|r|r.get(0))?;
            let (_,current)=conversion::inspect(&original)?;
            if converted["rules_version"] != 1 {converted["rules"]=current["rules"].clone();converted["rules_version"]=json!(1);}
            if converted["budgeting_version"] != 1 {converted["budgeting"]=current["budgeting"].clone();converted["budgeting_version"]=json!(1);}
            if converted.get("register_settings").is_none() { converted["register_settings"]=current["register_settings"].clone(); }
            if converted.get("account_groups").is_none() {
                converted["account_groups"]=current["account_groups"].clone();
                for account in converted["accounts"].as_array_mut().unwrap() {
                    if let Some(source)=current["accounts"].as_array().unwrap().iter().find(|row|row["id"]==account["id"]) {
                        for field in ["group_id","sort_order","tombstone"] {account[field]=source[field].clone();}
                    }
                }
            }
            if converted.get("category_groups").is_none() {
                converted["category_groups"]=current["category_groups"].clone();
                for category in converted["categories"].as_array_mut().unwrap() {
                    if let Some(source)=current["categories"].as_array().unwrap().iter().find(|row|row["id"]==category["id"]) {
                        for field in ["group_id","hidden","notes","sort_order","tombstone","replacement_id"] {category[field]=source[field].clone();}
                    }
                }
            }
        }
        if stage["status"]!="activated" {
            if db.query_row("SELECT EXISTS(SELECT 1 FROM budgets WHERE id=?1)",[budget],|row|row.get::<_,bool>(0))? {
                return Err(Error::new(StatusCode::CONFLICT,"import_budget_conflict","Assigned budget identity already exists; this import cannot replace it"));
            }
            conversion::install(db,budget,&converted)?;
            for (id,order) in original_orders(&db.query_row("SELECT original FROM staged_imports WHERE id=?1",[&id],|r|r.get::<_,Vec<u8>>(0))?)? {db.execute("UPDATE android_upgrade_order SET source_order=?3 WHERE budget_id=?1 AND id=?2",params![budget,id,order])?;}
            crate::transfers::validate(db,budget)?;
            let source=&stage["report"]["source"];
            db.execute("INSERT INTO imported_sources(budget_id,import_id,source_local_id,source_cloud_id,source_group_id,source_metadata) VALUES(?1,?2,?3,?4,?5,?6)",params![budget,id,source["id"].as_str(),source["cloudFileId"].as_str(),source["groupId"].as_str(),source.to_string()])?;
            db.execute("UPDATE staged_imports SET status='activated' WHERE id=?1",[&id])?;
        }
        Ok(json!({"id":budget,"name":converted["name"],"budget_type":converted["budget_type"]}))
    })).await?;
    Ok((StatusCode::CREATED, data(result)))
}

/// Add newly supported metadata without replaying source financial values or
/// overwriting any native fields that existed before this migration.
pub(crate) fn backfill_payees_tags(db: &rusqlite::Connection) -> Result<(), Error> {
    // Unactivated stages have no native budget archive yet. Refresh their
    // conversion without changing immutable upload receipts or original bytes.
    let mut staged = db.prepare(
        "SELECT id,original,archive_sha256 FROM staged_imports WHERE status<>'activated'",
    )?;
    let unactivated = staged
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, bytes, hash) in unactivated {
        let Ok((mut report, data)) = conversion::inspect(&bytes) else {
            continue;
        };
        report["archive_sha256"] = json!(hash);
        let status = if report["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["blocking"] == true)
        {
            "blocked"
        } else {
            "ready"
        };
        db.execute(
            "UPDATE staged_imports SET report=?2,conversion=?3,status=?4 WHERE id=?1",
            params![id, report.to_string(), data.to_string(), status],
        )?;
    }

    let mut q=db.prepare("SELECT s.budget_id,s.original FROM staged_imports s JOIN imported_sources i ON i.import_id=s.id WHERE s.status='activated' UNION SELECT budget_id,original FROM budget_provenance WHERE kind='actual-import' AND original IS NOT NULL")?;
    let sources = q
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (budget, bytes) in sources {
        let (_, data) = conversion::inspect(&bytes)?;
        for p in data["payees"].as_array().unwrap() {
            db.execute(
                "INSERT OR IGNORE INTO payees(budget_id,id,name) VALUES(?1,?2,?3)",
                params![budget, p["id"].as_str(), p["name"].as_str()],
            )?;
            db.execute("UPDATE payees SET favorite=?3,tombstone=?4,transfer_account_id=?5,category_id=?6,merged_into=?7 WHERE budget_id=?1 AND id=?2",params![budget,p["id"].as_str(),p["favorite"].as_bool(),p["tombstone"].as_bool(),p["transfer_account_id"].as_str(),p["category_id"].as_str(),p["merged_into"].as_str()])?;
        }
        for tag in data["tags"].as_array().unwrap() {
            db.execute("INSERT INTO tags(budget_id,id,tag,color,description,hidden,tombstone) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![budget,tag["id"].as_str(),tag["tag"].as_str(),tag["color"].as_str(),tag["description"].as_str(),tag["hidden"].as_bool(),tag["tombstone"].as_bool()])?;
        }
        for t in data["transactions"].as_array().unwrap() {
            db.execute(
                "UPDATE transactions SET imported_payee=?3 WHERE budget_id=?1 AND id=?2",
                params![budget, t["id"].as_str(), t["imported_payee"].as_str()],
            )?;
        }
        for c in data["clocks"].as_array().unwrap() {
            let dataset = c["dataset"].as_str().unwrap();
            let column = c["column"].as_str().unwrap();
            let mapped = match (dataset, column) {
                ("payees", "favorite" | "tombstone") => Some(column),
                ("payees", "transfer_acct") => Some("transfer_account_id"),
                ("payee_mapping", "targetId") => Some("merged_into"),
                ("payees", "category") => Some("category_id"),
                ("tags", "tag" | "color" | "description" | "hidden" | "tombstone") => Some(column),
                ("transactions", "imported_description") => Some("imported_payee"),
                _ => None,
            };
            if let Some(field) = mapped {
                crate::concurrency::seed_field_version(
                    db,
                    &budget,
                    if dataset == "payee_mapping" {
                        "payees"
                    } else {
                        dataset
                    },
                    c["row"].as_str().unwrap(),
                    field,
                    c["timestamp"].as_str().unwrap(),
                )?;
            }
        }
    }
    Ok(())
}

/// Recover exact original ordering from the retained source SQLite, not its
/// compressed native ranks or only the latest surviving CRDT field clock.
pub(crate) fn original_orders(bytes: &[u8]) -> Result<Vec<(String, Option<f64>)>, Error> {
    let (_, data) = conversion::inspect(bytes)?;
    Ok(data["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["id"].as_str().unwrap().to_owned(),
                r["source_sort_order"].as_f64(),
            )
        })
        .collect())
}
