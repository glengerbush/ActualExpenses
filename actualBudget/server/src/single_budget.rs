//! One active household ledger. Historical identities survive only as recovery evidence.
mod merge;
mod provenance;
use crate::{error::Error, models::NewBudget, store};
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;

pub fn current(db: &Connection) -> Result<Value, Error> {
    let id: Option<String> = db
        .query_row("SELECT id FROM budgets WHERE deleted=0", [], |r| r.get(0))
        .optional()?;
    let mut result = match id {
        Some(id) => crate::lifecycle::metadata(db, &id, false),
        None => store::create_budget(
            db,
            NewBudget {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Budget".into(),
                budget_type: "tracking".into(),
            },
        ),
    }?;
    result["empty"] = serde_json::json!(is_empty(db, result["id"].as_str().unwrap())?);
    Ok(result)
}

pub fn is_empty(db: &Connection, budget: &str) -> Result<bool, Error> {
    Ok(db.query_row("SELECT NOT(EXISTS(SELECT 1 FROM transactions WHERE budget_id=?1) OR EXISTS(SELECT 1 FROM category_groups WHERE budget_id=?1) OR EXISTS(SELECT 1 FROM categories WHERE budget_id=?1 AND NOT(name='Starting Balances' AND is_income=1)) OR EXISTS(SELECT 1 FROM budget_cells WHERE budget_id=?1) OR EXISTS(SELECT 1 FROM payees WHERE budget_id=?1) OR EXISTS(SELECT 1 FROM rules WHERE budget_id=?1) OR EXISTS(SELECT 1 FROM schedules WHERE budget_id=?1) OR EXISTS(SELECT 1 FROM reporting_documents WHERE budget_id=?1))", [budget], |r| r.get(0))?)
}

pub fn migrate(db: &Connection) -> Result<(), Error> {
    let ids = db
        .prepare("SELECT id FROM budgets WHERE deleted=0 ORDER BY rowid")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(target) = ids.first()
        && ids.len() > 1
    {
        // The outer schema migration also creates a standalone SQLite backup before its transaction.
        for id in &ids {
            let name: String =
                db.query_row("SELECT name FROM budgets WHERE id=?1", [id], |r| r.get(0))?;
            crate::lifecycle::make_point(db, id, "upgrade", &format!("Before merging {name}"))?;
        }
        db.execute_batch("PRAGMA defer_foreign_keys=ON")?;
        let primary =
            crate::lifecycle::archive::validate(&crate::lifecycle::archive::capture(db, target)?)?;
        provenance::preserve(db, target, &primary, &std::collections::BTreeMap::new())?;
        for source in ids.iter().skip(1) {
            merge::copy(db, source, target)?;
        }
        crate::lifecycle::archive::validate_financial_state(db, target)?;
        db.execute(
            "UPDATE budget_sync SET epoch=lower(hex(randomblob(16))) WHERE budget_id=?1",
            [target],
        )?;
        db.execute(
            "UPDATE budget_lifecycle SET generation=generation+1 WHERE budget_id=?1",
            [target],
        )?;
    }
    db.execute_batch("CREATE UNIQUE INDEX budgets_single_active ON budgets((1)) WHERE deleted=0")?;
    Ok(())
}

/// Restore archives from any former ledger into the only active identity.
/// Source receipts cannot be replayed as if they had run against a different budget.
pub fn retarget(
    mut value: crate::lifecycle::archive::Validated,
    target: &str,
) -> Result<crate::lifecycle::archive::Validated, Error> {
    let original = value.state.budget["id"]
        .as_str()
        .ok_or_else(|| Error::invalid("Missing archive identity"))?
        .to_owned();
    if original == target {
        return Ok(value);
    }
    provenance::materialize(&mut value)?;
    let mut occurrence_ids = std::collections::BTreeMap::new();
    if let Some(table) = value.state.tables.get("schedule_occurrences") {
        let column = |name: &str| table.columns.iter().position(|c| c == name).unwrap();
        for row in &table.rows {
            let id = row[column("id")].as_str().unwrap();
            let schedule = row[column("schedule_id")].as_str().unwrap();
            let date = row[column("occurrence_date")].as_str().unwrap();
            occurrence_ids.insert(
                id.to_owned(),
                merge::identity(&[
                    "actualBudget.schedule-occurrence.v1",
                    target,
                    schedule,
                    date,
                ]),
            );
        }
    }
    value.state.budget["id"] = serde_json::json!(target);
    value.state.receipts.clear();
    for (name, table) in &mut value.state.tables {
        if matches!(
            name.as_str(),
            "staged_imports" | "imported_sources" | "imported_field_clocks"
        ) {
            table.rows.clear();
            continue;
        }
        let index = table
            .columns
            .iter()
            .position(|c| c == "budget_id")
            .ok_or_else(|| Error::invalid("Missing archive scope"))?;
        for row in &mut table.rows {
            row[index] = serde_json::json!(target);
            for (column, cell) in table.columns.iter().zip(row.iter_mut()) {
                if (column == "id" || column.ends_with("_id"))
                    && let Some(mapped) = cell.as_str().and_then(|id| occurrence_ids.get(id))
                {
                    *cell = serde_json::json!(mapped);
                }
            }
            if name == "field_versions"
                && let Some(i) = table.columns.iter().position(|c| c == "entity_id")
                && row[i] == original
            {
                row[i] = serde_json::json!(target);
            }
        }
    }
    Ok(value)
}
