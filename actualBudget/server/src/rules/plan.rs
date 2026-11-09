//! A plan evaluates one captured ledger snapshot, sharing command-wide work limits.
use super::*;
use crate::ledger;
#[derive(Serialize)]
pub struct Entry {
    pub before: Value,
    pub after: Value,
    pub matched_rule_ids: Vec<String>,
}
pub fn selected_rules(db: &Connection, b: &str, body: &Value) -> Result<Vec<Rule>, Error> {
    if !body["rule"].is_null() {
        let rule: Rule = serde_json::from_value(body["rule"].clone())
            .map_err(|_| Error::invalid("Invalid preview rule"))?;
        engine::validate(&rule)?;
        return Ok(vec![rule]);
    }
    let mut rules = load(db, b)?;
    if let Some(ids) = body["rule_ids"].as_array() {
        if ids.is_empty() || ids.len() > 1000 {
            return Err(Error::invalid("Select1–1000 rules"));
        }
        let mut chosen = std::collections::BTreeSet::new();
        for id in ids {
            let id = id
                .as_str()
                .ok_or_else(|| Error::invalid("Rule ID required"))?;
            crate::models::validate_id(id)?;
            if !chosen.insert(id) {
                return Err(Error::invalid("Duplicate rule selection"));
            }
            if !rules.iter().any(|r| r.id == id) {
                return Err(Error::missing());
            }
        }
        rules.retain(|r| chosen.contains(r.id.as_str()));
    } else if !body["rule_ids"].is_null() {
        return Err(Error::invalid("Rule IDs must be a list"));
    }
    Ok(rules)
}
pub fn ids(body: &Value, required: bool) -> Result<Option<Vec<String>>, Error> {
    let Some(values) = body["ids"].as_array() else {
        if required || !body["ids"].is_null() {
            return Err(Error::invalid("Select1–100 distinct transactions"));
        }
        return Ok(None);
    };
    if values.is_empty() || values.len() > 100 {
        return Err(Error::invalid("Select1–100 transactions"));
    }
    let mut ids = vec![];
    for value in values {
        let id = value
            .as_str()
            .ok_or_else(|| Error::invalid("Transaction ID required"))?;
        crate::models::validate_id(id)?;
        if ids.iter().any(|v| v == id) {
            return Err(Error::invalid("Duplicate transaction selection"));
        }
        ids.push(id.to_owned());
    }
    Ok(Some(ids))
}
fn hydrate(db: &Connection, b: &str, ids: &[String]) -> Result<Vec<Value>, Error> {
    let mut rows = ledger::get_many(db, b, ids)?
        .into_iter()
        .map(|r| (r["id"].as_str().unwrap().to_owned(), r))
        .collect::<std::collections::BTreeMap<_, _>>();
    ids.iter().map(|id| {
        let live: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM transactions WHERE budget_id=?1 AND id=?2 AND tombstone=0)", params![b,id], |r|r.get(0))?;
        if !live { return Err(Error::missing()); }
        rows.remove(id).ok_or_else(Error::missing)
    }).collect()
}
pub fn evaluate(db: &Connection, b: &str, body: &Value) -> Result<(Vec<Entry>, usize), Error> {
    let rules = selected_rules(db, b, body)?;
    let mut runner = engine::Runner::new(rules);
    let direct = body["transaction"].is_object();
    let selected = ids(body, false)?;
    if direct && selected.is_some() {
        return Err(Error::invalid("Preview one draft or selected ledger rows"));
    }
    let mut inputs = if direct {
        vec![body["transaction"].clone()]
    } else if let Some(ids) = selected.as_ref() {
        hydrate(db, b, ids)?
    } else {
        let account = body["account_id"].as_str();
        if let Some(id) = account {
            crate::models::validate_id(id)?;
        }
        let mut q=db.prepare("SELECT id FROM transactions WHERE budget_id=?1 AND tombstone=0 AND (?2 IS NULL OR account_id=?2) ORDER BY date DESC,sort_order DESC,id LIMIT 10001")?;
        let ids = q
            .query_map(params![b, account], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if ids.len() > 10000 {
            return Err(Error::invalid(
                "Rule preview exceeds10000 candidates; choose an account or explicit entries",
            ));
        }
        hydrate(db, b, &ids)?
    };
    if let Some(ids) = &selected {
        if inputs.len() != ids.len() {
            return Err(Error::missing());
        }
    }
    for input in &inputs {
        if !direct && input["deleted"] == true {
            return Err(Error::conflict(
                "A selected entry was deleted. Refresh the preview.",
            ));
        }
    }
    let candidates = inputs.len();
    let mut entries = vec![];
    for input in &mut inputs {
        if direct {
            for field in ["payee_id", "category_id", "parent_id", "transfer_id"] {
                if input.get(field).is_none() {
                    input[field] = Value::Null;
                }
            }
        }
        let (after, matched) = runner.evaluate(db, b, input)?;
        if direct || selected.is_some() || !matched.is_empty() {
            entries.push(Entry {
                before: input.clone(),
                after,
                matched_rule_ids: matched,
            });
        }
    }
    Ok((entries, candidates))
}
pub fn preview(db: &Connection, b: &str, body: &Value) -> Result<Value, Error> {
    let (entries, candidate_count) = evaluate(db, b, body)?;
    let total = entries.len();
    let offset = body["offset"].as_u64().unwrap_or(0) as usize;
    let limit = body["limit"].as_u64().unwrap_or(100) as usize;
    if !(1..=500).contains(&limit) || offset > 10000 {
        return Err(Error::invalid("Invalid preview page"));
    }
    let ids = entries
        .iter()
        .filter_map(|e| e.before["id"].as_str())
        .filter(|id| crate::models::validate_id(id).is_ok())
        .collect::<Vec<_>>();
    let protection = if body["transaction"].is_object() || ids.is_empty() {
        vec![]
    } else {
        crate::transfers::protection::members(db, b, &ids)?
    };
    Ok(
        json!({"transactions":entries.into_iter().skip(offset).take(limit).collect::<Vec<_>>(),"candidate_count":candidate_count,"total_count":total,"next_offset":(offset+limit<total).then_some(offset+limit),"snapshot":sync::cursor(db,b)?,"epoch":crate::lifecycle::epoch(db,b)?,"transfer_protection_token":if protection.is_empty(){Value::Null}else{json!(crate::transfers::protection::token(b,&protection))},"protected_count":protection.iter().filter(|r|r["reconciled"]==true).count()}),
    )
}
fn patch(before: &Value, after: &Value) -> Value {
    let mut result = serde_json::Map::new();
    for field in [
        "account_id",
        "amount",
        "date",
        "cleared",
        "notes",
        "payee_id",
        "category_id",
    ] {
        if after[field] != before[field] {
            result.insert(field.into(), after[field].clone());
        }
    }
    if let Some(new) = after.get("new_payee") {
        result.remove("payee_id");
        result.insert("new_payee".into(), new.clone());
    }
    Value::Object(result)
}
pub fn apply(
    db: &Connection,
    b: &str,
    body: &Value,
    clock: &str,
    epoch: Option<&str>,
) -> Result<Value, Error> {
    let ids = ids(body, true)?.unwrap();
    let (entries, _) = evaluate(db, b, body)?;
    let members = crate::transfers::protection::members(
        db,
        b,
        &ids.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    if members.iter().any(|r| r["reconciled"] == true) {
        return Err(Error::conflict(
            "Unlock affected reconciled entries, then preview the rules again.",
        ));
    }
    // Evaluate both selected legs against the same initial snapshot. Actual's
    // batch propagation can otherwise persist two equal signed amounts.
    for entry in &entries {
        if let Some(peer) = entry.before["transfer_id"]
            .as_str()
            .and_then(|id| entries.iter().find(|e| e.before["id"] == id))
        {
            if entry.after["deleted"] != true
                && peer.after["deleted"] != true
                && entry.after["amount"].as_i64().unwrap()
                    != -peer.after["amount"].as_i64().unwrap()
            {
                return Err(Error::conflict(
                    "Selected rules give a transfer two contradictory amounts. No rule actions were saved.",
                ));
            }
        }
    }
    // A split is one mutation: merge selected children into their complete root
    // rather than trying to patch allocations independently.
    let mut grouped = std::collections::BTreeMap::<String, Entry>::new();
    let mut order = Vec::new();
    for entry in entries {
        let root = entry.before["parent_id"]
            .as_str()
            .unwrap_or_else(|| entry.before["id"].as_str().unwrap())
            .to_owned();
        if !grouped.contains_key(&root) {
            order.push(root.clone());
            let before = ledger::get(db, b, &root)?;
            grouped.insert(
                root.clone(),
                Entry {
                    after: before.clone(),
                    before,
                    matched_rule_ids: vec![],
                },
            );
        }
        let group = grouped.get_mut(&root).unwrap();
        group.matched_rule_ids.extend(entry.matched_rule_ids);
        if entry.before["parent_id"].is_null() {
            let old_children = group.after["children"].clone();
            group.after = entry.after;
            // Child edits selected earlier must survive a selected parent whose
            // rules did not replace the allocation list.
            if group.after["children"] == entry.before["children"]
                && old_children != group.before["children"]
            {
                group.after["children"] = old_children;
            }
        } else {
            let children = group.after["children"]
                .as_array_mut()
                .ok_or_else(|| Error::invalid("Missing split parent"))?;
            let position = children
                .iter()
                .position(|c| c["id"] == entry.before["id"])
                .ok_or_else(Error::missing)?;
            if entry.after["deleted"] == true {
                children.remove(position);
            } else {
                children[position] = entry.after;
            }
        }
    }
    let entries = order
        .into_iter()
        .map(|id| grouped.remove(&id).unwrap())
        .collect::<Vec<_>>();
    let mut changed = vec![];
    for entry in entries {
        if entry.matched_rule_ids.is_empty() || entry.after == entry.before {
            continue;
        }
        let id = entry.before["id"].as_str().unwrap();
        // Earlier operations in this command may already have propagated to
        // the peer. Skip only effects already made by this atomic command.
        let live: bool=db.query_row("SELECT EXISTS(SELECT 1 FROM transactions WHERE budget_id=?1 AND id=?2 AND tombstone=0)",params![b,id],|r|r.get(0))?;
        if !live && entry.after["deleted"] == true {
            continue;
        }
        let current = ledger::get(db, b, id)?;
        let mutation = if entry.after["deleted"] == true {
            sync::Mutation::Delete(id.to_owned())
        } else if entry.after["is_parent"] == true {
            let children = entry.after["children"]
                .as_array()
                .ok_or_else(|| Error::invalid("Missing rule allocations"))?
                .iter()
                .map(|c| {
                    Ok(crate::splits::Child {
                        id: c["id"].as_str().unwrap().to_owned(),
                        amount: c["amount"].as_i64().unwrap(),
                        payee_id: c["payee_id"].as_str().map(str::to_owned),
                        category_id: c["category_id"].as_str().map(str::to_owned),
                        notes: c["notes"].as_str().unwrap_or("").into(),
                        transfer_account_id: c["transfer_account_id"].as_str().map(str::to_owned),
                        counterpart_id: c["transfer_id"].as_str().map(str::to_owned),
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            let mut fields = patch(&entry.before, &entry.after);
            fields.as_object_mut().unwrap().remove("category_id");
            fields.as_object_mut().unwrap().remove("payee_id");
            sync::Mutation::Split(
                id.to_owned(),
                crate::splits::Change {
                    snapshot: sync::cursor(db, b)?,
                    action: crate::splits::Action::Replace,
                    children,
                    allocation: crate::splits::Allocation::Exact,
                    detach: vec![],
                    fields: (!fields.as_object().unwrap().is_empty()).then_some(fields),
                    confirm_reconciled: false,
                    reconciled_versions: std::collections::BTreeMap::new(),
                    reconciliation_token: None,
                    transfer_protection_token: None,
                    changes: None,
                },
            )
        } else {
            let fields = patch(&current, &entry.after);
            if fields.as_object().unwrap().is_empty() {
                continue;
            }
            sync::Mutation::Patch(id.to_owned(), fields)
        };
        let result = sync::execute(db, b, Some(clock), false, epoch, mutation)?;
        if result["response_status"].as_u64().unwrap_or(500) >= 400
            || result["response_body"]["conflicts"]
                .as_array()
                .is_some_and(|v| !v.is_empty())
        {
            return Err(Error::conflict(
                "A selected transaction changed. No rule actions were saved.",
            ));
        }
        changed.push(result["response_body"]["data"].clone());
    }
    Ok(json!({"affected_count":changed.len(),"selected_count":ids.len(),"entries":changed}))
}
