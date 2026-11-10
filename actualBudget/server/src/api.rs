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
        .merge(crate::transfers::router())
        .merge(crate::splits::router())
        .merge(crate::history::router())
        .merge(crate::accounts::router())
        .merge(crate::reconciliation::router())
        .merge(crate::budgeting::router())
        .merge(crate::rules::router())
        .merge(crate::schedules::router())
        .merge(crate::categories::router())
        .merge(crate::payees_tags::router())
        .merge(crate::payee_locations::router())
        .merge(crate::register_settings::router())
        .merge(crate::preferences::router())
        .merge(crate::recovery::router())
        .merge(concurrency::router())
        .merge(crate::companion::router())
        .merge(crate::android_upgrade::router())
        .route("/api", any(|| async { Error::missing() }))
        .route("/api/{*rest}", any(|| async { Error::missing() }))
        .layer(DefaultBodyLimit::max(65536))
        .merge(crate::imports::router())
        .merge(crate::transaction_imports::router())
        .merge(crate::themes::router())
        .merge(crate::lifecycle::router())
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
    if request.headers().contains_key("X-Actual-Receipt-Only") {
        if let Err(error) = concurrency::receipt_only_header(request.headers()) {
            return error.into_response();
        }
        let path = request
            .extensions()
            .get::<axum::extract::MatchedPath>()
            .map(|path| path.as_str())
            .unwrap_or("");
        if request
            .headers()
            .contains_key("X-Actual-Cancel-If-Uncommitted")
        {
            return Error::invalid("Receipt lookup cannot also cancel a request").into_response();
        }
        if !matches!(
            (request.method().as_str(), path),
            (
                "POST",
                "/api/v1/budgets/{budget}/schedules/commands"
                    | "/api/v1/budgets/{budget}/android-upgrade/operations"
                    | "/api/v1/budgets/{budget}/rules"
                    | "/api/v1/budgets/{budget}/rules/apply"
                    | "/api/v1/budgets/{budget}/rules/transactions"
                    | "/api/v1/budgets/{budget}/rules/learning"
                    | "/api/v1/budgets/{budget}/rules/rename-payee"
                    | "/api/v1/budgets/{budget}/budget"
                    | "/api/v1/budgets/{budget}/budget/templates/commands"
                    | "/api/v1/budgets/{budget}/transactions"
                    | "/api/v1/budgets/{budget}/transactions/bulk"
                    | "/api/v1/budgets/{budget}/history/{direction}"
                    | "/api/v1/budgets/{budget}/transfers"
                    | "/api/v1/budgets/{budget}/transfers/link"
            ) | (
                "PATCH" | "DELETE",
                "/api/v1/budgets/{budget}/transactions/{id}"
                    | "/api/v1/budgets/{budget}/rules/{id}"
            )
        ) {
            return Error::invalid("Receipt lookup is unsupported for this operation")
                .into_response();
        }
    }
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
            (
                "POST",
                "/api/v1/budgets/{budget}/recovery"
                    | "/api/v1/budgets/{budget}/schedules/commands"
                    | "/api/v1/budgets/{budget}/android-upgrade/operations"
                    | "/api/v1/budgets/{budget}/rules"
                    | "/api/v1/budgets/{budget}/rules/apply"
                    | "/api/v1/budgets/{budget}/rules/transactions"
                    | "/api/v1/budgets/{budget}/rules/learning"
                    | "/api/v1/budgets/{budget}/rules/rename-payee"
                    | "/api/v1/budgets/{budget}/budget"
                    | "/api/v1/budgets/{budget}/budget/templates/commands"
                    | "/api/v1/budgets/{budget}/transactions"
                    | "/api/v1/budgets/{budget}/transactions/bulk"
                    | "/api/v1/budgets/{budget}/history/{direction}"
                    | "/api/v1/budgets/{budget}/transfers"
                    | "/api/v1/budgets/{budget}/transfers/link"
            ) | (
                "PATCH" | "DELETE",
                "/api/v1/budgets/{budget}/transactions/{id}"
                    | "/api/v1/budgets/{budget}/rules/{id}"
            ) | (
                "POST",
                "/api/v1/budgets/{budget}/transactions/{id}/duplicate"
                    | "/api/v1/budgets/{budget}/transactions/{id}/splits"
            ) | (
                "POST",
                "/api/v1/budgets/{budget}/accounts"
                    | "/api/v1/budgets/{budget}/account-groups"
                    | "/api/v1/budgets/{budget}/categories"
                    | "/api/v1/budgets/{budget}/category-groups"
            ) | (
                "PATCH" | "DELETE",
                "/api/v1/budgets/{budget}/accounts/{id}"
                    | "/api/v1/budgets/{budget}/account-groups/{id}"
                    | "/api/v1/budgets/{budget}/categories/{id}"
                    | "/api/v1/budgets/{budget}/category-groups/{id}"
            ) | (
                "POST",
                "/api/v1/budgets/{budget}/accounts/{id}/reconciliation"
                    | "/api/v1/budgets/{budget}/transactions/{id}/unlock"
                    | "/api/v1/budgets/{budget}/accounts/{id}/close"
                    | "/api/v1/budgets/{budget}/accounts/{id}/reopen"
                    | "/api/v1/budgets/{budget}/accounts/{id}/move"
                    | "/api/v1/budgets/{budget}/account-groups/{id}/move"
            ) | (
                "POST",
                "/api/v1/budgets/{budget}/payees"
                    | "/api/v1/budgets/{budget}/payee-locations"
                    | "/api/v1/budgets/{budget}/tags"
                    | "/api/v1/budgets/{budget}/payees/batch"
                    | "/api/v1/budgets/{budget}/tags/batch"
                    | "/api/v1/budgets/{budget}/tags/discover"
            ) | (
                "PATCH",
                "/api/v1/budgets/{budget}/payees/{id}"
                    | "/api/v1/budgets/{budget}/payee-locations/{id}"
                    | "/api/v1/budgets/{budget}/tags/{id}"
            ) | ("PATCH" | "DELETE", "/api/v1/budgets/{budget}")
                | (
                    "POST",
                    "/api/v1/budgets/{budget}/duplicate"
                        | "/api/v1/budgets/{budget}/backups"
                        | "/api/v1/budgets/{budget}/backups/{id}/restore"
                        | "/api/v1/budgets/{budget}/restores/{id}/activate"
                )
        ) {
            return Error::invalid("Cancellation is unsupported for this operation")
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
        "capabilities": ["recovery", "recovery_cancellation","budget_templates", "budget_template_projection", "budget_automations", "schedules", "schedule_catch_up", "schedule_matching", "schedule_discovery", "schedule_conversion", "android_upgrade","rules", "rule_entry", "rule_preview", "rule_cancellation", "rule_receipt_lookup", "category_learning", "category_associations", "budget_planning", "tracking_budget", "envelope_budget", "bulk_transactions", "undo_redo", "history_cancellation", "history_receipt_lookup", "payee_locations", "location_cancellation", "reconciliation", "reconciled_confirmation", "paired_reconciled_confirmation", "reconciliation_cancellation", "expense_kind_precondition", "transfers", "transfer_cancellation", "transfer_receipt_lookup", "budgets", "accounts", "account_lifecycle", "account_cancellation", "category_cancellation", "account_groups", "opening_balances", "idempotency", "transactions", "payees", "categories", "category_management", "amount_calculator", "preferences", "field_ordering", "change_cursors", "transaction_cancellation", "transaction_receipt_lookup", "companion_expenses", "budget_epoch_preconditions", "budget_archives", "budget_lifecycle_cancellation", "budget_duplication", "custom_themes", "actual_imports", "payee_management", "reference_cancellation", "tags", "splits", "register_queries", "register_settings"],
        "openapi": "/api/v1/openapi.json"
    }))
}
async fn contract() -> Json<Value> {
    Json(serde_json::from_str(include_str!("openapi.json")).expect("valid embedded OpenAPI"))
}
async fn list_budgets(
    State(store): State<Store>,
    query: Result<Query<crate::lifecycle::BudgetQuery>, QueryRejection>,
) -> Result<Json<Value>, Error> {
    let query = query.map_err(|_| Error::invalid("Invalid budget query"))?.0;
    Ok(data(
        store
            .run(move |db| crate::lifecycle::list(db, query.include_deleted))
            .await?,
    ))
}
async fn list_accounts(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<crate::lifecycle::SnapshotQuery>, QueryRejection>,
) -> Result<Json<Value>, Error> {
    let budget = budget_id(path)?;
    let query = crate::lifecycle::snapshot_query(query)?;
    Ok(data(
        store
            .run(move |db| {
                crate::concurrency::check_snapshot(db, &budget, query.snapshot.as_deref())?;
                store::accounts(db, &budget)
            })
            .await?,
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
    let key = idempotency_key(&headers)?;
    let owner = crate::history::owner_header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let proposed = concurrency::header(&headers)?;
    let cancel = concurrency::cancel_header(&headers)?;
    let payload = serde_json::to_string(&input).map_err(Error::internal)?;
    store
        .run(move |db| {
            concurrency::write_owned(
                db,
                &format!("POST /budgets/{budget}/accounts"),
                &key,
                &payload,
                proposed.as_deref(),
                StatusCode::CREATED,
                owner.as_ref(),
                |db| {
                    if cancel {
                        return concurrency::canceled(db, &budget);
                    }
                    input.validate()?;
                    crate::lifecycle::require_epoch(db, &budget, epoch.as_deref())?;
                    if proposed.is_some() {
                        concurrency::clock(db, &budget, proposed.as_deref())?;
                    }
                    store::create_account(db, &budget, input)
                },
            )
        })
        .await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TransactionQuery {
    query: Option<String>,
    account_id: Option<String>,
    payee_id: Option<String>,
    tag: Option<String>,
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
    if let Some(id) = &query.payee_id {
        validate_id(id)?;
    }
    if let Some(tag) = &query.tag {
        crate::payees_tags::validate_tag(tag)?;
    }
    if let Some(date) = &query.as_of {
        crate::models::validate_date(date)?;
    }
    if query.limit == 0 || query.limit > 1000 || query.offset > 1000000 {
        return Err(Error::invalid(
            "limit must be 1–1000; offset at most 1000000",
        ));
    }
    if let Some(snapshot) = &query.snapshot
        && (snapshot.len() != 50
            || !snapshot.starts_with("c1")
            || !snapshot[2..]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    {
        return Err(Error::invalid("Invalid register snapshot"));
    }
    let filter: crate::register::Query = query
        .query
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|_| Error::invalid("Invalid register query"))?
        .unwrap_or_default();
    filter.validate()?;
    Ok(Json(
        store
            .run(move |db| {
                // One read transaction keeps rows, totals and snapshot coherent even
                // if another connection writes while this page is being assembled.
                let read = db.transaction()?;
                crate::register::list(
                    &read,
                    &budget,
                    ledger::RegisterFilters {
                        account: query.account_id.as_deref(),
                        as_of: query.as_of.as_deref(),
                        payee: query.payee_id.as_deref(),
                        tag: query.tag.as_deref(),
                    },
                    query.limit,
                    query.offset,
                    query.snapshot.as_deref(),
                    &filter,
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
    let owner = crate::history::owner_header(&headers)?;
    let clock = concurrency::header(&headers)?;
    let cancel = concurrency::cancel_header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let payload = serde_json::to_string(&input).map_err(Error::internal)?;
    store
        .run(move |db| {
            if headers.contains_key("X-Actual-Receipt-Only") {
                return concurrency::receipt_owned(
                    db,
                    &format!("POST /budgets/{budget}/transactions"),
                    &key,
                    &payload,
                    clock.as_deref(),
                    StatusCode::CREATED,
                    owner.as_ref(),
                );
            }
            concurrency::write_owned(
                db,
                &format!("POST /budgets/{budget}/transactions"),
                &key,
                &payload,
                clock.as_deref(),
                StatusCode::CREATED,
                owner.as_ref(),
                |db| {
                    concurrency::execute(
                        db,
                        &budget,
                        clock.as_deref(),
                        cancel,
                        epoch.as_deref(),
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
    let owner = crate::history::owner_header(&headers)?;
    let clock = concurrency::header(&headers)?;
    let cancel = concurrency::cancel_header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let payload = input.to_string();
    store
        .run(move |db| {
            if headers.contains_key("X-Actual-Receipt-Only") {
                return concurrency::receipt_owned(
                    db,
                    &format!("PATCH /budgets/{budget}/transactions/{id}"),
                    &key,
                    &payload,
                    clock.as_deref(),
                    StatusCode::OK,
                    owner.as_ref(),
                );
            }
            concurrency::write_owned(
                db,
                &format!("PATCH /budgets/{budget}/transactions/{id}"),
                &key,
                &payload,
                clock.as_deref(),
                StatusCode::OK,
                owner.as_ref(),
                |db| {
                    concurrency::execute(
                        db,
                        &budget,
                        clock.as_deref(),
                        cancel,
                        epoch.as_deref(),
                        if headers
                            .get("X-Actual-Expense-Only")
                            .is_some_and(|v| v == "true")
                        {
                            Mutation::ExpensePatch(id, input)
                        } else {
                            Mutation::Patch(id, input)
                        },
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
    let owner = crate::history::owner_header(&headers)?;
    let clock = concurrency::header(&headers)?;
    let cancel = concurrency::cancel_header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    store
        .run(move |db| {
            if headers.contains_key("X-Actual-Receipt-Only") {
                return concurrency::receipt_owned(
                    db,
                    &format!("DELETE /budgets/{budget}/transactions/{id}"),
                    &key,
                    "null",
                    clock.as_deref(),
                    StatusCode::OK,
                    owner.as_ref(),
                );
            }
            concurrency::write_owned(
                db,
                &format!("DELETE /budgets/{budget}/transactions/{id}"),
                &key,
                "null",
                clock.as_deref(),
                StatusCode::OK,
                owner.as_ref(),
                |db| {
                    concurrency::execute(
                        db,
                        &budget,
                        clock.as_deref(),
                        cancel,
                        epoch.as_deref(),
                        if headers
                            .get("X-Actual-Expense-Only")
                            .is_some_and(|v| v == "true")
                        {
                            Mutation::ExpenseDelete(id)
                        } else {
                            Mutation::Delete(id)
                        },
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
    let owner = crate::history::owner_header(&headers)?;
    let clock = concurrency::header(&headers)?;
    let cancel = concurrency::cancel_header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let payload = serde_json::to_string(&input).map_err(Error::internal)?;
    store
        .run(move |db| {
            concurrency::write_owned(
                db,
                &format!("POST /budgets/{budget}/transactions/{source}/duplicate"),
                &key,
                &payload,
                clock.as_deref(),
                StatusCode::CREATED,
                owner.as_ref(),
                |db| {
                    concurrency::execute(
                        db,
                        &budget,
                        clock.as_deref(),
                        cancel,
                        epoch.as_deref(),
                        Mutation::Duplicate(source, input.id),
                    )
                },
            )
        })
        .await
}
