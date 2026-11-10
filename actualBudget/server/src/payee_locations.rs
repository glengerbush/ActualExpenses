//! Saved budget coordinates. Lookups never acquire or persist the viewer's position.
use crate::{
    api::{budget_id, idempotency_key, json_error},
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
    routing::get,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};

pub fn router() -> Router<Store> {
    Router::new()
        .route(
            "/api/v1/budgets/{budget}/payee-locations",
            get(list).post(mutate),
        )
        .route(
            "/api/v1/budgets/{budget}/payee-locations/{id}",
            get(read).patch(mutate),
        )
}
pub fn coordinates(latitude: f64, longitude: f64) -> Result<(), Error> {
    if !latitude.is_finite()
        || !longitude.is_finite()
        || !(-90.0..=90.0).contains(&latitude)
        || !(-180.0..=180.0).contains(&longitude)
    {
        return Err(Error::invalid(
            "Latitude must be −90–90 and longitude −180–180",
        ));
    }
    Ok(())
}
pub fn record(db: &Connection, b: &str, id: &str) -> Result<Value, Error> {
    let mut value=db.query_row("SELECT payee_id,latitude,longitude,created_at,tombstone FROM payee_locations WHERE budget_id=?1 AND id=?2",params![b,id],|r|Ok(json!({"id":id,"payee_id":r.get::<_,String>(0)?,"latitude":r.get::<_,f64>(1)?,"longitude":r.get::<_,f64>(2)?,"created_at":r.get::<_,i64>(3)?,"deleted":r.get::<_,bool>(4)?}))).optional()?.ok_or_else(Error::missing)?;
    value["field_versions"] = crate::payees_tags::versions(db, b, "payee_locations", id)?;
    Ok(value)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    #[serde(default = "limit")]
    limit: u32,
    #[serde(default)]
    offset: u32,
    snapshot: Option<String>,
    payee_id: Option<String>,
    #[serde(default)]
    include_deleted: bool,
}
fn limit() -> u32 {
    100
}
async fn list(
    State(s): State<Store>,
    p: Result<Path<String>, PathRejection>,
    q: Result<Query<Page>, QueryRejection>,
) -> Result<Json<Value>, Error> {
    let b = budget_id(p)?;
    let q = q.map_err(|_| Error::invalid("Invalid location query"))?.0;
    if !(1..=1000).contains(&q.limit) || q.offset > 1000000 {
        return Err(Error::invalid("Invalid location pagination"));
    }
    if let Some(id) = &q.payee_id {
        validate_id(id)?;
    }
    Ok(Json(s.run(move|db|{
 concurrency::check_snapshot(db,&b,q.snapshot.as_deref())?;
 let mut query=db.prepare("SELECT id FROM payee_locations WHERE budget_id=?1 AND (?2 OR tombstone=0) AND (?3 IS NULL OR payee_id=?3) ORDER BY created_at DESC,id LIMIT ?4 OFFSET ?5")?;
 let ids=query.query_map(params![b,q.include_deleted,q.payee_id,q.limit+1,q.offset],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
 let rows=ids.iter().take(q.limit as usize).map(|id|record(db,&b,id)).collect::<Result<Vec<_>,_>>()?;
 Ok(json!({"data":rows,"next_offset":if ids.len()>q.limit as usize{Some(q.offset+q.limit)}else{None}}))
 }).await?))
}
async fn read(
    State(s): State<Store>,
    p: Result<Path<(String, String)>, PathRejection>,
) -> Result<Json<Value>, Error> {
    let Path((b, id)) = p.map_err(|_| Error::invalid("Invalid location path"))?;
    validate_id(&b)?;
    validate_id(&id)?;
    Ok(Json(
        json!({"data":s.run(move|db|{store::require_budget(db,&b)?;record(db,&b,&id)}).await?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewPayee {
    id: String,
    name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    id: String,
    payee_id: Option<String>,
    new_payee: Option<NewPayee>,
    latitude: f64,
    longitude: f64,
}
async fn mutate(
    State(s): State<Store>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let parts = uri.path().split('/').collect::<Vec<_>>();
    let b = parts[4].to_owned();
    validate_id(&b)?;
    let id = parts.get(6).map(|s| s.to_string());
    if let Some(id) = &id {
        validate_id(id)?;
    }
    let Json(body) = input.map_err(json_error)?;
    let payload = body.to_string();
    let key = idempotency_key(&headers)?;
    let proposed = concurrency::header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let cancel = concurrency::cancel_header(&headers)?;
    let scope = format!("{} {}", method, uri.path().trim_start_matches("/api/v1"));
    s.run(move |db| {
        concurrency::write(db, &scope, &key, &payload, proposed.as_deref(),
            if id.is_none() { StatusCode::CREATED } else { StatusCode::OK }, |db| {
                if cancel {
                    crate::lifecycle::metadata(db, &b, true)?;
                    return Ok(json!({"response_status":409,"response_body":{"error":{"code":"operation_canceled","message":"This location change was canceled before it was saved."}}}));
                }
                crate::lifecycle::require_epoch(db, &b, epoch.as_deref())?;
                let clock = concurrency::clock(db, &b, proposed.as_deref())?;
                if let Some(id) = id { forget(db, &b, &id, &body, &clock) }
                else { create(db, &b, body, &clock) }
            })
    }).await
}
fn forget(db: &Connection, b: &str, id: &str, body: &Value, clock: &str) -> Result<Value, Error> {
    if *body != json!({"deleted":true}) {
        return Err(Error::invalid(
            "Forgetting a location requires deleted: true",
        ));
    }
    let before = record(db, b, id)?;
    let old = before["field_versions"]["tombstone"]
        .as_str()
        .unwrap_or(concurrency::ZERO);
    let conflicts = if clock <= old {
        vec![json!({"field":"tombstone","winning_clock":old,"submitted_clock":clock})]
    } else {
        db.execute(
            "UPDATE payee_locations SET tombstone=1 WHERE budget_id=?1 AND id=?2",
            params![b, id],
        )?;
        concurrency::seed_field_version(db, b, "payee_locations", id, "tombstone", clock)?;
        vec![]
    };
    Ok(
        json!({"response_status":200,"response_body":{"data":record(db,b,id)?,"conflicts":conflicts}}),
    )
}
fn create(db: &Connection, b: &str, body: Value, clock: &str) -> Result<Value, Error> {
    let create: Create = serde_json::from_value(body)
        .map_err(|_| Error::invalid("Supply an id, a payee and valid coordinates"))?;
    validate_id(&create.id)?;
    coordinates(create.latitude, create.longitude)?;
    let payee = match (create.payee_id, create.new_payee) {
        (Some(id), None) => id,
        (None, Some(p)) => {
            validate_id(&p.id)?;
            validate_name(p.name.trim())?;
            crate::payees_tags::duplicate(db, b, "payees", &p.id, p.name.trim())?;
            if db.query_row(
                "SELECT EXISTS(SELECT 1 FROM payees WHERE budget_id=?1 AND id=?2)",
                params![b, p.id],
                |r| r.get::<_, bool>(0),
            )? {
                return Err(Error::conflict("The new payee ID already exists"));
            }
            db.execute(
                "INSERT INTO payees(budget_id,id,name) VALUES(?1,?2,?3)",
                params![b, p.id, p.name.trim()],
            )?;
            concurrency::seed_field_version(db, b, "payees", &p.id, "name", clock)?;
            p.id
        }
        _ => return Err(Error::invalid("Supply payee_id or new_payee, exclusively")),
    };
    validate_id(&payee)?;
    let p = crate::payees_tags::record(db, b, "payees", &payee)?;
    if p["deleted"] == true || !p["transfer_account_id"].is_null() {
        return Err(Error::conflict(
            "Choose an active ordinary payee before saving its location",
        ));
    }
    if db.query_row(
        "SELECT EXISTS(SELECT 1 FROM payee_locations WHERE budget_id=?1 AND id=?2)",
        params![b, create.id],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Error::conflict("Location ID already exists"));
    }
    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(Error::internal)?
        .as_millis() as i64;
    db.execute("INSERT INTO payee_locations(budget_id,id,payee_id,latitude,longitude,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![b,create.id,payee,create.latitude,create.longitude,created])?;
    for field in [
        "payee_id",
        "latitude",
        "longitude",
        "created_at",
        "tombstone",
    ] {
        concurrency::seed_field_version(db, b, "payee_locations", &create.id, field, clock)?;
    }
    record(db, b, &create.id)
}

pub fn validate_archive(db: &Connection, b: &str) -> Result<(), Error> {
    let mut q = db
        .prepare("SELECT id,payee_id,latitude,longitude FROM payee_locations WHERE budget_id=?1")?;
    for row in q.query_map([b], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, f64>(2)?,
            r.get::<_, f64>(3)?,
        ))
    })? {
        let (id, p, lat, lon) = row?;
        validate_id(&id)?;
        validate_id(&p)?;
        coordinates(lat, lon)?;
        crate::payees_tags::record(db, b, "payees", &p)?;
    }
    Ok(())
}

/// Newly supported data only: do not replay financial values, aliases, or receipts.
pub fn install_source(db: &Connection, b: &str, data: &Value) -> Result<(), Error> {
    for row in data["payee_locations"].as_array().into_iter().flatten() {
        let inserted=db.execute("INSERT OR IGNORE INTO payee_locations(budget_id,id,payee_id,latitude,longitude,created_at,tombstone) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![b,row["id"].as_str(),row["payee_id"].as_str(),row["latitude"].as_f64(),row["longitude"].as_f64(),row["created_at"].as_i64(),row["tombstone"].as_bool()])?;
        if inserted == 0 {
            continue;
        }
        for c in data["clocks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| c["dataset"] == "payee_locations" && c["row"] == row["id"])
        {
            let column = c["column"].as_str().unwrap();
            if [
                "payee_id",
                "latitude",
                "longitude",
                "created_at",
                "tombstone",
            ]
            .contains(&column)
            {
                concurrency::seed_field_version(
                    db,
                    b,
                    "payee_locations",
                    row["id"].as_str().unwrap(),
                    column,
                    c["timestamp"].as_str().unwrap(),
                )?;
            }
        }
    }
    validate_archive(db, b)
}
pub fn enrich_legacy_imports(db: &Connection, only_budget: Option<&str>) -> Result<(), Error> {
    // Refresh the conversion plan while preserving exact old upload receipts/bytes.
    let mut q=db.prepare("SELECT id,original,archive_sha256 FROM staged_imports WHERE COALESCE(json_extract(report,'$.source_format'),'actual') NOT IN('ynab4','nynab') AND status<>'activated' AND (?1 IS NULL OR budget_id=?1)")?;
    let stages = q
        .query_map([only_budget], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, bytes, hash) in stages {
        let Ok((mut report, data)) = crate::imports::conversion::inspect(&bytes) else {
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
    let mut q=db.prepare("SELECT s.budget_id,s.original FROM staged_imports s JOIN imported_sources i ON i.import_id=s.id AND COALESCE(json_extract(s.report,'$.source_format'),'actual') NOT IN('ynab4','nynab') WHERE s.status='activated' AND (?1 IS NULL OR s.budget_id=?1) UNION SELECT budget_id,original FROM budget_provenance WHERE kind='actual-import' AND original IS NOT NULL AND (?1 IS NULL OR budget_id=?1)")?;
    let sources = q
        .query_map([only_budget], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (b, bytes) in sources {
        let (_, data) = crate::imports::conversion::inspect(&bytes)?;
        install_source(db, &b, &data)?;
    }
    Ok(())
}
