//! Captured authorization for a complete connected split/transfer component.
//! Constructed before mutation, inside the caller's receipt transaction.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub struct Authorization {
    versions: BTreeMap<String, String>,
}
impl Authorization {
    pub fn protect(&self, row: &Value) -> Result<(), Error> {
        if row["reconciled"] == true && !self.versions.contains_key(row["id"].as_str().unwrap()) {
            return Err(Error::conflict(
                "An affected reconciled entry was not included in the confirmation. Refresh and confirm again.",
            ));
        }
        Ok(())
    }
}

pub fn members(db: &Connection, b: &str, ids: &[&str]) -> Result<Vec<Value>, Error> {
    let mut rows = BTreeMap::new();
    for id in ids {
        validate_id(id)?;
        // Keep the recursive member outermost: SQLite otherwise may scan every
        // budget row for each member, making large split proof reads quadratic.
        let mut query = db.prepare("WITH RECURSIVE connected(id) AS (
            SELECT id FROM transactions WHERE budget_id=?1 AND id=?2 AND tombstone=0
            UNION SELECT t.parent_id FROM connected c CROSS JOIN transactions t ON t.id=c.id WHERE t.budget_id=?1 AND t.tombstone=0 AND t.parent_id IS NOT NULL
            UNION SELECT t.id FROM connected c CROSS JOIN transactions t ON t.parent_id=c.id WHERE t.budget_id=?1 AND t.tombstone=0
            UNION SELECT p.id FROM connected c CROSS JOIN transactions t ON t.id=c.id JOIN transactions p ON p.budget_id=t.budget_id AND p.id=t.transfer_id AND p.transfer_id=t.id WHERE t.budget_id=?1 AND t.tombstone=0 AND p.tombstone=0
        ) SELECT t.id,t.parent_id,t.transfer_id,t.reconciled,v.clock FROM connected c CROSS JOIN transactions t ON t.budget_id=?1 AND t.id=c.id AND t.tombstone=0 LEFT JOIN field_versions v ON v.budget_id=t.budget_id AND v.entity='transactions' AND v.entity_id=t.id AND v.field='reconciled' ORDER BY t.id")?;
        for row in query.query_map(params![b,id], |r| Ok(json!({"id":r.get::<_,String>(0)?,"parent_id":r.get::<_,Option<String>>(1)?,"transfer_id":r.get::<_,Option<String>>(2)?,"reconciled":r.get::<_,bool>(3)?,"field_versions":{"reconciled":r.get::<_,Option<String>>(4)?.unwrap_or_else(||order::ZERO.into())}})))? {
            let row = row?;
            rows.insert(row["id"].as_str().unwrap().to_owned(), row);
        }
    }
    if rows.is_empty() {
        return Err(Error::missing());
    }
    Ok(rows.into_values().collect())
}

pub fn token(b: &str, rows: &[Value]) -> String {
    let state: Vec<_> = rows
        .iter()
        .map(|r| {
            json!([
                r["id"],
                r["parent_id"],
                r["transfer_id"],
                r["reconciled"],
                r["field_versions"]["reconciled"]
            ])
        })
        .collect();
    format!(
        "{:x}",
        Sha256::digest(
            json!(["actualBudget.transfer-protection.v1", b, state])
                .to_string()
                .as_bytes()
        )
    )
}

pub fn authorize(
    db: &Connection,
    b: &str,
    ids: &[&str],
    proof: Option<&str>,
    clock: &str,
) -> Result<Option<Authorization>, Error> {
    let Some(proof) = proof else {
        return Ok(None);
    };
    if proof.len() != 64
        || !proof
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(Error::invalid("Invalid transfer protection token"));
    }
    let rows = members(db, b, ids)?;
    if proof != token(b, &rows)
        || rows.iter().any(|r| {
            clock
                <= r["field_versions"]["reconciled"]
                    .as_str()
                    .unwrap_or(order::ZERO)
        })
    {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "reconciliation_changed",
            "The linked entries or their protection changed. Refresh and confirm the current entries.",
        ));
    }
    Ok(Some(Authorization {
        versions: rows
            .iter()
            .map(|r| {
                (
                    r["id"].as_str().unwrap().to_owned(),
                    r["field_versions"]["reconciled"]
                        .as_str()
                        .unwrap()
                        .to_owned(),
                )
            })
            .collect(),
    }))
}

pub fn protect(row: &Value, authorization: Option<&Authorization>) -> Result<(), Error> {
    match authorization {
        Some(a) => a.protect(row),
        None => crate::reconciliation::protect(row, None),
    }
}

