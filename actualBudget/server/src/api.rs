use crate::{
    error::Error,
    models::{NewAccount, NewBudget, validate_id},
    store::{self, Store},
};
use axum::{
    Json, Router,
    extract::{
        DefaultBodyLimit, Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
    routing::{any, get},
};
use serde_json::{Value, json};

pub fn router(store: Store) -> Router {
    Router::new()
        .route("/api/v1/info", get(info))
        .route("/api/v1/openapi.json", get(contract))
        .route("/api/v1/budgets", get(list_budgets).post(create_budget))
        .route(
            "/api/v1/budgets/{budget}/accounts",
            get(list_accounts).post(create_account),
        )
        .route(
            "/api/v1/budgets/{budget}/transactions",
            get(list_transactions),
        )
        .route("/api", any(|| async { Error::missing() }))
        .route("/api/{*rest}", any(|| async { Error::missing() }))
        .method_not_allowed_fallback(|| async {
            Error::new(
                StatusCode::METHOD_NOT_ALLOWED,
                "method_not_allowed",
                "HTTP method not supported",
            )
        })
        .layer(DefaultBodyLimit::max(65536))
        .with_state(store)
}
fn data(value: Value) -> Json<Value> {
    Json(json!({"data":value}))
}
async fn info() -> Json<Value> {
    data(json!({
        "name": "actualBudget",
        "version": env!("CARGO_PKG_VERSION"),
        "api_version": 1,
        "authentication": "none",
        "capabilities": ["budgets", "accounts", "opening_balances", "idempotency"],
        "openapi": "/api/v1/openapi.json"
    }))
}
async fn contract() -> Json<Value> {
    Json(serde_json::from_str(include_str!("openapi.json")).expect("valid embedded OpenAPI"))
}
async fn list_budgets(State(store): State<Store>) -> Result<Json<Value>, Error> {
    Ok(data(store.run(|db| store::budgets(db)).await?))
}
async fn list_accounts(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<Value>, Error> {
    let budget = budget_id(path)?;
    Ok(data(
        store.run(move |db| store::accounts(db, &budget)).await?,
    ))
}
async fn create_budget(
    State(store): State<Store>,
    headers: HeaderMap,
    input: Result<Json<NewBudget>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let Json(input) = input.map_err(json_error)?;
    input.validate()?;
    let key = idempotency_key(&headers)?;
    let payload = serde_json::to_string(&input).map_err(Error::internal)?;
    let result = store
        .run(move |db| {
            store::write(db, "POST /budgets", &key, &payload, |db| {
                store::create_budget(db, input)
            })
        })
        .await?;
    Ok((StatusCode::CREATED, data(result)))
}
async fn create_account(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<NewAccount>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let budget = budget_id(path)?;
    let Json(input) = input.map_err(json_error)?;
    input.validate()?;
    let key = idempotency_key(&headers)?;
    let payload = serde_json::to_string(&input).map_err(Error::internal)?;
    let result = store
        .run(move |db| {
            store::write(
                db,
                &format!("POST /budgets/{budget}/accounts"),
                &key,
                &payload,
                |db| store::create_account(db, &budget, input),
            )
        })
        .await?;
    Ok((StatusCode::CREATED, data(result)))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TransactionQuery {
    account_id: String,
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default)]
    offset: u32,
}
fn default_limit() -> u32 {
    100
}
async fn list_transactions(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<TransactionQuery>, QueryRejection>,
) -> Result<Json<Value>, Error> {
    let budget = budget_id(path)?;
    let Query(query) = query.map_err(|_| Error::invalid("Invalid transaction query"))?;
    validate_id(&query.account_id)?;
    if query.limit == 0 || query.limit > 1000 || query.offset > 1000000 {
        return Err(Error::invalid(
            "limit must be 1–1000; offset at most 1000000",
        ));
    }
    Ok(data(
        store
            .run(move |db| {
                store::transactions(db, &budget, &query.account_id, query.limit, query.offset)
            })
            .await?,
    ))
}
fn budget_id(path: Result<Path<String>, PathRejection>) -> Result<String, Error> {
    let Path(budget) = path.map_err(|_| Error::invalid("Invalid budget path"))?;
    validate_id(&budget)?;
    Ok(budget)
}
fn idempotency_key(headers: &HeaderMap) -> Result<String, Error> {
    let key = headers
        .get("Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Error::invalid("Idempotency-Key is required"))?;
    if key.is_empty() || key.len() > 128 || !key.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(Error::invalid(
            "Idempotency-Key must contain 1–128 visible ASCII characters",
        ));
    }
    Ok(key.into())
}
fn json_error(error: JsonRejection) -> Error {
    let status = match error.status() {
        StatusCode::PAYLOAD_TOO_LARGE => StatusCode::PAYLOAD_TOO_LARGE,
        StatusCode::UNSUPPORTED_MEDIA_TYPE => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        _ => StatusCode::BAD_REQUEST,
    };
    Error::new(
        status,
        "invalid_request",
        "Expected JSON matching the documented request schema",
    )
}
