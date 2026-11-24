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
mod prepared;
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
    let result = store
        .run(move |db| {
            store::write(db, "POST /imports/actual", &key, &hash, |db| {
                prepared::stage(db, prepared::Source::Actual, &bytes, &hash)
            })
        })
        .await?;
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
        let converted = if stage["status"] != "activated" { Some(prepared::load(db, &stage)?) } else { None };
        let initializing = !db.query_row("SELECT EXISTS(SELECT 1 FROM budgets WHERE deleted=0)",[],|r|r.get::<_,bool>(0))?;
        if initializing {
            let name:String=db.query_row("SELECT json_extract(conversion,'$.name') FROM staged_imports WHERE id=?1",[&id],|r|r.get(0))?;
            store::create_budget(db,crate::models::NewBudget{id:stage["budget_id"].as_str().unwrap().into(),name,budget_type:"tracking".into()})?;
        }
        let active=crate::single_budget::current(db)?;
        let budget=active["id"].as_str().unwrap();
        if let Some(converted) = converted {
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

            crate::transfers::validate(db,budget)?;
            let source=&stage["report"]["source"];
            db.execute("INSERT INTO imported_sources(budget_id,import_id,source_local_id,source_cloud_id,source_group_id,source_metadata) VALUES(?1,?2,?3,?4,?5,?6)",params![budget,id,source["id"].as_str(),source["cloudFileId"].as_str(),source["groupId"].as_str(),source.to_string()])?;
            db.execute("UPDATE staged_imports SET status='activated',budget_id=?2 WHERE id=?1",params![id,budget])?;
        }
        crate::lifecycle::metadata(db,budget,false)
    })).await?;
    Ok((StatusCode::CREATED, data(result)))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceFilename {
    filename: String,
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
    let result = store
        .run(move |db| {
            store::write(db, &format!("POST /imports/{kind}"), &key, &payload, |db| {
                prepared::stage(
                    db,
                    prepared::Source::Ynab {
                        kind,
                        filename: &query.filename,
                    },
                    &bytes,
                    &hash,
                )
            })
        })
        .await?;
    Ok((StatusCode::CREATED, data(result)))
}
