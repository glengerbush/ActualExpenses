//! Add source financial graphs without replacing destination settings or user edits.
use super::*;
use std::collections::BTreeMap;

const TABLES: &[&str] = &[
    "account_groups",
    "accounts",
    "category_groups",
    "categories",
    "payees",
    "transactions",
];

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Destination {
    budget_id: String,
    epoch: String,
}

pub(super) async fn merge(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<Destination>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    let id = import_id(path)?;
    let Json(destination) = input.map_err(json_error)?;
    validate_id(&destination.budget_id)?;
    let key = idempotency_key(&headers)?;
    let payload = serde_json::to_string(&destination).map_err(Error::internal)?;
    Ok(data(store.run(move |db| store::write(db, &format!("POST /imports/{id}/merge"), &key, &payload, |db| {
        crate::lifecycle::require_epoch(db, &destination.budget_id, Some(&destination.epoch))?;
        let stage = read_import(db, &id)?;
        if stage["status"] == "blocked" {
            return Err(Error::conflict("Resolve this archive's conversion requirements before importing."));
        }
        let original: Vec<u8> = db.query_row("SELECT original FROM staged_imports WHERE id=?1", [&id], |r| r.get(0))?;
        let (_, plan) = inspect_original(&original, &stage["report"])?;
        let source = stage["report"]["source"]["id"].as_str()
            .filter(|id| !id.is_empty()).unwrap_or(stage["report"]["archive_sha256"].as_str().unwrap());
        let same_source: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM imported_sources WHERE budget_id=?1 AND source_local_id=?2)", params![destination.budget_id, source], |r| r.get(0))?;
        let result = install(db, &destination.budget_id, source, same_source, &plan)?;
        let mut budget = crate::lifecycle::metadata(db, &destination.budget_id, false)?;
        budget["import_result"] = result;
        Ok(budget)
    })).await?))
}
fn identity(source: &str, table: &str, id: &str) -> String {
    let mut hash = Sha256::new();
    for part in ["archive-merge-v1", source, table, id] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    let mut bytes: [u8; 16] = hash.finalize()[..16].try_into().unwrap();
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    Uuid::from_bytes(bytes).to_string()
}
fn exists(db: &Connection, budget: &str, table: &str, id: &str) -> Result<bool, Error> {
    Ok(db.query_row(
        &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE budget_id=?1 AND id=?2)"),
        params![budget, id],
        |r| r.get(0),
    )?)
}
fn rows<'a>(plan: &'a Value, table: &str) -> &'a [Value] {
    plan[table].as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn mapped(
    maps: &BTreeMap<(&str, String), String>,
    table: &str,
    value: &Value,
) -> Result<Value, Error> {
    if value.is_null() {
        return Ok(Value::Null);
    }
    let id = value
        .as_str()
        .ok_or_else(|| Error::invalid("Invalid imported reference"))?;
    maps.get(&(table, id.to_owned()))
        .map(|v| json!(v))
        .ok_or_else(|| Error::invalid("Missing imported reference"))
}
fn install(
    db: &Connection,
    budget: &str,
    source: &str,
    same_source: bool,
    plan: &Value,
) -> Result<Value, Error> {
    let provenance_id = identity(source, "provenance", "mapping");
    let saved: Option<String> = db.query_row("SELECT metadata FROM budget_provenance WHERE budget_id=?1 AND id=?2 AND kind='archive-merge'", params![budget, provenance_id], |r| r.get(0)).optional()?;
    let mut retained: Value = saved
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(Error::internal)?
        .unwrap_or_else(|| json!({}));
    let mut maps = BTreeMap::new();
    // Match named metadata only when its meaning agrees. Never match financial rows by amount/date.
    for &table in TABLES {
        for row in rows(plan, table) {
            let original = row["id"]
                .as_str()
                .ok_or_else(|| Error::invalid("Missing imported identity"))?;
            let stable = identity(source, table, original);
            let mut target = stable.clone();
            if let Some(saved) = retained[table][original].as_str() {
                if !exists(db, budget, table, saved)? {
                    return Err(Error::conflict(
                        "An earlier import's destination is missing. Restore a complete budget before merging.",
                    ));
                }
                maps.insert((table, original.to_owned()), saved.to_owned());
                continue;
            }
            if same_source && exists(db, budget, table, original)? {
                target = original.to_owned();
            } else if !exists(db, budget, table, &stable)? && row["tombstone"] != true {
                let name = row["name"].as_str().unwrap_or("");
                let found: Option<String> = match table {
                    "category_groups" => db.query_row("SELECT id FROM category_groups WHERE budget_id=?1 AND name=?2 COLLATE NOCASE AND is_income=?3 AND tombstone=0 ORDER BY id LIMIT 1", params![budget, name, row["is_income"].as_bool().unwrap_or(false)], |r| r.get(0)).optional()?,
                    "categories" => {
                        let group = mapped(&maps, "category_groups", &row["group_id"])?;
                        db.query_row("SELECT id FROM categories WHERE budget_id=?1 AND name=?2 COLLATE NOCASE AND is_income=?3 AND group_id IS ?4 AND tombstone=0 ORDER BY id LIMIT 1", params![budget, name, row["is_income"].as_bool().unwrap_or(false), group.as_str()], |r| r.get(0)).optional()?
                    },
                    "payees" if row["transfer_account_id"].is_null() => db.query_row("SELECT id FROM payees WHERE budget_id=?1 AND name=?2 COLLATE NOCASE AND transfer_account_id IS NULL AND tombstone=0 ORDER BY id LIMIT 1", params![budget, name], |r| r.get(0)).optional()?,
                    _ => None,
                };
                if let Some(id) = found {
                    target = id;
                }
            }
            maps.insert((table, original.to_owned()), target);
        }
    }
    let mut financial = json!({"payee_locations":[],"tags":[],"balances":[]});
    let mut skipped = 0usize;
    for &table in TABLES {
        let mut added = vec![];
        for source_row in rows(plan, table) {
            let mut row = source_row.clone();
            row["id"] = mapped(&maps, table, &source_row["id"])?;
            if exists(db, budget, table, row["id"].as_str().unwrap())? {
                if table == "transactions" && source_row["tombstone"] != true {
                    skipped += 1;
                }
                continue;
            }
            let refs: &[(&str, &str)] = match table {
                "accounts" => &[("group_id", "account_groups")],
                "categories" => &[
                    ("group_id", "category_groups"),
                    ("replacement_id", "categories"),
                ],
                "payees" => &[
                    ("transfer_account_id", "accounts"),
                    ("category_id", "categories"),
                    ("merged_into", "payees"),
                ],
                "transactions" => &[
                    ("account_id", "accounts"),
                    ("payee_id", "payees"),
                    ("category_id", "categories"),
                    ("parent_id", "transactions"),
                    ("transfer_id", "transactions"),
                ],
                _ => &[],
            };
            for &(field, entity) in refs {
                row[field] = mapped(&maps, entity, &source_row[field])?;
            }
            // Source reconciliation is not part of a destination's completed manual entries.
            if table == "accounts" {
                row["last_reconciled"] = Value::Null;
            }
            added.push(row);
        }
        financial[table] = json!(added);
    }
    conversion::install_ledger(db, budget, &financial)?;
    crate::transfers::validate(db, budget)?;
    for ((table, original), target) in &maps {
        if !retained[*table].is_object() {
            retained[*table] = json!({});
        }
        retained[*table][original] = json!(target);
    }
    db.execute("INSERT INTO budget_provenance(budget_id,id,kind,source_budget_id,metadata) VALUES(?1,?2,'archive-merge',?3,?4) ON CONFLICT(budget_id,id) DO UPDATE SET metadata=excluded.metadata", params![budget, provenance_id, source, retained.to_string()])?;
    for &table in TABLES {
        for row in rows(&financial, table) {
            let id = row["id"].as_str().unwrap();
            if table == "transactions" {
                crate::concurrency::stamp_new_transaction(db, budget, id)?;
            } else {
                let clock = crate::concurrency::clock(db, budget, None)?;
                for field in row
                    .as_object()
                    .unwrap()
                    .keys()
                    .filter(|f| f.as_str() != "id")
                {
                    crate::concurrency::seed_field_version(db, budget, table, id, field, &clock)?;
                }
                crate::concurrency::mark(db, budget, table, id)?;
            }
        }
    }
    Ok(
        json!({"added": rows(&financial, "transactions").iter().filter(|r| r["tombstone"] != true).count(), "already_imported": skipped}),
    )
}
