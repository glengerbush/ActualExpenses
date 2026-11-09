//! Future conversion is one captured financial operation, not create-then-delete HTTP calls.
use super::*;
use chrono::NaiveDate;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Preview {
    snapshot: String,
    transaction_id: Option<String>,
    transaction: Option<Value>,
    transfer_account_id: Option<String>,
    as_of: Option<String>,
    upcoming_length: Option<String>,
}
struct Plan {
    transaction: Value,
    conditions: Vec<Value>,
    actions: Vec<Value>,
    affected: Vec<Value>,
    is_future: bool,
    is_beyond_window: bool,
    days_until: i64,
    upcoming: i64,
    as_of: String,
}
fn transaction_input(
    db: &Connection,
    b: &str,
    id: Option<&str>,
    draft: Option<&Value>,
) -> Result<(Value, Vec<Value>), Error> {
    match (id, draft) {
        (Some(id), None) => {
            crate::models::validate_id(id)?;
            let row = crate::ledger::get(db, b, id)?;
            if !row["parent_id"].is_null() {
                return Err(Error::invalid(
                    "Convert the complete split parent so every allocation and transfer remains intact.",
                ));
            }
            if row["starting_balance"] == true {
                return Err(Error::invalid(
                    "Opening balances cannot become scheduled transactions.",
                ));
            }
            let members = crate::transfers::protection::members(db, b, &[id])?;
            let ids = members
                .iter()
                .map(|r| r["id"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            Ok((row, crate::ledger::get_many(db, b, &ids)?))
        }
        (None, Some(value)) => {
            let input: crate::ledger::NewTransaction = serde_json::from_value(value.clone())
                .map_err(|_| Error::invalid("A complete ordinary transaction draft is required"))?;
            input.validate()?;
            crate::ledger::validate_references(db, b, &input, None)?;
            for (entity, field) in [("payees", "new_payee"), ("categories", "new_category")] {
                if value[field].is_object() {
                    let exists: bool = db.query_row(
                        &format!(
                            "SELECT EXISTS(SELECT 1 FROM {entity} WHERE budget_id=?1 AND id=?2)"
                        ),
                        params![b, value[field]["id"].as_str()],
                        |r| r.get(0),
                    )?;
                    if exists {
                        return Err(Error::conflict(
                            "A new reference ID already exists. Choose its existing payee or category before converting.",
                        ));
                    }
                }
            }
            Ok((value.clone(), vec![]))
        }
        _ => Err(Error::invalid(
            "Choose one existing transaction or one new draft, not both",
        )),
    }
}
fn prepare(
    db: &Connection,
    b: &str,
    id: Option<&str>,
    draft: Option<&Value>,
    transfer: Option<&str>,
    as_of: Option<&str>,
    length: Option<&str>,
) -> Result<Plan, Error> {
    let (transaction, affected) = transaction_input(db, b, id, draft)?;
    if affected
        .iter()
        .any(|row| row["is_parent"] == true && row["id"] != transaction["id"])
    {
        return Err(Error::invalid(
            "This transfer belongs to another split. Convert its complete parent; a scheduled rule cannot flatten that financial graph.",
        ));
    }

    if transfer.is_some() && id.is_some() {
        return Err(Error::invalid(
            "An existing transfer keeps its captured destination; edit it before converting.",
        ));
    }
    let account = transaction["account_id"]
        .as_str()
        .ok_or_else(|| Error::invalid("Choose an account"))?;
    super::account_open(db, b, account)?;
    if let Some(destination) = transfer {
        crate::models::validate_id(destination)?;
        super::account_open(db, b, destination)?;
        if destination == account {
            return Err(Error::invalid("Choose two different accounts"));
        }
        if !transaction["payee_id"].is_null() || transaction["new_payee"].is_object() {
            return Err(Error::invalid(
                "Choose a transfer destination or an ordinary payee, not both",
            ));
        }
    }
    let date = transaction["date"]
        .as_str()
        .ok_or_else(|| Error::invalid("A future date is required"))?;
    crate::models::validate_date(date)?;
    let as_of = as_of.map(str::to_owned).unwrap_or(calendar::day()?);
    crate::models::validate_date(&as_of)?;
    let horizon = match length {
        Some(s) => s.to_owned(),
        None => db.query_row(
            "SELECT value FROM schedule_settings WHERE budget_id=?1 AND id='horizon'",
            [b],
            |r| r.get::<_, String>(0),
        )?,
    };
    let upcoming = view::horizon_days(&horizon, &as_of)?;
    let days_until = (NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(Error::internal)?
        - NaiveDate::parse_from_str(&as_of, "%Y-%m-%d").map_err(Error::internal)?)
    .num_days();
    let mut conditions = vec![
        json!({"op":"is","field":"date","value":date}),
        json!({"op":"is","field":"amount","value":transaction["amount"]}),
    ];
    let payee = if transaction["new_payee"].is_object() {
        transaction["new_payee"]["id"].clone()
    } else if let Some(payee) = transaction["payee_id"].as_str() {
        json!(crate::payees_tags::canonical(db, b, payee)?)
    } else {
        Value::Null
    };
    if !payee.is_null() {
        conditions.push(json!({"op":"is","field":"payee","value":payee}));
    }
    conditions.push(json!({"op":"is","field":"account","value":account}));
    let mut actions = vec![];
    if transaction["is_parent"] == true {
        let mut transfer_actions = vec![];
        if let Some(notes) = transaction["notes"].as_str().filter(|s| !s.is_empty()) {
            actions
                .push(json!({"op":"set","field":"notes","value":notes,"options":{"splitIndex":0}}));
        }
        for (index, child) in transaction["children"]
            .as_array()
            .ok_or_else(|| Error::invalid("A split needs all its allocations"))?
            .iter()
            .enumerate()
        {
            let index = index + 1;
            actions.push(json!({"op":"set-split-amount","value":child["amount"],"options":{"splitIndex":index,"method":"fixed-amount"}}));
            if !child["category_id"].is_null() {
                actions.push(json!({"op":"set","field":"category","value":child["category_id"],"options":{"splitIndex":index}}));
            }
            if let Some(notes) = child["notes"].as_str().filter(|s| !s.is_empty()) {
                actions.push(json!({"op":"set","field":"notes","value":notes,"options":{"splitIndex":index}}));
            }
            // The pinned helper drops this action and loses the reciprocal leg.
            // Preserve the required paired graph; ordinary payee omission stays
            // source-compatible. Never silently flatten a transfer allocation.
            if child["transfer_id"].is_string() {
                let payee = child["payee_id"].as_str().ok_or_else(|| {
                    Error::invalid("A transfer allocation needs its original account-managed payee")
                })?;
                transfer_actions.push(json!({"op":"set","field":"payee","value":payee,"options":{"splitIndex":index}}));
            }
        }
        // Allocate every child before changing a payee: the pinned engine
        // deliberately inherits the preceding allocation's current payee.
        actions.extend(transfer_actions);
    } else {
        let category = if transaction["new_category"].is_object() {
            transaction["new_category"]["id"].clone()
        } else {
            crate::categories::resolve(
                db,
                b,
                transaction["category_id"].as_str().map(str::to_owned),
            )?
            .map_or(Value::Null, Value::String)
        };
        if !category.is_null() {
            actions.push(json!({"op":"set","field":"category","value":category}));
        }
        if let Some(notes) = transaction["notes"].as_str().filter(|s| !s.is_empty()) {
            actions.push(json!({"op":"set","field":"notes","value":notes}));
        }
    }
    let mut complete = vec![
        json!({"op":"link-schedule","value":data::identity(&["conversion-preview-schedule"])}),
    ];
    complete.extend(actions.clone());
    crate::rules::validate_definition(&crate::rules::Rule{id:data::identity(&["conversion-preview-rule"]),stage:None,conditions_op:"and".into(),conditions:conditions.clone(),actions:complete}).map_err(|e|if e.status.is_client_error(){Error::invalid("This transaction cannot be represented by a bounded scheduled rule. Keep it unchanged or review its allocations and references.")}else{e})?;
    Ok(Plan {
        transaction,
        conditions,
        actions,
        affected,
        is_future: days_until > 0,
        is_beyond_window: days_until > upcoming,
        days_until,
        upcoming,
        as_of,
    })
}
pub(super) async fn preview(
    State(store): State<Store>,
    Path(b): Path<String>,
    input: Result<Json<Preview>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    let Json(body) = input.map_err(crate::api::json_error)?;
    Ok(Json(store.run(move|db|{
        sync::check_snapshot(db,&b,Some(&body.snapshot))?;
        let p=prepare(db,&b,body.transaction_id.as_deref(),body.transaction.as_ref(),body.transfer_account_id.as_deref(),body.as_of.as_deref(),body.upcoming_length.as_deref())?;
        Ok(json!({"data":{"transaction":p.transaction,"conditions":p.conditions,"actions":p.actions,"removed_transaction_ids":p.affected.iter().map(|r|r["id"].clone()).collect::<Vec<_>>(),"affected_transactions":p.affected,"protected_count":p.affected.iter().filter(|r|r["reconciled"]==true).count(),"is_future":p.is_future,"is_beyond_window":p.is_beyond_window,"days_until_transaction":p.days_until,"upcoming_days":p.upcoming,"as_of":p.as_of,"snapshot":sync::cursor(db,&b)?,"epoch":crate::lifecycle::epoch(db,&b)?}}))
    }).await?))
}
pub fn apply(
    db: &Connection,
    b: &str,
    c: &commands::Command,
    clock: &str,
    epoch: Option<&str>,
) -> Result<Value, Error> {
    let p = prepare(
        db,
        b,
        c.transaction_id.as_deref(),
        c.transaction.as_ref(),
        c.transfer_account_id.as_deref(),
        None,
        None,
    )?;
    if !p.is_future {
        return Err(Error::invalid(
            "Choose a future date. Use an ordinary schedule for past or current payments.",
        ));
    }
    if p.affected.iter().any(|r| r["reconciled"] == true) {
        return Err(Error::conflict(
            "Unlock the complete affected graph, then preview this conversion again. Nothing was converted.",
        ));
    }
    for row in &p.affected {
        let id = row["id"].as_str().unwrap();
        let stale:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM field_versions WHERE budget_id=?1 AND entity='transactions' AND entity_id=?2 AND clock>=?3)",params![b,id,clock],|r|r.get(0))?;
        if stale {
            return Err(Error::conflict(
                "A newer transaction field prevents conversion. Refresh and review the complete graph.",
            ));
        }
    }
    if let Some(raw) = &c.transaction {
        let mut raw = raw.clone();
        for (entity, new, field) in [
            ("payees", "new_payee", "payee_id"),
            ("categories", "new_category", "category_id"),
        ] {
            if raw[new].is_object() {
                if entity == "payees" {
                    crate::payees_tags::create(db, b, entity, &raw[new], clock)?;
                } else {
                    crate::categories::create_inline_at(db, b, &raw[new], clock)?;
                }
                raw[field] = raw[new]["id"].clone();
                raw.as_object_mut().unwrap().remove(new);
            }
        }
        // This reserves the original draft identity without publishing a purchase.
        // budget_changes stores the final entity state, so only its tombstone is
        // exposed after this atomic receipt commits.
        if c.transfer_account_id.is_some() {
            raw["payee_id"] = Value::Null;
        }
        raw["learn_categories"] = json!(false);
        let input: crate::ledger::NewTransaction =
            serde_json::from_value(raw).map_err(Error::internal)?;
        let id = input.id.clone();
        let result = sync::execute(
            db,
            b,
            Some(clock),
            false,
            epoch,
            sync::Mutation::Create(Box::new(input)),
        )?;
        if result["response_status"].as_u64().unwrap_or(500) >= 400 {
            return Err(Error::conflict(
                "This draft was already saved. Review its transaction before converting.",
            ));
        }
        let result = sync::execute(db, b, Some(clock), false, epoch, sync::Mutation::Delete(id))?;
        if result["response_status"].as_u64().unwrap_or(500) >= 400 {
            return Err(Error::conflict(
                "The draft identity could not be reserved. Nothing was converted.",
            ));
        }
    }
    let mut create = json!({"action":"create","id":c.id,"name":c.name,"posts_transaction":true,"conditions":p.conditions,"actions":p.actions});
    if let Some(destination) = &c.transfer_account_id {
        create["transfer_account_id"] = json!(destination);
    }
    let create: commands::Command = serde_json::from_value(create).map_err(Error::internal)?;
    let schedule = commands::create(db, b, &create, clock)?;
    if let Some(id) = &c.transaction_id {
        let result = sync::execute(
            db,
            b,
            Some(clock),
            false,
            epoch,
            sync::Mutation::Delete(id.clone()),
        )?;
        if result["response_status"].as_u64().unwrap_or(500) >= 400 {
            return Err(Error::conflict(
                "The original transaction changed. Nothing was converted.",
            ));
        }
    }
    Ok(
        json!({"schedule":schedule,"removed_transaction_ids":p.affected.iter().map(|r|r["id"].clone()).collect::<Vec<_>>(),"is_beyond_window":p.is_beyond_window,"days_until_transaction":p.days_until,"upcoming_days":p.upcoming}),
    )
}
