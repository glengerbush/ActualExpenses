//! Desktop entry protection is separate from explicit ledger rule application.
use super::*;
fn should_apply(field: &str, current: &Value, next: &Value) -> bool {
    if current.is_null() || current == "" || current == 0 || current == false {
        return true;
    }
    if field != "notes" || current == next {
        return false;
    }
    let (Some(current), Some(next)) = (current.as_str(), next.as_str()) else {
        return false;
    };
    let Some(index) = next.find(current) else {
        return false;
    };
    let prepended = &next[..index];
    let appended = &next[index + current.len()..];
    !((prepended.is_empty() || current.starts_with(prepended))
        && (appended.is_empty() || current.ends_with(appended)))
}
pub(super) fn protect(before: &Value, after: &Value) -> Value {
    let mut result = before.clone();
    if let Some(fields) = after.as_object() {
        for (field, next) in fields {
            if next != &before[field] && should_apply(field, &before[field], next) {
                result[field] = next.clone();
            }
        }
    }
    if result["payee_id"] != after["payee_id"] {
        result.as_object_mut().unwrap().remove("new_payee");
    }
    result
}
pub(super) async fn create(
    State(store): State<Store>,
    Path(b): Path<String>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let Json(body) = input.map_err(crate::api::json_error)?;
    crate::models::validate_id(&b)?;
    let key = crate::api::idempotency_key(&headers)?;
    let owner = crate::history::owner_header(&headers)?;
    let proposed = sync::header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let cancel = sync::cancel_header(&headers)?;
    let lookup = sync::receipt_only_header(&headers)?;
    let payload = body.to_string();
    store.run(move|db|{
        let scope=format!("POST /budgets/{b}/rules/transactions");
        if lookup{return sync::receipt_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::CREATED,owner.as_ref());}
        sync::write_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::CREATED,owner.as_ref(),|db|{
            if cancel{return sync::canceled(db,&b);}
            crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;sync::check_snapshot(db,&b,Some(snapshot(&body)?))?;
            let mut fields=body.clone();fields.as_object_mut().ok_or_else(||Error::invalid("Automatic-entry object required"))?.remove("snapshot");
            let mut input:crate::ledger::NewTransaction=serde_json::from_value(fields).map_err(|_|Error::invalid("Invalid automatic-entry transaction"))?;input.validate()?;
            let learn=input.learn_categories==Some(true)&&(input.category_id.is_some()||input.new_category.is_some());input.learn_categories=None;
            let id=input.id.clone();
            let mut before=serde_json::to_value(&input).map_err(Error::internal)?;
            before["is_parent"]=json!(false);before["parent_id"]=Value::Null;
            for (new,field) in [("new_payee","payee_id"),("new_category","category_id")] {if before[new].is_object(){before[field]=before[new]["id"].clone();}}
            before["payee_name"]=if before["new_payee"].is_object(){before["new_payee"]["name"].clone()}else if let Some(payee)=before["payee_id"].as_str(){json!(crate::payees_tags::record(db,&b,"payees",payee)?["name"])}else{Value::Null};
            let mut runner=engine::Runner::new(load(db,&b)?);
            let (after,matched)=runner.evaluate(db,&b,&before)?;
            let mut after=protect(&before,&after);
            let clock=sync::clock(db,&b,proposed.as_deref())?;
            if after["deleted"]==true{return Ok(json!({"response_status":201,"response_body":{"data":after,"snapshot":sync::cursor(db,&b)?,"edit_clock":clock}}));}
            let mut fields=serde_json::Map::new();
            for field in ["id","account_id","amount","date","notes","cleared","payee_id","category_id","new_payee","new_category"] {if let Some(value)=after.get(field){fields.insert(field.into(),value.clone());}}
            for (new,field) in [("new_payee","payee_id"),("new_category","category_id")] {if fields.get(new).is_some_and(Value::is_object){fields.insert(field.into(),Value::Null);}}
            let transfer=if let Some(payee)=fields["payee_id"].as_str(){crate::payees_tags::record(db,&b,"payees",payee)?["transfer_account_id"].as_str().map(str::to_owned)}else{None};
            if transfer.is_some(){fields.insert("payee_id".into(),Value::Null);}
            let final_input:crate::ledger::NewTransaction=serde_json::from_value(Value::Object(fields)).map_err(|_|Error::invalid("Rules produced invalid transaction fields"))?;
            final_input.validate()?;
            let result=sync::execute(db,&b,Some(&clock),false,epoch.as_deref(),sync::Mutation::Create(Box::new(final_input)))?;
            if result["response_status"].as_u64().unwrap_or(500)>=400{return Err(Error::conflict("The original transaction could not be saved"));}
            let linked_schedule=after["schedule_id"].as_str().map(str::to_owned);
            after.as_object_mut().unwrap().remove("new_payee");
            if after["is_parent"]==true {
                let before=crate::ledger::get(db,&b,&id)?;
                plan::commit(db,&b,std::slice::from_ref(&id),vec![plan::Entry{before,after,matched_rule_ids:matched}],&clock,epoch.as_deref(),plan::Command{created:Some(&id),work:runner.into_work()})?;
            }else if let Some(destination)=transfer {
                let peer=super::actions::identity(&["actualBudget.rule-entry-pair.v1",&b,&id]);
                crate::transfers::composition::save_child(db,&b,&id,&destination,&peer,&clock,None)?;
                if let Some(schedule)=linked_schedule.as_deref(){crate::schedules::link(db,&b,&id,schedule,&clock)?;crate::schedules::link(db,&b,&peer,schedule,&clock)?;}
                peer_rules(db,&b,&peer,&clock,&mut runner)?;
            }
            if let Some(schedule)=linked_schedule {
                let row=crate::ledger::get(db,&b,&id)?;
                if row["schedule_id"]!=schedule {crate::schedules::link(db,&b,&id,&schedule,&clock)?;}
                if row["is_parent"]!=true && let Some(peer)=row["transfer_id"].as_str(){
                    let row=crate::ledger::get(db,&b,peer)?;
                    if row["schedule_id"]!=schedule{crate::schedules::link(db,&b,peer,&schedule,&clock)?;}
                }
            }
            if learn{learning::manual(db,&b,std::slice::from_ref(&id),&clock)?;}
            let mut saved=crate::ledger::get_any(db,&b,&id)?;
            saved["deleted"]=json!(db.query_row("SELECT tombstone FROM transactions WHERE budget_id=?1 AND id=?2",params![b,id],|r|r.get::<_,bool>(0))?);
            Ok(json!({"response_status":201,"response_body":{"data":saved,"snapshot":sync::cursor(db,&b)?,"edit_clock":clock}}))
        })
    }).await
}

