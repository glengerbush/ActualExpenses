//! Exact history evidence for both sides of split/transfer relationship changes.
use super::{Owner, journal};
use crate::error::Error;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

// This namespace exists only in auxiliary history_lineage, never field_versions.
pub(super) const PROTECTION_GENERATION: &str = "$reconciled_generation";
/// Only a verified inverse of an action that left the lock flag unchanged can
/// alias its restamp to the captured generation. Actual lock/unlock inverses
/// deliberately retain their new generation, even when they restore an old flag.
pub(super) fn generation(
    db: &Connection,
    budget: &str,
    owner: &Owner,
    id: &str,
    clock: &Value,
) -> Result<String, Error> {
    let clock = clock.as_str().unwrap_or(crate::concurrency::ZERO);
    Ok(db.prepare_cached("SELECT h.lineage FROM history_lineage h JOIN budget_sync s ON s.budget_id=h.budget_id AND s.epoch=h.epoch WHERE h.budget_id=?1 AND h.owner=?2 AND h.entity='transactions' AND h.entity_id=?3 AND h.field=?4 AND h.clock=?5")?
        .query_row(params![budget,owner.id,id,PROTECTION_GENERATION,clock],|r|r.get(0)).optional()?.unwrap_or_else(||clock.to_owned()))
}
pub(super) type Edges = BTreeMap<String, (Option<String>, Option<String>, bool)>;
#[derive(Serialize, Deserialize)]
pub(super) struct Guard {
    id: String,
    row: Value,
    versions: Value,
    protection: Value,
    #[serde(default)]
    ignore_notes: bool,
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
                ignore_notes: false,
                row: journal::row(db, budget, "transactions", id)?,
                protection: serde_json::json!(generation(
                    db,
                    budget,
                    owner,
                    id,
                    &journal::versions(db, budget, "transactions", id, &[])?["reconciled"]
                )?),
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
        let mut versions = journal::versions(db, budget, "transactions", &guard.id, &[])?;
        let mut current = journal::row(db, budget, "transactions", &guard.id)?;
        if guard.ignore_notes {
            current.as_object_mut().unwrap().remove("notes");
            versions.as_object_mut().unwrap().remove("notes");
        }
        if generation(db, budget, owner, &guard.id, &versions["reconciled"])?
            != guard
                .protection
                .as_str()
                .unwrap_or(crate::concurrency::ZERO)
            || current != guard.row
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
fn ignore_notes(guard: &mut Guard) {
    guard.ignore_notes = true;
    guard.row.as_object_mut().unwrap().remove("notes");
    guard.versions.as_object_mut().unwrap().remove("notes");
}
/// Link-only history never overwrites ordinary expense notes. Paired/split
/// snapshots retain the complete graph guard used by financial actions.
pub(super) fn capture_links(
    db: &Connection,
    budget: &str,
    owner: &Owner,
    ids: &BTreeSet<String>,
) -> Result<Vec<Guard>, Error> {
    let mut guards = capture(db, budget, owner, ids)?;
    for guard in &mut guards {
        if guard.row["is_parent"] == 0
            && guard.row["parent_id"].is_null()
            && guard.row["transfer_id"].is_null()
        {
            ignore_notes(guard);
        }
    }
    Ok(guards)
}
pub(super) fn refresh(
    db: &Connection,
    budget: &str,
    owner: &Owner,
    guards: &[Guard],
) -> Result<Vec<Guard>, Error> {
    let mut current = capture(
        db,
        budget,
        owner,
        &guards.iter().map(|g| g.id.clone()).collect(),
    )?;
    let ignored = guards
        .iter()
        .filter(|g| g.ignore_notes)
        .map(|g| g.id.as_str())
        .collect::<BTreeSet<_>>();
    for guard in &mut current {
        if ignored.contains(guard.id.as_str()) {
            ignore_notes(guard);
        }
    }
    Ok(current)
}

pub(super) fn ids(guards: &[Guard]) -> impl Iterator<Item = &str> {
    guards.iter().map(|g| g.id.as_str())
}
