//! Explicit matching changes links, never merges or removes financial rows.
use super::*;
pub fn apply(db: &Connection, b: &str, c: &commands::Command, clock: &str) -> Result<Value, Error> {
    let schedule =
        c.id.as_deref()
            .ok_or_else(|| Error::invalid("Choose a schedule"))?;
    crate::models::validate_id(schedule)?;
    let row = data::get(db, b, schedule)?;
    let selected = c
        .transaction_ids
        .as_ref()
        .ok_or_else(|| Error::invalid("Select transaction IDs"))?;
    if selected.is_empty() || selected.len() > 500 {
        return Err(Error::invalid("Select1–500 transactions"));
    }
    let mut unique = std::collections::BTreeSet::new();
    for id in selected {
        crate::models::validate_id(id)?;
        if !unique.insert(id.as_str()) {
            return Err(Error::invalid("Select each transaction only once"));
        }
    }
    let protected = crate::transfers::protection::members(
        db,
        b,
        &selected.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    if protected.iter().any(|r| r["reconciled"] == true) {
        return Err(Error::conflict(
            "Unlock affected reconciled entries, then review schedule matching again. No links were changed.",
        ));
    }
    let rows = crate::ledger::get_many(db, b, selected)?;
    if rows.len() != selected.len() {
        return Err(Error::missing());
    }
    let mut affected = std::collections::BTreeSet::new();
    for row in rows {
        let id = row["id"].as_str().unwrap();
        let live: bool = db.query_row(
            "SELECT tombstone=0 FROM transactions WHERE budget_id=?1 AND id=?2",
            params![b, id],
            |r| r.get(0),
        )?;
        if !live {
            return Err(Error::conflict(
                "A selected transaction was deleted. Refresh the matches.",
            ));
        }
        affected.insert(id.to_owned());
        // Parent allocations keep their own links; an ordinary leg or transfer child
        // shares the explicit link with its reciprocal transaction.
        if row["is_parent"] != true
            && let Some(peer) = row["transfer_id"].as_str()
        {
            affected.insert(peer.to_owned());
        }
    }
    let mut changed = vec![];

    for id in &affected {
        let current:Option<String>=db.query_row("SELECT schedule_id FROM schedule_links WHERE budget_id=?1 AND id=?2 AND tombstone=0",params![b,id],|r|r.get(0)).optional()?;
        if c.action != "unlink" {
            if current.as_deref() != Some(schedule) {
                data::link(db, b, id, schedule, clock)?;
                changed.push(id.clone());
            }
        } else {
            if current.as_deref() != Some(schedule) {
                return Err(Error::conflict(
                    "A selected transaction no longer belongs to this schedule. Refresh and review.",
                ));
            }
            data::guard(
                db,
                b,
                "schedule_links",
                id,
                &["schedule_id", "tombstone"],
                clock,
            )?;
            db.execute(
                "UPDATE schedule_links SET tombstone=1 WHERE budget_id=?1 AND id=?2",
                params![b, id],
            )?;
            data::stamp(db, b, "schedule_links", id, &["tombstone"], clock)?;
            sync::mark(db, b, "transactions", id)?;
            changed.push(id.clone());
        }
    }
    if c.action == "unlink" {
        for id in &changed {
            release_payment(db, b, id, None, clock)?;
        }
    }
    Ok(json!({"affected_count":changed.len(),"transaction_ids":changed,"schedule":row}))
}

/// A payment moved away from a schedule no longer proves that schedule paid.
/// The original tombstone-field HLC is retained as the next posting's release
/// token. Financial rows and original operation receipts remain untouched.
pub(super) fn release_payment(
    db: &Connection,
    b: &str,
    transaction: &str,
    retained_schedule: Option<&str>,
    clock: &str,
) -> Result<(), Error> {
    let claims=db.prepare("SELECT id FROM schedule_occurrences WHERE budget_id=?1 AND transaction_id=?2 AND tombstone=0 AND (?3 IS NULL OR schedule_id<>?3)")?.query_map(params![b,transaction,retained_schedule],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
    for id in claims {
        data::guard(
            db,
            b,
            "schedule_occurrences",
            &id,
            &["status", "transaction_id", "tombstone"],
            clock,
        )?;
        db.execute(
            "UPDATE schedule_occurrences SET tombstone=1 WHERE budget_id=?1 AND id=?2",
            params![b, id],
        )?;
        data::stamp(db, b, "schedule_occurrences", &id, &["tombstone"], clock)?;
    }
    Ok(())
}