/// Only newly inserted counterparts enter here. Actual transfer.addTransfer
/// executes the whole chain, but persists only notes and cleared from it.
pub(super) fn peer_rules(
    db: &Connection,
    b: &str,
    id: &str,
    clock: &str,
    runner: &mut engine::Runner,
) -> Result<(), Error> {
    let before = crate::ledger::get(db, b, id)?;
    let (after, _) = runner.evaluate(db, b, &before)?;
    let notes = after["notes"].as_str().unwrap_or("");
    if notes.chars().count() > 10000 {
        return Err(Error::invalid("Rule notes exceed10000 characters"));
    }
    let cleared = after["cleared"]
        .as_bool()
        .ok_or_else(|| Error::invalid("Invalid counterpart cleared result"))?;
    if before["notes"] != notes || before["cleared"] != cleared {
        db.execute(
            "UPDATE transactions SET notes=?3,cleared=?4 WHERE budget_id=?1 AND id=?2",
            params![b, id, notes, cleared],
        )?;
        for field in ["notes", "cleared"] {
            sync::seed_field_version(db, b, "transactions", id, field, clock)?;
        }
        sync::mark(db, b, "transactions", id)?;
        crate::ledger::refresh_account(db, b, before["account_id"].as_str().unwrap())?;
    }
    Ok(())
}

/// Automatic posting executes the linked rule (even for a manual post-today)
/// and the ordinary rule chain. It shares guarded split/transfer writers.
pub(crate) fn post_schedule(
    db: &Connection,
    b: &str,
    input: &Value,
    clock: &str,
    epoch: Option<&str>,
    work: engine::Work,
) -> Result<(engine::Work, bool), Error> {
    let id = input["id"]
        .as_str()
        .ok_or_else(|| Error::invalid("Scheduled transaction identity required"))?;
    let mut runner = engine::Runner::with_work(load(db, b)?, work);
    let (mut after, matched) = runner.evaluate(db, b, input)?;
    if after["deleted"] == true {
        return Ok((runner.into_work(), false));
    }
    let schedule = input["schedule_id"]
        .as_str()
        .ok_or_else(|| Error::invalid("Schedule identity required"))?;
    let mut fields = serde_json::Map::new();
    for field in [
        "id",
        "account_id",
        "amount",
        "date",
        "notes",
        "cleared",
        "payee_id",
        "category_id",
        "new_payee",
    ] {
        if let Some(value) = after.get(field) {
            fields.insert(field.into(), value.clone());
        }
    }
    if fields.get("new_payee").is_some_and(Value::is_object) {
        fields.insert("payee_id".into(), Value::Null);
    }
    let transfer = if let Some(payee) = fields.get("payee_id").and_then(Value::as_str) {
        crate::payees_tags::record(db, b, "payees", payee)?["transfer_account_id"]
            .as_str()
            .map(str::to_owned)
    } else {
        None
    };
    if transfer.is_some() {
        fields.insert("payee_id".into(), Value::Null);
    }
    let final_input: crate::ledger::NewTransaction = serde_json::from_value(Value::Object(fields))
        .map_err(|_| Error::invalid("Schedule rules produced invalid transaction fields"))?;
    final_input.validate()?;
    let result = sync::execute(
        db,
        b,
        Some(clock),
        false,
        epoch,
        sync::Mutation::Create(Box::new(final_input)),
    )?;
    if result["response_status"].as_u64().unwrap_or(500) >= 400 {
        return Err(Error::conflict("The scheduled entry could not be saved"));
    }
    after.as_object_mut().unwrap().remove("new_payee");
    if after["is_parent"] == true {
        let before = crate::ledger::get(db, b, id)?;
        let (_, work) = plan::commit_with_work(
            db,
            b,
            &[id.to_owned()],
            vec![plan::Entry {
                before,
                after,
                matched_rule_ids: matched,
            }],
            clock,
            epoch,
            plan::Command {
                created: Some(id),
                work: runner.into_work(),
            },
        )?;
        // The rule plan persists schedule-only effects alongside the split.
        if crate::ledger::get(db, b, id)?["schedule_id"] != schedule {
            crate::schedules::link(db, b, id, schedule, clock)?;
        }
        return Ok((work, true));
    } else if let Some(destination) = transfer {
        let peer = super::actions::identity(&["actualBudget.schedule-pair.v1", b, id]);
        crate::transfers::composition::save_child(db, b, id, &destination, &peer, clock, None)?;
        crate::schedules::link(db, b, &peer, schedule, clock)?;
        peer_rules(db, b, &peer, clock, &mut runner)?;
    }
    crate::schedules::link(db, b, id, schedule, clock)?;
    Ok((runner.into_work(), true))
}
