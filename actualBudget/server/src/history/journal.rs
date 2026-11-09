use super::Owner;
use crate::{concurrency as sync, error::Error, ledger};
use axum::http::StatusCode;
use rusqlite::{
    Connection, OptionalExtension, params,
    types::{Value as SqlValue, ValueRef},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
const TABLES: &[&str] = &[
    "transactions",
    "accounts",
    "account_groups",
    "categories",
    "category_groups",
    "payees",
    "tags",
];
#[derive(Serialize, Deserialize)]
pub struct Change {
    table: String,
    id: String,
    before: Value,
    after: Value,
    fields: Vec<String>,
    full: bool,
    guard: Value,
    versions: Value,
    before_versions: Value,
    after_versions: Value,
    members: Option<Vec<String>>,
}
fn columns(db: &Connection, table: &str) -> Result<Vec<String>, Error> {
    if !TABLES.contains(&table) {
        return Err(Error::internal("Unsupported history table"));
    }
    Ok(db
        .prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |r| r.get(1))?
        .collect::<Result<Vec<_>, _>>()?)
}
pub fn configure(db: &Connection) -> Result<(), Error> {
    db.execute_batch("CREATE TEMP TABLE history_capture(budget_id TEXT NOT NULL,table_name TEXT NOT NULL,id TEXT NOT NULL,before_row TEXT,before_versions TEXT,PRIMARY KEY(budget_id,table_name,id)); CREATE TEMP TABLE history_context(budget_id TEXT PRIMARY KEY);")?;
    for table in TABLES {
        let fields = columns(db, table)?
            .iter()
            .map(|c| format!("'{c}',OLD.\"{c}\""))
            .collect::<Vec<_>>()
            .join(",");
        for (event, reference, before) in [
            (
                "INSERT",
                "NEW",
                format!(
                    "(SELECT json_object({}) FROM main.{table} WHERE budget_id=NEW.budget_id AND id=NEW.id)",
                    fields.replace("OLD.", "")
                ),
            ),
            ("UPDATE", "OLD", format!("json_object({fields})")),
        ] {
            db.execute_batch(&format!("CREATE TEMP TRIGGER history_{table}_{event} BEFORE {event} ON main.{table} WHEN EXISTS(SELECT 1 FROM history_context WHERE budget_id={reference}.budget_id) BEGIN INSERT INTO history_capture VALUES({reference}.budget_id,'{table}',{reference}.id,{before},(SELECT json_group_object(field,clock) FROM field_versions WHERE budget_id={reference}.budget_id AND entity='{table}' AND entity_id={reference}.id)) ON CONFLICT(budget_id,table_name,id) DO NOTHING; END;"))?;
        }
    }
    Ok(())
}
fn row(db: &Connection, budget: &str, table: &str, id: &str) -> Result<Value, Error> {
    let columns = columns(db, table)?;
    Ok(db
        .query_row(
            &format!("SELECT * FROM {table} WHERE budget_id=?1 AND id=?2"),
            params![budget, id],
            |r| {
                let mut values = serde_json::Map::new();
                for (i, column) in columns.iter().enumerate() {
                    let value = match r.get_ref(i)? {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(v) => json!(v),
                        ValueRef::Real(v) => json!(v),
                        ValueRef::Text(v) => json!(String::from_utf8_lossy(v)),
                        ValueRef::Blob(_) => return Err(rusqlite::Error::InvalidQuery),
                    };
                    values.insert(column.clone(), value);
                }
                Ok(Value::Object(values))
            },
        )
        .optional()?
        .unwrap_or(Value::Null))
}
fn versions(
    db: &Connection,
    budget: &str,
    table: &str,
    id: &str,
    fields: &[String],
) -> Result<Value, Error> {
    let mut values = serde_json::Map::new();
    let protected = [
        "tombstone",
        "is_parent",
        "parent_id",
        "reconciled",
        "splits",
    ];
    let mut q = db.prepare(
        "SELECT field,clock FROM field_versions WHERE budget_id=?1 AND entity=?2 AND entity_id=?3",
    )?;
    for pair in q.query_map(params![budget, table, id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (field, clock) = pair?;
        if fields.is_empty()
            || fields.contains(&field)
            || table == "transactions" && protected.contains(&field.as_str())
        {
            values.insert(field, json!(clock));
        }
    }
    Ok(Value::Object(values))
}
fn members(db: &Connection, budget: &str, id: &str) -> Result<Vec<String>, Error> {
    Ok(db.prepare("SELECT id FROM transactions WHERE budget_id=?1 AND parent_id=?2 AND tombstone=0 ORDER BY id")?.query_map(params![budget,id],|r|r.get(0))?.collect::<Result<Vec<_>,_>>()?)
}
fn capture_guard(
    db: &Connection,
    budget: &str,
    owner: &Owner,
    c: &mut Change,
) -> Result<(), Error> {
    let current = row(db, budget, &c.table, &c.id)?;
    c.guard = if c.full {
        current
    } else {
        json!(
            c.fields
                .iter()
                .chain(
                    [
                        "tombstone".to_string(),
                        "is_parent".to_string(),
                        "parent_id".to_string()
                    ]
                    .iter()
                    .filter(|_| c.table == "transactions")
                )
                .map(|f| (f.clone(), current[f].clone()))
                .collect::<serde_json::Map<_, _>>()
        )
    };
    c.versions = normalize(
        db,
        budget,
        owner,
        &c.table,
        &c.id,
        versions(
            db,
            budget,
            &c.table,
            &c.id,
            if c.full { &[] } else { &c.fields },
        )?,
    )?;
    if c.full && c.table == "transactions" {
        c.members = Some(members(db, budget, &c.id)?);
    }
    Ok(())
}
pub fn record(
    db: &Connection,
    budget: &str,
    owner: &Owner,
    label: &str,
    operation: impl FnOnce(&Connection) -> Result<Value, Error>,
) -> Result<Value, Error> {
    db.execute("INSERT INTO history_context VALUES(?1)", [budget])?;
    let result = operation(db)?;
    db.execute("DELETE FROM history_context", [])?;
    let mut changes = vec![];
    let mut q=db.prepare("SELECT table_name,id,before_row,before_versions FROM history_capture WHERE budget_id=?1 ORDER BY rowid")?;
    for item in q.query_map([budget], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?,
        ))
    })? {
        let (table, id, before, before_versions) = item?;
        let before_versions = normalize(
            db,
            budget,
            owner,
            &table,
            &id,
            serde_json::from_str(&before_versions).map_err(Error::internal)?,
        )?;
        let after_versions = normalize(
            db,
            budget,
            owner,
            &table,
            &id,
            versions(db, budget, &table, &id, &[])?,
        )?;
        let before = before
            .map(|v| serde_json::from_str::<Value>(&v))
            .transpose()
            .map_err(Error::internal)?
            .unwrap_or(Value::Null);
        let after = row(db, budget, &table, &id)?;
        let fields: Vec<String> = after
            .as_object()
            .ok_or_else(|| Error::internal("History row removed"))?
            .iter()
            .filter(|(f, v)| !matches!(f.as_str(), "id" | "budget_id") && before[*f] != **v)
            .map(|(f, _)| f.clone())
            .collect();
        if fields.is_empty() && before_versions == after_versions {
            continue;
        }
        let full = before.is_null()
            || fields.is_empty()
            || fields.iter().any(|f| {
                matches!(
                    f.as_str(),
                    "tombstone" | "is_parent" | "parent_id" | "account_id"
                )
            })
            || table == "transactions"
                && (before["is_parent"] == 1
                    || after["is_parent"] == 1
                    || !before["parent_id"].is_null()
                    || !after["parent_id"].is_null());
        let mut change = Change {
            table,
            id,
            before,
            after,
            fields,
            full,
            guard: Value::Null,
            versions: Value::Null,
            before_versions,
            after_versions,
            members: None,
        };
        capture_guard(db, budget, owner, &mut change)?;
        changes.push(change);
    }
    db.execute("DELETE FROM history_capture", [])?;
    if !changes.is_empty() {
        let epoch = crate::lifecycle::epoch(db, budget)?;
        db.execute("DELETE FROM history_actions WHERE budget_id=?1 AND owner=?2 AND (epoch<>?3 OR undone=1)",params![budget,owner.id,epoch])?;
        db.execute("INSERT INTO history_actions(budget_id,owner,epoch,id,label,changes) VALUES(?1,?2,?3,?4,?5,?6)",params![budget,owner.id,epoch,uuid::Uuid::new_v4().to_string(),label_for(label),serde_json::to_string(&changes).map_err(Error::internal)?])?;
        db.execute("DELETE FROM history_actions WHERE budget_id=?1 AND owner=?2 AND epoch=?3 AND sequence NOT IN(SELECT sequence FROM history_actions WHERE budget_id=?1 AND owner=?2 AND epoch=?3 ORDER BY sequence DESC LIMIT 20)",params![budget,owner.id,epoch])?;
    }
    Ok(result)
}
pub fn state(db: &Connection, budget: &str, owner: &Owner) -> Result<Value, Error> {
    crate::store::require_budget(db, budget)?;
    let epoch = crate::lifecycle::epoch(db, budget)?;
    let mut output = json!({"undo":null,"redo":null,"limit":20,"epoch":epoch,"edit_clock":db.query_row("SELECT clock FROM budget_sync WHERE budget_id=?1",[budget],|r|r.get::<_,String>(0))?});
    for (name, undone, order) in [("undo", 0, "DESC"), ("redo", 1, "ASC")] {
        let item=db.query_row(&format!("SELECT id,label FROM history_actions WHERE budget_id=?1 AND owner=?2 AND epoch=?3 AND undone=?4 ORDER BY sequence {order} LIMIT 1"),params![budget,owner.id,epoch,undone],|r|Ok(json!({"id":r.get::<_,String>(0)?,"label":r.get::<_,String>(1)?}))).optional()?;
        output[name] = item.unwrap_or(Value::Null);
    }
    Ok(output)
}
fn conflict() -> Error {
    Error::new(
        StatusCode::CONFLICT,
        "history_conflict",
        "A newer change overlaps this action. Nothing was changed. Refresh and review the affected entries.",
    )
}
fn inverse_error(error: rusqlite::Error) -> Error {
    if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
        conflict()
    } else {
        Error::internal(error)
    }
}
fn sql(value: &Value) -> Result<SqlValue, Error> {
    Ok(match value {
        Value::Null => SqlValue::Null,
        Value::String(v) => SqlValue::Text(v.clone()),
        Value::Number(v) => {
            if let Some(v) = v.as_i64() {
                SqlValue::Integer(v)
            } else {
                SqlValue::Real(
                    v.as_f64()
                        .ok_or_else(|| Error::internal("Invalid history number"))?,
                )
            }
        }
        _ => return Err(Error::internal("Invalid history value")),
    })
}
pub fn apply(
    db: &Connection,
    budget: &str,
    owner: &Owner,
    id: &str,
    redo: bool,
    clock: &str,
) -> Result<Value, Error> {
    let available = state(db, budget, owner)?;
    if available[if redo { "redo" } else { "undo" }]["id"] != id {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "history_changed",
            "The available history action changed. Refresh before trying again.",
        ));
    }
    let saved: String = db.query_row(
        "SELECT changes FROM history_actions WHERE id=?1",
        [id],
        |r| r.get(0),
    )?;
    let mut changes: Vec<Change> = serde_json::from_str(&saved).map_err(Error::internal)?;
    for c in &changes {
        let current = row(db, budget, &c.table, &c.id)?;
        if versions(
            db,
            budget,
            &c.table,
            &c.id,
            if c.full { &[] } else { &c.fields },
        )?
        .as_object()
        .unwrap()
        .values()
        .any(|v| v.as_str().is_some_and(|v| v >= clock))
        {
            return Err(conflict());
        }
        if c.full && current != c.guard
            || !c.full
                && c.guard
                    .as_object()
                    .unwrap()
                    .iter()
                    .any(|(f, v)| current[f] != *v)
            || normalize(
                db,
                budget,
                owner,
                &c.table,
                &c.id,
                versions(
                    db,
                    budget,
                    &c.table,
                    &c.id,
                    if c.full { &[] } else { &c.fields },
                )?,
            )? != c.versions
            || c.members.as_ref().is_some_and(|expected| {
                members(db, budget, &c.id)
                    .as_ref()
                    .map_or(true, |actual| actual != expected)
            })
        {
            return Err(conflict());
        }
        if c.table == "transactions"
            && current["reconciled"] == 1
            && !c.fields.iter().any(|f| f == "reconciled")
        {
            return Err(conflict());
        }
    }
    let mut accounts = BTreeSet::new();
    for c in &changes {
        let target = if redo { &c.after } else { &c.before };
        let mut fields = if target.is_null() {
            vec!["tombstone".to_string()]
        } else {
            c.fields.clone()
        };
        if target.is_null()
            && c.table == "transactions"
            && changes.iter().any(|category| {
                category.table == "categories"
                    && category.before.is_null()
                    && c.after["category_id"] == category.id
            })
        {
            fields.push("category_id".into());
        }
        let mut values = fields
            .iter()
            .map(|f| {
                if target.is_null() {
                    Ok(if f == "tombstone" {
                        SqlValue::Integer(1)
                    } else {
                        SqlValue::Null
                    })
                } else {
                    sql(&target[f])
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        values.push(SqlValue::Text(budget.to_owned()));
        values.push(SqlValue::Text(c.id.clone()));
        if !fields.is_empty() {
            db.execute(
                &format!(
                    "UPDATE {} SET {} WHERE budget_id=?{} AND id=?{}",
                    c.table,
                    fields
                        .iter()
                        .enumerate()
                        .map(|(i, f)| format!("\"{f}\"=?{}", i + 1))
                        .collect::<Vec<_>>()
                        .join(","),
                    fields.len() + 1,
                    fields.len() + 2
                ),
                rusqlite::params_from_iter(values),
            )
            .map_err(inverse_error)?;
        }
        // Creation redo must explicitly resurrect its retained identity.
        if redo && c.before.is_null() {
            db.execute(
                &format!(
                    "UPDATE {} SET tombstone=0 WHERE budget_id=?1 AND id=?2",
                    c.table
                ),
                params![budget, c.id],
            )
            .map_err(inverse_error)?;
        }
        let mut stamp_fields = fields.iter().cloned().collect::<BTreeSet<_>>();
        if c.full {
            stamp_fields.extend(c.versions.as_object().unwrap().keys().cloned());
        }
        if c.before.is_null() {
            stamp_fields.insert("tombstone".into());
        }
        let target_versions = if redo {
            &c.after_versions
        } else {
            &c.before_versions
        };
        for field in stamp_fields {
            sync::seed_field_version(db, budget, &c.table, &c.id, &field, clock)?;
            let lineage = target_versions[&field].as_str().unwrap_or(sync::ZERO);
            db.execute(
                "INSERT INTO history_lineage VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    budget,
                    owner.id,
                    crate::lifecycle::epoch(db, budget)?,
                    c.table,
                    c.id,
                    field,
                    clock,
                    lineage
                ],
            )?;
        }
        sync::mark(db, budget, &c.table, &c.id)?;
        if c.table == "transactions" {
            for value in [&c.before, &c.after] {
                if let Some(account) = value["account_id"].as_str() {
                    accounts.insert(account.to_string());
                }
            }
        }
    }
    // Structural safety is checked after all rows, within the receipt transaction.
    for change in &changes {
        if change.table == "account_groups" {
            let restored = row(db, budget, &change.table, &change.id)?;
            if restored["tombstone"] == 0 {
                crate::accounts::group_name(
                    db,
                    budget,
                    &change.id,
                    restored["name"].as_str().ok_or_else(conflict)?,
                )
                .map_err(|error| {
                    if error.status.is_client_error() {
                        conflict()
                    } else {
                        error
                    }
                })?;
            }
        }
    }
    crate::splits::validate(db, budget).map_err(|_| conflict())?;
    crate::categories::validate_archive(db, budget).map_err(|_| conflict())?;
    crate::payees_tags::validate_archive(db, budget).map_err(|_| conflict())?;
    let invalid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM transactions t JOIN accounts a ON a.budget_id=t.budget_id AND a.id=t.account_id WHERE t.budget_id=?1 AND t.tombstone=0 AND a.tombstone=1)",[budget],|r|r.get(0))?;
    if invalid {
        return Err(conflict());
    }
    let dangling_group:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM accounts a LEFT JOIN account_groups g ON g.budget_id=a.budget_id AND g.id=a.group_id WHERE a.budget_id=?1 AND a.tombstone=0 AND a.group_id IS NOT NULL AND (g.id IS NULL OR g.tombstone=1))",[budget],|r|r.get(0))?;
    if dangling_group {
        return Err(conflict());
    }
    for account in accounts {
        ledger::refresh_account(db, budget, &account)?;
    }
    ledger::validate_budget_total(db, budget)?;
    for c in &mut changes {
        capture_guard(db, budget, owner, c)?;
    }
    db.execute(
        "UPDATE history_actions SET undone=?2,changes=?3 WHERE id=?1",
        params![
            id,
            !redo,
            serde_json::to_string(&changes).map_err(Error::internal)?
        ],
    )?;
    Ok(
        json!({"action_id":id,"affected_count":changes.iter().filter(|c|c.table=="transactions").count(),"history":state(db,budget,owner)?}),
    )
}

