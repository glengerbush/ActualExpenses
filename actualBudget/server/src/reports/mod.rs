//! Versioned reporting documents and bounded, read-only financial projections.
pub mod import;
mod query;
use crate::{
    concurrency as sync,
    error::Error,
    store::{self, Store},
};
use axum::{
    Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub id: String,
    pub kind: String,
    pub revision: i64,
    pub config: Value,
    pub tombstone: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Command {
    writes: Vec<Document>,
}
pub fn router() -> Router<Store> {
    Router::new()
        .route("/api/v1/budgets/{budget}/reporting", get(read))
        .route("/api/v1/budgets/{budget}/reporting/commands", post(command))
        .route("/api/v1/budgets/{budget}/reports/query", post(query::read))
        .merge(query::forecast_router())
        .merge(query::analysis_router())
        .merge(query::sankey_router())
        .merge(query::crossover_router())
        .route(
            "/api/v1/budgets/{budget}/reports/drilldown",
            post(query::drilldown),
        )
}
pub fn documents(db: &Connection, b: &str) -> Result<Value, Error> {
    store::require_budget(db, b)?;
    let documents=db.prepare("SELECT id,kind,revision,config,tombstone FROM reporting_documents WHERE budget_id=?1 ORDER BY kind,id")?.query_map([b],|r|Ok(Document{id:r.get(0)?,kind:r.get(1)?,revision:r.get(2)?,config:serde_json::from_str(&r.get::<_,String>(3)?).unwrap_or(Value::Null),tombstone:r.get(4)?}))?.collect::<Result<Vec<_>,_>>()?;
    Ok(
        json!({"documents":documents,"epoch":&sync::cursor(db,b)?[2..34],"snapshot":sync::cursor(db,b)?}),
    )
}
async fn read(State(store): State<Store>, Path(b): Path<String>) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    Ok(Json(
        json!({"data":store.run(move|db|{let tx=db.transaction()?;documents(&tx,&b)}).await?}),
    ))
}
fn validate(d: &Document) -> Result<(), Error> {
    crate::models::validate_id(&d.id)?;
    if !["page", "widget", "custom_report"].contains(&d.kind.as_str())
        || d.revision < 0
        || !d.config.is_object()
        || d.config.to_string().len() > 65536
    {
        return Err(Error::invalid("Invalid reporting document"));
    }
    if d.kind == "page" || d.kind == "custom_report" {
        crate::models::validate_name(d.config["name"].as_str().unwrap_or(""))?;
    }
    if d.kind == "widget" {
        crate::models::validate_id(d.config["dashboard_page_id"].as_str().unwrap_or(""))?;
        if d.config["type"]
            .as_str()
            .is_none_or(|v| v.is_empty() || v.len() > 100)
        {
            return Err(Error::invalid("Invalid widget type"));
        }
        for (key, max) in [("x", 11), ("y", 100000), ("width", 12), ("height", 100)] {
            let n = d.config[key]
                .as_i64()
                .ok_or_else(|| Error::invalid("Invalid widget layout"))?;
            if n < 0 || n > max || ((key == "width" || key == "height") && n == 0) {
                return Err(Error::invalid("Invalid widget layout"));
            }
        }
        if d.config["x"].as_i64().unwrap() + d.config["width"].as_i64().unwrap() > 12 {
            return Err(Error::invalid("Widget exceeds dashboard width"));
        }
    }
    Ok(())
}
pub fn validate_archive(db: &Connection, b: &str) -> Result<(), Error> {
    let all = documents(db, b)?;
    for row in all["documents"].as_array().unwrap() {
        let d: Document = serde_json::from_value(row.clone()).map_err(Error::internal)?;
        validate(&d)?;
        if d.revision < 1 {
            return Err(Error::invalid("Invalid reporting revision"));
        }
        if d.kind == "widget" && !d.tombstone {
            let page:Option<bool>=db.query_row("SELECT tombstone FROM reporting_documents WHERE budget_id=?1 AND id=?2 AND kind='page'",params![b,d.config["dashboard_page_id"].as_str()],|r|r.get(0)).optional()?;
            if page != Some(false) {
                return Err(Error::invalid("Widget requires a live dashboard"));
            }
        }
    }
    Ok(())
}
async fn command(
    State(store): State<Store>,
    Path(b): Path<String>,
    headers: HeaderMap,
    input: Result<Json<Command>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    crate::models::validate_id(&b)?;
    let Json(input) = input.map_err(crate::api::json_error)?;
    let key = crate::api::idempotency_key(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?
        .ok_or_else(|| Error::invalid("Budget epoch required"))?;
    let proposed = sync::header(&headers)?;
    let cancel = sync::cancel_header(&headers)?;
    let lookup = sync::receipt_only_header(&headers)?;
    let payload = serde_json::to_string(&(&epoch, &input)).map_err(Error::internal)?;
    store.run(move |db| {
        let scope = format!("POST /budgets/{b}/reporting/commands");
        if lookup {
            return sync::receipt(db, &scope, &key, &payload, proposed.as_deref(), StatusCode::OK);
        }
        sync::write(db, &scope, &key, &payload, proposed.as_deref(), StatusCode::OK, |db| {
            // A rejected draft can still be canceled durably. Never validate a
            // domain payload before checking its existing receipt/cancel fence.
            if cancel { return sync::canceled(db, &b); }
            crate::lifecycle::require_epoch(db, &b, Some(&epoch))?;
            if input.writes.is_empty() || input.writes.len() > 200 {
                return Err(Error::invalid("Expected 1–200 document writes"));
            }
            let mut ids = BTreeSet::new();
            for document in &input.writes {
                validate(document)?;
                if !ids.insert(&document.id) {
                    return Err(Error::invalid("Repeated reporting document"));
                }
            }
            let _ = sync::clock(db, &b, proposed.as_deref())?;
            let clock: String = db.query_row("SELECT clock FROM budget_sync WHERE budget_id=?1", [&b], |row| row.get(0))?;
            for document in &input.writes {
                let current: Option<(i64, String)> = db.query_row(
                    "SELECT revision,kind FROM reporting_documents WHERE budget_id=?1 AND id=?2",
                    params![b, document.id], |row| Ok((row.get(0)?, row.get(1)?)),
                ).optional()?;
                if current.as_ref().map_or(0, |row| row.0) != document.revision
                    || current.as_ref().is_some_and(|row| row.1 != document.kind) {
                    return Err(Error::new(StatusCode::CONFLICT, "reporting_changed", "Reporting settings changed elsewhere. Reload before saving; your draft is retained."));
                }
                if let Some(proposed) = proposed.as_deref() {
                    let newest: Option<String> = db.query_row(
                        "SELECT max(clock) FROM field_versions WHERE budget_id=?1 AND entity='reporting_documents' AND entity_id=?2",
                        params![b, document.id], |row| row.get(0),
                    )?;
                    if newest.as_deref().is_some_and(|newest| proposed <= newest) {
                        return Err(Error::new(StatusCode::CONFLICT, "reporting_changed", "This reporting document has newer field ordering. Reload before saving; the original request is retained."));
                    }
                }
                let revision = document.revision.checked_add(1).ok_or_else(|| Error::invalid("Reporting revision exhausted"))?;
                db.execute(
                    "INSERT INTO reporting_documents(budget_id,id,kind,config,revision,tombstone) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(budget_id,id) DO UPDATE SET config=excluded.config,revision=excluded.revision,tombstone=excluded.tombstone",
                    params![b, document.id, document.kind, document.config.to_string(), revision, document.tombstone],
                )?;
                sync::seed_field_version(db, &b, "reporting_documents", &document.id, "document", &clock)?;
                sync::mark(db, &b, "reporting_documents", &document.id)?;
            }
            validate_archive(db, &b)?;
            Ok(json!({"response_status":200,"response_body":{"data":documents(db,&b)?,"edit_clock":clock}}))
        })
    }).await
}
