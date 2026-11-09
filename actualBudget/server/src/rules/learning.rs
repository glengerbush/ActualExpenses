//! Learning is explicit manual evidence, never an inference from rule execution.
use super::*;
use chrono::{NaiveDate, TimeDelta};
use std::collections::{BTreeMap, BTreeSet};
pub fn enabled(db: &Connection, b: &str) -> Result<bool, Error> {
    Ok(db.query_row(
        "SELECT enabled FROM rule_settings WHERE budget_id=?1 AND id='learning'",
        [b],
        |r| r.get(0),
    )?)
}
pub fn associations(db: &Connection, b: &str) -> Result<BTreeMap<String, String>, Error> {
    let mut values = BTreeMap::<String, BTreeSet<String>>::new();
    for rule in load(db, b)? {
        if rule.stage.is_some() || rule.conditions.len() != 1 || rule.actions.len() != 1 {
            continue;
        }
        let c = &rule.conditions[0];
        let a = &rule.actions[0];
        if c["field"] != "payee"
            || c["op"] != "is"
            || a["op"] != "set"
            || a["field"] != "category"
            || !a["options"]["template"].is_null()
            || !a["options"]["formula"].is_null()
        {
            continue;
        }
        let (Some(payee), Some(category)) = (c["value"].as_str(), a["value"].as_str()) else {
            continue;
        };
        let Ok(payee) = crate::payees_tags::canonical(db, b, payee) else {
            continue;
        };
        let Ok(category) = crate::categories::resolve(db, b, Some(category.to_owned())) else {
            continue;
        };
        let Some(category) = category else { continue };
        let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM payees p JOIN categories c ON c.budget_id=p.budget_id LEFT JOIN category_groups g ON g.budget_id=c.budget_id AND g.id=c.group_id WHERE p.budget_id=?1 AND p.id=?2 AND p.tombstone=0 AND p.transfer_account_id IS NULL AND c.id=?3 AND c.tombstone=0 AND c.is_income=0 AND c.hidden=0 AND COALESCE(g.hidden,0)=0)",params![b,payee,category],|r|r.get(0))?;
        if valid {
            values.entry(payee).or_default().insert(category);
        }
    }
    Ok(values
        .into_iter()
        .filter_map(|(p, c)| (c.len() == 1).then(|| (p, c.into_iter().next().unwrap())))
        .collect())
}
pub fn manual(db: &Connection, b: &str, ids: &[String], clock: &str) -> Result<(), Error> {
    if ids.is_empty() || !enabled(db, b)? {
        return Ok(());
    }
    let mut candidates = BTreeSet::new();
    let mut changed = BTreeSet::new();
    let mut oldest: Option<String> = None;
    for id in ids {
        let value:Option<(String,Option<String>,Option<String>)>=db.query_row("SELECT date,payee_id,category_id FROM transactions WHERE budget_id=?1 AND id=?2 AND tombstone=0 AND is_parent=0",params![b,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((date, Some(payee), Some(_))) = value {
            oldest = Some(match oldest {
                Some(old) => old.min(date),
                None => date,
            });
            candidates.insert(payee);
            changed.insert(id.clone());
        }
    }
    let Some(oldest) = oldest else { return Ok(()) };
    let start = NaiveDate::parse_from_str(&oldest, "%Y-%m-%d")
        .map_err(Error::internal)?
        .checked_sub_signed(TimeDelta::days(180))
        .unwrap_or(NaiveDate::MIN)
        .to_string();
    let end = (chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).date_naive()
        + TimeDelta::days(180))
    .to_string();
    let placeholders = (4..4 + candidates.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let mut args = vec![b.to_owned(), start, end];
    args.extend(candidates.iter().cloned());
    let mut query=db.prepare(&format!("SELECT t.id,t.payee_id,t.category_id FROM transactions t JOIN accounts a ON a.budget_id=t.budget_id AND a.id=t.account_id JOIN payees p ON p.budget_id=t.budget_id AND p.id=t.payee_id WHERE t.budget_id=?1 AND t.date>=?2 AND t.date<=?3 AND t.payee_id IN ({placeholders}) AND t.tombstone=0 AND t.is_parent=0 AND a.closed=0 AND p.learn_categories=1 ORDER BY t.date DESC,t.sort_order DESC,t.id DESC"))?;
    let mut latest = BTreeMap::<String, Vec<(String, Option<String>)>>::new();
    for row in query.query_map(rusqlite::params_from_iter(args.iter()), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })? {
        let (id, p, c) = row?;
        let rows = latest.entry(p).or_default();
        if rows.len() < 5 {
            rows.push((id, c));
        }
    }
    let rules = load(db, b)?;
    for (payee, rows) in latest {
        if !rows.iter().any(|(id, _)| changed.contains(id)) {
            continue;
        }
        // The pinned reducer seeds from the newest row. An uncategorized newest
        // row therefore suppresses a winner, even when older rows have three votes.
        let Some(Some(first)) = rows.first().map(|(_, c)| c.as_ref()) else {
            continue;
        };
        let mut scores = BTreeMap::<&str, usize>::new();
        for (_, c) in &rows {
            if let Some(c) = c {
                *scores.entry(c).or_default() += 1;
            }
        }
        let mut winner = first.as_str();
        for (_, c) in &rows {
            if let Some(c) = c
                && scores[c.as_str()] > scores[winner]
            {
                winner = c;
            }
        }
        if scores[winner] < 3 {
            continue;
        }
        let setters = rules
            .iter()
            .filter(|r| {
                r.stage.is_none()
                    && r.conditions.len() == 1
                    && r.actions.len() == 1
                    && r.conditions[0]["field"] == "payee"
                    && matches!(r.conditions[0]["op"].as_str(), Some("is" | "isNot"))
                    && r.conditions[0]["value"] == payee
                    && r.actions[0]["op"] == "set"
                    && r.actions[0]["field"] == "category"
            })
            .collect::<Vec<_>>();
        if setters.is_empty() {
            let id = uuid::Uuid::new_v4().to_string();
            let c = json!([{"field":"payee","op":"is","value":payee}]);
            let a = json!([{"op":"set","field":"category","value":winner}]);
            db.execute("INSERT INTO rules(budget_id,id,stage,conditions_op,conditions,actions) VALUES(?1,?2,NULL,'and',?3,?4)",params![b,id,c.to_string(),a.to_string()])?;
            for field in [
                "stage",
                "conditions_op",
                "conditions",
                "actions",
                "tombstone",
            ] {
                sync::seed_field_version(db, b, "rules", &id, field, clock)?;
            }
            sync::mark(db, b, "rules", &id)?;
        } else {
            for rule in setters {
                if rule.actions[0]["value"] == winner {
                    continue;
                }
                guard(db, b, "rules", &rule.id, "actions", clock)?;
                let mut actions = rule.actions.clone();
                actions[0]["value"] = json!(winner);
                db.execute(
                    "UPDATE rules SET actions=?3 WHERE budget_id=?1 AND id=?2",
                    params![
                        b,
                        rule.id,
                        serde_json::to_string(&actions).map_err(Error::internal)?
                    ],
                )?;
                sync::seed_field_version(db, b, "rules", &rule.id, "actions", clock)?;
                sync::mark(db, b, "rules", &rule.id)?;
            }
        }
    }
    Ok(())
}
