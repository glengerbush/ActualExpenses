use super::{journal, owner_header};
use crate::{api, concurrency as sync, error::Error, store::Store};
use axum::{
    Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
pub fn router() -> Router<Store> {
    Router::new()
        .route("/api/v1/budgets/{budget}/transactions/bulk", post(bulk))
        .route("/api/v1/budgets/{budget}/history", get(history))
        .route(
            "/api/v1/budgets/{budget}/history/{direction}",
            post(navigate),
        )
}
use super::bulk::Bulk;
async fn bulk(
    State(store): State<Store>,
    Path(budget): Path<String>,
    headers: HeaderMap,
    input: Result<Json<Bulk>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    crate::models::validate_id(&budget)?;
    let Json(input) = input.map_err(api::json_error)?;
    input.validate()?;
    let owner = owner_header(&headers)?;
    let owner = owner
        .ok_or_else(|| Error::invalid("X-Actual-History-Owner is required for bulk actions"))?;
    let key = api::idempotency_key(&headers)?;
    let proposed = sync::header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let cancel = sync::cancel_header(&headers)?;
    let payload = serde_json::to_string(&input).map_err(Error::internal)?;
    let lookup = sync::receipt_only_header(&headers)?;
    store.run(move|db|{
        let scope=format!("POST /budgets/{budget}/transactions/bulk");
        if lookup{return sync::receipt_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,Some(&owner));}
        sync::write_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,Some(&owner),|db|{
            if cancel{return sync::canceled(db,&budget);}
            crate::lifecycle::require_epoch(db,&budget,epoch.as_deref())?;
            sync::check_snapshot(db,&budget,Some(&input.snapshot))?;
            let clock=sync::clock(db,&budget,proposed.as_deref())?;
            let data=super::bulk::apply(db,&budget,&input,&clock,epoch.as_deref())?;
            Ok(json!({"response_status":200,"response_body":{"data":data,"edit_clock":clock,"cursor":sync::cursor(db,&budget)?}}))
        })
    }).await
}
async fn history(
    State(store): State<Store>,
    Path(budget): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&budget)?;
    let owner = owner_header(&headers)?
        .ok_or_else(|| Error::invalid("X-Actual-History-Owner is required"))?;
    Ok(Json(
        json!({"data":store.run(move|db|journal::state(db,&budget,&owner)).await?}),
    ))
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Navigate {
    action_id: String,
}
async fn navigate(
    State(store): State<Store>,
    Path((budget, direction)): Path<(String, String)>,
    headers: HeaderMap,
    input: Result<Json<Navigate>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    crate::models::validate_id(&budget)?;
    if !["undo", "redo"].contains(&direction.as_str()) {
        return Err(Error::missing());
    }
    let Json(input) = input.map_err(api::json_error)?;
    crate::models::validate_id(&input.action_id)?;
    let owner = owner_header(&headers)?
        .ok_or_else(|| Error::invalid("X-Actual-History-Owner is required"))?;
    let key = api::idempotency_key(&headers)?;
    let proposed = sync::header(&headers)?;
    let cancel = sync::cancel_header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let lookup = sync::receipt_only_header(&headers)?;
    let payload = super::payload(
        &serde_json::to_string(&input).map_err(Error::internal)?,
        Some(&owner),
    );
    store.run(move|db|{
        let scope=format!("POST /budgets/{budget}/history/{direction}");
        if lookup{return sync::receipt(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK);}
        sync::write(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,|db|{
            if cancel{return sync::canceled(db,&budget);}
            crate::lifecycle::require_epoch(db,&budget,epoch.as_deref())?;
            let clock=sync::clock(db,&budget,proposed.as_deref())?;
            let data=journal::apply(db,&budget,&owner,&input.action_id,direction=="redo",&clock)?;
            Ok(json!({"response_status":200,"response_body":{"data":data,"edit_clock":clock,"cursor":sync::cursor(db,&budget)?}}))
        })
    }).await
}
