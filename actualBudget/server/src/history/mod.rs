//! Durable, opt-in engine history. Capture authoritative rows only; totals and
//! cursors are recomputed. Receipt replay never enters capture a second time.
mod bulk;
mod journal;
mod routes;
use crate::{error::Error, models::validate_id};
use axum::http::HeaderMap;
pub use journal::{configure, record};
pub use routes::router;
use serde_json::json;
#[derive(Clone)]
pub struct Owner {
    pub id: String,
    pub epoch: Option<String>,
}
pub fn owner_header(headers: &HeaderMap) -> Result<Option<Owner>, Error> {
    headers
        .get("X-Actual-History-Owner")
        .map(|v| {
            let id = v
                .to_str()
                .map_err(|_| Error::invalid("Invalid history owner"))?
                .to_string();
            validate_id(&id)?;
            Ok(Owner {
                id,
                epoch: crate::lifecycle::epoch_header(headers)?,
            })
        })
        .transpose()
}
pub fn payload(payload: &str, owner: Option<&Owner>) -> String {
    owner
        .map(|o| json!({"payload":payload,"history_owner":o.id,"budget_epoch":o.epoch}).to_string())
        .unwrap_or_else(|| payload.to_string())
}
