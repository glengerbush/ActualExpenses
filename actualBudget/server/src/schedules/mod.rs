//! Backend schedule authority. Explicit commands own all durable effects.
mod calendar;
mod transaction;
pub(crate) use transaction::apply as apply_recurrence;
mod forecast;
pub(crate) use calendar::next as template_next;
pub(crate) use forecast::inputs as forecast_inputs;
mod commands;
mod conversion;
mod data;
mod discovery;
mod import;
mod links;
mod matching;
mod occurrences;
pub(crate) use import::{
    convert as convert_actual, enrich, import_clock, install as install_actual,
};
mod view;
use crate::{concurrency as sync, error::Error, store::Store};
use axum::{
    Json, Router,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
pub(crate) use data::{decorate, link};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn router() -> Router<Store> {
    Router::new()
        .route("/api/v1/schedules/preview-dates", post(preview_dates))
        .route("/api/v1/budgets/{budget}/schedules", get(list))
        .route("/api/v1/budgets/{budget}/schedules/due", get(due))
        .route(
            "/api/v1/budgets/{budget}/schedules/convert-preview",
            post(conversion::preview),
        )
        .route(
            "/api/v1/budgets/{budget}/schedules/discovery",
            get(discovery::read),
        )
        .route(
            "/api/v1/budgets/{budget}/schedules/matches",
            post(matching::read),
        )
        .route(
            "/api/v1/budgets/{budget}/schedules/{id}/transactions",
            get(matching::linked),
        )
        .route(
            "/api/v1/budgets/{budget}/schedules/{id}/occurrences",
            get(occurrences::read),
        )
        .route("/api/v1/budgets/{budget}/schedules/commands", post(command))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Dates {
    config: Value,
    from: String,
    count: usize,
    upcoming_length: Option<String>,
}
async fn preview_dates(input: Result<Json<Dates>, JsonRejection>) -> Result<Json<Value>, Error> {
    let Json(body) = input.map_err(crate::api::json_error)?;
    if body.count == 0 || body.count > 1000 {
        return Err(Error::invalid("Preview1–1000 dates"));
    }
    let mut data =
        json!({"data":calendar::dates(&body.config,&body.from,body.count,&mut 2_000_000)?});
    if let Some(horizon) = body.upcoming_length {
        data["upcoming_days"] = json!(view::horizon_days(&horizon, &body.from)?);
    }
    Ok(Json(data))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Page {
    #[serde(default)]
    offset: usize,
    #[serde(default = "page_limit")]
    limit: usize,
    account_id: Option<String>,
    snapshot: Option<String>,
}
fn page_limit() -> usize {
    100
}
async fn list(
    State(store): State<Store>,
    Path(b): Path<String>,
    query: Result<Query<Page>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    let Query(query) = query.map_err(|_| Error::invalid("Invalid schedule page"))?;
    if query.limit == 0 || query.limit > 1000 || query.offset > 100000 {
        return Err(Error::invalid("Schedule pages contain1–1000 rows"));
    }
    if let Some(account) = &query.account_id {
        crate::models::validate_id(account)?;
    }
    Ok(Json(
        store
            .run(move |db| {
                sync::check_snapshot(db, &b, query.snapshot.as_deref())?;
                if let Some(account) = &query.account_id {
                    crate::ledger::account_exists(db, &b, account)?;
                }
                Ok(json!({"data":view::list(db,&b,&query)?}))
            })
            .await?,
    ))
}
async fn due(State(store): State<Store>, Path(b): Path<String>) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    Ok(Json(
        store
            .run(move |db| Ok(json!({"data":view::due(db,&b)?})))
            .await?,
    ))
}
async fn command(
    State(store): State<Store>,
    Path(b): Path<String>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let Json(body) = input.map_err(crate::api::json_error)?;
    crate::models::validate_id(&b)?;
    let key = crate::api::idempotency_key(&headers)?;
    let owner = crate::history::owner_header(&headers)?;
    let proposed = sync::header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let cancel = sync::cancel_header(&headers)?;
    let lookup = sync::receipt_only_header(&headers)?;
    let payload = body.to_string();
    let status = if matches!(body["action"].as_str(), Some("create" | "convert")) {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    store.run(move|db|{
  let scope=format!("POST /budgets/{b}/schedules/commands");
  if lookup{return sync::receipt_owned(db,&scope,&key,&payload,proposed.as_deref(),status,owner.as_ref());}
  sync::write_owned(db,&scope,&key,&payload,proposed.as_deref(),status,owner.as_ref(),|db|{
   if cancel{return sync::canceled(db,&b);}
   crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;
   let c:commands::Command=serde_json::from_value(body).map_err(|_|Error::invalid("Invalid schedule command"))?;
   if c.action!="catch-up"{sync::check_snapshot(db,&b,Some(c.snapshot.as_deref().ok_or_else(||Error::invalid("A captured snapshot is required"))?))?;}
   let clock=sync::clock(db,&b,proposed.as_deref())?;
   let data=commands::apply(db,&b,c,&clock,epoch.as_deref())?;
   Ok(json!({"response_status":status.as_u16(),"response_body":{"data":data,"snapshot":sync::cursor(db,&b)?,"edit_clock":clock}}))
  })
 }).await
}
pub(crate) fn account_open(db: &Connection, b: &str, id: &str) -> Result<(), Error> {
    crate::ledger::account_exists(db, b, id)?;
    if db.query_row(
        "SELECT closed FROM accounts WHERE budget_id=?1 AND id=?2",
        params![b, id],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Error::conflict(
            "Reopen the account before posting its schedules",
        ));
    }
    Ok(())
}

pub(crate) fn record_entity(
    db: &Connection,
    b: &str,
    table: &str,
    id: &str,
) -> Result<Value, Error> {
    if ![
        "schedules",
        "schedule_occurrences",
        "schedule_links",
        "schedule_settings",
    ]
    .contains(&table)
    {
        return Err(Error::internal("Unknown schedule entity"));
    }
    let mut query = db.prepare(&format!(
        "SELECT * FROM {table} WHERE budget_id=?1 AND id=?2"
    ))?;
    let fields = query
        .column_names()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let value = query
        .query_row(params![b, id], |r| {
            let mut row = serde_json::Map::new();
            for (i, field) in fields.iter().enumerate() {
                let value = match r.get_ref(i)? {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(n) => json!(n),
                    rusqlite::types::ValueRef::Text(s) => json!(String::from_utf8_lossy(s)),
                    _ => return Err(rusqlite::Error::InvalidQuery),
                };
                row.insert(field.clone(), value);
            }
            Ok(Value::Object(row))
        })
        .optional()?
        .ok_or_else(Error::missing)?;
    let mut value = value;
    value["deleted"] = json!(value["tombstone"] == 1);
    Ok(value)
}
pub(crate) fn validate(db: &Connection, b: &str) -> Result<(), Error> {
    for row in data::list(db, b)? {
        crate::models::validate_id(row["id"].as_str().unwrap())?;
        let conditions = row["conditions"].as_array().unwrap();
        calendar::date_condition(conditions)?;
        if let Some(date) = row["next_date"].as_str() {
            crate::models::validate_date(date)?;
        }
        if !row["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["op"] == "link-schedule" && a["value"] == row["id"])
        {
            return Err(Error::invalid("Schedule is missing its linked rule action"));
        }
        if let Some(horizon) = row["custom_upcoming_length"].as_str() {
            view::horizon_days(horizon, "2026-01-01")?;
        }
    }
    if db.query_row("SELECT EXISTS(SELECT 1 FROM schedule_links l LEFT JOIN schedules s ON s.budget_id=l.budget_id AND s.id=l.schedule_id LEFT JOIN transactions t ON t.budget_id=l.budget_id AND t.id=l.id WHERE l.budget_id=?1 AND (s.id IS NULL OR t.id IS NULL))",[b],|r|r.get::<_,bool>(0))?{return Err(Error::invalid("Schedule link has a missing schedule or transaction"));}
    Ok(())
}
pub(crate) fn archive_tables() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            "schedules",
            vec![
                "budget_id",
                "id",
                "rule_id",
                "name",
                "posts_transaction",
                "completed",
                "next_date",
                "custom_upcoming_length",
                "tombstone",
            ],
        ),
        (
            "schedule_occurrences",
            vec![
                "budget_id",
                "id",
                "schedule_id",
                "occurrence_date",
                "status",
                "transaction_id",
                "tombstone",
            ],
        ),
        (
            "schedule_links",
            vec!["budget_id", "id", "schedule_id", "tombstone"],
        ),
        (
            "schedule_settings",
            vec!["budget_id", "id", "value", "tombstone"],
        ),
    ]
}
