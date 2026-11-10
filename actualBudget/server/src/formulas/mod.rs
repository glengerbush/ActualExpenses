//! Server-owned spreadsheet expressions and their bounded financial context.
mod context;
mod runtime;
use crate::{
    concurrency,
    error::Error,
    store::{self, Store},
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State, rejection::JsonRejection},
    routing::post,
};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    snapshot: String,
    as_of: String,
    formula: String,
    #[serde(default)]
    locale: Option<String>,
    #[serde(default = "object")]
    queries: Value,
    #[serde(default = "object")]
    named_expressions: Value,
}
fn object() -> Value {
    json!({})
}
pub fn router() -> Router<Store> {
    Router::new().route(
        "/api/v1/budgets/{budget}/reports/formula",
        post(read).layer(DefaultBodyLimit::max(65536)),
    )
}
async fn read(
    State(store): State<Store>,
    Path(b): Path<String>,
    input: Result<Json<Input>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    let Json(input) = input.map_err(crate::api::json_error)?;
    crate::models::validate_date(&input.as_of)?;
    if input.formula.len() > 8192
        || !input.queries.is_object()
        || !input.named_expressions.is_object()
        || input.queries.to_string().len() > 32768
        || input.named_expressions.to_string().len() > 32768
    {
        return Err(Error::invalid("Formula input exceeds supported limits"));
    }
    if input.queries.as_object().unwrap().len() > 32
        || input.named_expressions.as_object().unwrap().len() > 512
        || input.locale.as_ref().is_some_and(|v| v.len() > 100)
    {
        return Err(Error::invalid(
            "Formula permits32 queries,512 named values and a100-byte locale",
        ));
    }
    for (name, value) in input.named_expressions.as_object().unwrap() {
        if name.len() > 100 || !(value.is_number() || value.is_string()) {
            return Err(Error::invalid(
                "Named expressions require text or numeric values",
            ));
        }
    }
    for (name, config) in input.queries.as_object().unwrap() {
        if name.len() > 512
            || !config.is_object()
            || config
                .as_object()
                .unwrap()
                .keys()
                .any(|k| !matches!(k.as_str(), "conditions" | "conditionsOp" | "timeFrame"))
            || config
                .get("conditions")
                .is_some_and(|v| !v.is_array() || v.as_array().unwrap().len() > 20)
            || config
                .get("timeFrame")
                .is_some_and(|v| !v.is_null() && !v.is_object())
        {
            return Err(Error::invalid(
                "Invalid named query shape or condition limit",
            ));
        }
    }
    let result=store.run(move|db|{
        let tx=db.transaction()?;store::require_budget(&tx,&b)?;
        let snapshot=concurrency::cursor(&tx,&b)?;
        if input.snapshot!=snapshot{return Err(Error::conflict("Formula snapshot changed; refresh before evaluating"));}
        let today=chrono::NaiveDate::parse_from_str(&input.as_of,"%Y-%m-%d").map_err(Error::internal)?;
        let output=crate::reports::formula_work(&tx,||crate::register::with_report_matcher(&tx,|matcher|{
            let mut context=context::Context{db:&tx,budget:&b,queries:&input.queries,today,work:500000,prefetch_values:0,matcher};
            runtime::evaluate(&json!({"formula":input.formula,"named_expressions":input.named_expressions,"as_of":input.as_of,"locale":input.locale.as_deref().unwrap_or("en-US")}),|requests|context.prefetch(requests))
        }))?;
        Ok(json!({"output":output,"snapshot":snapshot,"as_of":input.as_of,"locale":input.locale.as_deref().unwrap_or("en-US")}))
    }).await?;
    Ok(Json(json!({"data":result})))
}
