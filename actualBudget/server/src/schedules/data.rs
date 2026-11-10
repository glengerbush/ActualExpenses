use super::*;
use sha2::{Digest, Sha256};
pub fn identity(parts: &[&str]) -> String {
    let bytes = Sha256::digest(json!(parts).to_string().as_bytes());
    let mut id = [0u8; 16];
    id.copy_from_slice(&bytes[..16]);
    id[6] = (id[6] & 15) | 64;
    id[8] = (id[8] & 63) | 128;
    uuid::Uuid::from_bytes(id).to_string()
}
pub fn stamp(
    db: &Connection,
    b: &str,
    table: &str,
    id: &str,
    fields: &[&str],
    clock: &str,
) -> Result<(), Error> {
    for field in fields {
        sync::seed_field_version(db, b, table, id, field, clock)?;
    }
    sync::mark(db, b, table, id)
}
pub fn guard(
    db: &Connection,
    b: &str,
    table: &str,
    id: &str,
    fields: &[&str],
    clock: &str,
) -> Result<(), Error> {
    for field in fields {
        if db.query_row("SELECT EXISTS(SELECT 1 FROM field_versions WHERE budget_id=?1 AND entity=?2 AND entity_id=?3 AND field=?4 AND clock>=?5)",params![b,table,id,field,clock],|r|r.get::<_,bool>(0))?{return Err(Error::conflict("A newer schedule change must be reviewed before saving"));}
    }
    Ok(())
}

fn read(
    db: &Connection,
    b: &str,
    id: Option<&str>,
    offset: usize,
    limit: usize,
    account: Option<&str>,
    active_only: bool,
) -> Result<Vec<Value>, Error> {
    crate::store::require_budget(db, b)?;
    let mut query=db.prepare("SELECT s.id,s.rule_id,s.name,s.posts_transaction,s.completed,s.next_date,s.custom_upcoming_length,r.conditions,r.actions FROM schedules s LEFT JOIN rules r ON r.budget_id=s.budget_id AND r.id=s.rule_id AND r.tombstone=0 WHERE s.budget_id=?1 AND s.tombstone=0 AND (?6=0 OR s.completed=0) AND (?2 IS NULL OR s.id=?2) AND (?5 IS NULL OR EXISTS(SELECT 1 FROM json_each(r.conditions) c WHERE json_extract(c.value,'$.field') IN('account','acct') AND json_extract(c.value,'$.op')='is' AND json_extract(c.value,'$.value')=?5)) ORDER BY s.next_date,s.id LIMIT ?3 OFFSET ?4")?;
    let rows=query.query_map(params![b,id,limit as i64,offset as i64,account,active_only],|r|Ok((json!({"id":r.get::<_,String>(0)?,"rule_id":r.get::<_,String>(1)?,"name":r.get::<_,Option<String>>(2)?,"posts_transaction":r.get::<_,bool>(3)?,"completed":r.get::<_,bool>(4)?,"next_date":r.get::<_,Option<String>>(5)?,"custom_upcoming_length":r.get::<_,Option<String>>(6)?}),r.get::<_,Option<String>>(7)?,r.get::<_,Option<String>>(8)?)))?.collect::<Result<Vec<_>,_>>()?;
    rows.into_iter()
        .map(|(mut row, conditions, actions)| {
            row["conditions"] = serde_json::from_str(
                &conditions.ok_or_else(|| Error::invalid("Schedule is missing its live rule"))?,
            )
            .map_err(Error::internal)?;
            row["actions"] = serde_json::from_str(
                &actions.ok_or_else(|| Error::invalid("Schedule is missing its live rule"))?,
            )
            .map_err(Error::internal)?;
            Ok(row)
        })
        .collect()
}
pub(super) fn forecast_rows(db: &Connection, b: &str) -> Result<Vec<Value>, Error> {
    let sizes=db.prepare("SELECT coalesce(length(CAST(r.conditions AS BLOB)),0)+coalesce(length(CAST(r.actions AS BLOB)),0) FROM schedules s LEFT JOIN rules r ON r.budget_id=s.budget_id AND r.id=s.rule_id AND r.tombstone=0 WHERE s.budget_id=?1 AND s.tombstone=0 AND s.completed=0 LIMIT 5001")?.query_map([b],|r|r.get::<_,i64>(0))?.collect::<Result<Vec<_>,_>>()?;
    if sizes.len() > 5000 || sizes.iter().sum::<i64>() > 8 * 1024 * 1024 {
        return Err(Error::invalid(
            "The active schedule set exceeds forecast limits. Review large or obsolete schedules before calculating.",
        ));
    }
    read(db, b, None, 0, 5000, None, true)
}
pub fn get(db: &Connection, b: &str, id: &str) -> Result<Value, Error> {
    read(db, b, Some(id), 0, 1, None, false)?
        .pop()
        .ok_or_else(Error::missing)
}
pub fn list(db: &Connection, b: &str) -> Result<Vec<Value>, Error> {
    let rows = read(db, b, None, 0, 100001, None, false)?;
    if rows.len() > 100000 {
        return Err(Error::invalid("Schedule set exceeds100000 rows"));
    }
    Ok(rows)
}
pub fn page(db: &Connection, b: &str, query: &super::Page) -> Result<Vec<Value>, Error> {
    read(
        db,
        b,
        None,
        query.offset,
        query.limit + 1,
        query.account_id.as_deref(),
        false,
    )
}
pub fn link(db: &Connection, b: &str, id: &str, schedule: &str, clock: &str) -> Result<(), Error> {
    get(db, b, schedule)?;
    guard(
        db,
        b,
        "schedule_links",
        id,
        &["schedule_id", "tombstone"],
        clock,
    )?;
    let previous: Option<String> = db
        .query_row(
            "SELECT schedule_id FROM schedule_links WHERE budget_id=?1 AND id=?2 AND tombstone=0",
            params![b, id],
            |r| r.get(0),
        )
        .optional()?;
    if previous.as_deref().is_some_and(|old| old != schedule) {
        super::links::release_payment(db, b, id, Some(schedule), clock)?;
    }
    db.execute("INSERT INTO schedule_links(budget_id,id,schedule_id) VALUES(?1,?2,?3) ON CONFLICT(budget_id,id) DO UPDATE SET schedule_id=excluded.schedule_id,tombstone=0",params![b,id,schedule])?;
    stamp(
        db,
        b,
        "schedule_links",
        id,
        &["schedule_id", "tombstone"],
        clock,
    )?;
    sync::mark(db, b, "transactions", id)
}
pub fn decorate(db: &Connection, b: &str, rows: &mut [Value]) -> Result<(), Error> {
    for batch in rows.chunks_mut(500) {
        if batch.is_empty() {
            continue;
        }
        let placeholders = (2..=batch.len() + 1)
            .map(|n| format!("?{n}"))
            .collect::<Vec<_>>()
            .join(",");
        let mut args = vec![b.to_owned()];
        args.extend(batch.iter().map(|r| r["id"].as_str().unwrap().to_owned()));
        let values=db.prepare(&format!("SELECT l.id,l.schedule_id FROM schedule_links l JOIN schedules s ON s.budget_id=l.budget_id AND s.id=l.schedule_id AND s.tombstone=0 WHERE l.budget_id=?1 AND l.tombstone=0 AND l.id IN ({placeholders})"))?.query_map(rusqlite::params_from_iter(args.iter()),|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<Result<std::collections::BTreeMap<_,_>,_>>()?;
        for row in batch {
            if let Some(id) = values.get(row["id"].as_str().unwrap()) {
                row["schedule_id"] = json!(id);
            }
        }
    }
    Ok(())
}
