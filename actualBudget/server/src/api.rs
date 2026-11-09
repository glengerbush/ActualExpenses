use crate::{
    concurrency::{self, Mutation},
    error::Error,
    ledger::{self, NewTransaction},
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
    response::IntoResponse,
    routing::{any, get},
};
use serde_json::{Value, json};

pub fn router(store: Store) -> Router {
    Router::new()
        .route(
            "/api/v1/amounts/evaluate",
            axum::routing::post(crate::calculator::evaluate),
        )
        .route("/api/v1/info", get(info))
        .route("/api/v1/openapi.json", get(contract))
        .route("/api/v1/budgets", get(list_budgets).post(create_budget))
        .route(
            "/api/v1/budgets/{budget}/accounts",
            get(list_accounts).post(create_account),
        )
        .route(
            "/api/v1/budgets/{budget}/transactions",
            get(list_transactions).post(create_transaction),
        )
        .route(
            "/api/v1/budgets/{budget}/transactions/{id}",
            get(get_transaction)
                .patch(update_transaction)
                .delete(delete_transaction),
        )
        .route(
            "/api/v1/budgets/{budget}/transactions/{id}/duplicate",
            axum::routing::post(duplicate_transaction),
        )
        .merge(crate::references::router())
        .merge(crate::preferences::router())
        .merge(concurrency::router())
        .route("/api", any(|| async { Error::missing() }))
        .route("/api/{*rest}", any(|| async { Error::missing() }))
        .layer(DefaultBodyLimit::max(65536))
        .merge(crate::imports::router())
        .merge(crate::themes::router())
        .method_not_allowed_fallback(|| async {
            Error::new(
                StatusCode::METHOD_NOT_ALLOWED,
                "method_not_allowed",
                "HTTP method not supported",
            )
        })
        .layer(axum::middleware::from_fn(cancellation_scope))
        .with_state(store)
}
async fn cancellation_scope(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if request
        .headers()
        .contains_key("X-Actual-Cancel-If-Uncommitted")
    {
        if let Err(error) = concurrency::cancel_header(request.headers()) {
            return error.into_response();
        }
        let path = request
            .extensions()
            .get::<axum::extract::MatchedPath>()
            .map(|path| path.as_str())
            .unwrap_or("");
        if !matches!(
            (request.method().as_str(), path),
            ("POST", "/api/v1/budgets/{budget}/transactions")
                | (
                    "PATCH" | "DELETE",
                    "/api/v1/budgets/{budget}/transactions/{id}"
                )
                | (
                    "POST",
                    "/api/v1/budgets/{budget}/transactions/{id}/duplicate"
                )
        ) {
            return Error::invalid("Cancellation is supported only for transaction mutations")
                .into_response();
        }
    }
    next.run(request).await
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
        "capabilities": ["budgets", "accounts", "opening_balances", "idempotency", "transactions", "payees", "categories", "amount_calculator", "preferences", "field_ordering", "change_cursors", "transaction_cancellation", "custom_themes", "actual_imports"],
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
    account_id: Option<String>,
    as_of: Option<String>,
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default)]
    offset: u32,
    snapshot: Option<String>,
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
    if let Some(id) = &query.account_id {
        validate_id(id)?;
    }
    if let Some(date) = &query.as_of {
        crate::models::validate_date(date)?;
    }
    if query.limit == 0 || query.limit > 1000 || query.offset > 1000000 {
        return Err(Error::invalid(
            "limit must be 1–1000; offset at most 1000000",
        ));
    }
    if let Some(snapshot) = &query.snapshot {
        if snapshot.len() != 50
            || !snapshot.starts_with("c1")
            || !snapshot[2..]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::invalid("Invalid register snapshot"));
        }
    }
    Ok(Json(
        store
            .run(move |db| {
                // One read transaction keeps rows, totals and snapshot coherent even
                // if another connection writes while this page is being assembled.
                let read = db.transaction()?;
                ledger::list(
                    &read,
                    &budget,
                    query.account_id.as_deref(),
                    query.as_of.as_deref(),
                    query.limit,
                    query.offset,
                    query.snapshot.as_deref(),
                )
            })
            .await?,
    ))
}
pub(crate) fn budget_id(path: Result<Path<String>, PathRejection>) -> Result<String, Error> {
    let Path(budget) = path.map_err(|_| Error::invalid("Invalid budget path"))?;
    validate_id(&budget)?;
    Ok(budget)
}
pub(crate) fn idempotency_key(headers: &HeaderMap) -> Result<String, Error> {
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
pub(crate) fn json_error(error: JsonRejection) -> Error {
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

async fn create_transaction(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<NewTransaction>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let budget = budget_id(path)?;
    let Json(input) = input.map_err(json_error)?;
    input.validate()?;
    let key = idempotency_key(&headers)?;
    let clock = concurrency::header(&headers)?;
    let cancel = concurrency::cancel_header(&headers)?;
    let payload = serde_json::to_string(&input).map_err(Error::internal)?;
    store
        .run(move |db| {
            concurrency::write(
                db,
                &format!("POST /budgets/{budget}/transactions"),
                &key,
                &payload,
                clock.as_deref(),
                StatusCode::CREATED,
                |db| {
                    concurrency::execute(
                        db,
                        &budget,
                        clock.as_deref(),
                        cancel,
                        Mutation::Create(Box::new(input)),
                    )
                },
            )
        })
        .await
}

fn transaction_path(
    path: Result<Path<(String, String)>, PathRejection>,
) -> Result<(String, String), Error> {
    let Path((budget, id)) = path.map_err(|_| Error::invalid("Invalid transaction path"))?;
    validate_id(&budget)?;
    validate_id(&id)?;
    Ok((budget, id))
}
async fn get_transaction(
    State(store): State<Store>,
    path: Result<Path<(String, String)>, PathRejection>,
) -> Result<Json<Value>, Error> {
    let (budget, id) = transaction_path(path)?;
    Ok(data(
        store.run(move |db| ledger::get(db, &budget, &id)).await?,
    ))
}
async fn update_transaction(
    State(store): State<Store>,
    path: Result<Path<(String, String)>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let (budget, id) = transaction_path(path)?;
    let Json(input) = input.map_err(json_error)?;
    let key = idempotency_key(&headers)?;
    let clock = concurrency::header(&headers)?;
    let cancel = concurrency::cancel_header(&headers)?;
    let payload = input.to_string();
    store
        .run(move |db| {
            concurrency::write(
                db,
                &format!("PATCH /budgets/{budget}/transactions/{id}"),
                &key,
                &payload,
                clock.as_deref(),
                StatusCode::OK,
                |db| {
                    concurrency::execute(
                        db,
                        &budget,
                        clock.as_deref(),
                        cancel,
                        Mutation::Patch(id, input),
                    )
                },
            )
        })
        .await
}
async fn delete_transaction(
    State(store): State<Store>,
    path: Result<Path<(String, String)>, PathRejection>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<Value>), Error> {
    let (budget, id) = transaction_path(path)?;
    let key = idempotency_key(&headers)?;
    let clock = concurrency::header(&headers)?;
    let cancel = concurrency::cancel_header(&headers)?;
    store
        .run(move |db| {
            concurrency::write(
                db,
                &format!("DELETE /budgets/{budget}/transactions/{id}"),
                &key,
                "null",
                clock.as_deref(),
                StatusCode::OK,
                |db| {
                    concurrency::execute(
                        db,
                        &budget,
                        clock.as_deref(),
                        cancel,
                        Mutation::Delete(id),
                    )
                },
            )
        })
        .await
}
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Duplicate {
    id: String,
}
async fn duplicate_transaction(
    State(store): State<Store>,
    path: Result<Path<(String, String)>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<Duplicate>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let (budget, source) = transaction_path(path)?;
    let Json(input) = input.map_err(json_error)?;
    validate_id(&input.id)?;
    let key = idempotency_key(&headers)?;
    let clock = concurrency::header(&headers)?;
    let cancel = concurrency::cancel_header(&headers)?;
    let payload = serde_json::to_string(&input).map_err(Error::internal)?;
    store
        .run(move |db| {
            concurrency::write(
                db,
                &format!("POST /budgets/{budget}/transactions/{source}/duplicate"),
                &key,
                &payload,
                clock.as_deref(),
                StatusCode::CREATED,
                |db| {
                    concurrency::execute(
                        db,
                        &budget,
                        clock.as_deref(),
                        cancel,
                        Mutation::Duplicate(source, input.id),
                    )
                },
            )
        })
        .await
}