pub fn protect_group(
    db: &Connection,
    b: &str,
    row: &Value,
    authorization: Option<&Authorization>,
) -> Result<(), Error> {
    // authorize() already verified every original member in this SQLite
    // transaction. Rechecking the hydrated aggregate for every date target would
    // be quadratic. Scope each later touch to its own captured ID and parent.
    if let Some(authorization) = authorization {
        authorization.protect(row)?;
        if let Some(parent) = row["parent_id"].as_str() {
            let reconciled: bool = db.query_row(
                "SELECT reconciled FROM transactions WHERE budget_id=?1 AND id=?2 AND tombstone=0",
                params![b, parent],
                |r| r.get(0),
            )?;
            authorization.protect(&json!({"id":parent,"reconciled":reconciled}))?;
        }
        return Ok(());
    }
    // Unconfirmed edits need only detect one protected member; no row hydration.
    let parent = row["parent_id"]
        .as_str()
        .unwrap_or_else(|| row["id"].as_str().unwrap());
    let protected: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM transactions WHERE budget_id=?1 AND tombstone=0 AND reconciled=1 AND (id=?2 OR parent_id=?2))",params![b,parent],|r|r.get(0))?;
    protect(&json!({"id":parent,"reconciled":protected}), None)
}

/// Resource-limited callers must bound discovery itself, not truncate a fully
/// materialized recursive proof. Ordinary confirmation uses members() unchanged.
/// Indexed child cursors stop on the first member over the allowance; no large
/// fanout is first enqueued/expanded by a recursive CTE.
pub fn members_bounded(
    db: &Connection,
    b: &str,
    ids: &[&str],
    limit: usize,
) -> Result<Vec<Value>, Error> {
    use std::collections::VecDeque;
    fn read(db: &Connection, b: &str, id: &str) -> Result<Option<Value>, Error> {
        Ok(db.prepare_cached("SELECT t.id,t.parent_id,t.transfer_id,t.reconciled,v.clock FROM transactions t LEFT JOIN field_versions v ON v.budget_id=t.budget_id AND v.entity='transactions' AND v.entity_id=t.id AND v.field='reconciled' WHERE t.budget_id=?1 AND t.id=?2 AND t.tombstone=0")?.query_row(params![b,id],|r|Ok(json!({"id":r.get::<_,String>(0)?,"parent_id":r.get::<_,Option<String>>(1)?,"transfer_id":r.get::<_,Option<String>>(2)?,"reconciled":r.get::<_,bool>(3)?,"field_versions":{"reconciled":r.get::<_,Option<String>>(4)?.unwrap_or_else(||order::ZERO.into())}}))).optional()?)
    }
    fn add(
        row: Option<Value>,
        rows: &mut BTreeMap<String, Value>,
        queue: &mut VecDeque<String>,
        limit: usize,
    ) -> Result<(), Error> {
        if let Some(row) = row {
            let id = row["id"].as_str().unwrap();
            if !rows.contains_key(id) {
                if rows.len() >= limit {
                    return Err(Error::new(
                        StatusCode::BAD_REQUEST,
                        "graph_limit",
                        "Financial graph exceeds the requested member allowance; use smaller independent commands or explicitly review a larger allowance.",
                    ));
                }
                queue.push_back(id.to_owned());
                rows.insert(id.to_owned(), row);
            }
        }
        Ok(())
    }
    if limit == 0 || limit > 100000 {
        return Err(Error::new(
            StatusCode::BAD_REQUEST,
            "graph_limit",
            "Financial graph exceeds the requested member allowance; use smaller independent commands or explicitly review a larger allowance.",
        ));
    }
    let mut rows = BTreeMap::new();
    let mut queue = VecDeque::new();
    for id in ids {
        validate_id(id)?;
        add(read(db, b, id)?, &mut rows, &mut queue, limit)?;
    }
    while let Some(id) = queue.pop_front() {
        let row = rows[&id].clone();
        if let Some(parent) = row["parent_id"].as_str()
            && !rows.contains_key(parent)
        {
            add(read(db, b, parent)?, &mut rows, &mut queue, limit)?;
        }
        if let Some(peer) = row["transfer_id"].as_str()
            && !rows.contains_key(peer)
        {
            let peer = read(db, b, peer)?;
            if peer.as_ref().is_some_and(|r| r["transfer_id"] == id) {
                add(peer, &mut rows, &mut queue, limit)?;
            }
        }
        // This uses transactions_register's (budget,tombstone,parent) prefix;
        // no ORDER BY/sort or get_many expansion before the allowance check.
        let mut children=db.prepare_cached("SELECT id FROM transactions WHERE budget_id=?1 AND tombstone=0 AND parent_id=?2 LIMIT ?3")?;
        let mut cursor = children.query(params![b, id, (limit + 1) as i64])?;
        while let Some(child) = cursor.next()? {
            let child: String = child.get(0)?;
            if !rows.contains_key(&child) {
                add(read(db, b, &child)?, &mut rows, &mut queue, limit)?;
            }
        }
    }
    if rows.is_empty() {
        return Err(Error::missing());
    }
    Ok(rows.into_values().collect())
}
