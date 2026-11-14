//! Merge native records without matching financial rows by amount, date, or name.
//! Each source keeps a complete archive and a retired identity for stale-client safety.
use crate::{error::Error, lifecycle::archive};
use rusqlite::{Connection, OptionalExtension, params, types::Value as SqlValue};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const SETTINGS: &[&str] = &[
    "budget_settings",
    "rule_settings",
    "schedule_settings",
    "budget_preferences",
    "register_settings",
    "budget_months",
];
const EVIDENCE: &[&str] = &[
    "staged_imports",
    "imported_sources",
    "imported_field_clocks",
];
pub(super) fn identity(parts: &[&str]) -> String {
    let hash = Sha256::digest(serde_json::to_vec(parts).expect("string array"));
    let mut bytes: [u8; 16] = hash[..16].try_into().unwrap();
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    uuid::Uuid::from_bytes(bytes).to_string()
}
fn mapped(value: &str, ids: &BTreeMap<String, String>) -> String {
    ids.get(value).cloned().unwrap_or_else(|| value.to_owned())
}
fn remap_json(value: &mut Value, ids: &BTreeMap<String, String>) {
    match value {
        Value::String(s) => {
            if let Some(id) = ids.get(s) {
                *s = id.clone();
            }
        }
        Value::Array(a) => {
            for v in a {
                remap_json(v, ids);
            }
        }
        Value::Object(o) => {
            for (key, v) in o {
                if !matches!(
                    key.as_str(),
                    "name" | "label" | "title" | "description" | "notes"
                ) {
                    remap_json(v, ids);
                }
            }
        }
        _ => {}
    }
}
fn cell<'a>(table: &archive::Table, row: &'a [Value], column: &str) -> &'a Value {
    table
        .columns
        .iter()
        .position(|c| c == column)
        .and_then(|i| row.get(i))
        .unwrap_or(&Value::Null)
}
pub(super) fn copy(db: &Connection, source: &str, target: &str) -> Result<(), Error> {
    let source_name: String =
        db.query_row("SELECT name FROM budgets WHERE id=?1", [source], |r| {
            r.get(0)
        })?;
    let saved = archive::validate(&archive::capture(db, source)?)?;
    let mut ids = BTreeMap::new();
    // Map native entity identities once, preserving references shared across datasets.
    for (name, table) in &saved.state.tables {
        if SETTINGS.contains(&name.as_str())
            || EVIDENCE.contains(&name.as_str())
            || matches!(name.as_str(), "budget_cells" | "schedule_occurrences")
        {
            continue;
        }
        for row in &table.rows {
            if let Some(id) = cell(table, row, "id").as_str() {
                ids.entry(id.into())
                    .or_insert_with(|| identity(&["single-budget-v1", source, id]));
            }
        }
    }
    // Tags are retired UI metadata, but old archives can contain the same tag
    // in several budgets. Keep the primary metadata and bind identical names once.
    let mut shared_tags = BTreeSet::new();
    if let Some(table) = saved.state.tables.get("tags") {
        for row in &table.rows {
            let tag = cell(table, row, "tag").as_str().unwrap();
            let source_id = cell(table, row, "id").as_str().unwrap();
            if let Some(existing) = db
                .query_row(
                    "SELECT id FROM tags WHERE budget_id=?1 AND tag=?2",
                    params![target, tag],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
            {
                ids.insert(source_id.into(), existing);
                shared_tags.insert(source_id.to_owned());
            }
        }
    }
    if let Some(table) = saved.state.tables.get("transaction_import_settings") {
        for row in &table.rows {
            let id = cell(table, row, "id").as_str().unwrap();
            let account = cell(table, row, "account_id").as_str().unwrap();
            // The preference key encodes the account UUID, sometimes followed by a format suffix.
            ids.insert(id.into(), id.replacen(account, &mapped(account, &ids), 1));
        }
    }
    if let Some(table) = saved.state.tables.get("budget_cells") {
        for row in &table.rows {
            if let (Some(id), Some(method), Some(month), Some(category)) = (
                cell(table, row, "id").as_str(),
                cell(table, row, "method").as_str(),
                cell(table, row, "month").as_str(),
                cell(table, row, "category_id").as_str(),
            ) {
                ids.insert(
                    id.into(),
                    format!("{method}:{month}:{}", mapped(category, &ids)),
                );
            }
        }
    }
    if let Some(table) = saved.state.tables.get("schedule_occurrences") {
        for row in &table.rows {
            if let (Some(id), Some(schedule), Some(date)) = (
                cell(table, row, "id").as_str(),
                cell(table, row, "schedule_id").as_str(),
                cell(table, row, "occurrence_date").as_str(),
            ) {
                ids.insert(
                    id.into(),
                    identity(&[
                        "actualBudget.schedule-occurrence.v1",
                        target,
                        &mapped(schedule, &ids),
                        date,
                    ]),
                );
            }
        }
    }
    for &name in archive::TABLES {
        if SETTINGS.contains(&name) || EVIDENCE.contains(&name) {
            continue;
        }
        let table = &saved.state.tables[name];
        let placeholders = (1..=table.columns.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "INSERT INTO {name}({}) VALUES({placeholders})",
            table.columns.join(",")
        );
        for row in &table.rows {
            if name == "budget_provenance" && cell(table, row, "kind") == "archive-merge" {
                continue;
            }
            if (name == "tags"
                && cell(table, row, "id")
                    .as_str()
                    .is_some_and(|id| shared_tags.contains(id)))
                || (name == "field_versions"
                    && cell(table, row, "entity") == "tags"
                    && cell(table, row, "entity_id")
                        .as_str()
                        .is_some_and(|id| shared_tags.contains(id)))
            {
                continue;
            }
            // Global preference clocks are not source entity records and must not overwrite the chosen settings.
            if name == "field_versions"
                && cell(table, row, "entity").as_str().is_some_and(|e| {
                    SETTINGS.contains(&e) || matches!(e, "preferences" | "budget_preferences")
                })
            {
                continue;
            }
            let mut values = Vec::with_capacity(row.len());
            for (column, value) in table.columns.iter().zip(row) {
                let mut value = value.clone();
                if column == "budget_id" {
                    value = Value::String(target.into());
                } else if let Some(text) = value.as_str() {
                    if (column == "id" || column.ends_with("_id") || column == "merged_into")
                        && !(name == "budget_provenance" && column == "source_budget_id")
                    {
                        value = Value::String(mapped(text, &ids));
                    } else if matches!(
                        (name, column.as_str()),
                        ("rules", "conditions" | "actions")
                            | ("reporting_documents", "config")
                            | ("budget_cleanup_groups", "definition")
                    ) {
                        let mut config: Value =
                            serde_json::from_str(text).map_err(Error::internal)?;
                        remap_json(&mut config, &ids);
                        value = Value::String(config.to_string());
                    }
                }
                // Schedule names are unique in one budget; retain the source name without colliding.
                if name == "schedules" && column == "name" && !value.is_null() {
                    value = Value::String(format!(
                        "{} · {}",
                        value.as_str().unwrap().chars().take(60).collect::<String>(),
                        source_name.chars().take(40).collect::<String>()
                    ));
                    let candidate = value.as_str().unwrap();
                    if db.query_row("SELECT EXISTS(SELECT 1 FROM schedules WHERE budget_id=?1 AND name=?2 AND tombstone=0)",params![target,candidate],|r|r.get::<_,bool>(0))? {
                        value=Value::String(format!("{candidate} · {}",&mapped(cell(table,row,"id").as_str().unwrap(),&ids)[..8]));
                    }
                }
                values.push(match value {
                    Value::Null => SqlValue::Null,
                    Value::String(v) => SqlValue::Text(v),
                    Value::Number(n) => {
                        if let Some(v) = n.as_i64() {
                            SqlValue::Integer(v)
                        } else {
                            SqlValue::Real(
                                n.as_f64()
                                    .ok_or_else(|| Error::invalid("Invalid archived number"))?,
                            )
                        }
                    }
                    Value::Bool(v) => SqlValue::Integer(i64::from(v)),
                    Value::Object(ref v) if v.len() == 1 && v.contains_key("$blob") => {
                        SqlValue::Blob(
                            saved
                                .blobs
                                .get(v["$blob"].as_str().unwrap_or(""))
                                .ok_or_else(|| Error::invalid("Missing source archive data"))?
                                .clone(),
                        )
                    }
                    _ => return Err(Error::invalid("Unsupported native archive value")),
                });
            }
            db.execute(&sql, rusqlite::params_from_iter(values))?;
        }
    }
    super::provenance::preserve(db, target, &saved, &ids)?;
    let accounts = db
        .prepare("SELECT id FROM accounts WHERE budget_id=?1")?
        .query_map([target], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in accounts {
        crate::ledger::refresh_account(db, target, &id)?;
    }
    crate::categories::validate_archive(db, target)?;
    crate::splits::validate(db, target)?;
    crate::transfers::validate(db, target)?;
    crate::schedules::validate(db, target)?;
    db.execute(
        "UPDATE budget_sync SET clock=MAX(clock,?2) WHERE budget_id=?1",
        params![target, saved.state.clock],
    )?;
    db.execute(
        "UPDATE budget_backups SET budget_id=?2 WHERE budget_id=?1",
        params![source, target],
    )?;
    db.execute("UPDATE budgets SET deleted=1 WHERE id=?1", [source])?;
    Ok(())
}
