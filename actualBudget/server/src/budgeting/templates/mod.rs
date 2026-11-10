mod calculators;
mod cleanup;
mod configuration;
mod context;
mod currency_lineage;
mod engine;
mod migration;
mod render;
pub(super) use migration::preview as migration_preview;
mod parse;
mod storage;
mod validation;
pub(super) use configuration::read;
mod schedule;
use crate::{error::Error, store::Store};
use axum::{
    Json,
    extract::{Path, State},
};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
pub use storage::{clock, convert, enrich, enrich_archive, install, record};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Notes {
    notes: String,
}
pub(super) async fn parse_notes(
    State(store): State<Store>,
    Path(b): Path<String>,
    input: Result<Json<Notes>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>, Error> {
    let Json(body) = input.map_err(crate::api::json_error)?;
    crate::models::validate_id(&b)?;
    store
        .run(move |db| {
            crate::store::require_budget(db, &b)?;
            Ok(serde_json::json!({"data":parse::notes(&body.notes)?}))
        })
        .await
        .map(Json)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Preview {
    month: String,
    snapshot: String,
    category_ids: Option<Vec<String>>,
    force: Option<bool>,
    action: Option<String>,
}
pub(super) async fn preview(
    State(store): State<Store>,
    Path(b): Path<String>,
    input: Result<Json<Preview>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>, Error> {
    let Json(body) = input.map_err(crate::api::json_error)?;
    crate::models::validate_id(&b)?;
    store
        .run(move |db| {
            let tx = db.transaction()?;
            crate::store::require_budget(&tx, &b)?;
            crate::concurrency::check_snapshot(&tx, &b, Some(&body.snapshot))?;
            if let Some(action) = body.action {
                if action != "cleanup" || body.category_ids.is_some() || body.force.is_some() {
                    return Err(Error::invalid(
                        "Cleanup previews use only action, month and snapshot",
                    ));
                }
                return Ok(json!({"data":cleanup::compute(&tx,&b,&body.month)?}));
            }
            let mut input = json!({"month":body.month,"force":body.force.unwrap_or(false)});
            if let Some(ids) = body.category_ids {
                input["category_ids"] = json!(ids);
            }
            Ok(json!({"data":engine::compute(&tx,&b,&input)?}))
        })
        .await
        .map(Json)
}
pub(super) async fn mutate(
    State(store): State<Store>,
    Path(b): Path<String>,
    headers: axum::http::HeaderMap,
    input: Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> Result<(axum::http::StatusCode, Json<Value>), Error> {
    use crate::concurrency as sync;
    use axum::http::StatusCode;
    crate::models::validate_id(&b)?;
    let Json(body) = input.map_err(crate::api::json_error)?;
    let key = crate::api::idempotency_key(&headers)?;
    let owner = crate::history::owner_header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let proposed = sync::header(&headers)?;
    let lookup = sync::receipt_only_header(&headers)?;
    let cancel = sync::cancel_header(&headers)?;
    let payload = body.to_string();
    store.run(move|db|{let scope=format!("POST /budgets/{b}/budget/templates/commands");if lookup{return sync::receipt_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref());}
 sync::write_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref(),|db|{
  if cancel{return sync::canceled(db,&b);}crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;sync::check_snapshot(db,&b,Some(body["snapshot"].as_str().ok_or_else(||Error::invalid("A current snapshot is required"))?))?;
  let fields:&[&str]=if body["action"]=="configure"{&["action","snapshot","category_id","source","templates","cleanup"]}else if body["action"]=="migrate"{&["action","month","snapshot","category_id","target_source","notes"]}else if body["action"]=="cleanup"{&["action","month","snapshot"]}else{&["action","month","snapshot","category_ids"]};
  if body.as_object().is_none_or(|o|o.keys().any(|k|!fields.contains(&k.as_str()))){return Err(Error::invalid("Unknown template command field"));}
  if body["action"]=="configure"{let clock=sync::clock(db,&b,proposed.as_deref())?;let data=configuration::configure(db,&b,&body,&clock)?;return Ok(json!({"response_status":200,"response_body":{"data":data,"edit_clock":clock,"cursor":sync::cursor(db,&b)?}}));}
  if body["action"]=="migrate"{let clock=sync::clock(db,&b,proposed.as_deref())?;let data=migration::apply(db,&b,&body,&clock)?;return Ok(json!({"response_status":200,"response_body":{"data":data,"edit_clock":clock,"cursor":sync::cursor(db,&b)?}}));}
  if !matches!(body["action"].as_str(),Some("apply"|"overwrite"|"cleanup")){return Err(Error::invalid("Unknown template action"));}
  let is_cleanup=body["action"]=="cleanup";
  let plan=if is_cleanup{cleanup::compute(db,&b,body["month"].as_str().ok_or_else(||Error::invalid("month is required"))?)?}else{engine::compute(db,&b,&body)?};if !plan["errors"].as_array().unwrap().is_empty(){return Err(Error::invalid(format!("Template validation failed: {}",plan["errors"])));}
  let clock=sync::clock(db,&b,proposed.as_deref())?;let month=plan["month"].as_str().unwrap();let method=plan["method"].as_str().unwrap();
  if is_cleanup{cleanup::persist_definitions(db,&b,&plan,&clock)?;}
  for row in plan["categories"].as_array().unwrap(){let id=row["id"].as_str().unwrap();if !is_cleanup{storage::save_definition(db,&b,id,&row["templates"],&clock)?;}
  if row["applies"]==true||row["clear_goal"]==true{
   if row["budgeted"]!=row["before_budgeted"]{super::actions::put(db,&b,method,month,id,row["budgeted"].as_i64().unwrap() as i128,&clock)?;}
   let cell=format!("{method}:{month}:{id}");for field in ["goal","long_goal"]{super::actions::guard_field(db,&b,"budget_cells",&cell,field,&clock)?;}super::actions::ensure_cell(db,&b,method,month,id)?;
   db.execute("UPDATE budget_cells SET goal=?3,long_goal=?4 WHERE budget_id=?1 AND id=?2",rusqlite::params![b,cell,if row["goal"].is_null(){None}else{Some(row["goal"].to_string())},if row["long_goal"].is_null(){None}else{Some(row["long_goal"].to_string())}])?;
   for field in ["goal","long_goal"]{sync::seed_field_version(db,&b,"budget_cells",&cell,field,&clock)?;}sync::mark(db,&b,"budget_cells",&cell)?;
  }}
  let mut response=json!({"data":super::view(db,&b,month,1)?,"edit_clock":clock,"cursor":sync::cursor(db,&b)?});if is_cleanup{response["notification"]=plan["notification"].clone();}Ok(json!({"response_status":200,"response_body":response}))
 })}).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Projection {
    month: String,
    snapshot: String,
    category_id: String,
    templates: Vec<Value>,
}
pub(super) async fn projection(
    State(store): State<Store>,
    Path(b): Path<String>,
    input: Result<Json<Projection>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>, Error> {
    let Json(body) = input.map_err(crate::api::json_error)?;
    crate::models::validate_id(&b)?;
    crate::models::validate_id(&body.category_id)?;
    validation::templates(&body.templates)?;
    store.run(move|db|{let tx=db.transaction()?;crate::concurrency::check_snapshot(&tx,&b,Some(&body.snapshot))?;let mut overrides=json!({});overrides[&body.category_id]=json!(body.templates);let plan=engine::compute(&tx,&b,&json!({"month":body.month,"category_ids":[body.category_id],"force":true,"projection":true,"template_overrides":overrides}))?;
 let row=plan["categories"].as_array().unwrap().iter().find(|r|r["id"]==body.category_id).unwrap();let failed=!plan["errors"].as_array().unwrap().is_empty();Ok(json!({"data":{"category_id":body.category_id,"budgeted":if failed{json!(0)}else{row["budgeted"].clone()},"per_template":if failed{json!(vec![0;body.templates.len()])}else{row["per_template"].clone()},"goal":row["goal"],"long_goal":row["long_goal"],"errors":plan["errors"],"snapshot":plan["snapshot"],"epoch":plan["epoch"]}}))}).await.map(Json)
}
