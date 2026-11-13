//! Pre20 preference writes carried durable values/revisions, not field HLCs.
//! Reconstruct only a proven path from the imported defaults to the retained
//! current state. A revision bump alone never proves a currency edit.
use super::*;
use std::collections::BTreeMap;
#[derive(Clone)]
struct Receipt {
    changes: Value,
    response: Value,
}
fn comparable_preferences(value: Value) -> Value {
    let mut value = crate::preferences::display_defaults(value);
    crate::preferences::remove_retired_features(&mut value);
    value
}
pub(super) fn resolve(
    db: &Connection,
    b: &str,
    base: &Value,
    raw: &str,
    archived: Option<&[(String, String, String)]>,
) -> Result<(String, bool), Error> {
    let (settings, revision): (String, i64) = db.query_row(
        "SELECT settings,revision FROM budget_preferences WHERE budget_id=?1",
        [b],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut current =
        comparable_preferences(serde_json::from_str(&settings).map_err(Error::internal)?);
    current["revision"] = json!(revision);
    let mut initial = comparable_preferences(base.clone());
    initial["revision"] = json!(0);
    if revision == 0 {
        return Ok((raw.into(), current != initial));
    }
    if !(0..=100000).contains(&revision) {
        return Ok((raw.into(), true));
    }
    if let Some(archived) = archived {
        let scope = format!("PATCH /budgets/{b}/preferences");
        if archived
            .iter()
            .filter(|r| r.0 == scope)
            .map(|r| r.0.len() + r.1.len() + r.2.len())
            .sum::<usize>()
            > 16 * 1024 * 1024
        {
            return Ok((raw.into(), true));
        }
    } else {
        let bytes:i64=db.query_row("SELECT COALESCE(SUM(length(payload)+length(response)),0) FROM operations WHERE scope=?1",[format!("PATCH /budgets/{b}/preferences")],|r|r.get(0))?;
        if bytes > 16 * 1024 * 1024 {
            return Ok((raw.into(), true));
        }
    }
    let receipts = if let Some(archived) = archived {
        let scope = format!("PATCH /budgets/{b}/preferences");
        archived
            .iter()
            .filter(|r| r.0 == scope)
            .map(|r| (r.1.clone(), r.2.clone()))
            .collect::<Vec<_>>()
    } else {
        let mut q =
            db.prepare("SELECT payload,response FROM operations WHERE scope=?1 LIMIT 100001")?;
        q.query_map([format!("PATCH /budgets/{b}/preferences")], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    if receipts.len() > 100000 {
        return Ok((raw.into(), true));
    }
    let mut by_revision = BTreeMap::<i64, Vec<Receipt>>::new();
    for (payload, response) in receipts {
        let Ok(mut changes) = serde_json::from_str::<Value>(&payload) else {
            continue;
        };
        let Ok(response) = serde_json::from_str::<Value>(&response) else {
            continue;
        };
        let Some(n) = response["revision"].as_i64() else {
            continue;
        };
        // Removed bank flags cannot change calculator currency provenance.
        // Normalize only proof copies; original receipts remain immutable.
        crate::preferences::remove_retired_features(&mut changes);
        let response = comparable_preferences(response);
        if n > 0 && n <= revision {
            by_revision
                .entry(n)
                .or_default()
                .push(Receipt { changes, response });
        }
    }
    // Equal states merge their two possible provenance bits: unchanged/touched.
    let mut states = BTreeMap::from([(initial.to_string(), 1_u8)]);
    let mut work = 2_000_000_usize;
    for n in 1..=revision {
        let Some(receipts) = by_revision.get(&n) else {
            return Ok((raw.into(), true));
        };
        let mut next = BTreeMap::<String, u8>::new();
        for (state, bits) in states {
            let previous: Value = serde_json::from_str(&state).map_err(Error::internal)?;
            for receipt in receipts {
                let Some(left) = work.checked_sub(1) else {
                    return Ok((raw.into(), true));
                };
                work = left;
                let Some(changes) = receipt.changes.as_object() else {
                    continue;
                };
                let mut candidate = previous.clone();
                candidate.as_object_mut().unwrap().remove("revision");
                let mut valid = true;
                for (key, value) in changes {
                    if crate::preferences::validate_change(key, value).is_err() {
                        valid = false;
                        break;
                    }
                    if value.is_null() {
                        candidate[key] = if key == "sync_transfer_date" {
                            json!(false)
                        } else {
                            crate::preferences::defaults()[key].clone()
                        };
                    } else if key == "experimental" {
                        for (name, enabled) in value.as_object().unwrap() {
                            candidate[key][name] = enabled.clone();
                        }
                    } else {
                        candidate[key] = value.clone();
                    }
                }
                if !valid {
                    continue;
                }
                crate::preferences::normalize_currency(&mut candidate);
                candidate["revision"] = json!(n);
                if candidate == receipt.response {
                    let touched = changes.contains_key("currency_code")
                        || changes.get("experimental").is_some_and(|flags| {
                            flags.is_null() || flags.get("currency").is_some()
                        });
                    let proof = if touched { 2 } else { bits };
                    *next.entry(candidate.to_string()).or_default() |= proof;
                }
            }
        }
        if next.is_empty() {
            return Ok((raw.into(), true));
        }
        states = next;
    }
    match states.get(&current.to_string()).copied() {
        Some(1) => Ok((raw.into(), false)),
        Some(2) => Ok((
            current["currency_code"].as_str().unwrap_or("").into(),
            false,
        )),
        _ => Ok((raw.into(), true)),
    }
}
