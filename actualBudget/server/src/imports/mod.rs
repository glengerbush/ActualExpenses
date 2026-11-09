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
mod conversion;
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
            if let Some(id)=db.query_row("SELECT id FROM staged_imports WHERE archive_sha256=?1",[&hash],|r|r.get::<_,String>(0)).optional()? {return read_import(db,&id);}
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
        let converted:Value=serde_json::from_str(&converted).map_err(Error::internal)?;
        if stage["status"]!="activated" {
            if db.query_row("SELECT EXISTS(SELECT 1 FROM budgets WHERE id=?1)",[budget],|row|row.get::<_,bool>(0))? {
                return Err(Error::new(StatusCode::CONFLICT,"import_budget_conflict","Assigned budget identity already exists; this import cannot replace it"));
            }
            conversion::install(db,budget,&converted)?;
            let source=&stage["report"]["source"];
            db.execute("INSERT INTO imported_sources(budget_id,import_id,source_local_id,source_cloud_id,source_group_id,source_metadata) VALUES(?1,?2,?3,?4,?5,?6)",params![budget,id,source["id"].as_str(),source["cloudFileId"].as_str(),source["groupId"].as_str(),source.to_string()])?;
            db.execute("UPDATE staged_imports SET status='activated' WHERE id=?1",[&id])?;
        }
        Ok(json!({"id":budget,"name":converted["name"],"budget_type":converted["budget_type"]}))
    })).await?;
    Ok((StatusCode::CREATED, data(result)))
}
