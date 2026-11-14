//! Reconciliation compares every candidate against the same captured ledger.
//! Duplicate primary IDs coalesce only genuinely changed source proposals.
use super::*;
pub(super) fn ignored(before: &Value, next: &Value) -> bool {
    if before["reconciled"] == true {
        return true;
    }
    for field in ["payee_id", "category_id"] {
        if (before[field].is_null() || before[field] == "")
            && !next[field].is_null()
            && next[field] != ""
            && before[field] != next[field]
        {
            return false;
        }
    }
    if before["cleared"] != true && next["cleared"] == true {
        return false;
    }
    for field in ["imported_id", "imported_payee"] {
        let value = |row: &Value| {
            row[field]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        if value(before) != value(next) {
            return false;
        }
    }
    !(before["raw_synced_data"].is_null()
        && next["raw_synced_data"]
            .as_str()
            .is_some_and(|s| !s.is_empty()))
}
pub(super) struct Intent<'a> {
    pub clock: &'a str,
    pub epoch: Option<&'a str>,
    pub work: &'a mut crate::rules::Work,
}
pub(super) fn patch(
    db: &Connection,
    b: &str,
    id: &str,
    before: &Value,
    fields: Value,
    intent: Intent<'_>,
) -> Result<Value, Error> {
    let Intent { clock, epoch, work } = intent;
    let mutation = if before["is_parent"] == true {
        if fields["category_id"].is_string() {
            return Err(Error::conflict(
                "Review the matched split's allocations before assigning a category; no file rows were saved",
            ));
        }
        let rows = before["children"]
            .as_array()
            .ok_or_else(|| Error::invalid("The matched split needs review"))?;
        work.remaining = work
            .remaining
            .checked_sub(rows.len() + 1)
            .ok_or_else(|| Error::invalid("Import graph work limit reached. Select fewer rows"))?;
        let children = rows
            .iter()
            .map(|row| {
                Ok(crate::splits::Child {
                    id: row["id"].as_str().unwrap().into(),
                    amount: row["amount"].as_i64().unwrap(),
                    payee_id: row["retained_payee_id"].as_str().map(str::to_owned),
                    category_id: row["category_id"].as_str().map(str::to_owned),
                    notes: row["notes"].as_str().unwrap_or("").into(),
                    transfer_account_id: row["transfer_account_id"].as_str().map(str::to_owned),
                    counterpart_id: row["transfer_id"].as_str().map(str::to_owned),
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        sync::Mutation::Split(
            id.into(),
            crate::splits::Change {
                clear_parent_references: false,
                snapshot: sync::cursor(db, b)?,
                action: crate::splits::Action::Replace,
                children,
                allocation: crate::splits::Allocation::Exact,
                detach: vec![],
                fields: Some(fields),
                confirm_reconciled: false,
                reconciled_versions: BTreeMap::new(),
                reconciliation_token: None,
                transfer_protection_token: None,
                changes: None,
            },
        )
    } else {
        sync::Mutation::Patch(id.into(), fields)
    };
    sync::execute(db, b, Some(clock), false, epoch, mutation)
}
/// Only rows created by this command receive source file ordering. Existing
/// Android/native anchors and their saved history generations remain untouched.
pub(super) fn order(
    db: &Connection,
    b: &str,
    added: &[String],
    base: i64,
    clock: &str,
    work: &mut crate::rules::Work,
) -> Result<(), Error> {
    let mut rows = vec![];
    for root in added {
        let mut q=db.prepare_cached("SELECT id,transfer_id FROM transactions WHERE budget_id=?1 AND tombstone=0 AND (id=?2 OR parent_id=?2) ORDER BY parent_id IS NOT NULL,sort_order DESC,id")?;
        for row in q.query_map(params![b, root], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })? {
            work.remaining = work.remaining.checked_sub(1).ok_or_else(|| {
                Error::invalid("Import graph work limit reached. Select fewer rows")
            })?;
            rows.push(row?);
        }
    }
    let end = base
        .checked_add(rows.len() as i64)
        .ok_or_else(|| Error::invalid("Transaction order is outside supported range"))?;
    for (index, (id, peer)) in rows.iter().enumerate() {
        let rank = end - index as i64;
        db.prepare_cached("UPDATE transactions SET sort_order=?3 WHERE budget_id=?1 AND id=?2")?
            .execute(params![b, id, rank])?;
        if let Some(peer) = peer {
            db.prepare_cached(
                "UPDATE transactions SET sort_order=?3 WHERE budget_id=?1 AND id=?2",
            )?
            .execute(params![b, peer, rank])?;
        }
        sync::seed_field_version(db, b, "transactions", id, "sort_order", clock)?;
        sync::mark(db, b, "transactions", id)?;
        if let Some(peer) = peer {
            sync::seed_field_version(db, b, "transactions", peer, "sort_order", clock)?;
            sync::mark(db, b, "transactions", peer)?;
        }
    }
    Ok(())
}
