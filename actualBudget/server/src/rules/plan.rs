//! A plan evaluates one captured ledger snapshot, sharing command-wide work limits.
use super::*;
use crate::ledger;
#[derive(Serialize)]
pub struct Entry {
    pub before: Value,
    pub after: Value,
    pub matched_rule_ids: Vec<String>,
}
pub fn validate_body(body: &Value, applying: bool) -> Result<(), Error> {
    let allowed = if applying {
        &["snapshot", "ids", "rule_ids"][..]
    } else {
        &[
            "snapshot",
            "ids",
            "rule_ids",
            "rule",
            "transaction",
            "account_id",
            "offset",
            "limit",
            "manual_entry",
        ][..]
    };
    if body
        .as_object()
        .is_none_or(|o| o.keys().any(|k| !allowed.contains(&k.as_str())))
    {
        return Err(Error::invalid("Unknown rule command field"));
    }
    if !body["rule"].is_null() && !body["rule_ids"].is_null() {
        return Err(Error::invalid(
            "Preview a draft rule or saved rules, not both",
        ));
    }
    if !body["manual_entry"].is_null() && body["manual_entry"].as_bool().is_none() {
        return Err(Error::invalid("manual_entry must be boolean"));
    }
    if body["manual_entry"] == true && !body["transaction"].is_object() {
        return Err(Error::invalid(
            "Manual entry preview requires one draft transaction",
        ));
    }
    for name in ["offset", "limit"] {
        if !body[name].is_null() && body[name].as_u64().is_none() {
            return Err(Error::invalid("Invalid preview page"));
        }
    }
    if !body["transaction"].is_null() && !body["transaction"].is_object() {
        return Err(Error::invalid("Preview transaction must be an object"));
    }
    Ok(())
}
fn validate_draft(row: &Value) -> Result<(), Error> {
    let mut value = serde_json::Map::new();
    for key in [
        "id",
        "account_id",
        "amount",
        "date",
        "notes",
        "cleared",
        "payee_id",
        "category_id",
    ] {
        if let Some(v) = row.get(key) {
            value.insert(key.into(), v.clone());
        }
    }
    if value.get("notes").is_none_or(Value::is_null) {
        value.insert("notes".into(), json!(""));
    }
    let input: ledger::NewTransaction = serde_json::from_value(Value::Object(value))
        .map_err(|_| Error::invalid("Invalid preview transaction fields"))?;
    input.validate()?;
    if let Some(children) = row.get("children") {
        let children = children
            .as_array()
            .filter(|v| v.len() <= 100_000)
            .ok_or_else(|| Error::invalid("Invalid preview allocations"))?;
        let mut total = 0i128;
        for child in children {
            let mut child = child.clone();
            for key in ["account_id", "date", "cleared"] {
                child[key] = row[key].clone();
            }
            validate_draft(&child)?;
            total += i128::from(child["amount"].as_i64().unwrap());
        }
        if total != i128::from(row["amount"].as_i64().unwrap()) {
            return Err(Error::invalid(
                "Rule split allocations do not equal the parent amount",
            ));
        }
    }
    Ok(())
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
pub fn evaluate(
    db: &Connection,
    b: &str,
    body: &Value,
) -> Result<(Vec<Entry>, usize, engine::Work), Error> {
    validate_body(body, false)?;
    let rules = selected_rules(db, b, body)?;
    let direct = body["transaction"].is_object();
    let mut runner = if direct {
        engine::Runner::new(rules)
    } else {
        engine::Runner::for_apply(rules)
    };
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
    if let Some(ids) = &selected
        && inputs.len() != ids.len()
    {
        return Err(Error::missing());
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
            validate_draft(input)?;
            for field in ["payee_id", "category_id", "parent_id", "transfer_id"] {
                if input.get(field).is_none() {
                    input[field] = Value::Null;
                }
            }
        }
        let (mut after, matched) = if direct {
            runner.evaluate(db, b, input)?
        } else {
            runner.evaluate_stored(db, b, input)?
        };
        if body["manual_entry"] == true {
            after = super::entry::protect(input, &after);
        }
        // Source-generated children have no notes variable until an action sets
        // it. Expose the persisted empty-text shape only after all rules finish.
        if let Some(children) = after.get_mut("children").and_then(Value::as_array_mut) {
            for child in children {
                child
                    .as_object_mut()
                    .unwrap()
                    .entry("notes")
                    .or_insert(json!(""));
            }
        }

        if direct || selected.is_some() || !matched.is_empty() {
            entries.push(Entry {
                before: input.clone(),
                after,
                matched_rule_ids: matched,
            });
        }
    }
    Ok((entries, candidates, runner.into_work()))
}
pub fn preview(db: &Connection, b: &str, body: &Value) -> Result<Value, Error> {
    let (entries, candidate_count, _) = evaluate(db, b, body)?;
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
    if let Some(new) = after.get("new_payee").filter(|v| v.is_object()) {
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
    validate_body(body, true)?;
    let ids = ids(body, true)?.unwrap();
    let (entries, _, work) = evaluate(db, b, body)?;
    commit(
        db,
        b,
        &ids,
        entries,
        clock,
        epoch,
        Command {
            created: None,
            work,
        },
    )
}
pub(super) struct Command<'a> {
    pub created: Option<&'a str>,
    pub work: engine::Work,
}
pub(super) fn commit(
    db: &Connection,
    b: &str,
    ids: &[String],
    entries: Vec<Entry>,
    clock: &str,
    epoch: Option<&str>,
    command: Command<'_>,
) -> Result<Value, Error> {
    commit_with_work(db, b, ids, entries, clock, epoch, command).map(|result| result.0)
}
pub(super) fn commit_with_work(
    db: &Connection,
    b: &str,
    ids: &[String],
    entries: Vec<Entry>,
    clock: &str,
    epoch: Option<&str>,
    command: Command<'_>,
) -> Result<(Value, engine::Work), Error> {
    let Command { created, work } = command;
    // Source previews can contain fractional minor units. Refuse persistence
    // before transfer arithmetic or any payee/ledger mutation; never truncate.
    for entry in &entries {
        if entry.after["deleted"] != true {
            validate_draft(&entry.after)?;
        }
    }
    let mut peer_runner = engine::Runner::with_work(load(db, b)?, work);
    let mut created_peers = std::collections::BTreeSet::new();
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
            && entry.after["deleted"] != true
            && peer.after["deleted"] != true
            && entry.after["amount"] != entry.before["amount"]
            && peer.after["amount"] != peer.before["amount"]
            && entry.after["amount"].as_i64().unwrap() != -peer.after["amount"].as_i64().unwrap()
        {
            return Err(Error::conflict(
                "Selected rules give a transfer two contradictory amounts. No rule actions were saved.",
            ));
        }
    }
    for entry in &entries {
        if entry.matched_rule_ids.is_empty() || entry.before == entry.after {
            continue;
        }
        let structural = entry.after["is_parent"] == true
            || !entry.before["parent_id"].is_null()
            || entry.after["deleted"] == true;
        let fields = patch(&entry.before, &entry.after);
        let affected =
            crate::transfers::protection::members(db, b, &[entry.before["id"].as_str().unwrap()])?;
        for row in affected {
            let id = row["id"].as_str().unwrap();
            if created == Some(id) {
                continue;
            }
            if structural {
                let stale:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM field_versions WHERE budget_id=?1 AND entity='transactions' AND entity_id=?2 AND clock>=?3)",params![b,id,clock],|r|r.get(0))?;
                if stale {
                    return Err(Error::conflict(
                        "A newer entry prevents applying the rules. Refresh and review again.",
                    ));
                }
            } else {
                for field in fields.as_object().unwrap().keys() {
                    guard(
                        db,
                        b,
                        "transactions",
                        id,
                        if field == "new_payee" {
                            "payee_id"
                        } else {
                            field
                        },
                        clock,
                    )?;
                }
            }
        }
    }
    // Payee-name actions are planned during preview; create each planned name
    // once inside this same atomic receipt, including allocation-only names.
    let mut schedule_links = std::collections::BTreeMap::<String, String>::new();
    for entry in &entries {
        for (before, after) in std::iter::once((&entry.before, &entry.after)).chain(
            entry.after["children"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|after| {
                    let before = entry.before["children"]
                        .as_array()
                        .and_then(|rows| rows.iter().find(|row| row["id"] == after["id"]))
                        .unwrap_or(&Value::Null);
                    (before, after)
                }),
        ) {
            if after["deleted"] != true
                && after["schedule_id"] != before["schedule_id"]
                && let (Some(id), Some(schedule)) =
                    (after["id"].as_str(), after["schedule_id"].as_str())
            {
                schedule_links.insert(id.into(), schedule.into());
            }
        }
    }
    let mut entries = entries;
    let mut planned = std::collections::BTreeMap::new();
    for entry in &entries {
        for row in std::iter::once(&entry.after)
            .chain(entry.after["children"].as_array().into_iter().flatten())
        {
            if let Some(new) = row.get("new_payee").filter(|v| v.is_object()) {
                planned.insert(new["id"].as_str().unwrap().to_owned(), new.clone());
            }
        }
    }
    for (id, new) in planned {
        crate::payees_tags::create(db, b, "payees", &new, clock)?;
        sync::mark(db, b, "payees", &id)?;
    }
    for entry in &mut entries {
        entry.after.as_object_mut().unwrap().remove("new_payee");
        if let Some(children) = entry.after["children"].as_array_mut() {
            for child in children {
                child.as_object_mut().unwrap().remove("new_payee");
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
        let mut current = ledger::get(db, b, id)?;
        let desired_payee = entry.after["payee_id"].as_str();
        let destination = if let Some(payee) = desired_payee {
            crate::payees_tags::record(db, b, "payees", payee)?["transfer_account_id"]
                .as_str()
                .map(str::to_owned)
        } else {
            None
        };
        if entry.after["is_parent"] == true && current["transfer_id"].is_string() {
            crate::transfers::composition::remove_child(db, b, id, clock, None)?;
            current = ledger::get(db, b, id)?;
        }
        // A changed transfer payee is a graph operation, not an ordinary
        // payee patch. Original graph locks and winning clocks were checked
        // above, before any same-command detach stamps existed.
        if entry.after["deleted"] != true
            && entry.after["is_parent"] != true
            && entry.before["payee_id"] != entry.after["payee_id"]
            && (current["transfer_id"].is_string() || destination.is_some())
        {
            if current["transfer_id"].is_string() {
                crate::transfers::composition::remove_child(db, b, id, clock, None)?;
            }
            let mut fields = patch(&entry.before, &entry.after);
            if destination.is_some() {
                fields.as_object_mut().unwrap().remove("payee_id");
            }
            if !fields.as_object().unwrap().is_empty() {
                ledger::update(db, b, id, fields.clone())?;
                for field in fields.as_object().unwrap().keys() {
                    sync::seed_field_version(db, b, "transactions", id, field, clock)?;
                }
                sync::mark(db, b, "transactions", id)?;
            }
            if let Some(destination) = destination {
                let peer = super::actions::identity(&["actualBudget.rule-pair.v1", b, id, clock]);
                crate::transfers::composition::save_child(
                    db,
                    b,
                    id,
                    &destination,
                    &peer,
                    clock,
                    None,
                )?;
                created_peers.insert(peer);
            }
            changed.push(ledger::get(db, b, id)?);
            continue;
        }
        let mutation = if entry.after["deleted"] == true {
            sync::Mutation::Delete(id.to_owned())
        } else if entry.after["is_parent"] == true {
            let children = entry.after["children"]
                .as_array()
                .ok_or_else(|| Error::invalid("Missing rule allocations"))?
                .iter()
                .map(|c| {
                    let child_id = c["id"].as_str().unwrap();
                    let destination = if let Some(payee) = c["payee_id"].as_str() {
                        crate::payees_tags::record(db, b, "payees", payee)?["transfer_account_id"]
                            .as_str()
                            .map(str::to_owned)
                    } else {
                        None
                    };
                    let peer = destination.as_ref().map(|destination| {
                        let old = current["children"]
                            .as_array()
                            .and_then(|cs| cs.iter().find(|r| r["id"] == c["id"]));
                        old.filter(|r| r["transfer_account_id"] == *destination)
                            .and_then(|r| r["transfer_id"].as_str())
                            .map(str::to_owned)
                            .unwrap_or_else(|| {
                                super::actions::identity(&[
                                    "actualBudget.rule-child-pair.v1",
                                    b,
                                    child_id,
                                    clock,
                                ])
                            })
                    });
                    Ok(crate::splits::Child {
                        id: c["id"].as_str().unwrap().to_owned(),
                        amount: c["amount"].as_i64().unwrap(),
                        payee_id: c["payee_id"].as_str().map(str::to_owned),
                        category_id: c["category_id"].as_str().map(str::to_owned),
                        notes: c["notes"].as_str().unwrap_or("").into(),
                        transfer_account_id: destination,
                        counterpart_id: peer,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            for child in &children {
                if let Some(peer) = &child.counterpart_id {
                    let exists: bool = db.query_row(
                        "SELECT EXISTS(SELECT 1 FROM transactions WHERE budget_id=?1 AND id=?2)",
                        params![b, peer],
                        |r| r.get(0),
                    )?;
                    if !exists {
                        created_peers.insert(peer.clone());
                    }
                }
            }
            let mut fields = patch(&entry.before, &entry.after);
            fields.as_object_mut().unwrap().remove("category_id");
            fields.as_object_mut().unwrap().remove("payee_id");
            sync::Mutation::Split(
                id.to_owned(),
                crate::splits::Change {
                    clear_parent_references: true,
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
            let mut fields = patch(&entry.before, &entry.after);
            fields
                .as_object_mut()
                .unwrap()
                .retain(|field, value| current[field] != *value);
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
    // A schedule-only action is still a durable rule effect. Ordinary paired
    // legs share the link; a split parent's allocations remain independent.
    let mut expanded = schedule_links.clone();
    for (id, schedule) in &schedule_links {
        let row = ledger::get(db, b, id)?;
        if row["is_parent"] != true
            && let Some(peer) = row["transfer_id"].as_str()
        {
            if schedule_links
                .get(peer)
                .is_some_and(|other| other != schedule)
            {
                return Err(Error::conflict(
                    "Selected rules link the transfer legs to different schedules. No rule actions were saved.",
                ));
            }
            expanded.insert(peer.into(), schedule.clone());
        }
    }
    for (id, schedule) in expanded {
        let row = ledger::get(db, b, &id)?;
        if row["schedule_id"] != schedule {
            crate::schedules::link(db, b, &id, &schedule, clock)?;
            if let Some(saved) = changed.iter_mut().find(|row| row["id"] == id) {
                *saved = ledger::get(db, b, &id)?;
            } else {
                changed.push(ledger::get(db, b, &id)?);
            }
        }
    }
    for peer in created_peers {
        super::entry::peer_rules(db, b, &peer, clock, &mut peer_runner)?;
    }
    Ok((
        json!({"affected_count":changed.len(),"selected_count":ids.len(),"entries":changed}),
        peer_runner.into_work(),
    ))
}
