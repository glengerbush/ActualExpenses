//! Receipt-first cancellation. Commit the fence before returning its HTTP error.
use crate::{error::Error, store};
use axum::http::StatusCode;
use rusqlite::Connection;
use serde_json::{Value, json};

pub fn write(
    db: &mut Connection,
    scope: &str,
    key: &str,
    payload: &str,
    cancel: bool,
    operation: impl FnOnce(&Connection) -> Result<Value, Error>,
) -> Result<Value, Error> {
    let marker = json!({"actualBudget_lifecycle_canceled": 1});
    let outcome = store::write(db, scope, key, payload, |db| {
        if cancel {
            Ok(marker.clone())
        } else {
            operation(db)
        }
    })?;
    if outcome == marker {
        Err(Error::new(
            StatusCode::CONFLICT,
            "operation_canceled",
            "This budget operation was canceled before it was saved. Delayed retries cannot apply it.",
        ))
    } else {
        Ok(outcome)
    }
}
