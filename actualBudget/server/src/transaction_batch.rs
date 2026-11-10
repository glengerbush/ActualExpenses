//! Atomic external-caller transaction commands. Existing financial writers own
//! validation, propagation, protection and HLC ordering; this adapter owns only
//! one transaction/receipt/history and action-wide refusal on any conflict.
use crate::{concurrency as sync, error::Error, ledger, store::Store};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Batch {
    snapshot: String,
    operations: Vec<Operation>,
    #[serde(default = "graph_limit")]
    max_graph_members: usize,
}
fn graph_limit() -> usize {
    100000
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Operation {
    Create {
        transaction: Box<ledger::NewTransaction>,
    },
    Update {
        id: String,
        fields: Value,
    },
    Delete {
        id: String,
    },
}
impl Operation {
    fn id(&self) -> &str {
        match self {
            Self::Create { transaction } => &transaction.id,
            Self::Update { id, .. } | Self::Delete { id } => id,
        }
    }
    fn action(&self) -> &str {
        match self {
            Self::Create { .. } => "create",
            Self::Update { .. } => "update",
            Self::Delete { .. } => "delete",
        }
    }
}
pub fn router() -> Router<Store> {
    Router::new().route(
        "/api/v1/budgets/{budget}/transactions/batch",
        post(command).layer(DefaultBodyLimit::max(65536)),
    )
}
fn apply(
    db: &Connection,
    b: &str,
    input: Batch,
    proposed: Option<&str>,
    epoch: Option<&str>,
) -> Result<Value, Error> {
    crate::lifecycle::require_epoch(db, b, epoch)?;
    sync::check_snapshot(db, b, Some(&input.snapshot))?;
    if input.operations.is_empty()
        || input.operations.len() > 100
        || input.max_graph_members == 0
        || input.max_graph_members > 100000
    {
        return Err(Error::invalid(
            "Provide1–100 transaction operations and graph allowance1–100000",
        ));
    }
    let mut targets = BTreeSet::new();
    let mut graph = BTreeSet::new();
    for op in &input.operations {
        crate::models::validate_id(op.id())?;
        if !targets.insert(op.id()) {
            return Err(Error::invalid("Use one operation per transaction identity"));
        }
        match op {
            Operation::Create { transaction } => {
                transaction.validate()?;
                if transaction.new_payee.is_some() || transaction.new_category.is_some() {
                    return Err(Error::invalid(
                        "Resolve payee/category references through their resource API before capturing a batch",
                    ));
                }
            }
            Operation::Update { fields, .. } => {
                if fields.as_object().is_none_or(|o| {
                    o.is_empty() || o.contains_key("new_payee") || o.contains_key("new_category")
                }) {
                    return Err(Error::invalid(
                        "Provide nonempty update fields with existing reference IDs",
                    ));
                }
            }
            Operation::Delete { .. } => {}
        }
        if !matches!(op, Operation::Create { .. }) {
            // Validate every original target before writes; a published tombstone
            // is never silently revived as a new batch purchase.
            let members = crate::transfers::protection::members_bounded(
                db,
                b,
                &[op.id()],
                input.max_graph_members - graph.len(),
            )?;
            for row in members {
                if !graph.insert(row["id"].as_str().unwrap().to_owned()) {
                    return Err(Error::conflict(
                        "Use one operation per connected split/transfer graph; review and submit a single financial intent for that graph",
                    ));
                }
            }
        }
    }
    if input
        .operations
        .iter()
        .any(|op| matches!(op, Operation::Create { .. }) && graph.contains(op.id()))
    {
        return Err(Error::conflict(
            "A new identity overlaps an existing financial graph",
        ));
    }
    let clock = sync::clock(db, b, proposed)?;
    let mut results = vec![];
    let count = input.operations.len();
    let mut output = Output(0);
    for op in input.operations {
        let id = op.id().to_owned();
        let action = op.action().to_owned();
        let mutation = match op {
            Operation::Create { transaction } => sync::Mutation::Create(transaction),
            Operation::Update { id, fields } => sync::Mutation::Patch(id, fields),
            Operation::Delete { id } => sync::Mutation::Delete(id),
        };
        let result =
            sync::execute(db, b, Some(&clock), false, epoch, mutation).map_err(|mut error| {
                error.message = format!("Batch not saved (transaction {id}): {}", error.message);
                error
            })?;
        let body = &result["response_body"];
        if result["response_status"].as_u64().is_none_or(|s| s >= 400)
            || body["conflicts"].as_array().is_some_and(|a| !a.is_empty())
        {
            let reason = body["error"]["message"]
                .as_str()
                .unwrap_or("Newer fields prevent this intent");
            return Err(Error::new(
                StatusCode::CONFLICT,
                "batch_conflict",
                format!(
                    "Batch not saved (transaction {id}): {reason}. Refresh/review the entire original batch before preparing a new intent."
                ),
            ));
        }
        let item = json!({"action":action,"id":id,"data":body["data"]});
        serde_json::to_writer(&mut output, &item).map_err(|_| {
            Error::invalid("Batch response exceeds4MiB; use smaller independent commands")
        })?;
        results.push(item);
    }
    Ok(
        json!({"response_status":200,"response_body":{"data":{"results":results,"operation_count":count},"edit_clock":clock,"snapshot":sync::cursor(db,b)?,"epoch":crate::lifecycle::epoch(db,b)?}}),
    )
}
// Count output without allocating a second serialized response. Reserve framing.
struct Output(usize);
impl std::io::Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self.0.saturating_add(bytes.len() + 1);
        if self.0 > 4 * 1024 * 1024 - 1024 {
            return Err(std::io::Error::other("Batch output limit"));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
async fn command(
    State(store): State<Store>,
    Path(b): Path<String>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    crate::models::validate_id(&b)?;
    let Json(body) = input.map_err(crate::api::json_error)?;
    let key = crate::api::idempotency_key(&headers)?;
    let owner = crate::history::owner_header(&headers)?;
    let proposed = sync::header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let cancel = sync::cancel_header(&headers)?;
    let lookup = sync::receipt_only_header(&headers)?;
    let payload = json!({"request":body,"epoch":epoch}).to_string();
    store
        .run(move |db| {
            let scope = format!("POST /budgets/{b}/transactions/batch");
            if lookup {
                return sync::receipt_owned(
                    db,
                    &scope,
                    &key,
                    &payload,
                    proposed.as_deref(),
                    StatusCode::OK,
                    owner.as_ref(),
                );
            }
            sync::write_owned(
                db,
                &scope,
                &key,
                &payload,
                proposed.as_deref(),
                StatusCode::OK,
                owner.as_ref(),
                |db| {
                    if cancel {
                        return sync::canceled(db, &b);
                    }
                    let input = serde_json::from_value(body).map_err(|_| {
                        Error::invalid(
                            "Batch requires snapshot and typed create/update/delete operations",
                        )
                    })?;
                    apply(db, &b, input, proposed.as_deref(), epoch.as_deref())
                },
            )
        })
        .await
}
