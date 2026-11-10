//! Two existing purchases merge as one guarded, reciprocal financial graph.
use super::*;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    snapshot: String,
    ids: Vec<String>,
    #[serde(default)]
    protection_token: Option<String>,
}
struct Plan {
    keeper: String,
    before: BTreeMap<String, Value>,
    after: BTreeMap<String, Value>,
    members: Vec<Value>,
}
fn keep(a: &Value, b: &Value) -> bool {
    for field in ["imported_id", "imported_payee"] {
        let exists = |row: &Value| row[field].as_str().is_some_and(|s| !s.is_empty());
        if exists(a) != exists(b) {
            return exists(a);
        }
    }
    a["date"].as_str().unwrap() < b["date"].as_str().unwrap()
}
fn truth(value: &Value) -> bool {
    !value.is_null() && value != "" && value != false && value != 0
}
fn choose(a: &str, b: &str, rows: &BTreeMap<String, Value>) -> (String, String) {
    if keep(&rows[a], &rows[b]) {
        (a.into(), b.into())
    } else {
        (b.into(), a.into())
    }
}
fn validate_pair(a: &Value, b: &Value) -> Result<(), Error> {
    if a["account_id"] != b["account_id"] {
        return Err(Error::invalid(
            "Cannot merge transactions from different accounts",
        ));
    }
    if a["amount"] != b["amount"] {
        return Err(Error::invalid(
            "Cannot merge transactions with different amounts",
        ));
    }
    if a["transfer_id"].is_string()
        && b["transfer_id"].is_string()
        && a["payee_id"] != b["payee_id"]
    {
        return Err(Error::invalid(
            "Cannot merge transfers to different accounts",
        ));
    }
    Ok(())
}
fn merge_rows(a: &str, b: &str, rows: &mut BTreeMap<String, Value>) -> Result<String, Error> {
    validate_pair(&rows[a], &rows[b])?;
    let (keep, drop) = choose(a, b, rows);
    let other = rows[&drop].clone();
    let kept = rows[&keep].clone();
    let keep_children = rows
        .values()
        .filter(|row| row["deleted"] != true && row["parent_id"] == keep)
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let drop_children = rows
        .values()
        .filter(|row| row["deleted"] != true && row["parent_id"] == drop)
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let moving = keep_children.is_empty() && !drop_children.is_empty();
    {
        let row = rows.get_mut(&keep).unwrap();
        for field in ["payee_id", "category_id", "notes", "schedule_id"] {
            if !truth(&row[field]) {
                row[field] = other[field].clone();
            }
        }
        row["retained_payee_id"] = row["payee_id"].clone();
        for field in ["cleared", "reconciled"] {
            row[field] = json!(row[field] == true || other[field] == true);
        }
        if moving {
            row["is_parent"] = json!(true);
            row["category_id"] = Value::Null;
        }
    }
    if moving {
        for id in drop_children {
            rows.get_mut(&id).unwrap()["parent_id"] = json!(keep);
        }
    } else {
        for id in drop_children {
            let peer = rows[&id]["transfer_id"].as_str().map(str::to_owned);
            rows.get_mut(&id).unwrap()["deleted"] = json!(true);
            if let Some(peer) = peer {
                rows.get_mut(&peer)
                    .ok_or_else(|| Error::invalid("The complete linked split needs review"))?["deleted"] =
                    json!(true);
            }
        }
    }
    rows.get_mut(&drop).unwrap()["deleted"] = json!(true);
    // The source retains the keeper's date, bank identifiers and private rank.
    debug_assert_eq!(rows[&keep]["date"], kept["date"]);
    Ok(keep)
}
fn valid_graph(rows: &BTreeMap<String, Value>) -> Result<(), Error> {
    let mut sums = BTreeMap::<String, (usize, i128)>::new();
    for (id, row) in rows.iter().filter(|(_, r)| r["deleted"] != true) {
        if let Some(parent) = row["parent_id"].as_str() {
            let parent = rows
                .get(parent)
                .filter(|p| p["deleted"] != true && p["is_parent"] == true)
                .ok_or_else(|| {
                    Error::conflict("Review this split before merging; a live parent must remain")
                })?;
            if [
                "account_id",
                "date",
                "cleared",
                "reconciled",
                "starting_balance",
            ]
            .iter()
            .any(|field| row[*field] != parent[*field])
            {
                return Err(Error::conflict(
                    "Review the split's dates and status before merging; the original graph is retained",
                ));
            }
            let entry = sums
                .entry(parent["id"].as_str().unwrap().into())
                .or_default();
            entry.0 += 1;
            entry.1 += row["amount"].as_i64().unwrap() as i128;
        }
        if let Some(peer) = row["transfer_id"].as_str() {
            let peer = rows
                .get(peer)
                .filter(|p| p["deleted"] != true)
                .ok_or_else(|| {
                    Error::conflict("Merging must preserve every reciprocal transfer leg")
                })?;
            if peer["transfer_id"] != *id
                || row["account_id"] == peer["account_id"]
                || row["amount"].as_i64().unwrap() as i128
                    + peer["amount"].as_i64().unwrap() as i128
                    != 0
            {
                return Err(Error::conflict(
                    "Review the reciprocal transfer before merging",
                ));
            }
        }
    }
    for (id, row) in rows
        .iter()
        .filter(|(_, r)| r["deleted"] != true && r["is_parent"] == true)
    {
        if sums.get(id).is_none_or(|(count, sum)| {
            *count == 0 || *sum != row["amount"].as_i64().unwrap() as i128
        }) {
            return Err(Error::conflict(
                "Merging cannot change an existing split total; review its allocations first",
            ));
        }
    }
    Ok(())
}
fn plan(db: &Connection, b: &str, body: &Request) -> Result<Plan, Error> {
    sync::check_snapshot(db, b, Some(&body.snapshot))?;
    if body.ids.len() != 2 || body.ids[0] == body.ids[1] {
        return Err(Error::invalid("Select exactly two distinct transactions"));
    }
    for id in &body.ids {
        crate::models::validate_id(id)?;
        crate::ledger::get(db, b, id)?;
    }
    let members = crate::transfers::protection::members(
        db,
        b,
        &body.ids.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    if members.len() > 400000 {
        return Err(Error::invalid(
            "The connected merge graph exceeds the bounded work limit. Review smaller aggregates first",
        ));
    }
    let ids = members
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let mut before = BTreeMap::new();
    // Each root is hydrated once; children are then flattened into this ID map.
    let roots = members
        .iter()
        .filter(|r| r["parent_id"].is_null())
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    for row in crate::ledger::get_many(db, b, &roots)? {
        let mut row = row;
        for mut child in row
            .as_object_mut()
            .unwrap()
            .remove("children")
            .and_then(|c| c.as_array().cloned())
            .unwrap_or_default()
        {
            child["deleted"] = json!(false);
            before.insert(child["id"].as_str().unwrap().to_owned(), child);
        }
        row["deleted"] = json!(false);
        before.insert(row["id"].as_str().unwrap().to_owned(), row);
    }
    if ids.iter().any(|id| !before.contains_key(id)) {
        return Err(Error::conflict(
            "The complete connected merge graph is unavailable",
        ));
    }
    let mut after = before.clone();
    let a = &body.ids[0];
    let c = &body.ids[1];
    validate_pair(&after[a], &after[c])?;
    let peer_a = after[a]["transfer_id"].as_str().map(str::to_owned);
    let peer_b = after[c]["transfer_id"].as_str().map(str::to_owned);
    let transfer_payee = if peer_a.is_some() {
        after[a]["payee_id"].clone()
    } else {
        after[c]["payee_id"].clone()
    };
    let peer = match (&peer_a, &peer_b) {
        (Some(a), Some(c)) => Some(merge_rows(a, c, &mut after)?),
        (Some(id), None) | (None, Some(id)) => Some(id.clone()),
        _ => None,
    };
    let keeper = merge_rows(a, c, &mut after)?;
    if let Some(peer) = peer {
        after.get_mut(&keeper).unwrap()["transfer_id"] = json!(peer);
        after.get_mut(&keeper).unwrap()["payee_id"] = transfer_payee.clone();
        after.get_mut(&keeper).unwrap()["retained_payee_id"] = transfer_payee;
        after.get_mut(&peer).unwrap()["transfer_id"] = json!(keeper);
        if peer_a.is_some() != peer_b.is_some() {
            let account = after[&peer]["account_id"].as_str().unwrap();
            let off: bool = db.query_row(
                "SELECT off_budget FROM accounts WHERE budget_id=?1 AND id=?2",
                params![b, account],
                |r| r.get(0),
            )?;
            if !off {
                after.get_mut(&keeper).unwrap()["category_id"] = Value::Null;
            }
        }
    }
    valid_graph(&after)?;
    Ok(Plan {
        keeper,
        before,
        after,
        members,
    })
}
fn preview_data(db: &Connection, b: &str, plan: &Plan) -> Result<Value, Error> {
    Ok(
        json!({"keeper_id":plan.keeper,"before":plan.before.values().collect::<Vec<_>>(),"after":plan.after.values().filter(|r|r["deleted"]!=true).collect::<Vec<_>>(),"deleted_ids":plan.after.iter().filter(|(_,r)|r["deleted"]==true).map(|(id,_)|id).collect::<Vec<_>>(),"affected_ids":plan.before.keys().collect::<Vec<_>>(),"protected_count":plan.members.iter().filter(|r|r["reconciled"]==true).count(),"protection_token":crate::transfers::protection::token(b,&plan.members),"snapshot":sync::cursor(db,b)?,"epoch":crate::lifecycle::epoch(db,b)?}),
    )
}
pub(super) async fn preview(
    State(store): State<Store>,
    Path(b): Path<String>,
    input: Result<Json<Request>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    let Json(body) = input.map_err(crate::api::json_error)?;
    store
        .run(move |db| {
            let tx = db.transaction()?;
            let plan = plan(&tx, &b, &body)?;
            Ok(json!({"data":preview_data(&tx,&b,&plan)?}))
        })
        .await
        .map(Json)
}
pub(super) async fn commit(
    State(store): State<Store>,
    Path(b): Path<String>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    crate::models::validate_id(&b)?;
    let Json(body) = input.map_err(crate::api::json_error)?;
    let payload = body.to_string();
    let key = crate::api::idempotency_key(&headers)?;
    let proposed = sync::header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let owner = crate::history::owner_header(&headers)?;
    let lookup = sync::receipt_only_header(&headers)?;
    let cancel = sync::cancel_header(&headers)?;
    store.run(move|db|{let scope=format!("POST /budgets/{b}/transactions/merge");if lookup{return sync::receipt_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref());}sync::write_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref(),|db|{
  if cancel{return sync::canceled(db,&b);}crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;let body:Request=serde_json::from_value(body).map_err(|e|Error::invalid(format!("Invalid merge command: {e}")))?;let plan=plan(db,&b,&body)?;let clock=sync::clock(db,&b,proposed.as_deref())?;
  let authorization=crate::transfers::protection::authorize(db,&b,&body.ids.iter().map(String::as_str).collect::<Vec<_>>(),body.protection_token.as_deref(),&clock)?;for row in &plan.members{crate::transfers::protection::protect(row,authorization.as_ref())?;}
  let fields=["payee_id","category_id","notes","cleared","reconciled","schedule_id","is_parent","parent_id","transfer_id","tombstone"];
  for (id,after)in &plan.after{let before=&plan.before[id];if after["deleted"]==true{let newer:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM field_versions WHERE budget_id=?1 AND entity='transactions' AND entity_id=?2 AND clock>=?3)",params![b,id,clock],|r|r.get(0))?;if newer{return Err(Error::conflict("A newer entry must be reviewed before removing the duplicate"));}}
   for field in fields{let key=if field=="tombstone"{"deleted"}else{field};if before[key]!=after[key]{guard(db,&b,"transactions",id,field,&clock)?;}}
  }
  for (id,after)in &plan.after{let before=&plan.before[id];let changed=fields.iter().filter(|field|{let key=if **field=="tombstone"{"deleted"}else{field};before[key]!=after[key]}).copied().collect::<Vec<_>>();if changed.is_empty(){continue;}
   db.prepare_cached("UPDATE transactions SET payee_id=?3,category_id=?4,notes=?5,cleared=?6,reconciled=?7,is_parent=?8,parent_id=?9,transfer_id=?10,tombstone=?11 WHERE budget_id=?1 AND id=?2")?.execute(params![b,id,after["payee_id"].as_str(),after["category_id"].as_str(),after["notes"].as_str().unwrap_or(""),after["cleared"]==true,after["reconciled"]==true,after["is_parent"]==true,after["parent_id"].as_str(),after["transfer_id"].as_str(),after["deleted"]==true])?;
   if before["schedule_id"]!=after["schedule_id"]{if let Some(schedule)=after["schedule_id"].as_str(){crate::schedules::link(db,&b,id,schedule,&clock)?;}else{db.execute("UPDATE schedule_links SET tombstone=1 WHERE budget_id=?1 AND id=?2",params![b,id])?;sync::mark(db,&b,"schedule_links",id)?;}}
   for field in changed{sync::seed_field_version(db,&b,"transactions",id,field,&clock)?;}sync::mark(db,&b,"transactions",id)?;
  }
  crate::splits::validate(db,&b)?;crate::transfers::validate(db,&b)?;for account in plan.before.values().filter_map(|r|r["account_id"].as_str()).collect::<BTreeSet<_>>(){crate::ledger::refresh_account(db,&b,account)?;}crate::ledger::validate_budget_total(db,&b)?;
  Ok(json!({"response_status":200,"response_body":{"data":{"keeper_id":plan.keeper,"affected_count":plan.before.len(),"transaction_ids":plan.before.keys().collect::<Vec<_>>(),"deleted_ids":plan.after.iter().filter(|(_,r)|r["deleted"]==true).map(|(id,_)|id).collect::<Vec<_>>()},"edit_clock":clock,"cursor":sync::cursor(db,&b)?}}))
 })}).await
}
