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
pub fn enrich(db: &Connection, only: Option<&str>) -> Result<(), Error> {
    let mut q=db.prepare("SELECT i.budget_id,s.original FROM imported_sources i JOIN staged_imports s ON s.id=i.import_id AND COALESCE(json_extract(s.report,'$.source_format'),'actual') NOT IN('ynab4','nynab') WHERE (?1 IS NULL OR i.budget_id=?1)")?;
    let imports = q
        .query_map([only], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (budget, original) in imports {
        for (id, order) in crate::imports::original_orders(&original)? {
            seed(db, &budget, &id, order)?;
        }
    }
    Ok(())
}
fn between(lower: &str, upper: Option<&str>) -> Result<String, Error> {
    if upper.is_some_and(|b| lower >= b) {
        return Err(Error::internal("Invalid ordering anchors"));
    }
    let mut candidate = format!("{lower}8");
    if let Some(upper) = upper
        && candidate.as_str() >= upper
    {
        let suffix = upper
            .strip_prefix(lower)
            .ok_or_else(|| Error::internal("Invalid dense order interval"))?;
        let mut prefix = lower.to_owned();
        let mut found = false;
        for c in suffix.chars() {
            let digit = c
                .to_digit(16)
                .ok_or_else(|| Error::internal("Invalid order key"))?;
            if digit == 0 {
                prefix.push('0');
                continue;
            }
            prefix.push(char::from_digit(digit / 2, 16).unwrap());
            while prefix.len() < 16 {
                prefix.push('0');
            }
            prefix.push('8');
            candidate = prefix;
            found = true;
            break;
        }
        if !found {
            return Err(Error::conflict(
                "Saved ordering needs review; original work is retained",
            ));
        }
    }
    if candidate.len() > 4096 || upper.is_some_and(|b| candidate.as_str() >= b) {
        return Err(Error::conflict(
            "Saved ordering needs review; original work is retained",
        ));
    }
    Ok(candidate)
}
pub fn insert(db: &Connection, budget: &str, id: &str, source: f64) -> Result<(), Error> {
    if !source.is_finite() {
        return Err(Error::invalid("Invalid original transaction order"));
    }
    // Source equal-order groups share a key, retaining the original ID tie-break.
    let equal:Option<String>=db.query_row("SELECT o.display_key FROM android_upgrade_order o JOIN transactions t ON t.budget_id=o.budget_id AND t.id=o.id WHERE o.budget_id=?1 AND o.source_order=?2 ORDER BY o.display_key LIMIT 1",params![budget,source],|r|r.get(0)).optional()?;
    let display = if let Some(equal) = equal {
        equal
    } else {
        let upper_source:Option<String>=db.query_row("SELECT o.display_key FROM android_upgrade_order o JOIN transactions t ON t.budget_id=o.budget_id AND t.id=o.id WHERE o.budget_id=?1 AND o.source_order>?2 ORDER BY o.source_order,o.display_key LIMIT 1",params![budget,source],|r|r.get(0)).optional()?;
        let lower_source:Option<String>=db.query_row("SELECT o.display_key FROM android_upgrade_order o JOIN transactions t ON t.budget_id=o.budget_id AND t.id=o.id WHERE o.budget_id=?1 AND (o.source_order<?2 OR o.source_order IS NULL) ORDER BY o.source_order DESC,o.display_key DESC LIMIT 1",params![budget,source],|r|r.get(0)).optional()?;
        // Native rows after the last source row stay above the recovered old queue.
        let upper = if upper_source.is_some() {
            upper_source
        } else {
            db.query_row(&format!("SELECT {SQL} FROM transactions t WHERE t.budget_id=?1 AND t.id<>?2 AND (?3 IS NULL OR {SQL}>?3) ORDER BY {SQL} LIMIT 1"),params![budget,id,lower_source],|r|r.get::<_,String>(0)).optional()?
        };
        let lower:Option<String>=db.query_row(&format!("SELECT {SQL} FROM transactions t WHERE t.budget_id=?1 AND t.id<>?2 AND (?3 IS NULL OR {SQL}<?3) ORDER BY {SQL} DESC LIMIT 1"),params![budget,id,upper],|r|r.get(0)).optional()?;
        between(
            lower.as_deref().unwrap_or("0000000000000000"),
            upper.as_deref(),
        )?
    };
    db.execute(
        "INSERT INTO android_upgrade_order VALUES(?1,?2,?3,?4)",
        params![budget, id, source, display],
    )?;
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
