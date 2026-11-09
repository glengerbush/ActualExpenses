//! Exact history evidence for both sides of split/transfer relationship changes.
use super::{Owner, journal};
use crate::error::Error;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(super) type Edges = BTreeMap<String, (Option<String>, Option<String>, bool)>;
#[derive(Serialize, Deserialize)]
pub(super) struct Guard {
    id: String,
    row: Value,
    versions: Value,
    protection: Value,
}
pub(super) fn edges(db: &Connection, budget: &str) -> Result<Edges, Error> {
    Ok(db
        .prepare("SELECT id,parent_id,transfer_id,tombstone FROM transactions WHERE budget_id=?1")?
        .query_map([budget], |r| {
            Ok((r.get(0)?, (r.get(1)?, r.get(2)?, r.get(3)?)))
        })?
        .collect::<Result<_, _>>()?)
}
pub(super) type Links = BTreeMap<String, Vec<String>>;
pub(super) fn adjacency(states: &[&Edges]) -> Links {
    let mut links: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for state in states {
        for (id, (parent, peer, deleted)) in state.iter() {
            if *deleted {
                continue;
            }
            let mut connect = |other: &str| {
                links.entry(id.clone()).or_default().push(other.to_owned());
                links.entry(other.to_owned()).or_default().push(id.clone());
            };
            if let Some(parent) = parent.as_deref()
                && state.get(parent).is_some_and(|r| !r.2)
            {
                connect(parent);
            }
            if let Some(peer) = peer.as_deref()
                && state
                    .get(peer)
                    .is_some_and(|r| !r.2 && r.1.as_deref() == Some(id))
            {
                connect(peer);
            }
        }
    }
    links
}
pub(super) fn connected(links: &Links, seeds: &[String]) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut pending = seeds.to_vec();
    while let Some(id) = pending.pop() {
        if found.insert(id.clone()) {
            pending.extend(links.get(&id).into_iter().flatten().cloned());
        }
    }
    found
}
pub(super) fn capture(
    db: &Connection,
    budget: &str,
    owner: &Owner,
    ids: &BTreeSet<String>,
) -> Result<Vec<Guard>, Error> {
    ids.iter()
        .map(|id| {
            Ok(Guard {
                id: id.clone(),
                row: journal::row(db, budget, "transactions", id)?,
                protection: journal::versions(db, budget, "transactions", id, &[])?["reconciled"]
                    .clone(),
                versions: journal::normalize(
                    db,
                    budget,
                    owner,
                    "transactions",
                    id,
                    journal::versions(db, budget, "transactions", id, &[])?,
                )?,
            })
        })
        .collect()
}
pub(super) fn verify(
    db: &Connection,
    budget: &str,
    owner: &Owner,
    guards: &[Guard],
    clock: &str,
) -> Result<(), Error> {
    if guards.is_empty() {
        return Ok(());
    }
    let ids = guards.iter().map(|g| g.id.clone()).collect::<Vec<_>>();
    if connected(&adjacency(&[&edges(db, budget)?]), &ids) != ids.iter().cloned().collect() {
        return Err(journal::conflict());
    }
    for guard in guards {
        let versions = journal::versions(db, budget, "transactions", &guard.id, &[])?;
        if versions["reconciled"] != guard.protection
            || journal::row(db, budget, "transactions", &guard.id)? != guard.row
            || versions
                .as_object()
                .unwrap()
                .values()
                .any(|v| v.as_str().is_some_and(|v| v >= clock))
            || journal::normalize(db, budget, owner, "transactions", &guard.id, versions)?
                != guard.versions
        {
            return Err(journal::conflict());
        }
    }
    Ok(())
}
pub(super) fn refresh(
    db: &Connection,
    budget: &str,
    owner: &Owner,
    guards: &[Guard],
) -> Result<Vec<Guard>, Error> {
    capture(
        db,
        budget,
        owner,
        &guards.iter().map(|g| g.id.clone()).collect(),
    )
}

pub(super) fn ids(guards: &[Guard]) -> impl Iterator<Item = &str> {
    guards.iter().map(|g| g.id.as_str())
}
