use super::*;
use crate::models::{validate_id, validate_name};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub action: String,
    pub transfer_account_id: Option<String>,
    pub id: Option<String>,
    pub snapshot: Option<String>,
    pub name: Option<String>,
    pub posts_transaction: Option<bool>,
    pub completed: Option<bool>,
    pub conditions: Option<Vec<Value>>,
    pub actions: Option<Vec<Value>>,
    pub occurrence_date: Option<String>,
    pub today: Option<bool>,
    pub custom_upcoming_length: Option<String>,
    pub upcoming_length: Option<String>,
    pub through: Option<String>,
    pub use_global_upcoming_length: Option<bool>,
    pub reset_next_date: Option<bool>,
}
fn required<'a>(v: &'a Option<String>, name: &str) -> Result<&'a str, Error> {
    v.as_deref()
        .ok_or_else(|| Error::invalid(format!("{name} is required")))
}
fn conditions(c: &[Value]) -> Result<(), Error> {
    let date = calendar::date_condition(c)?;
    let mut work = 2_000_000;
    calendar::next(&date["value"], &calendar::day()?, &mut work)?;
    // Validate full conditions against the same live rule parser.
    let rule = crate::rules::Rule {
        id: data::identity(&["validate"]),
        stage: None,
        conditions_op: "and".into(),
        conditions: c.to_vec(),
        actions: vec![],
    };
    crate::rules::validate_definition(&rule)
}
pub fn create(db: &Connection, b: &str, c: &Command, clock: &str) -> Result<Value, Error> {
    let id = required(&c.id, "id")?;
    validate_id(id)?;
    let prepared = condition_changes(db, b, c, &[], clock)?;
    let conds = prepared
        .as_ref()
        .ok_or_else(|| Error::invalid("Schedule conditions required"))?;
    conditions(conds)?;
    let name = c.name.as_deref().map(str::trim).filter(|v| !v.is_empty());
    if let Some(name) = name {
        validate_name(name)?;
        unique_name(db, b, id, name)?;
    }
    if db.query_row(
        "SELECT EXISTS(SELECT 1 FROM schedules WHERE budget_id=?1 AND id=?2)",
        params![b, id],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Error::conflict("Schedule identity already exists"));
    }
    let rule = data::identity(&["actualBudget.schedule-rule.v1", b, id]);
    let mut actions = vec![json!({"op":"link-schedule","value":id})];
    actions.extend(c.actions.clone().unwrap_or_default());
    let saved = crate::rules::Rule {
        id: rule.clone(),
        stage: None,
        conditions_op: "and".into(),
        conditions: conds.clone(),
        actions,
    };
    crate::rules::validate_definition(&saved)?;
    let next = calendar::next(
        &calendar::date_condition(conds)?["value"],
        &calendar::day()?,
        &mut 2_000_000,
    )?;
    db.execute("INSERT INTO rules(budget_id,id,conditions_op,conditions,actions) VALUES(?1,?2,'and',?3,?4)",params![b,rule,serde_json::to_string(conds).map_err(Error::internal)?,serde_json::to_string(&saved.actions).map_err(Error::internal)?])?;
    if let Some(value) = &c.custom_upcoming_length {
        view::horizon_days(value, &calendar::day()?)?;
    }
    db.execute("INSERT INTO schedules(budget_id,id,rule_id,name,posts_transaction,completed,next_date,custom_upcoming_length) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![b,id,rule,name,c.posts_transaction.unwrap_or(false),c.completed.unwrap_or(false),next,c.custom_upcoming_length])?;
    data::stamp(
        db,
        b,
        "rules",
        &rule,
        &[
            "stage",
            "conditions_op",
            "conditions",
            "actions",
            "tombstone",
        ],
        clock,
    )?;
    data::stamp(
        db,
        b,
        "schedules",
        id,
        &[
            "rule_id",
            "name",
            "posts_transaction",
            "completed",
            "next_date",
            "custom_upcoming_length",
            "tombstone",
        ],
        clock,
    )?;
    data::get(db, b, id)
}
pub fn apply(
    db: &Connection,
    b: &str,
    c: Command,
    clock: &str,
    epoch: Option<&str>,
) -> Result<Value, Error> {
    match c.action.as_str() {
        "create" => return create(db, b, &c, clock),
        "catch-up" => return catch_up(db, b, clock, epoch, c.through.as_deref()),
        "settings" => {
            let value = required(&c.upcoming_length, "upcoming_length")?;
            view::horizon_days(value, &calendar::day()?)?;
            data::guard(db, b, "schedule_settings", "horizon", &["value"], clock)?;
            db.execute(
                "UPDATE schedule_settings SET value=?2 WHERE budget_id=?1",
                params![b, value],
            )?;
            data::stamp(db, b, "schedule_settings", "horizon", &["value"], clock)?;
            return Ok(json!({"upcoming_length":value}));
        }
        _ => {}
    }
    let id = required(&c.id, "id")?;
    validate_id(id)?;
    let row = data::get(db, b, id)?;
    match c.action.as_str() {
        "update" => edit(db, b, &row, &c, clock),
        "delete" => {
            data::guard(
                db,
                b,
                "schedules",
                id,
                &[
                    "rule_id",
                    "name",
                    "posts_transaction",
                    "completed",
                    "next_date",
                    "custom_upcoming_length",
                    "tombstone",
                ],
                clock,
            )?;
            let rule = row["rule_id"].as_str().unwrap();
            data::guard(
                db,
                b,
                "rules",
                rule,
                &[
                    "stage",
                    "conditions_op",
                    "conditions",
                    "actions",
                    "tombstone",
                ],
                clock,
            )?;
            db.execute(
                "UPDATE schedules SET tombstone=1 WHERE budget_id=?1 AND id=?2",
                params![b, id],
            )?;
            db.execute(
                "UPDATE rules SET tombstone=1 WHERE budget_id=?1 AND id=?2",
                params![b, rule],
            )?;
            data::stamp(db, b, "schedules", id, &["tombstone"], clock)?;
            data::stamp(db, b, "rules", rule, &["tombstone"], clock)?;
            Ok(json!({"id":id,"deleted":true}))
        }
        "post" => {
            let date = required(&c.occurrence_date, "occurrence_date")?;
            let mut work = crate::rules::Work::default();
            let mut remaining = 500;
            let result = post(
                db,
                b,
                &row,
                date,
                c.today == Some(true),
                clock,
                epoch,
                &mut work,
                &mut remaining,
            )?;
            Ok(json!({"schedule":data::get(db,b,id)?,"transaction":result}))
        }
        "skip" => {
            let date = required(&c.occurrence_date, "occurrence_date")?;
            check_current(&row, date)?;
            data::guard(db, b, "schedules", id, &["next_date"], clock)?;
            if advance(db, b, &row, clock, &mut 2_000_000)? {
                record(db, b, id, date, "skipped", None, clock)?;
            }
            data::get(db, b, id)
        }
        _ => Err(Error::invalid("Unknown schedule command")),
    }
}
fn check_current(row: &Value, date: &str) -> Result<(), Error> {
    crate::models::validate_date(date)?;
    if row["completed"] == true || row["next_date"] != date {
        return Err(Error::conflict(
            "The schedule occurrence changed. Refresh and review it.",
        ));
    }
    Ok(())
}
fn existing(
    db: &Connection,
    b: &str,
    id: &str,
    date: &str,
) -> Result<Option<(String, Option<String>)>, Error> {
    Ok(db.query_row("SELECT status,transaction_id FROM schedule_occurrences WHERE budget_id=?1 AND schedule_id=?2 AND occurrence_date=?3 AND tombstone=0",params![b,id,date],|r|Ok((r.get(0)?,r.get(1)?))).optional()?)
}
fn record(
    db: &Connection,
    b: &str,
    schedule: &str,
    date: &str,
    status: &str,
    transaction: Option<&str>,
    clock: &str,
) -> Result<(), Error> {
    let id = data::identity(&["actualBudget.schedule-occurrence.v1", b, schedule, date]);
    if let Some((old, tx)) = existing(db, b, schedule, date)? {
        if old == status && tx.as_deref() == transaction {
            return Ok(());
        }
        return Err(Error::conflict(
            "This occurrence was already handled. Review it before changing its completion.",
        ));
    }
    data::guard(
        db,
        b,
        "schedule_occurrences",
        &id,
        &["status", "transaction_id", "tombstone"],
        clock,
    )?;
    db.execute("INSERT INTO schedule_occurrences(budget_id,id,schedule_id,occurrence_date,status,transaction_id) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(budget_id,id) DO UPDATE SET status=excluded.status,transaction_id=excluded.transaction_id,tombstone=0",params![b,id,schedule,date,status,transaction])?;
    data::stamp(
        db,
        b,
        "schedule_occurrences",
        &id,
        &[
            "schedule_id",
            "occurrence_date",
            "status",
            "transaction_id",
            "tombstone",
        ],
        clock,
    )
}
#[allow(clippy::too_many_arguments)]
fn post(
    db: &Connection,
    b: &str,
    row: &Value,
    date: &str,
    today: bool,
    clock: &str,
    epoch: Option<&str>,
    work: &mut crate::rules::Work,
    remaining: &mut usize,
) -> Result<Value, Error> {
    let id = row["id"].as_str().unwrap();
    if let Some((status, transaction)) = existing(db, b, id, date)? {
        return match (status.as_str(), transaction) {
            ("posted" | "matched", Some(tx)) => crate::ledger::get_any(db, b, &tx),
            ("suppressed", None) => Ok(Value::Null),
            _ => Err(Error::conflict(
                "The occurrence was skipped; review it before posting",
            )),
        };
    }
    check_current(row, date)?;
    if let Some(transaction) = view::matched_transaction(db, b, row, date)? {
        record(db, b, id, date, "matched", Some(&transaction), clock)?;
        return crate::ledger::get(db, b, &transaction);
    }
    *remaining = remaining.checked_sub(1).ok_or_else(|| {
        Error::invalid("Catch-up exceeds500 occurrences. Review older dates before continuing.")
    })?;
    let conds = row["conditions"].as_array().unwrap();
    let account = conds
        .iter()
        .find(|c| matches!(c["field"].as_str(), Some("account" | "acct")) && c["op"] == "is")
        .and_then(|c| c["value"].as_str())
        .ok_or_else(|| Error::invalid("Select an account before posting this schedule"))?;
    super::account_open(db, b, account)?;
    let payee = conds
        .iter()
        .find(|c| matches!(c["field"].as_str(), Some("payee" | "description")) && c["op"] == "is")
        .map(|c| c["value"].clone())
        .unwrap_or(Value::Null);
    let amount = conds
        .iter()
        .find(|c| {
            c["field"] == "amount"
                && matches!(c["op"].as_str(), Some("is" | "isapprox" | "isbetween"))
        })
        .map(|c| c["value"].clone())
        .unwrap_or(json!(0));
    let amount = scheduled_amount(&amount)?;
    let tx = data::identity(&["actualBudget.schedule-transaction.v1", b, id, date]);
    let input = json!({"id":tx,"account_id":account,"payee_id":payee,"category_id":null,"amount":amount,"date":if today{calendar::day()?}else{date.to_owned()},"notes":"","cleared":false,"schedule_id":id,"is_parent":false,"parent_id":null});
    let (remaining, inserted) =
        crate::rules::post_schedule(db, b, &input, clock, epoch, std::mem::take(work))?;
    *work = remaining;
    if !inserted {
        record(db, b, id, date, "suppressed", None, clock)?;
        return Ok(Value::Null);
    }
    record(db, b, id, date, "posted", Some(&tx), clock)?;
    crate::ledger::get(db, b, &tx)
}
fn advance(
    db: &Connection,
    b: &str,
    row: &Value,
    clock: &str,
    work: &mut usize,
) -> Result<bool, Error> {
    let id = row["id"].as_str().unwrap();
    let Some(old) = row["next_date"].as_str() else {
        return Ok(false);
    };
    let value = &calendar::date_condition(row["conditions"].as_array().unwrap())?["value"];
    let Some(next) = calendar::after(value, old, work)? else {
        return Ok(false);
    };
    db.execute(
        "UPDATE schedules SET next_date=?3 WHERE budget_id=?1 AND id=?2",
        params![b, id, next],
    )?;
    data::stamp(db, b, "schedules", id, &["next_date"], clock)?;
    Ok(true)
}
fn catch_up(
    db: &Connection,
    b: &str,
    clock: &str,
    epoch: Option<&str>,
    through: Option<&str>,
) -> Result<Value, Error> {
    let actual_today = calendar::day()?;
    let today = through.unwrap_or(&actual_today).to_owned();
    crate::models::validate_date(&today)?;
    if today > actual_today {
        return Err(Error::invalid(
            "Automatic catch-up cannot post future dates",
        ));
    }
    let mut work = crate::rules::Work::default();
    let mut remaining = 500;
    let mut posted = vec![];
    for initial in data::list(db, b)? {
        let id = initial["id"].as_str().unwrap();
        if initial["completed"] == true {
            continue;
        }
        // Closed/missing account schedules remain visible without posting/advancing.
        if let Some(account) = initial["conditions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["field"] == "account" && c["op"] == "is")
            .and_then(|c| c["value"].as_str())
        {
            if super::account_open(db, b, account).is_err() {
                continue;
            }
        } else {
            continue;
        }
        let mut next_guarded = false;
        loop {
            let row = data::get(db, b, id)?;
            let Some(date) = row["next_date"].as_str() else {
                break;
            };
            if date > today.as_str() {
                break;
            }
            let paid = existing(db, b, id, date)?.is_some() || view::has_transactions(db, b, &row)?;
            let recurring =
                calendar::date_condition(row["conditions"].as_array().unwrap())?["value"]
                    .is_object();
            if row["posts_transaction"] == true && (recurring || !paid) {
                if !paid {
                    let transaction = post(
                        db,
                        b,
                        &row,
                        date,
                        false,
                        clock,
                        epoch,
                        &mut work,
                        &mut remaining,
                    )?;
                    posted.push(transaction["id"].as_str().map(str::to_owned));
                }
                if date != today && recurring && !next_guarded {
                    data::guard(db, b, "schedules", id, &["next_date"], clock)?;
                    next_guarded = true;
                }
                if date == today || !recurring || !advance(db, b, &row, clock, &mut work.remaining)?
                {
                    break;
                }
            } else if paid {
                if recurring {
                    let next = calendar::next(
                        &calendar::date_condition(row["conditions"].as_array().unwrap())?["value"],
                        &today,
                        &mut work.remaining,
                    )?;
                    if next.as_deref() != Some(date) {
                        data::guard(db, b, "schedules", id, &["next_date"], clock)?;
                        db.execute(
                            "UPDATE schedules SET next_date=?3 WHERE budget_id=?1 AND id=?2",
                            params![b, id, next],
                        )?;
                        data::stamp(db, b, "schedules", id, &["next_date"], clock)?;
                    }
                } else if date < today.as_str() {
                    data::guard(db, b, "schedules", id, &["completed"], clock)?;
                    db.execute(
                        "UPDATE schedules SET completed=1 WHERE budget_id=?1 AND id=?2",
                        params![b, id],
                    )?;
                    data::stamp(db, b, "schedules", id, &["completed"], clock)?;
                }
                break;
            } else {
                break;
            }
        }
    }
    Ok(
        json!({"posted_count":posted.iter().flatten().count(),"handled_count":posted.len(),"transaction_ids":posted.into_iter().flatten().collect::<Vec<_>>()}),
    )
}

fn edit(db: &Connection, b: &str, row: &Value, c: &Command, clock: &str) -> Result<Value, Error> {
    let id = row["id"].as_str().unwrap();
    let mut fields = vec![];
    let mut updated = row.clone();
    if let Some(name) = &c.name {
        let name = name.trim();
        if !name.is_empty() {
            validate_name(name)?;
            unique_name(db, b, id, name)?;
        }
        updated["name"] = if name.is_empty() {
            Value::Null
        } else {
            json!(name)
        };
        fields.push("name");
    }
    if let Some(automatic) = c.posts_transaction {
        updated["posts_transaction"] = json!(automatic);
        fields.push("posts_transaction");
    }
    if let Some(completed) = c.completed {
        updated["completed"] = json!(completed);
        fields.push("completed");
    }
    if let Some(horizon) = &c.custom_upcoming_length {
        view::horizon_days(horizon, &calendar::day()?)?;
        updated["custom_upcoming_length"] = json!(horizon);
        fields.push("custom_upcoming_length");
    }
    if c.use_global_upcoming_length == Some(true) {
        updated["custom_upcoming_length"] = Value::Null;
        if !fields.contains(&"custom_upcoming_length") {
            fields.push("custom_upcoming_length");
        }
    }
    let rule = row["rule_id"].as_str().unwrap();
    let mut changed_rule = vec![];
    let prepared = condition_changes(db, b, c, row["conditions"].as_array().unwrap(), clock)?;
    if let Some(changes) = &prepared {
        let mut merged = row["conditions"].as_array().unwrap().clone();
        for change in changes {
            let field = change["field"]
                .as_str()
                .ok_or_else(|| Error::invalid("Condition field required"))?;
            let eligible = |item: &Value| {
                item["field"] == field
                    && match field {
                        "date" => matches!(item["op"].as_str(), Some("is" | "isapprox")),
                        "amount" => {
                            matches!(item["op"].as_str(), Some("is" | "isapprox" | "isbetween"))
                        }
                        "account" | "payee" => item["op"] == "is",
                        _ => false,
                    }
            };
            if let Some(index) = merged.iter().position(eligible) {
                merged[index] = change.clone();
            } else {
                merged.push(change.clone());
            }
        }
        conditions(&merged)?;
        let old_conditions = row["conditions"].as_array().unwrap();
        let context = |items: &[Value]| {
            items
                .iter()
                .filter(|c| matches!(c["field"].as_str(), Some("date" | "account")))
                .map(|v| {
                    let mut v = v.clone();
                    v.as_object_mut().unwrap().remove("type");
                    v
                })
                .collect::<Vec<_>>()
        };
        if c.reset_next_date == Some(true) || context(&merged) != context(old_conditions) {
            updated["next_date"] = json!(calendar::next(
                &calendar::date_condition(&merged)?["value"],
                &calendar::day()?,
                &mut 2_000_000
            )?);
            fields.push("next_date");
        }
        updated["conditions"] = json!(merged);
        changed_rule.push("conditions");
        let scheduled = updated["conditions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| {
                c["field"] == "amount"
                    && matches!(c["op"].as_str(), Some("is" | "isapprox" | "isbetween"))
            })
            .map(|c| scheduled_amount(&c["value"]))
            .transpose()?
            .unwrap_or(0);
        let mut actions = updated["actions"].as_array().unwrap().clone();
        let mut changed = false;
        for action in &mut actions {
            if action["op"] == "set"
                && action["field"] == "amount"
                && action["options"]["template"].is_null()
                && action["options"]["formula"].is_null()
                && action["value"] != scheduled
            {
                action["value"] = json!(scheduled);
                changed = true;
            }
        }
        if changed {
            updated["actions"] = json!(actions);
            changed_rule.push("actions");
        }
    } else if c.reset_next_date == Some(true) {
        updated["next_date"] = json!(calendar::next(
            &calendar::date_condition(row["conditions"].as_array().unwrap())?["value"],
            &calendar::day()?,
            &mut 2_000_000
        )?);
        fields.push("next_date");
    }
    if let Some(actions) = &c.actions {
        updated["actions"] = json!(actions);
        if !changed_rule.contains(&"actions") {
            changed_rule.push("actions");
        }
    }
    if !changed_rule.is_empty() {
        if !updated["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["op"] == "link-schedule" && a["value"] == id)
        {
            return Err(Error::invalid(
                "Keep this schedule's linked action when editing its rule",
            ));
        }
        let saved = crate::rules::Rule {
            id: rule.into(),
            stage: None,
            conditions_op: "and".into(),
            conditions: updated["conditions"].as_array().unwrap().clone(),
            actions: updated["actions"].as_array().unwrap().clone(),
        };
        crate::rules::validate_definition(&saved)?;
        data::guard(db, b, "rules", rule, &changed_rule, clock)?;
        db.execute(
            "UPDATE rules SET conditions=?3,actions=?4 WHERE budget_id=?1 AND id=?2",
            params![
                b,
                rule,
                updated["conditions"].to_string(),
                updated["actions"].to_string()
            ],
        )?;
        data::stamp(db, b, "rules", rule, &changed_rule, clock)?;
    }
    if fields.is_empty() && changed_rule.is_empty() {
        return Err(Error::invalid("Choose a schedule field to change"));
    }
    data::guard(db, b, "schedules", id, &fields, clock)?;
    db.execute("UPDATE schedules SET name=?3,posts_transaction=?4,completed=?5,next_date=?6,custom_upcoming_length=?7 WHERE budget_id=?1 AND id=?2",params![b,id,updated["name"].as_str(),updated["posts_transaction"].as_bool(),updated["completed"].as_bool(),updated["next_date"].as_str(),updated["custom_upcoming_length"].as_str()])?;
    if !fields.is_empty() {
        data::stamp(db, b, "schedules", id, &fields, clock)?;
    }
    data::get(db, b, id)
}
pub(super) fn scheduled_amount(value: &Value) -> Result<i64, Error> {
    if value.is_null() {
        return Ok(0);
    }
    if let Some(n) = value.as_i64() {
        if n.unsigned_abs() > crate::models::MAX_MONEY as u64 {
            return Err(Error::invalid(
                "Scheduled amount exceeds safe integer range",
            ));
        }
        return Ok(n);
    }
    let a = value["num1"]
        .as_i64()
        .ok_or_else(|| Error::invalid("Invalid scheduled range"))?;
    let b = value["num2"]
        .as_i64()
        .ok_or_else(|| Error::invalid("Invalid scheduled range"))?;
    // Pinned getScheduledAmount uses JS Number addition before Math.round.
    // The result remains an exact integer; the oracle covers binary64 midpoint
    // loss near the safe-integer boundary and negative half-cent ties.
    let average = (a as f64 + b as f64) / 2.0;
    let lower = average.floor();
    let rounded = if average - lower >= 0.5 {
        lower + 1.0
    } else {
        lower
    };
    if !rounded.is_finite() || rounded.abs() > crate::models::MAX_MONEY as f64 {
        return Err(Error::invalid(
            "Scheduled midpoint exceeds safe integer range",
        ));
    }
    Ok(rounded as i64)
}

fn unique_name(db: &Connection, b: &str, id: &str, name: &str) -> Result<(), Error> {
    if db.query_row("SELECT EXISTS(SELECT 1 FROM schedules WHERE budget_id=?1 AND name=?2 AND id<>?3 AND tombstone=0)",params![b,name,id],|r|r.get::<_,bool>(0))?{return Err(Error::conflict("A schedule with this name already exists. Choose another name."));}
    Ok(())
}
fn condition_changes(
    db: &Connection,
    b: &str,
    c: &Command,
    base: &[Value],
    clock: &str,
) -> Result<Option<Vec<Value>>, Error> {
    let mut changes = c.conditions.clone();
    if let Some(destination) = &c.transfer_account_id {
        validate_id(destination)?;
        crate::ledger::account_exists(db, b, destination)?;
        let conditions = changes.get_or_insert_with(Vec::new);
        let origin = conditions
            .iter()
            .chain(base)
            .find(|c| matches!(c["field"].as_str(), Some("account" | "acct")) && c["op"] == "is")
            .and_then(|c| c["value"].as_str())
            .ok_or_else(|| Error::invalid("Choose an origin account for this transfer schedule"))?;
        if origin == destination {
            return Err(Error::invalid("Transfer accounts must differ"));
        }
        crate::ledger::account_exists(db, b, origin)?;
        let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM payees WHERE budget_id=?1 AND transfer_account_id=?2 AND tombstone=0)",params![b,destination],|r|r.get(0))?;
        let payee = crate::transfers::payee(db, b, destination)?;
        if !exists {
            data::stamp(
                db,
                b,
                "payees",
                &payee,
                &[
                    "name",
                    "favorite",
                    "transfer_account_id",
                    "category_id",
                    "merged_into",
                    "learn_categories",
                    "tombstone",
                ],
                clock,
            )?;
        }
        let condition = json!({"field":"payee","op":"is","value":payee});
        if let Some(index) = conditions.iter().position(|c| {
            matches!(c["field"].as_str(), Some("payee" | "description")) && c["op"] == "is"
        }) {
            conditions[index] = condition;
        } else {
            conditions.push(condition);
        }
    }
    Ok(changes)
}
