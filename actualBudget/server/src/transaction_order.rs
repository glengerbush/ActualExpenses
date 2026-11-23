//! Dense display keys preserve source ordering without rewriting native ranks or
//! saved history guards. Existing native rows remain stable ordering anchors.
use crate::error::Error;
use rusqlite::{Connection, OptionalExtension, functions::FunctionFlags, params};
use serde_json::Value;

pub const SQL: &str = "COALESCE((SELECT display_key FROM android_upgrade_order ao WHERE ao.budget_id=t.budget_id AND ao.id=t.id),actual_order_key(t.sort_order))";
pub fn key(rank: i64) -> String {
    format!("{:016x}", (rank as u64) ^ (1_u64 << 63))
}
pub fn configure(db: &Connection) -> Result<(), Error> {
    db.create_scalar_function(
        "actual_order_key",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| Ok(key(ctx.get(0)?)),
    )?;
    Ok(())
}
pub fn seed(db: &Connection, budget: &str, id: &str, source: Option<f64>) -> Result<(), Error> {
    if source.is_some_and(|v| !v.is_finite()) {
        return Err(Error::invalid("Non-finite original transaction order"));
    }
    let rank: Option<i64> = db
        .query_row(
            "SELECT sort_order FROM transactions WHERE budget_id=?1 AND id=?2",
            params![budget, id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(rank) = rank {
        db.execute("INSERT OR IGNORE INTO android_upgrade_order(budget_id,id,source_order,display_key) VALUES(?1,?2,?3,?4)",params![budget,id,source,key(rank)])?;
    }
    Ok(())
}

pub fn decorate(db: &Connection, budget: &str, rows: &mut [Value]) -> Result<(), Error> {
    if rows.is_empty() {
        return Ok(());
    }
    let positions = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (r["id"].as_str().unwrap().to_owned(), i))
        .collect::<std::collections::HashMap<_, _>>();
    let mut values = vec![budget.to_owned()];
    values.extend(rows.iter().map(|r| r["id"].as_str().unwrap().to_owned()));
    let placeholders = (2..=values.len())
        .map(|n| format!("?{n}"))
        .collect::<Vec<_>>()
        .join(",");
    let mut q=db.prepare(&format!("SELECT id,display_key FROM android_upgrade_order WHERE budget_id=?1 AND id IN ({placeholders})"))?;
    for r in q.query_map(rusqlite::params_from_iter(values.iter()), |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (id, display) = r?;
        let row = &mut rows[positions[&id]];
        if display != key(row["sort_order"].as_i64().unwrap_or(0)) {
            row["order_key"] = serde_json::json!(display);
        }
    }
    Ok(())
}
pub fn validate(db: &Connection, budget: &str) -> Result<(), Error> {
    let mut q = db
        .prepare("SELECT source_order,display_key FROM android_upgrade_order WHERE budget_id=?1")?;
    for row in q.query_map([budget], |r| {
        Ok((r.get::<_, Option<f64>>(0)?, r.get::<_, String>(1)?))
    })? {
        let (value, key) = row?;
        if value.is_some_and(|n| !n.is_finite())
            || key.len() < 16
            || key.len() > 4096
            || !key
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(Error::invalid("Invalid original transaction ordering"));
        }
    }
    Ok(())
}
