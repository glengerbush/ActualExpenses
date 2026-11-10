//! Explicit mapping; provider IDs alone never choose a local account by name.
use super::*;
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    Link {
        snapshot: String,
        connection_id: String,
        generation: String,
        remote_account_id: String,
        opening_balance_mode: String,
        opening_amount: Option<i64>,
        opening_date: Option<String>,
    },
    Unlink {
        snapshot: String,
        revision: i64,
    },
    Settings {
        snapshot: String,
        revision: Option<i64>,
        changes: Value,
    },
}
fn guard_link(db: &Connection, b: &str, a: &str, clock: &str) -> Result<(), Error> {
    for field in [
        "link",
        "bank",
        "account_id",
        "account_sync_source",
        "tombstone",
    ] {
        crate::transaction_imports::guard(db, b, "bank_links", a, field, clock)?;
    }
    Ok(())
}
pub(super) fn link(db: &Connection, b: &str, a: &str) -> Result<Value, Error> {
    records::links(db, b)?
        .into_iter()
        .find(|r| r["id"] == a)
        .ok_or_else(Error::missing)
}
pub(super) async fn read(
    State(store): State<Store>,
    Path((b, a)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    models::validate_id(&b)?;
    models::validate_id(&a)?;
    Ok(Json(
        json!({"data":store.run(move|db|{let tx=db.transaction()?;Ok(json!({"link":link(&tx,&b,&a).ok(),"preferences":settings::read(&tx,&b,&a)?,"snapshot":sync::cursor(&tx,&b)?,"epoch":crate::lifecycle::epoch(&tx,&b)?}))}).await?}),
    ))
}
pub(super) async fn command(
    State(store): State<Store>,
    Path((b, a)): Path<(String, String)>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    models::validate_id(&b)?;
    models::validate_id(&a)?;
    let Json(value) = input.map_err(api::json_error)?;
    let key = api::idempotency_key(&headers)?;
    let proposed = sync::header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let owner = crate::history::owner_header(&headers)?;
    let payload = json!({"request":value,"epoch":epoch}).to_string();
    let lookup = sync::receipt_only_header(&headers)?;
    let cancel = sync::cancel_header(&headers)?;
    store.run(move|db|{let scope=format!("POST /budgets/{b}/accounts/{a}/banking/commands");
 if lookup{return sync::receipt_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref());}
 sync::write_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref(),|db|{
  if cancel{return sync::canceled(db,&b);}
  crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;
  let action:Action=serde_json::from_value(value).map_err(|_|Error::invalid("Choose a valid bank link, unlink or settings command"))?;
  if matches!(&action,Action::Unlink{..}){
   let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE budget_id=?1 AND id=?2)",params![b,a],|r|r.get(0))?;
   if !exists{return Err(Error::missing());}
  }else{crate::ledger::account_exists(db,&b,&a)?;}
  let snapshot=match &action{Action::Link{snapshot,..}|Action::Unlink{snapshot,..}|Action::Settings{snapshot,..}=>snapshot};sync::check_snapshot(db,&b,Some(snapshot))?;
  let clock=sync::clock(db,&b,proposed.as_deref())?;
  match action{
   Action::Link{connection_id,generation,remote_account_id,opening_balance_mode,opening_amount,opening_date,..}=>{
    for id in [&connection_id,&generation,&remote_account_id]{models::validate_id(id)?;}
    if !matches!(opening_balance_mode.as_str(),"auto"|"none"|"custom"){return Err(Error::invalid("Choose automatic, no opening entry, or a custom opening balance"));}
    if opening_balance_mode=="custom"{let amount=opening_amount.ok_or_else(||Error::invalid("A custom opening amount is required"))?;if amount.unsigned_abs()>models::MAX_MONEY as u64{return Err(Error::invalid("Opening amount exceeds the exact money range"));}models::validate_date(opening_date.as_deref().ok_or_else(||Error::invalid("A custom opening date is required"))?)?;}else if opening_amount.is_some()||opening_date.is_some(){return Err(Error::invalid("Only a custom opening choice accepts an amount and date"));}
    let connection=super::connection(db,&b,&connection_id)?;let remote=records::remote(db,&b,&remote_account_id)?;
    if connection["generation"]!=generation||connection["state"]!="connected"||connection["deleted"]==true||remote["connection_id"]!=connection_id||remote["generation"]!=generation||remote["deleted"]==true{return Err(Error::conflict("Discover this connection again before linking the selected account"));}
    let duplicate:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM bank_links WHERE budget_id=?1 AND remote_account_id=?2 AND id<>?3 AND tombstone=0)",params![b,remote_account_id,a],|r|r.get(0))?;if duplicate{return Err(Error::conflict("This provider account is already linked to another local account"));}
    let occupied:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM transactions WHERE budget_id=?1 AND account_id=?2 AND tombstone=0)",params![b,a],|r|r.get(0))?;
    if occupied&&opening_balance_mode=="custom"{return Err(Error::conflict("Custom opening balances require an empty local account"));}
    guard_link(db,&b,&a,&clock)?;
    // An Actual export has no provider connection selector. Only an explicit
    // relink on the same connection and same remote account ID can replace
    // that legacy namespace; preserve the original transaction ownership.
    let legacy:Option<String>=db.query_row("SELECT r.id FROM bank_links l JOIN bank_remote_accounts r ON r.budget_id=l.budget_id AND r.id=l.remote_account_id WHERE l.budget_id=?1 AND l.id=?2 AND l.connection_id=?3 AND r.remote_id=?4 AND substr(r.remote_connection_id,1,14)='actual-export:'",params![b,a,connection_id,remote["remote_id"].as_str()],|r|r.get(0)).optional()?;
    if let Some(old)=legacy {
     let ids=db.prepare("SELECT id FROM bank_transaction_origins WHERE budget_id=?1 AND remote_account_id=?2")?.query_map(params![b,old],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
     for id in ids {
      crate::transaction_imports::guard(db,&b,"bank_transaction_origins",&id,"remote_account_id",&clock)?;
      db.execute("UPDATE bank_transaction_origins SET remote_account_id=?3 WHERE budget_id=?1 AND id=?2",params![b,id,remote_account_id])?;
      sync::seed_field_version(db,&b,"bank_transaction_origins",&id,"remote_account_id",&clock)?;
      sync::mark(db,&b,"bank_transaction_origins",&id)?;
     }
    }
    db.execute("INSERT INTO bank_links(budget_id,id,connection_id,remote_account_id,connection_generation,initial_sync,opening_mode,opening_amount,opening_date) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(budget_id,id) DO UPDATE SET connection_id=excluded.connection_id,remote_account_id=excluded.remote_account_id,connection_generation=excluded.connection_generation,initial_sync=CASE WHEN bank_links.remote_account_id=excluded.remote_account_id THEN bank_links.initial_sync ELSE excluded.initial_sync END,last_sync_end=CASE WHEN bank_links.remote_account_id=excluded.remote_account_id THEN bank_links.last_sync_end ELSE NULL END,opening_mode=excluded.opening_mode,opening_amount=excluded.opening_amount,opening_date=excluded.opening_date,revision=bank_links.revision+1,tombstone=0",params![b,a,connection_id,remote_account_id,generation,!occupied&&opening_balance_mode!="none",opening_balance_mode,opening_amount,opening_date])?;
    sync::seed_field_version(db,&b,"bank_links",&a,"link",&clock)?;sync::mark(db,&b,"bank_links",&a)?;
   },
   Action::Unlink{revision,..}=>{
    let before=link(db,&b,&a)?;if before["revision"]!=revision||before["deleted"]==true{return Err(Error::conflict("Account link changed. Reload before unlinking"));}
    guard_link(db,&b,&a,&clock)?;
    db.execute("UPDATE bank_links SET tombstone=1,revision=revision+1 WHERE budget_id=?1 AND id=?2",params![b,a])?;sync::seed_field_version(db,&b,"bank_links",&a,"link",&clock)?;sync::mark(db,&b,"bank_links",&a)?;
   },
   Action::Settings{revision,changes,..}=>{
    let before=match link(db,&b,&a){Ok(v)=>Some(v),Err(e)if e.status==StatusCode::NOT_FOUND=>None,Err(e)=>return Err(e)};
    if before.as_ref().map(|v|v["revision"].as_i64().unwrap())!=revision{return Err(Error::conflict("Bank settings or account link changed. Reload before saving"));}
    if settings::update(db,&b,&a,&changes,&clock)?&&before.is_some(){db.execute("UPDATE bank_links SET revision=revision+1 WHERE budget_id=?1 AND id=?2",params![b,a])?;sync::mark(db,&b,"bank_links",&a)?;}
   }
  }
  Ok(json!({"response_status":200,"response_body":{"data":match link(db,&b,&a){Ok(v)=>v,Err(e)if e.status==StatusCode::NOT_FOUND=>Value::Null,Err(e)=>return Err(e)},"preferences":settings::read_retained(db,&b,&a)?,"edit_clock":clock,"cursor":sync::cursor(db,&b)?}}))
 })}).await
}
