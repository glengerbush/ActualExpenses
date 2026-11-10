//! Rules own parsing/order/evaluation; guarded ledger seams own financial writes.
mod actions;
pub(crate) use expressions::Budget as ExpressionBudget;
pub(crate) fn template_actions(
    db: &rusqlite::Connection,
    b: &str,
    row: &serde_json::Value,
    rule: &Rule,
    budget: &mut ExpressionBudget,
) -> Result<serde_json::Value, crate::error::Error> {
    engine::validate_with_budget(rule, budget)?;
    let mut context = if rule.actions.iter().any(expressions::calculated) {
        Some(budget.read(db, || {
            let mut context = expressions::Context::new(db, b, row, false, None)?;
            context.prefetch(db, b, std::iter::once(rule))?;
            context.begin_rule(db, b, row, rule)?;
            Ok(context)
        })?)
    } else {
        None
    };
    actions::apply(db, b, row, rule, context.as_mut(), budget)
}
mod conditions;
mod engine;
mod entry;
mod expressions;
mod import;
pub mod learning;
mod plan;
mod split_preview;
use crate::{concurrency as sync, error::Error, store::Store};
use axum::{
    Json, Router,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
pub(crate) use engine::{Runner, Work};
pub(crate) use entry::save_import;
pub(crate) fn import_runner(db: &Connection, b: &str, work: Work) -> Result<Runner, Error> {
    Ok(Runner::with_work(load(db, b)?, work))
}
pub(crate) use engine::validate as validate_definition;
pub(crate) use entry::post_schedule;
pub use import::{
    convert as convert_actual, enrich as enrich_legacy_imports, import_clock,
    install as install_actual, validate,
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
        .route("/api/v1/budgets/{budget}/rules/apply", post(apply))
        .route(
            "/api/v1/budgets/{budget}/rules/transactions",
            post(entry::create),
        )
        .route(
            "/api/v1/budgets/{budget}/rules/rename-payee",
            post(rename_payee),
        )
        .route(
            "/api/v1/budgets/{budget}/rules/learning",
            post(set_learning),
        )
        .route(
            "/api/v1/budgets/{budget}/rules/associations",
            get(read_associations),
        )
        .route(
            "/api/v1/budgets/{budget}/rules/{id}",
            get(read_one).patch(update).delete(remove),
        )
}
pub fn load(db: &Connection, b: &str) -> Result<Vec<Rule>, Error> {
    let mut q=db.prepare("SELECT id,stage,conditions_op,conditions,actions FROM rules WHERE budget_id=?1 AND tombstone=0 LIMIT 100001")?;
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
    if rules.len() > 100000 {
        return Err(Error::invalid("Rule set exceeds100000 saved rules"));
    }
    // Resolve real merges once per load. Never normalize an unreplaced removal
    // to null: that would broaden equality into an uncategorized predicate.
    let mut redirects = std::collections::BTreeMap::new();
    for (field, table, column) in [
        ("payee", "payees", "merged_into"),
        ("category", "categories", "replacement_id"),
    ] {
        let mut q = db.prepare(&format!(
            "SELECT id,{column} FROM {table} WHERE budget_id=?1 AND {column} IS NOT NULL"
        ))?;
        for row in q.query_map([b], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (id, target) = row?;
            redirects.insert((field.to_owned(), id), target);
        }
    }
    for rule in &mut rules {
        for item in rule.conditions.iter_mut().chain(rule.actions.iter_mut()) {
            let field = item["field"].as_str().unwrap_or("").to_owned();
            if !matches!(field.as_str(), "payee" | "category") {
                continue;
            }
            let canonical = |value: &mut Value| -> Result<(), Error> {
                let Some(mut id) = value.as_str().map(str::to_owned) else {
                    return Ok(());
                };
                let mut seen = std::collections::BTreeSet::new();
                while let Some(next) = redirects.get(&(field.clone(), id.clone())) {
                    if !seen.insert(id) {
                        return Err(Error::invalid("Cyclic rule reference aliases"));
                    }
                    id = next.clone();
                }
                *value = json!(id);
                Ok(())
            };
            if let Some(values) = item["value"].as_array_mut() {
                for value in values {
                    canonical(value)?;
                }
            } else {
                canonical(&mut item["value"])?;
            }
        }
    }
    engine::rank(&mut rules);
    Ok(rules)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListQuery {
    snapshot: Option<String>,
    payee_id: Option<String>,
}
async fn list(
    State(store): State<Store>,
    Path(b): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    if let Some(id) = &query.payee_id {
        crate::models::validate_id(id)?;
    }
    Ok(Json(store.run(move|db|{let tx=db.transaction()?;crate::store::require_budget(&tx,&b)?;sync::check_snapshot(&tx,&b,query.snapshot.as_deref())?;let rules=load(&tx,&b)?.into_iter().filter(|r|query.payee_id.as_ref().is_none_or(|id|r.conditions.iter().chain(r.actions.iter()).any(|v|v["field"]=="payee"&&(v["value"]==*id||v["value"].as_array().is_some_and(|values|values.contains(&json!(id))))))).collect::<Vec<_>>();Ok(json!({"data":rules,"snapshot":sync::cursor(&tx,&b)?,"epoch":crate::lifecycle::epoch(&tx,&b)?,"learning_enabled":learning::enabled(&tx,&b)?,"edit_clock":tx.query_row("SELECT clock FROM budget_sync WHERE budget_id=?1",[&b],|r|r.get::<_,String>(0))?}))}).await?))
}
async fn create(
    State(store): State<Store>,
    Path(b): Path<String>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let Json(mut body) = input.map_err(crate::api::json_error)?;
    crate::models::validate_id(&b)?;
    let key = crate::api::idempotency_key(&headers)?;
    let owner = crate::history::owner_header(&headers)?;
    let proposed = sync::header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let cancel = sync::cancel_header(&headers)?;
    let lookup = sync::receipt_only_header(&headers)?;
    let payload = body.to_string();
    store.run(move|db|{let scope=format!("POST /budgets/{b}/rules");if lookup{return sync::receipt_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::CREATED,owner.as_ref());}sync::write_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::CREATED,owner.as_ref(),|db|{
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
    input: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    let Json(body) = input.map_err(crate::api::json_error)?;
    crate::models::validate_id(&b)?;
    Ok(Json(
        store
            .run(move |db| {
                let tx = db.transaction()?;
                crate::store::require_budget(&tx, &b)?;
                sync::check_snapshot(&tx, &b, Some(snapshot(&body)?))?;
                Ok(json!({"data":plan::preview(&tx,&b,&body)?}))
            })
            .await?,
    ))
}
async fn apply(
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
    store.run(move|db|{let scope=format!("POST /budgets/{b}/rules/apply");if lookup{return sync::receipt_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref());}sync::write_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref(),|db|{
 if cancel{return sync::canceled(db,&b);}crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;sync::check_snapshot(db,&b,Some(snapshot(&body)?))?;let clock=sync::clock(db,&b,proposed.as_deref())?;let data=plan::apply(db,&b,&body,&clock,epoch.as_deref())?;
 Ok(json!({"response_status":200,"response_body":{"data":data,"snapshot":sync::cursor(db,&b)?,"edit_clock":clock}}))
 })}).await
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
    let raw=db.query_row("SELECT stage,conditions_op,conditions,actions,tombstone FROM rules WHERE budget_id=?1 AND id=?2",params![b,id],|r|Ok((r.get::<_,Option<String>>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,bool>(4)?))).optional()?.ok_or_else(Error::missing)?;
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
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let Json(body) = input.map_err(crate::api::json_error)?;
    change(store, b, id, headers, body, false).await
}
async fn remove(
    State(store): State<Store>,
    Path((b, id)): Path<(String, String)>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let Json(body) = input.map_err(crate::api::json_error)?;
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
 let linked:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM schedules WHERE budget_id=?1 AND rule_id=?2 AND tombstone=0)",params![b,id],|r|r.get(0))?;
 if linked{return Err(Error::conflict("Edit or delete this rule through Scheduled transactions so its calendar remains consistent."));}
 let clock=sync::clock(db,&b,proposed.as_deref())?;let mut value=old;value.as_object_mut().unwrap().remove("field_versions");value.as_object_mut().unwrap().remove("deleted");let mut fields=vec![];
 if delete{let stale:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM field_versions WHERE budget_id=?1 AND entity='rules' AND entity_id=?2 AND clock>=?3)",params![b,id,clock],|r|r.get(0))?;if stale{return Err(Error::conflict("A newer rule edit prevents deleting it"));}guard(db,&b,"rules",&id,"tombstone",&clock)?;db.execute("UPDATE rules SET tombstone=1 WHERE budget_id=?1 AND id=?2",params![b,id])?;fields.push("tombstone".to_string());}else{
 for (field,n) in object{if field=="snapshot"{continue;}let column=if field=="conditionsOp"{"conditions_op"}else{field};guard(db,&b,"rules",&id,column,&clock)?;value[field]=n.clone();fields.push(column.to_string());}
 if fields.is_empty(){return Err(Error::invalid("Choose a rule field to change"));}let rule:Rule=serde_json::from_value(value).map_err(|_|Error::invalid("Invalid rule object"))?;engine::validate(&rule)?;
 db.execute("UPDATE rules SET stage=?3,conditions_op=?4,conditions=?5,actions=?6 WHERE budget_id=?1 AND id=?2",params![b,id,rule.stage,rule.conditions_op,serde_json::to_string(&rule.conditions).map_err(Error::internal)?,serde_json::to_string(&rule.actions).map_err(Error::internal)?])?;
 }
 for field in fields{sync::seed_field_version(db,&b,"rules",&id,&field,&clock)?;}sync::mark(db,&b,"rules",&id)?;
 Ok(json!({"response_status":200,"response_body":{"data":record(db,&b,&id)?,"snapshot":sync::cursor(db,&b)?,"edit_clock":clock}}))
 })}).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    snapshot: Option<String>,
}
async fn read_associations(
    State(store): State<Store>,
    Path(b): Path<String>,
    axum::extract::Query(query): axum::extract::Query<Checkpoint>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    Ok(Json(store.run(move|db|{let tx=db.transaction()?;crate::store::require_budget(&tx,&b)?;sync::check_snapshot(&tx,&b,query.snapshot.as_deref())?;Ok(json!({"data":learning::associations(&tx,&b)?,"snapshot":sync::cursor(&tx,&b)?,"epoch":crate::lifecycle::epoch(&tx,&b)?,"learning_enabled":learning::enabled(&tx,&b)?}))}).await?))
}
async fn set_learning(
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
    store.run(move|db|{let scope=format!("POST /budgets/{b}/rules/learning");if lookup{return sync::receipt_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref());}sync::write_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref(),|db|{
 if cancel{return sync::canceled(db,&b);}crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;sync::check_snapshot(db,&b,Some(snapshot(&body)?))?;if body.as_object().is_none_or(|o|o.keys().any(|k|!["snapshot","enabled","payee_id"].contains(&k.as_str()))){return Err(Error::invalid("Unknown learning setting"));}let enabled=body["enabled"].as_bool().ok_or_else(||Error::invalid("Choose whether to learn categories"))?;let clock=sync::clock(db,&b,proposed.as_deref())?;
 let value=if let Some(payee)=body["payee_id"].as_str(){crate::models::validate_id(payee)?;let p=crate::payees_tags::record(db,&b,"payees",payee)?;if p["deleted"]==true||!p["transfer_account_id"].is_null(){return Err(Error::invalid("Choose an ordinary active payee"));}guard(db,&b,"payees",payee,"learn_categories",&clock)?;db.execute("UPDATE payees SET learn_categories=?3 WHERE budget_id=?1 AND id=?2",params![b,payee,enabled])?;sync::seed_field_version(db,&b,"payees",payee,"learn_categories",&clock)?;sync::mark(db,&b,"payees",payee)?;json!({"payee_id":payee,"enabled":enabled})}else{if !body["payee_id"].is_null(){return Err(Error::invalid("Invalid payee"));}guard(db,&b,"rule_settings","learning","enabled",&clock)?;db.execute("UPDATE rule_settings SET enabled=?2 WHERE budget_id=?1 AND id='learning'",params![b,enabled])?;sync::seed_field_version(db,&b,"rule_settings","learning","enabled",&clock)?;sync::mark(db,&b,"rule_settings","learning")?;json!({"enabled":enabled})};
 Ok(json!({"response_status":200,"response_body":{"data":value,"snapshot":sync::cursor(db,&b)?,"edit_clock":clock}}))
 })}).await
}

