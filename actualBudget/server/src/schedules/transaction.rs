//! Recurrence changes share the transaction's receipt, history entry and rollback.
use super::*;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Recurrence {
    enabled: bool,
    config: Value,
}
pub(crate) fn apply(
    db: &Connection,
    b: &str,
    id: &str,
    value: Value,
    clock: &str,
) -> Result<(), Error> {
    let input: Recurrence =
        serde_json::from_value(value).map_err(|_| Error::invalid("Invalid recurrence settings"))?;
    let row = crate::ledger::get(db, b, id)?;
    if row["is_parent"] == true
        || !row["parent_id"].is_null()
        || !row["transfer_id"].is_null()
        || row["starting_balance"] == true
    {
        return Err(Error::invalid(
            "Recurrence requires an ordinary transaction",
        ));
    }
    let existing = row["schedule_id"].as_str();
    if let Some(schedule) = existing {
        data::guard(
            db,
            b,
            "schedule_links",
            id,
            &["schedule_id", "tombstone"],
            clock,
        )?;
        let old = data::get(db, b, schedule)?;
        data::guard(
            db,
            b,
            "schedules",
            schedule,
            &["completed", "next_date", "tombstone"],
            clock,
        )?;
        data::guard(
            db,
            b,
            "rules",
            old["rule_id"].as_str().unwrap(),
            &["conditions", "actions", "tombstone"],
            clock,
        )?;
    }
    if !input.enabled {
        if let Some(schedule) = existing {
            let command =
                serde_json::from_value(json!({"action":"update","id":schedule,"completed":true}))
                    .map_err(Error::internal)?;
            commands::apply(db, b, command, clock, None)?;
            sync::mark(db, b, "transactions", id)?;
        }
        return Ok(());
    }
    if !input.config.is_object() {
        return Err(Error::invalid("Choose a repeating frequency"));
    }
    // Validate before changing either the schedule or its financial entry.
    calendar::dates(
        &input.config,
        row["date"].as_str().unwrap(),
        1,
        &mut 2_000_000,
    )?;
    let schedule = existing.map(str::to_owned).unwrap_or_else(|| {
        data::identity(&["actualBudget.transaction-recurrence.v1", b, id, clock])
    });
    let mut conditions = json!([
        {"field":"date","op":"is","value":input.config},
        {"field":"account","op":"is","value":row["account_id"]},
        {"field":"amount","op":"is","value":row["amount"]},
        {"field":"payee","op":"is","value":row["payee_id"]}
    ]);
    let mut actions = json!([
        {"op":"set","field":"category","value":row["category_id"]}
    ]);
    if let Some(existing) = existing {
        let old = data::get(db, b, existing)?;
        for condition in old["conditions"].as_array().unwrap() {
            if !matches!(
                condition["field"].as_str(),
                Some("date" | "account" | "acct" | "amount" | "payee" | "description")
            ) {
                conditions.as_array_mut().unwrap().push(condition.clone());
            }
        }
        for action in old["actions"].as_array().unwrap() {
            if !(action["op"] == "set"
                && matches!(action["field"].as_str(), Some("category" | "notes")))
            {
                actions.as_array_mut().unwrap().push(action.clone());
            }
        }
    }
    let command = serde_json::from_value(json!({"action":if existing.is_some(){"update"}else{"create"},"id":schedule,"conditions":conditions,"actions":actions,"posts_transaction":true,"completed":false})).map_err(Error::internal)?;
    commands::apply(db, b, command, clock, None)?;
    if existing.is_none() {
        data::link(db, b, id, &schedule, clock)?;
    }
    // Never repeat the entry being saved or an occurrence already handled.
    let handled: Option<String> = db.query_row("SELECT max(occurrence_date) FROM schedule_occurrences WHERE budget_id=?1 AND schedule_id=?2 AND tombstone=0", params![b,schedule], |r|r.get(0))?;
    let yesterday = calendar::shift(&calendar::day()?, -1)?;
    let mut after = row["date"].as_str().unwrap().max(yesterday.as_str());
    if let Some(ref handled) = handled {
        after = after.max(handled.as_str());
    }
    let next = calendar::after(&input.config, after, &mut 2_000_000)?;
    db.execute(
        "UPDATE schedules SET next_date=?3,completed=?4 WHERE budget_id=?1 AND id=?2",
        params![b, schedule, next, next.is_none()],
    )?;
    data::stamp(
        db,
        b,
        "schedules",
        &schedule,
        &["next_date", "completed"],
        clock,
    )?;
    sync::mark(db, b, "transactions", id)?;
    Ok(())
}
