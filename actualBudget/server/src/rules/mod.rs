//! Rules own parsing/order/evaluation; guarded ledger seams own financial writes.
mod conditions;
mod engine;
use crate::{concurrency as sync, error::Error, store::Store};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    pub stage: Option<String>,
    #[serde(rename = "conditionsOp")]
    pub conditions_op: String,
    pub conditions: Vec<Value>,
    pub actions: Vec<Value>,
}
pub fn router() -> Router<Store> {
    Router::new()
        .route("/api/v1/budgets/{budget}/rules", get(list).post(create))
        .route("/api/v1/budgets/{budget}/rules/preview", post(preview))
        .route(
            "/api/v1/budgets/{budget}/rules/{id}",
            get(read_one).patch(update).delete(remove),
        )
}
pub fn load(db: &Connection, b: &str) -> Result<Vec<Rule>, Error> {
    let mut q=db.prepare("SELECT id,stage,conditions_op,conditions,actions FROM rules WHERE budget_id=?1 AND tombstone=0")?;
    let raw = q
        .query_map([b], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut rules = raw
        .into_iter()
        .map(|(id, stage, conditions_op, c, a)| {
            Ok(Rule {
                id,
                stage,
                conditions_op,
                conditions: serde_json::from_str(&c).map_err(Error::internal)?,
                actions: serde_json::from_str(&a).map_err(Error::internal)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    engine::rank(&mut rules);
    Ok(rules)
}
async fn list(State(store): State<Store>, Path(b): Path<String>) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    Ok(Json(store.run(move|db|{crate::store::require_budget(db,&b)?;Ok(json!({"data":load(db,&b)?,"snapshot":sync::cursor(db,&b)?,"epoch":crate::lifecycle::epoch(db,&b)?}))}).await?))
}
async fn create(
    State(store): State<Store>,
    Path(b): Path<String>,
    headers: HeaderMap,
    Json(mut body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), Error> {
    crate::models::validate_id(&b)?;
    let key = crate::api::idempotency_key(&headers)?;
    let owner = crate::history::owner_header(&headers)?;
    let proposed = sync::header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let cancel = sync::cancel_header(&headers)?;
    let payload = body.to_string();
    store.run(move|db|{let scope=format!("POST /budgets/{b}/rules");sync::write_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::CREATED,owner.as_ref(),|db|{
  if cancel{return sync::canceled(db,&b);}crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;sync::check_snapshot(db,&b,Some(snapshot(&body)?))?;
  body.as_object_mut().ok_or_else(||Error::invalid("Rule object required"))?.remove("snapshot");let rule:Rule=serde_json::from_value(body.clone()).map_err(|_|Error::invalid("Invalid rule object"))?;engine::validate(&rule)?;
  let clock=sync::clock(db,&b,proposed.as_deref())?;let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM rules WHERE budget_id=?1 AND id=?2)",params![b,rule.id],|r|r.get(0))?;if exists{return Err(Error::conflict("Rule identity already exists"));}
  db.execute("INSERT INTO rules(budget_id,id,stage,conditions_op,conditions,actions) VALUES(?1,?2,?3,?4,?5,?6)",params![b,rule.id,rule.stage,rule.conditions_op,serde_json::to_string(&rule.conditions).map_err(Error::internal)?,serde_json::to_string(&rule.actions).map_err(Error::internal)?])?;
  for field in ["stage","conditions_op","conditions","actions","tombstone"]{sync::seed_field_version(db,&b,"rules",&rule.id,field,&clock)?;}sync::mark(db,&b,"rules",&rule.id)?;
  Ok(json!({"response_status":201,"response_body":{"data":rule,"snapshot":sync::cursor(db,&b)?,"edit_clock":clock}}))
 })}).await
}
async fn preview(
    State(store): State<Store>,
    Path(b): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    Ok(Json(store.run(move|db|{let tx=db.transaction()?;crate::store::require_budget(&tx,&b)?;sync::check_snapshot(&tx,&b,Some(snapshot(&body)?))?;let rules=load(&tx,&b)?;let input=&body["transaction"];if !input.is_object(){return Err(Error::invalid("Preview a transaction"));}let (after,matched)=engine::evaluate(&tx,&b,input,&rules)?;Ok(json!({"data":{"transactions":[{"before":input,"after":after,"matched_rule_ids":matched}],"snapshot":sync::cursor(&tx,&b)?}}))}).await?))
}

fn snapshot(body: &Value) -> Result<&str, Error> {
    body["snapshot"]
        .as_str()
        .ok_or_else(|| Error::invalid("A current snapshot is required"))
}
pub fn guard(
    db: &Connection,
    b: &str,
    entity: &str,
    id: &str,
    field: &str,
    clock: &str,
) -> Result<(), Error> {
    let newer:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM field_versions WHERE budget_id=?1 AND entity=?2 AND entity_id=?3 AND field=?4 AND clock>=?5)",params![b,entity,id,field,clock],|r|r.get(0))?;
    if newer {
        Err(Error::conflict(
            "A newer or equal field clock prevents this rule action. Refresh and review.",
        ))
    } else {
        Ok(())
    }
}
pub fn record(db: &Connection, b: &str, id: &str) -> Result<Value, Error> {
    let raw=db.query_row("SELECT stage,conditions_op,conditions,actions,tombstone FROM rules WHERE budget_id=?1 AND id=?2",params![b,id],|r|Ok((r.get::<_,Option<String>>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,bool>(4)?)))?;
    let mut value = json!({"id":id,"stage":raw.0,"conditionsOp":raw.1,"conditions":serde_json::from_str::<Value>(&raw.2).map_err(Error::internal)?,"actions":serde_json::from_str::<Value>(&raw.3).map_err(Error::internal)?,"deleted":raw.4});
    let mut q=db.prepare_cached("SELECT field,clock FROM field_versions WHERE budget_id=?1 AND entity='rules' AND entity_id=?2")?;
    let versions = q
        .query_map(params![b, id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<Result<std::collections::BTreeMap<_, _>, _>>()?;
    value["field_versions"] = json!(versions);
    Ok(value)
}
async fn read_one(
    State(store): State<Store>,
    Path((b, id)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    crate::models::validate_id(&id)?;
    Ok(Json(
        store
            .run(move |db| {
                crate::store::require_budget(db, &b)?;
                Ok(json!({"data":record(db,&b,&id)?,"snapshot":sync::cursor(db,&b)?}))
            })
            .await?,
    ))
}
async fn update(
    State(store): State<Store>,
    Path((b, id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), Error> {
    change(store, b, id, headers, body, false).await
}
async fn remove(
    State(store): State<Store>,
    Path((b, id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), Error> {
    change(store, b, id, headers, body, true).await
}
async fn change(
    store: Store,
    b: String,
    id: String,
    headers: HeaderMap,
    body: Value,
    delete: bool,
) -> Result<(StatusCode, Json<Value>), Error> {
    crate::models::validate_id(&b)?;
    crate::models::validate_id(&id)?;
    let key = crate::api::idempotency_key(&headers)?;
    let owner = crate::history::owner_header(&headers)?;
    let proposed = sync::header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let cancel = sync::cancel_header(&headers)?;
    let lookup = sync::receipt_only_header(&headers)?;
    let payload = body.to_string();
    store.run(move|db|{let scope=format!("{} /budgets/{b}/rules/{id}",if delete{"DELETE"}else{"PATCH"});if lookup{return sync::receipt_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref());}sync::write_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref(),|db|{
 if cancel{return sync::canceled(db,&b);}crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;sync::check_snapshot(db,&b,Some(snapshot(&body)?))?;let old=record(db,&b,&id)?;if old["deleted"]==true{return Err(Error::missing());}
 let object=body.as_object().ok_or_else(||Error::invalid("Rule changes required"))?;if object.keys().any(|k|!if delete{["snapshot"].as_slice()}else{["snapshot","stage","conditionsOp","conditions","actions"].as_slice()}.contains(&k.as_str())){return Err(Error::invalid("Unknown rule field"));}
 let clock=sync::clock(db,&b,proposed.as_deref())?;let mut value=old;value.as_object_mut().unwrap().remove("field_versions");value.as_object_mut().unwrap().remove("deleted");let mut fields=vec![];
 if delete{guard(db,&b,"rules",&id,"tombstone",&clock)?;db.execute("UPDATE rules SET tombstone=1 WHERE budget_id=?1 AND id=?2",params![b,id])?;fields.push("tombstone".to_string());}else{
 for (field,n) in object{if field=="snapshot"{continue;}let column=if field=="conditionsOp"{"conditions_op"}else{field};guard(db,&b,"rules",&id,column,&clock)?;value[field]=n.clone();fields.push(column.to_string());}
 if fields.is_empty(){return Err(Error::invalid("Choose a rule field to change"));}let rule:Rule=serde_json::from_value(value).map_err(|_|Error::invalid("Invalid rule object"))?;engine::validate(&rule)?;
 db.execute("UPDATE rules SET stage=?3,conditions_op=?4,conditions=?5,actions=?6 WHERE budget_id=?1 AND id=?2",params![b,id,rule.stage,rule.conditions_op,serde_json::to_string(&rule.conditions).map_err(Error::internal)?,serde_json::to_string(&rule.actions).map_err(Error::internal)?])?;
 }
 for field in fields{sync::seed_field_version(db,&b,"rules",&id,&field,&clock)?;}sync::mark(db,&b,"rules",&id)?;
 Ok(json!({"response_status":200,"response_body":{"data":record(db,&b,&id)?,"snapshot":sync::cursor(db,&b)?,"edit_clock":clock}}))
 })}).await
}