async fn rename_payee(
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
    store.run(move|db|{
        let scope=format!("POST /budgets/{b}/rules/rename-payee");
        if lookup{return sync::receipt_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref());}
        sync::write_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref(),|db|{
            if cancel{return sync::canceled(db,&b);}
            crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;
            sync::check_snapshot(db,&b,Some(snapshot(&body)?))?;
            if body.as_object().is_none_or(|o|o.keys().any(|k|!["snapshot","payee_id","imported_payees"].contains(&k.as_str()))){return Err(Error::invalid("Unknown rename rule field"));}
            let payee=body["payee_id"].as_str().ok_or_else(||Error::invalid("Choose a payee"))?;
            crate::models::validate_id(payee)?;
            let payee=crate::payees_tags::canonical(db,&b,payee)?;
            let target=crate::payees_tags::record(db,&b,"payees",&payee)?;
            if target["deleted"]==true || !target["transfer_account_id"].is_null(){return Err(Error::invalid("Choose a live ordinary payee"));}
            let values=body["imported_payees"].as_array().filter(|v|!v.is_empty()&&v.len()<=100).ok_or_else(||Error::invalid("Provide1–100 imported names"))?;
            let mut names=vec![];
            for value in values {let value=value.as_str().filter(|s|s.chars().count()<=10000).ok_or_else(||Error::invalid("Invalid imported payee name"))?;if !value.is_empty()&&!names.contains(&json!(value)){names.push(json!(value));}}
            if names.is_empty(){return Err(Error::invalid("Provide a nonempty imported name"));}
            let existing=load(db,&b)?.into_iter().find(|r|r.stage.as_deref()==Some("pre")&&r.conditions.len()==1&&r.conditions[0]["field"]=="imported_payee"&&r.conditions[0]["op"]=="oneOf"&&r.actions.len()==1&&r.actions[0]["op"]=="set"&&r.actions[0]["field"]=="payee"&&r.actions[0]["value"]==payee);
            let clock=sync::clock(db,&b,proposed.as_deref())?;
            let id=if let Some(mut rule)=existing {
                guard(db,&b,"rules",&rule.id,"conditions",&clock)?;
                let previous=rule.conditions[0]["value"].as_array_mut().ok_or_else(||Error::invalid("Invalid rename condition"))?;
                for name in names{if !previous.contains(&name){previous.push(name);}}
                db.execute("UPDATE rules SET conditions=?3 WHERE budget_id=?1 AND id=?2",params![b,rule.id,serde_json::to_string(&rule.conditions).map_err(Error::internal)?])?;
                sync::seed_field_version(db,&b,"rules",&rule.id,"conditions",&clock)?;
                rule.id
            }else{
                let id=uuid::Uuid::new_v4().to_string();
                let conditions=json!([{"field":"imported_payee","op":"oneOf","value":names}]);
                let actions=json!([{"op":"set","field":"payee","value":payee}]);
                db.execute("INSERT INTO rules(budget_id,id,stage,conditions_op,conditions,actions) VALUES(?1,?2,'pre','and',?3,?4)",params![b,id,conditions.to_string(),actions.to_string()])?;
                for field in ["stage","conditions_op","conditions","actions","tombstone"] {sync::seed_field_version(db,&b,"rules",&id,field,&clock)?;}
                id
            };
            sync::mark(db,&b,"rules",&id)?;
            Ok(json!({"response_status":200,"response_body":{"data":record(db,&b,&id)?,"snapshot":sync::cursor(db,&b)?,"edit_clock":clock}}))
        })
    }).await
}

/// Matching and rule entry share condition semantics and one bounded work allowance.
pub(crate) fn match_schedule_conditions(
    db: &Connection,
    b: &str,
    conditions: &[Value],
    row: &Value,
    account_off: Option<bool>,
    work: &mut Work,
) -> Result<bool, Error> {
    let context = conditions::Context { db, b, account_off };
    for condition in conditions {
        if !conditions::matches(&context, condition, row, work)? {
            return Ok(false);
        }
    }
    Ok(!conditions.is_empty())
}
