//! Remote reads precede the owned SQLite mutation. Every financial graph and
//! its provider-origin binding commits with the original immutable receipt.
use super::*;
use chrono::{NaiveDate, Utc};
use serde::Deserialize;
use std::collections::BTreeSet;
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selected {
    account_id: String,
    revision: i64,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Refresh {
    action: String,
    generation: String,
    snapshot: String,
    start_date: String,
    end_date: String,
    accounts: Vec<Selected>,
}
fn validate(body: &Refresh) -> Result<(i64, i64), Error> {
    if body.action != "refresh" {
        return Err(Error::invalid("Choose refresh"));
    }
    models::validate_id(&body.generation)?;
    models::validate_date(&body.start_date)?;
    models::validate_date(&body.end_date)?;
    let start = NaiveDate::parse_from_str(&body.start_date, "%Y-%m-%d").map_err(Error::internal)?;
    let end = NaiveDate::parse_from_str(&body.end_date, "%Y-%m-%d").map_err(Error::internal)?;
    if !(1..=90).contains(&(end - start).num_days()) {
        return Err(Error::invalid(
            "Select an inclusive start and exclusive end within 90 days",
        ));
    }
    if body.accounts.is_empty() || body.accounts.len() > 100 {
        return Err(Error::invalid("Select 1–100 linked accounts per refresh"));
    }
    let mut seen = BTreeSet::new();
    for a in &body.accounts {
        models::validate_id(&a.account_id)?;
        if a.revision < 1 || !seen.insert(&a.account_id) {
            return Err(Error::invalid(
                "Select distinct accounts with their captured link revisions",
            ));
        }
    }
    Ok((
        start.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp(),
        end.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp(),
    ))
}
fn links(
    db: &Connection,
    b: &str,
    c: &str,
    body: &Refresh,
) -> Result<Vec<(Value, Value, settings::Settings)>, Error> {
    let mut result = vec![];
    for selected in &body.accounts {
        let account = crate::accounts::get(db, b, &selected.account_id)?;
        if account["deleted"] == true || account["closed"] == true {
            return Err(Error::conflict(
                "A selected account is closed or deleted. Review the captured refresh",
            ));
        }
        let link = linking::link(db, b, &selected.account_id)?;
        if link["deleted"] == true
            || link["connection_id"] != c
            || link["connection_generation"] != body.generation
            || link["revision"] != selected.revision
        {
            return Err(Error::conflict(
                "An account link or import preference changed. Keep this request and review a fresh refresh",
            ));
        }
        let remote = records::remote(db, b, link["remote_account_id"].as_str().unwrap())?;
        result.push((link, remote, settings::get(db, b, &selected.account_id)?));
    }
    Ok(result)
}
fn starting_payee(db: &Connection, b: &str) -> Result<Option<String>, Error> {
    let mut query=db.prepare("SELECT id,name FROM payees WHERE budget_id=?1 AND tombstone=0 AND transfer_account_id IS NULL ORDER BY rowid LIMIT 100001")?;
    for (index, row) in query
        .query_map([b], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .enumerate()
    {
        if index >= 100000 {
            return Err(Error::invalid(
                "Starting reference lookup exceeds 100000 payees. Choose no automatic opening entry",
            ));
        }
        let (id, name) = row?;
        if name.to_lowercase() == "starting balance" {
            return Ok(Some(id));
        }
    }
    Ok(None)
}
fn opening(
    db: &Connection,
    b: &str,
    a: &str,
    amount: i64,
    date: &str,
    clock: &str,
    epoch: Option<&str>,
) -> Result<String, Error> {
    if amount.unsigned_abs() > models::MAX_MONEY as u64 {
        return Err(Error::invalid(
            "Opening balance exceeds the exact money range",
        ));
    }
    models::validate_date(date)?;
    let account = crate::accounts::get(db, b, a)?;
    let existing = starting_payee(db, b)?;
    let payee = existing
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let category: Option<String> = if account["off_budget"] == true {
        None
    } else {
        db.query_row("SELECT id FROM categories WHERE budget_id=?1 AND is_income=1 AND tombstone=0 ORDER BY LOWER(name)='starting balances' DESC,rowid LIMIT 1",[b],|r|r.get(0)).optional()?
    };
    let id = uuid::Uuid::new_v4().to_string();
    let input:crate::ledger::NewTransaction=serde_json::from_value(json!({"id":id,"account_id":a,"amount":amount,"date":date,"cleared":true,"payee_id":if existing.is_some(){Some(payee.clone())}else{None},"new_payee":if existing.is_none(){Some(json!({"id":payee,"name":"Starting Balance"}))}else{None},"category_id":category})).map_err(Error::internal)?;
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
            "The starting balance needs review. No provider transactions were saved",
        ));
    }
    db.execute(
        "UPDATE transactions SET starting_balance=1 WHERE budget_id=?1 AND id=?2",
        params![b, id],
    )?;
    sync::seed_field_version(db, b, "transactions", &id, "starting_balance", clock)?;
    sync::mark(db, b, "transactions", &id)?;
    Ok(id)
}
fn origin(
    db: &Connection,
    b: &str,
    id: &str,
    connection: &str,
    remote: &str,
    clock: &str,
) -> Result<(), Error> {
    let before=db.query_row("SELECT connection_id,remote_account_id,tombstone FROM bank_transaction_origins WHERE budget_id=?1 AND id=?2",params![b,id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,bool>(2)?))).optional()?;
    if before
        .as_ref()
        .is_some_and(|(c, r, deleted)| c == connection && r == remote && !*deleted)
    {
        return Ok(());
    }
    for field in ["connection_id", "remote_account_id", "tombstone"] {
        crate::transaction_imports::guard(db, b, "bank_transaction_origins", id, field, clock)?;
    }
    db.execute("INSERT INTO bank_transaction_origins(budget_id,id,connection_id,remote_account_id) VALUES(?1,?2,?3,?4) ON CONFLICT(budget_id,id) DO UPDATE SET connection_id=excluded.connection_id,remote_account_id=excluded.remote_account_id,tombstone=0",params![b,id,connection,remote])?;
    for field in ["connection_id", "remote_account_id", "tombstone"] {
        sync::seed_field_version(db, b, "bank_transaction_origins", id, field, clock)?;
    }
    sync::mark(db, b, "bank_transaction_origins", id)
}
pub(super) async fn refresh(
    store: Store,
    b: String,
    id: String,
    headers: HeaderMap,
    value: Value,
) -> Result<(StatusCode, Json<Value>), Error> {
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let request = remote::Request {
        budget: b.clone(),
        connection: id.clone(),
        key: api::idempotency_key(&headers)?,
        payload: json!({"request":value,"epoch":epoch}).to_string(),
        epoch,
        generation: value["generation"].as_str().unwrap_or("").to_owned(),
        proposed: sync::header(&headers)?,
        owner: crate::history::owner_header(&headers)?,
    };
    if sync::receipt_only_header(&headers)? {
        return request.receipt(store).await;
    }
    if sync::cancel_header(&headers)? {
        return request.cancel(store).await;
    }
    // Known outcomes always precede current eligibility/schema checks.
    let probe = request.clone();
    let known = store
        .run(move |db| {
            Ok(
                crate::store::receipt(db, &probe.scope(), &probe.key, &probe.fingerprint())?
                    .is_some(),
            )
        })
        .await?;
    if known {
        return request.receipt(store).await;
    }
    let body: Refresh = serde_json::from_value(value).map_err(|_| {
        Error::invalid("Choose a valid refresh range, snapshot and linked account revisions")
    })?;
    let (start, end) = validate(&body)?;
    let (bb, ii, pre, req) = (b.clone(), id.clone(), body.clone(), request.clone());
    let remote_ids = store
        .run(move |db| {
            let tx = db.transaction()?;
            crate::lifecycle::require_epoch(&tx, &bb, req.epoch.as_deref())?;
            sync::check_snapshot(&tx, &bb, Some(&pre.snapshot))?;
            Ok(links(&tx, &bb, &ii, &pre)?
                .into_iter()
                .map(|(_, r, _)| r["remote_id"].as_str().unwrap().to_owned())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect())
        })
        .await?;
    let Some(download) = remote::capture(
        store.clone(),
        request.clone(),
        transport::Download::Transactions {
            accounts: remote_ids,
            start,
            end,
        },
    )
    .await?
    else {
        return request.receipt(store).await;
    };
    store.run(move|db|sync::write_owned(db,&request.scope(),&request.key,&request.payload,request.proposed.as_deref(),StatusCode::OK,request.owner.as_ref(),|db|{
  crate::lifecycle::require_epoch(db,&b,request.epoch.as_deref())?;
  let current=connection(db,&b,&id)?;if current["generation"]!=body.generation||current["deleted"]==true{return Err(Error::conflict("Provider credentials changed while refreshing"));}
  sync::check_snapshot(db,&b,Some(&body.snapshot))?;
  let selected=links(db,&b,&id,&body)?;
  let raw=match download{Ok(v)=>v,Err(e)=>{
   db.execute("UPDATE bank_connections SET error_code=?3,message=?4,state=CASE WHEN ?3='provider_revoked' THEN 'credentials_missing' ELSE state END,revision=revision+1 WHERE budget_id=?1 AND id=?2",params![b,id,e.code,e.message])?;sync::mark(db,&b,"bank_connections",&id)?;
   db.execute("DELETE FROM bank_jobs WHERE budget_id=?1 AND connection_id=?2 AND id=?3",params![b,id,request.key])?;
   return Ok(json!({"response_status":200,"response_body":{"data":{"status":"failed","accounts":[],"errors":[{"code":e.code,"message":e.message}]},"cursor":sync::cursor(db,&b)?}}));
  }};
  let(remotes,errors)=records::install_remote(db,&b,&id,&body.generation,&raw,false)?;
  let clock=sync::clock(db,&b,request.proposed.as_deref())?;
  let mut results=vec![];let mut work=crate::rules::Work::default();let mut total=0usize;
  for(link,prior,prefs)in selected{
   let account=link["id"].as_str().unwrap();let remote_id=link["remote_account_id"].as_str().unwrap();
   let Some(remote)=remotes.iter().find(|r|r["id"]==remote_id)else{db.execute("UPDATE bank_remote_accounts SET tombstone=1 WHERE budget_id=?1 AND id=?2",params![b,remote_id])?;sync::mark(db,&b,"bank_remote_accounts",remote_id)?;results.push(json!({"account_id":account,"status":"failed","code":"provider_account_missing","message":"This provider account was not returned. Retry or review its link"}));continue;};
   if remote["diagnostics"].as_array().is_some_and(|e|!e.is_empty()){results.push(json!({"account_id":account,"status":"failed","diagnostics":remote["diagnostics"]}));continue;}
   if !prefs.import_transactions{results.push(json!({"account_id":account,"status":"balance_only","added":[],"updated":[],"ignored":[]}));continue;}
   let source=raw["accounts"].as_array().unwrap().iter().find(|a|a["id"]==prior["remote_id"]&&a["conn_id"].as_str().or_else(||a["org"]["id"].as_str()).unwrap_or("legacy")==prior["remote_connection_id"].as_str().unwrap()).ok_or_else(||Error::invalid("Provider account identity changed during normalization"))?;
   let core=normalize::core(source,&body.start_date,&body.end_date,&prefs)?;total=total.checked_add(core.len()).filter(|n|*n<=10000).ok_or_else(||Error::invalid("Select fewer accounts or a narrower range: at most10000 transactions per refresh"))?;
   let mut opening_id=None;
   let occupied:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM transactions WHERE budget_id=?1 AND account_id=?2 AND tombstone=0)",params![b,account],|r|r.get(0))?;
   if link["initial_sync"]==true&&!occupied&&link["opening_mode"]!="none"{
    let amount=if link["opening_mode"]=="custom"{link["opening_amount"].as_i64().ok_or_else(||Error::invalid("Custom opening balance is missing"))?}else{
     let mut amount=normalize::minor(source["balance"].as_str().ok_or_else(||Error::invalid("Missing provider balance"))?)? as i128;
     for t in &core{amount-=normalize::minor(t["transactionAmount"]["amount"].as_str().unwrap())? as i128;}
     if amount.abs()>models::MAX_MONEY as i128{return Err(Error::invalid("Calculated opening balance exceeds the exact money range"));}amount as i64
    };
    let date=link["opening_date"].as_str().map(str::to_owned).or_else(||core.last().and_then(|t|t["date"].as_str()).map(str::to_owned)).unwrap_or_else(||chrono::DateTime::<Utc>::from(std::time::SystemTime::now()).format("%Y-%m-%d").to_string());
    opening_id=Some(opening(db,&b,account,amount,&date,&clock,request.epoch.as_deref())?);
   }
   let rows=normalize::transactions(&core,&prefs)?;let snapshot=sync::cursor(db,&b)?;
   let(value,remaining)=crate::transaction_imports::apply_bank(db,&b,crate::transaction_imports::BankInput{account,remote:remote_id,identity:&format!("{id}/{}/{account}",request.key),snapshot:&snapshot,rows,proposed:Some(&clock),epoch:request.epoch.as_deref(),update_dates:prefs.update_dates,work})?;work=remaining;
   let mut data=value["response_body"]["data"].clone();
   for field in ["added","updated"]{for transaction in data[field].as_array().unwrap(){origin(db,&b,transaction.as_str().unwrap(),&id,remote_id,&clock)?;}}
   if let Some(id)=opening_id{data["added"].as_array_mut().unwrap().insert(0,json!(id));}
   data["account_id"]=json!(account);data["status"]=json!("complete");results.push(data);
   db.execute("UPDATE bank_links SET initial_sync=0,last_sync_end=CASE WHEN last_sync_end IS NULL OR last_sync_end<?3 THEN ?3 ELSE last_sync_end END WHERE budget_id=?1 AND id=?2",params![b,account,body.end_date])?;sync::mark(db,&b,"bank_links",account)?;
  }
  let failed=results.iter().filter(|a|a["status"]=="failed").count();
  let status=if failed==results.len(){"failed"}else if failed>0{"partial"}else{"complete"};
  db.execute("UPDATE bank_connections SET error_code=?3,message=?4,last_success=CASE WHEN ?3 IS NULL THEN strftime('%Y-%m-%dT%H:%M:%fZ','now') ELSE last_success END,revision=revision+1 WHERE budget_id=?1 AND id=?2",params![b,id,errors.first().and_then(|e|e["code"].as_str()),errors.first().and_then(|e|e["message"].as_str())])?;sync::mark(db,&b,"bank_connections",&id)?;
  db.execute("DELETE FROM bank_jobs WHERE budget_id=?1 AND connection_id=?2 AND id=?3",params![b,id,request.key])?;
  Ok(json!({"response_status":200,"response_body":{"data":{"status":status,"accounts":results,"errors":errors},"edit_clock":clock,"cursor":sync::cursor(db,&b)?}}))
 })).await
}
