mod capabilities;
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
        DefaultBodyLimit, Path, Query, State,
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
mod merge;
pub(crate) use merge::identity as merge_identity;
pub(crate) mod conversion;
mod ynab;
const MAX_ARCHIVE: usize = 16 * 1024 * 1024;

pub fn router() -> Router<Store> {
    Router::new()
        .route(
            "/api/v1/imports/ynab4",
            post(upload_ynab4).layer(DefaultBodyLimit::max(MAX_ARCHIVE)),
        )
        .route(
            "/api/v1/imports/nynab",
            post(upload_nynab).layer(DefaultBodyLimit::max(MAX_ARCHIVE)),
        )
        .route("/api/v1/imports/{id}/original", get(original))
        .route(
            "/api/v1/imports/actual",
            post(upload).layer(DefaultBodyLimit::max(MAX_ARCHIVE)),
        )
        .route("/api/v1/imports", get(list))
        .route("/api/v1/imports/{id}", get(read))
        .route("/api/v1/imports/{id}/original.zip", get(original))
        .route("/api/v1/imports/{id}/activate", post(activate))
        .route("/api/v1/imports/{id}/merge", post(merge::merge))
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
    let (bytes, source_format) = store
        .run(move |db| {
            db.query_row(
                "SELECT original,report FROM staged_imports WHERE id=?1",
                [id],
                |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?
            .ok_or_else(Error::missing)
        })
        .await?;
    let report: Value = serde_json::from_str(&source_format).map_err(Error::internal)?;
    let (content_type, attachment) = match report["source_format"].as_str() {
        Some("ynab4") => ("application/zip", "attachment; filename=ynab4-original.zip"),
        Some("nynab") => (
            "application/json",
            "attachment; filename=nynab-original.json",
        ),
        _ => (
            "application/zip",
            "attachment; filename=actual-original.zip",
        ),
    };
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::CONTENT_DISPOSITION, attachment),
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
                let existing=read_import(db,&id)?;if matches!(existing["report"]["source_format"].as_str(),Some("ynab4"|"nynab")){return Err(Error::conflict("These original bytes were already staged using another source format. Review the existing import."));}
                let (status,plan):(String,String)=db.query_row("SELECT status,conversion FROM staged_imports WHERE id=?1",[&id],|r|Ok((r.get(0)?,r.get(1)?)))?;
                let plan:Value=serde_json::from_str(&plan).map_err(Error::internal)?;
                // A new upload intent explicitly reviews a stage predating newer converters.
                // The original upload key still replays its exact original receipt first.
                if status!="activated" && (plan.get("category_groups").is_none() || plan["notes_version"] != 1 || plan["transfer_version"] != 1 || plan["budgeting_version"] != 1 || plan["rules_version"] != capabilities::RULES_VERSION || plan["schedules_version"] != 1 || plan["accounts"].as_array().is_some_and(|rows|rows.iter().any(|row|row.get("last_reconciled").is_none()))) {
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
            let budget=db.query_row("SELECT id FROM budgets WHERE deleted=0",[],|r|r.get::<_,String>(0)).optional()?.unwrap_or_else(||Uuid::new_v4().to_string());
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
        let initializing = !db.query_row("SELECT EXISTS(SELECT 1 FROM budgets WHERE deleted=0)",[],|r|r.get::<_,bool>(0))?;
        if initializing {
            let name:String=db.query_row("SELECT json_extract(conversion,'$.name') FROM staged_imports WHERE id=?1",[&id],|r|r.get(0))?;
            store::create_budget(db,crate::models::NewBudget{id:stage["budget_id"].as_str().unwrap().into(),name,budget_type:"tracking".into()})?;
        }
        let active=crate::single_budget::current(db)?;
        let budget=active["id"].as_str().unwrap();
        if stage["status"]=="blocked" {return Err(Error::new(StatusCode::CONFLICT,"import_requires_conversion","Import contains unsupported data; review its conversion requirements"));}
        let converted:String=db.query_row("SELECT conversion FROM staged_imports WHERE id=?1",[&id],|r|r.get(0))?;
        let mut converted:Value=serde_json::from_str(&converted).map_err(Error::internal)?;
        if stage["status"]!="activated" && (converted.get("category_groups").is_none() || converted.get("account_groups").is_none() || converted.get("register_settings").is_none() || converted["notes_version"] != 1 || converted["budgeting_version"] != 1 || converted["rules_version"] != capabilities::RULES_VERSION || converted["schedules_version"] != 1) {
            let original:Vec<u8>=db.query_row("SELECT original FROM staged_imports WHERE id=?1",[&id],|r|r.get(0))?;
            let (_,current)=inspect_original(&original,&stage["report"])?;
            if converted["notes_version"] != 1 {converted["notes"]=current["notes"].clone();converted["notes_version"]=json!(1);}
            if converted["schedules_version"] != 1 {converted["schedules"]=current["schedules"].clone();converted["schedules_version"]=json!(1);}
            if converted["rules_version"] != capabilities::RULES_VERSION {converted["rules"]=current["rules"].clone();converted["rules_version"]=json!(capabilities::RULES_VERSION);}
            if converted["automations_version"] != 1 {converted["automations"]=current["automations"].clone();converted["automations_version"]=json!(1);}
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
            if !crate::single_budget::is_empty(db,budget)? {
                return Err(Error::new(StatusCode::CONFLICT,"import_budget_conflict","This budget already contains data. Merge the archive instead."));
            }
            crate::lifecycle::make_point(db,budget,"before_restore","Before initializing from an import")?;
            conversion::install(db,budget,&converted)?;
            if !initializing {
                db.execute("UPDATE budget_sync SET epoch=lower(hex(randomblob(16))) WHERE budget_id=?1",[budget])?;
                db.execute("UPDATE budget_lifecycle SET generation=generation+1 WHERE budget_id=?1",[budget])?;
            }
            if matches!(stage["report"]["source_format"].as_str(),Some("ynab4"|"nynab")){let clock=crate::concurrency::clock(db,budget,None)?;crate::rules::learning::imported(db,budget,&clock)?;}

            if !matches!(stage["report"]["source_format"].as_str(),Some("ynab4"|"nynab")){for (id,order) in original_orders(&db.query_row("SELECT original FROM staged_imports WHERE id=?1",[&id],|r|r.get::<_,Vec<u8>>(0))?)? {db.execute("UPDATE android_upgrade_order SET source_order=?3 WHERE budget_id=?1 AND id=?2",params![budget,id,order])?;}}
            crate::transfers::validate(db,budget)?;
            let source=&stage["report"]["source"];
            db.execute("INSERT INTO imported_sources(budget_id,import_id,source_local_id,source_cloud_id,source_group_id,source_metadata) VALUES(?1,?2,?3,?4,?5,?6)",params![budget,id,source["id"].as_str(),source["cloudFileId"].as_str(),source["groupId"].as_str(),source.to_string()])?;
            db.execute("UPDATE staged_imports SET status='activated',budget_id=?2 WHERE id=?1",params![id,budget])?;
        }
        crate::lifecycle::metadata(db,budget,false)
    })).await?;
    Ok((StatusCode::CREATED, data(result)))
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

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceFilename {
    filename: String,
}
fn inspect_original(bytes: &[u8], source: &Value) -> Result<(Value, Value), Error> {
    match source["source_format"].as_str() {
        Some(kind @ ("ynab4" | "nynab")) => ynab::inspect(
            kind,
            source["source"]["filename"]
                .as_str()
                .ok_or_else(|| Error::invalid("Original filename is missing"))?,
            bytes,
        ),
        _ => conversion::inspect(bytes),
    }
}
async fn upload_ynab4(
    State(store): State<Store>,
    Query(query): Query<SourceFilename>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    upload_ynab(store, query, headers, body, "ynab4").await
}
async fn upload_nynab(
    State(store): State<Store>,
    Query(query): Query<SourceFilename>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    upload_ynab(store, query, headers, body, "nynab").await
}
async fn upload_ynab(
    store: Store,
    query: SourceFilename,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
    kind: &'static str,
) -> Result<(StatusCode, Json<Value>), Error> {
    crate::models::validate_name(&query.filename)?;
    let key = idempotency_key(&headers)?;
    let bytes = body.map_err(|_| {
        Error::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "archive_too_large",
            "Export must be at most16 MiB",
        )
    })?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let payload = json!({"filename":query.filename,"sha256":hash}).to_string();
    let result=store.run(move|db|store::write(db,&format!("POST /imports/{kind}"),&key,&payload,|db|{
  if let Some(id)=db.query_row("SELECT id FROM staged_imports WHERE archive_sha256=?1",[&hash],|r|r.get::<_,String>(0)).optional()?{let existing=read_import(db,&id)?;if existing["report"]["source_format"]!=kind{return Err(Error::conflict("These original bytes were already staged using another source format. Review the existing import."));}return Ok(existing);}
  let (mut report,converted)=ynab::inspect(kind,&query.filename,&bytes)?;report["archive_sha256"]=json!(hash);let status=if report["requirements"].as_array().unwrap().iter().any(|r|r["blocking"]==true){"blocked"}else{"ready"};let id=Uuid::new_v4().to_string();let budget=db.query_row("SELECT id FROM budgets WHERE deleted=0",[],|r|r.get::<_,String>(0)).optional()?.unwrap_or_else(||Uuid::new_v4().to_string());db.execute("INSERT INTO staged_imports(id,archive_sha256,original,budget_id,status,report,conversion) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,hash,bytes.as_ref(),budget,status,report.to_string(),converted.to_string()])?;read_import(db,&id)
 })).await?;
    Ok((StatusCode::CREATED, data(result)))
}
