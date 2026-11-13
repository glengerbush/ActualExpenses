//! Legacy archive/token compatibility. Manual entries are never reconciled.
use crate::{concurrency, error::Error, ledger};
use axum::http::StatusCode;
use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub fn members(db: &Connection, budget: &str, id: &str) -> Result<Vec<Value>, Error> {
    let row = ledger::get(db, budget, id)?;
    let parent = match row["parent_id"].as_str() {
        Some(parent) => ledger::get(db, budget, parent)?,
        None => row,
    };
    let mut rows = parent["children"].as_array().cloned().unwrap_or_default();
    rows.insert(0, parent);
    Ok(rows)
}
/// Fixed-size proof of the complete logical aggregate. Never hash presentation
/// order, caller-supplied membership, or unrelated mutable ledger fields.
pub fn protection_token(budget: &str, rows: &[Value]) -> String {
    let mut state: Vec<_> = rows
        .iter()
        .map(|row| {
            (
                row["id"].as_str().unwrap(),
                row["reconciled"].as_bool().unwrap_or(false),
                row["field_versions"]["reconciled"]
                    .as_str()
                    .unwrap_or(concurrency::ZERO),
            )
        })
        .collect();
    state.sort_unstable_by(|a, b| a.0.cmp(b.0));
    let payload = json!([
        "actualBudget.reconciliation.v1",
        budget,
        rows[0]["id"],
        state
    ]);
    format!("{:x}", Sha256::digest(payload.to_string().as_bytes()))
}
/// Called only after hydrating every current member inside the receipt transaction.
/// Fresh digest alone is insufficient: a delayed original clock must still refuse.
pub fn verify_token(budget: &str, rows: &[Value], token: &str, clock: &str) -> Result<(), Error> {
    if token.len() != 64
        || !token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::invalid("Invalid reconciliation token"));
    }
    if token != protection_token(budget, rows)
        || rows.iter().any(|row| {
            clock
                <= row["field_versions"]["reconciled"]
                    .as_str()
                    .unwrap_or(concurrency::ZERO)
        })
    {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "reconciliation_changed",
            "The split membership or protection changed. Refresh and confirm the current transaction.",
        ));
    }
    Ok(())
}
pub fn protect_members(
    rows: &[Value],
    versions: Option<&BTreeMap<String, String>>,
    clock: &str,
) -> Result<(), Error> {
    for row in rows {
        protect(
            row,
            versions
                .and_then(|versions| versions.get(row["id"].as_str().unwrap()))
                .map(|version| Confirmation { version, clock }),
        )?;
    }
    Ok(())
}
/// Proof captured when the user confirmed a protected edit. Aggregate callers
/// must construct one proof per affected protected row, never reuse another leg's.
#[derive(Clone, Copy)]
pub struct Confirmation<'a> {
    pub version: &'a str,
    pub clock: &'a str,
}
/// Shared guard for ordinary, split, transfer and future bulk mutations.
pub fn protect(row: &Value, confirmation: Option<Confirmation<'_>>) -> Result<(), Error> {
    if row["reconciled"] != true {
        return Ok(());
    }
    let Some(confirmation) = confirmation else {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "reconciled_protected",
            "This transaction is reconciled. Confirm the edit or explicitly unlock it first.",
        ));
    };
    let current = row["field_versions"]["reconciled"]
        .as_str()
        .unwrap_or(concurrency::ZERO);
    if confirmation.version != current || confirmation.clock <= current {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "reconciliation_changed",
            "Reconciliation protection changed after this confirmation. Refresh and confirm the current transaction before editing.",
        ));
    }
    Ok(())
}
pub fn source_timestamp(value: &Value) -> Result<Option<String>, Error> {
    if value.is_null() {
        return Ok(None);
    }
    let raw = value
        .as_str()
        .ok_or_else(|| Error::invalid("Invalid last reconciliation timestamp"))?;
    let ms = raw
        .parse::<i64>()
        .map_err(|_| Error::invalid("Invalid last reconciliation timestamp"))?;
    if ms < 0 || chrono::DateTime::from_timestamp_millis(ms).is_none() {
        return Err(Error::invalid("Invalid last reconciliation timestamp"));
    }
    Ok(Some(raw.to_owned()))
}
