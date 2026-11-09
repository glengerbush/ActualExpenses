use crate::{
    api::{budget_id, idempotency_key, json_error},
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
    http::{HeaderMap, StatusCode},
    routing::get,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub fn router() -> Router<Store> {
    Router::new()
        .route("/api/v1/budgets/{budget}/payees", get(list).post(create))
        .route(
            "/api/v1/budgets/{budget}/categories",
            get(list).post(create),
        )
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NewReference {
    id: String,
    name: String,
    #[serde(default)]
    is_income: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default)]
    offset: u32,
    snapshot: Option<String>,
}
fn default_limit() -> u32 {
    100
}
async fn list(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    OriginalUri(uri): OriginalUri,
    page: Result<Query<Page>, QueryRejection>,
) -> Result<Json<Value>, Error> {
    let budget = budget_id(path)?;
    let Query(page) = page.map_err(|_| Error::invalid("Invalid reference pagination"))?;
    if !(1..=1000).contains(&page.limit) || page.offset > 1000000 {
        return Err(Error::invalid("Invalid reference pagination"));
    }
    let categories = uri.path().ends_with("/categories");
    let result=store.run(move|db| {
        crate::concurrency::check_snapshot(db,&budget,page.snapshot.as_deref())?;
        let sql=if categories {"SELECT id,name,is_income FROM categories WHERE budget_id=?1 ORDER BY name,id LIMIT ?2 OFFSET ?3"} else {"SELECT id,name FROM payees WHERE budget_id=?1 ORDER BY name,id LIMIT ?2 OFFSET ?3"};
        let mut query=db.prepare(sql)?;
        let rows=query.query_map(params![budget,page.limit+1,page.offset],|r| {
            let mut value=json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?});
            if categories {value["is_income"]=json!(r.get::<_,bool>(2)?);} Ok(value)
        })?;
        let mut rows=rows.collect::<Result<Vec<_>,_>>()?;let more=rows.len()>page.limit as usize;rows.truncate(page.limit as usize);
        Ok(json!({"data":rows,"next_offset":if more{Some(page.offset+page.limit)}else{None}}))
    }).await?;
    Ok(Json(result))
}
async fn create(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    input: Result<Json<NewReference>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let budget = budget_id(path)?;
    let Json(input) = input.map_err(json_error)?;
    validate_id(&input.id)?;
    validate_name(&input.name)?;
    let categories = uri.path().ends_with("/categories");
    if !categories && input.is_income.is_some() {
        return Err(Error::invalid("Payees cannot specify is_income"));
    }
    let key = idempotency_key(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let payload = serde_json::to_string(&input).map_err(Error::internal)?;
    let table = if categories { "categories" } else { "payees" };
    let result=store.run(move|db|store::write(db,&format!("POST /budgets/{budget}/{table}"),&key,&payload,|db| {
        crate::lifecycle::require_epoch(db,&budget,epoch.as_deref())?;
        if db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE budget_id=?1 AND id=?2)"),params![budget,input.id],|r|r.get::<_,bool>(0))? {return Err(Error::conflict("Reference ID already exists"));}
        let mut result=json!({"id":input.id,"name":input.name});
        if categories {
            let income=input.is_income.unwrap_or(false);
            db.execute("INSERT INTO categories(budget_id,id,name,is_income) VALUES(?1,?2,?3,?4)",params![budget,input.id,input.name,income])?;result["is_income"]=json!(income);
        } else {db.execute("INSERT INTO payees(budget_id,id,name) VALUES(?1,?2,?3)",params![budget,input.id,input.name])?;}
        Ok(result)
    })).await?;
    Ok((StatusCode::CREATED, Json(json!({"data":result}))))
}
