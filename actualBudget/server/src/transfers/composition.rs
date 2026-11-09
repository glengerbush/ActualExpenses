//! Split/transfer composition stays inside the caller's receipt transaction.
use super::*;
use std::collections::BTreeSet;

pub fn guard(db: &Connection, b: &str, row: &Value, clock: &str) -> Result<(), Error> {
    if row["reconciled"] == true || newer(row, clock) {
        return Err(Error::conflict(
            "A reconciled or newer transfer entry prevents changing this split. Refresh and review both accounts.",
        ));
    }
    if let Some(parent) = row["parent_id"].as_str() {
        let parent = ledger::get(db, b, parent)?;
        if parent["reconciled"] == true || newer(&parent, clock) {
            return Err(Error::conflict(
                "A reconciled or newer split prevents this transfer change. Refresh the split.",
            ));
        }
    }
    Ok(())
}
pub fn newer_related(
    db: &Connection,
    b: &str,
    children: &[Value],
    clock: &str,
) -> Result<bool, Error> {
    for child in children {
        if let Some(peer) = child["transfer_id"].as_str() {
            let other = ledger::get(db, b, peer)?;
            if newer(&other, clock) {
                return Ok(true);
            }
            if let Some(parent) = other["parent_id"].as_str()
                && newer(&ledger::get(db, b, parent)?, clock)
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
pub fn guard_children(
    db: &Connection,
    b: &str,
    children: &[Value],
    clock: &str,
) -> Result<(), Error> {
    for child in children {
        if let Some(peer) = child["transfer_id"].as_str() {
            guard(db, b, &ledger::get(db, b, peer)?, clock)?;
        }
    }
    Ok(())
}

/// Date/cleared belong to the whole split. Date synchronization follows every
/// connected transfer and split, including a transfer between two children.
pub fn field_rows(
    db: &Connection,
    b: &str,
    ids: &[&str],
    field: &str,
    sync: bool,
) -> Result<Vec<Value>, Error> {
    let mut todo: Vec<String> = ids.iter().map(|s| (*s).into()).collect();
    let mut seen = BTreeSet::new();
    let mut rows = vec![];
    while let Some(id) = todo.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let row = ledger::get_any(db, b, &id)?;
        if field == "date" || field == "cleared" {
            if let Some(parent) = row["parent_id"].as_str() {
                todo.push(parent.into());
            }
            if let Some(children) = row["children"].as_array() {
                todo.extend(
                    children
                        .iter()
                        .filter_map(|c| c["id"].as_str().map(str::to_owned)),
                );
            }
            if field == "date"
                && sync
                && let Some(peer) = row["transfer_id"].as_str()
            {
                todo.push(peer.into());
            }
        }
        rows.push(row);
    }
    Ok(rows)
}

pub fn apply_field(
    db: &Connection,
    b: &str,
    row: &Value,
    field: &str,
    value: rusqlite::types::Value,
    clock: &str,
) -> Result<(), Error> {
    let id = row["id"].as_str().unwrap();
    if field == "amount"
        && let Some(parent) = row["parent_id"].as_str()
    {
        let before = ledger::get(db, b, parent)?;
        let rusqlite::types::Value::Integer(amount) = value else {
            return Err(Error::invalid("Invalid amount"));
        };
        let next = before["amount"].as_i64().unwrap() as i128 + amount as i128
            - row["amount"].as_i64().unwrap() as i128;
        if next.abs() > MAX_MONEY as i128 {
            return Err(Error::invalid(
                "Split total exceeds the exact integer range",
            ));
        }
        db.execute(
            "UPDATE transactions SET amount=?3 WHERE budget_id=?1 AND id=?2",
            params![b, parent, next as i64],
        )?;
        stamp(db, b, parent, &["amount", "splits"], clock)?;
    }
    db.execute(
        &format!("UPDATE transactions SET {field}=?3 WHERE budget_id=?1 AND id=?2"),
        params![b, id, value],
    )?;
    stamp(db, b, id, &[field], clock)
}

/// Delete an allocation's paired ordinary row, or detach a paired allocation.
/// The caller owns removing/reallocating the local child and its parent's total.
pub fn remove_child(db: &Connection, b: &str, id: &str, clock: &str) -> Result<(), Error> {
    if let Some(peer) = counterpart(db, b, id)? {
        let row = ledger::get(db, b, &peer)?;
        guard(db, b, &row, clock)?;
        if row["parent_id"].is_null() {
            db.execute(
                "UPDATE transactions SET tombstone=1 WHERE budget_id=?1 AND id=?2",
                params![b, peer],
            )?;
            stamp(db, b, &peer, &["tombstone"], clock)?;
        } else {
            detach(db, b, &peer, clock)?;
        }
        detach(db, b, id, clock)?;
        ledger::refresh_account(db, b, row["account_id"].as_str().unwrap())?;
    }
    Ok(())
}
pub fn detach(db: &Connection, b: &str, id: &str, clock: &str) -> Result<(), Error> {
    db.execute(
        "UPDATE transactions SET transfer_id=NULL,payee_id=NULL WHERE budget_id=?1 AND id=?2",
        params![b, id],
    )?;
    stamp(db, b, id, &["transfer_id", "payee_id"], clock)
}

/// Called after writing the child allocation. Parent amount is already explicit.
pub fn save_child(
    db: &Connection,
    b: &str,
    id: &str,
    destination: &str,
    peer_id: &str,
    clock: &str,
) -> Result<(), Error> {
    let row = ledger::get(db, b, id)?;
    let from = row["account_id"].as_str().unwrap();
    validate_id(peer_id)?;
    if id == peer_id || from == destination {
        return Err(Error::invalid(
            "Transfer accounts and identities must differ",
        ));
    }
    let mut existing = counterpart(db, b, id)?;
    let retained = existing.as_deref() == Some(peer_id);
    let from_off = if retained {
        existing_account(db, b, from)?
    } else {
        account(db, b, from)?
    };
    let to_off = if retained {
        existing_account(db, b, destination)?
    } else {
        account(db, b, destination)?
    };
    if existing.as_deref().is_some_and(|old| old != peer_id) {
        remove_child(db, b, id, clock)?;
        existing = None;
    }
    let payee_from = payee(db, b, destination)?;
    let payee_to = payee(db, b, from)?;
    if let Some(peer) = existing {
        let other = ledger::get(db, b, &peer)?;
        guard(db, b, &other, clock)?;
        if other["account_id"] != destination {
            return Err(Error::invalid(
                "Unlink this split before choosing another transfer account",
            ));
        }
        apply_field(
            db,
            b,
            &other,
            "amount",
            rusqlite::types::Value::Integer(-row["amount"].as_i64().unwrap()),
            clock,
        )?;
        apply_field(
            db,
            b,
            &other,
            "notes",
            rusqlite::types::Value::Text(row["notes"].as_str().unwrap().into()),
            clock,
        )?;
        db.execute(
            "UPDATE transactions SET payee_id=?3 WHERE budget_id=?1 AND id=?2",
            params![b, peer, payee_to],
        )?;
        stamp(db, b, &peer, &["payee_id"], clock)?;
        if crate::preferences::get_preferences(db, b)?["sync_transfer_date"] == true {
            for target in field_rows(db, b, &[id], "date", true)? {
                // Local split clocks were just stamped; check only foreign aggregates.
                if target["account_id"] != row["account_id"] {
                    guard(db, b, &target, clock)?;
                }
                apply_field(
                    db,
                    b,
                    &target,
                    "date",
                    rusqlite::types::Value::Text(row["date"].as_str().unwrap().into()),
                    clock,
                )?;
            }
        }
        if from_off == to_off {
            db.execute(
                "UPDATE transactions SET category_id=NULL WHERE budget_id=?1 AND id=?2",
                params![b, peer],
            )?;
            stamp(db, b, &peer, &["category_id"], clock)?;
        }
    } else {
        if db.query_row(
            "SELECT EXISTS(SELECT 1 FROM transactions WHERE budget_id=?1 AND id=?2)",
            params![b, peer_id],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(Error::conflict("Counterpart identity already exists"));
        }
        db.execute("INSERT INTO transactions(budget_id,id,account_id,amount,date,notes,payee_id,transfer_id,sort_order) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,(SELECT COALESCE(MAX(sort_order),0)+1 FROM transactions WHERE budget_id=?1))",params![b,peer_id,destination,-row["amount"].as_i64().unwrap(),row["date"].as_str(),row["notes"].as_str(),payee_to,id])?;
        stamp(db, b, peer_id, FIELDS, clock)?;
    }
    db.execute("UPDATE transactions SET transfer_id=?3,payee_id=?4,category_id=CASE WHEN ?5 THEN NULL ELSE category_id END WHERE budget_id=?1 AND id=?2",params![b,id,peer_id,payee_from,from_off==to_off])?;
    stamp(
        db,
        b,
        id,
        &["transfer_id", "payee_id", "category_id"],
        clock,
    )?;
    ledger::refresh_account(db, b, destination)?;
    Ok(())
}

pub fn clone_child(
    db: &Connection,
    b: &str,
    source: &Value,
    id: &str,
    clock: &str,
) -> Result<(), Error> {
    let Some(peer) = source["transfer_id"].as_str() else {
        return Ok(());
    };
    let other = ledger::get(db, b, peer)?;
    let peer_id = uuid::Uuid::new_v4().to_string();
    save_child(
        db,
        b,
        id,
        other["account_id"].as_str().unwrap(),
        &peer_id,
        clock,
    )?;
    db.execute("UPDATE transactions SET date=?3,category_id=?4,imported_payee=?5 WHERE budget_id=?1 AND id=?2",params![b,peer_id,other["date"].as_str(),other["category_id"].as_str(),other["imported_payee"].as_str()])?;
    Ok(())
}

pub fn promote_only_child(
    db: &Connection,
    b: &str,
    parent: &str,
    child: &Value,
    clock: &str,
) -> Result<(), Error> {
    let Some(peer) = child["transfer_id"].as_str() else {
        return Ok(());
    };
    guard(db, b, &ledger::get(db, b, peer)?, clock)?;
    db.execute(
        "UPDATE transactions SET transfer_id=?3,payee_id=?4,notes=?5 WHERE budget_id=?1 AND id=?2",
        params![
            b,
            parent,
            peer,
            child["payee_id"].as_str(),
            child["notes"].as_str()
        ],
    )?;
    db.execute(
        "UPDATE transactions SET transfer_id=?3 WHERE budget_id=?1 AND id=?2",
        params![b, peer, parent],
    )?;
    stamp(db, b, parent, &["transfer_id", "payee_id", "notes"], clock)?;
    stamp(db, b, peer, &["transfer_id"], clock)?;
    detach(db, b, child["id"].as_str().unwrap(), clock)
}
