//! Keep archive deduplication mappings while retiring executable source identities.
use crate::{error::Error, imports::merge_identity, lifecycle::archive};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const TABLES: &[&str] = &[
    "account_groups",
    "accounts",
    "category_groups",
    "categories",
    "payees",
    "transactions",
];

fn cell<'a>(table: &archive::Table, row: &'a [Value], name: &str) -> &'a Value {
    table
        .columns
        .iter()
        .position(|column| column == name)
        .and_then(|index| row.get(index))
        .unwrap_or(&Value::Null)
}

// Only destination values are translated. Source keys identify the original archive.
fn extend(
    destination: &mut Value,
    source: &Value,
    ids: &BTreeMap<String, String>,
) -> Result<(), Error> {
    let destination = destination
        .as_object_mut()
        .ok_or_else(|| Error::invalid("Invalid import provenance"))?;
    let source = source
        .as_object()
        .ok_or_else(|| Error::invalid("Invalid import provenance"))?;
    for (table, entries) in source {
        let entries = entries
            .as_object()
            .ok_or_else(|| Error::invalid("Invalid import provenance mapping"))?;
        let saved = destination
            .entry(table.clone())
            .or_insert_with(|| json!({}));
        let saved = saved
            .as_object_mut()
            .ok_or_else(|| Error::invalid("Invalid import provenance mapping"))?;
        for (original, target) in entries {
            let target = target
                .as_str()
                .ok_or_else(|| Error::invalid("Invalid import provenance identity"))?;
            saved
                .entry(original.clone())
                .or_insert_with(|| json!(ids.get(target).map(String::as_str).unwrap_or(target)));
        }
    }
    Ok(())
}

fn mappings(saved: &archive::Validated) -> Result<BTreeMap<String, Value>, Error> {
    let mut result = BTreeMap::new();
    let provenance = &saved.state.tables["budget_provenance"];
    for row in &provenance.rows {
        if cell(provenance, row, "kind") != "archive-merge" {
            continue;
        }
        let source = cell(provenance, row, "source_budget_id")
            .as_str()
            .ok_or_else(|| Error::invalid("Missing import provenance source"))?;
        let metadata: Value = serde_json::from_str(
            cell(provenance, row, "metadata")
                .as_str()
                .ok_or_else(|| Error::invalid("Missing import provenance mapping"))?,
        )
        .map_err(Error::internal)?;
        extend(
            result.entry(source.into()).or_insert_with(|| json!({})),
            &metadata,
            &BTreeMap::new(),
        )?;
    }

    // Initial activation uses original entity IDs, without an archive-merge row.
    // Materialize that implicit mapping before imported_sources is retired.
    let sources = &saved.state.tables["imported_sources"];
    let stages = &saved.state.tables["staged_imports"];
    for row in &sources.rows {
        let stage_id = cell(sources, row, "import_id");
        let stage = stages
            .rows
            .iter()
            .find(|stage| cell(stages, stage, "id") == stage_id)
            .ok_or_else(|| Error::invalid("Missing original import provenance"))?;
        let source = cell(sources, row, "source_local_id")
            .as_str()
            .filter(|id| !id.is_empty())
            .or_else(|| cell(stages, stage, "archive_sha256").as_str())
            .ok_or_else(|| Error::invalid("Missing original import identity"))?;
        let converted: Value = serde_json::from_str(
            cell(stages, stage, "conversion")
                .as_str()
                .ok_or_else(|| Error::invalid("Missing original import conversion"))?,
        )
        .map_err(Error::internal)?;
        let mut mapping = json!({});
        for &table in TABLES {
            let records = &saved.state.tables[table];
            let present: BTreeSet<&str> = records
                .rows
                .iter()
                .filter_map(|row| cell(records, row, "id").as_str())
                .collect();
            for row in converted[table].as_array().into_iter().flatten() {
                if let Some(id) = row["id"].as_str().filter(|id| present.contains(id)) {
                    if !mapping[table].is_object() {
                        mapping[table] = json!({});
                    }
                    mapping[table][id] = json!(id);
                }
            }
        }
        extend(
            result.entry(source.into()).or_insert_with(|| json!({})),
            &mapping,
            &BTreeMap::new(),
        )?;
    }
    Ok(result)
}

/// Primary destination mappings win when several budgets imported the same source.
/// All financial copies remain; subsequent imports select one existing copy.
pub(super) fn preserve(
    db: &Connection,
    target: &str,
    saved: &archive::Validated,
    ids: &BTreeMap<String, String>,
) -> Result<(), Error> {
    for (source, mapping) in mappings(saved)? {
        let id = merge_identity(&source, "provenance", "mapping");
        let previous: Option<String> = db.query_row("SELECT metadata FROM budget_provenance WHERE budget_id=?1 AND id=?2 AND kind='archive-merge'", params![target,id], |row| row.get(0)).optional()?;
        let mut combined = previous
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(Error::internal)?
            .unwrap_or_else(|| json!({}));
        extend(&mut combined, &mapping, ids)?;
        db.execute("INSERT INTO budget_provenance(budget_id,id,kind,source_budget_id,metadata) VALUES(?1,?2,'archive-merge',?3,?4) ON CONFLICT(budget_id,id) DO UPDATE SET metadata=excluded.metadata", params![target,id,source,combined.to_string()])?;
    }
    Ok(())
}

/// A replacement restore keeps entity IDs, but cannot keep executable import sessions.
pub(super) fn materialize(saved: &mut archive::Validated) -> Result<(), Error> {
    let mappings = mappings(saved)?;
    let budget = saved.state.budget["id"].clone();
    let table = saved.state.tables.get_mut("budget_provenance").unwrap();
    let kind = table
        .columns
        .iter()
        .position(|column| column == "kind")
        .unwrap();
    table.rows.retain(|row| row[kind] != "archive-merge");
    for (source, mapping) in mappings {
        let record = json!({"budget_id":budget,"id":merge_identity(&source,"provenance","mapping"),"kind":"archive-merge","source_budget_id":source,"metadata":mapping.to_string(),"archive_sha256":null,"original":null});
        table.rows.push(
            table
                .columns
                .iter()
                .map(|column| record[column].clone())
                .collect(),
        );
    }
    Ok(())
}