fn normalize(
    db: &Connection,
    budget: &str,
    owner: &Owner,
    table: &str,
    id: &str,
    mut values: Value,
) -> Result<Value, Error> {
    let epoch = crate::lifecycle::epoch(db, budget)?;
    let map = values
        .as_object_mut()
        .ok_or_else(|| Error::internal("Invalid history versions"))?;
    for (field, value) in map.iter_mut() {
        let clock = value
            .as_str()
            .ok_or_else(|| Error::internal("Invalid history clock"))?;
        let lineage:Option<String>=db.query_row("SELECT lineage FROM history_lineage WHERE budget_id=?1 AND owner=?2 AND epoch=?3 AND entity=?4 AND entity_id=?5 AND field=?6 AND clock=?7",params![budget,owner.id,epoch,table,id,field,clock],|r|r.get(0)).optional()?;
        if let Some(lineage) = lineage {
            *value = json!(lineage);
        }
    }
    map.retain(|_, v| v != sync::ZERO);
    Ok(values)
}

fn label_for(scope: &str) -> &'static str {
    if scope.contains("/transactions/") && scope.ends_with("/bulk") {
        "Bulk transaction change"
    } else if scope.ends_with("/splits") {
        "Edit split"
    } else if scope.ends_with("/duplicate") {
        "Duplicate transaction"
    } else if scope.contains("/transactions") {
        if scope.starts_with("DELETE") {
            "Delete transaction"
        } else if scope.starts_with("POST") {
            "Add transaction"
        } else {
            "Edit transaction"
        }
    } else if scope.contains("/account-groups") {
        "Edit account groups"
    } else if scope.contains("/accounts") {
        "Edit accounts"
    } else if scope.contains("/category") {
        "Edit categories"
    } else if scope.contains("/payees") {
        "Edit payees"
    } else if scope.contains("/tags") {
        "Edit tags"
    } else {
        "Edit budget"
    }
}
