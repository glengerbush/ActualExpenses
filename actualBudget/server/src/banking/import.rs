//! Actual exports contain account mapping/cache, never provider credentials.
//! Unknown provider currency and successful request windows stay unknown.
use super::*;
fn id(namespace: &str, value: &str) -> String {
    records::identity(namespace, "actual-export", value)
}
fn money(value: &Value) -> Result<Option<String>, Error> {
    if value.is_null() {
        return Ok(None);
    }
    let value = value
        .as_i64()
        .filter(|n| n.unsigned_abs() <= models::MAX_MONEY as u64)
        .ok_or_else(|| Error::invalid("Invalid retained bank balance"))?;
    let abs = value.unsigned_abs();
    Ok(Some(format!(
        "{}{}.{:02}",
        if value < 0 { "-" } else { "" },
        abs / 100,
        abs % 100
    )))
}
pub(crate) fn convert(source: &Value) -> Result<Value, Error> {
    let mut accounts = vec![];
    let mut origins = vec![];
    let linked = source["accounts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| a["account_sync_source"] == "simpleFin")
        .collect::<Vec<_>>();
    if linked.len() > 5000 {
        return Err(Error::invalid("Retained bank mappings exceed5000 accounts"));
    }
    let institutions = source["banks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["id"].as_str().map(|id| (id, r)))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut ownership = std::collections::BTreeMap::<String, Vec<Value>>::new();
    for t in source["transactions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| t["financial_id"].is_string())
    {
        // Original normalized account and matching financial ID are evidence;
        // a manual/file ID alone cannot acquire provider ownership.
        let raw = t["raw_synced_data"]
            .as_str()
            .and_then(|s| serde_json::from_str::<Value>(s).ok());
        if let Some(raw) = raw.filter(|r| r["transactionId"] == t["financial_id"])
            && let Some(account) = raw["account"].as_str()
        {
            ownership.entry(account.to_owned()).or_default().push(json!({"id":t["id"],"account":account,"deleted":t["tombstone"]==1||t["tombstone"]==true}));
        }
    }
    for a in linked {
        let local = a["id"]
            .as_str()
            .ok_or_else(|| Error::invalid("Missing bank account identity"))?;
        let remote = a["account_id"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 512)
            .ok_or_else(|| Error::invalid("Missing retained bank account mapping"))?;
        let bank = a["bank"]
            .as_str()
            .ok_or_else(|| Error::invalid("Missing retained bank institution"))?;
        let institution = institutions
            .get(bank)
            .ok_or_else(|| Error::invalid("Retained bank institution is missing"))?;
        let name = a["official_name"]
            .as_str()
            .unwrap_or_else(|| a["name"].as_str().unwrap_or(""));
        models::validate_name(name)?;
        let institution_name = institution["name"].as_str().unwrap_or("");
        models::validate_name(institution_name)?;
        let remote_connection = format!("actual-export:{bank}");
        accounts.push(json!({"id":local,"remote_id":remote,"remote_connection_id":remote_connection,"institution":institution_name,"name":name,"balance":money(&a["balance_current"])? ,"available_balance":money(&a["balance_available"])? ,"deleted":a["tombstone"]==1||a["tombstone"]==true}));
        origins.extend(ownership.remove(local).unwrap_or_default());
    }
    Ok(json!({"accounts":accounts,"origins":origins}))
}
pub(crate) fn install(db: &Connection, b: &str, data: &Value) -> Result<(), Error> {
    let mut rows = vec![];
    for row in data["accounts"].as_array().into_iter().flatten() {
        // Native mappings, including deliberate unlink tombstones, supersede
        // retained source bytes. A clone retains IDs from its original budget.
        let missing:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE budget_id=?1 AND id=?2) AND NOT EXISTS(SELECT 1 FROM bank_links WHERE budget_id=?1 AND id=?2)",params![b,row["id"].as_str()],|r|r.get(0))?;
        if missing {
            rows.push(row.clone());
        }
    }
    if rows.is_empty() {
        return Ok(());
    }
    let connection = id(b, "simplefin");
    let generation = id(b, "import-generation");
    db.execute("INSERT OR IGNORE INTO bank_connections(budget_id,id,provider,name,state,generation,message) VALUES(?1,?2,'simplefin','SimpleFIN','credentials_missing',?3,'Reconnect and review imported mappings before refreshing. Provider currency and balance date were not retained by Actual exports.')",params![b,connection,generation])?;
    // If already present, preserve current generation, user edits and tombstones.
    let generation: String = db.query_row(
        "SELECT generation FROM bank_connections WHERE budget_id=?1 AND id=?2",
        params![b, connection],
        |r| r.get(0),
    )?;
    for row in &rows {
        let remote = records::identity(
            &connection,
            row["remote_connection_id"].as_str().unwrap(),
            row["remote_id"].as_str().unwrap(),
        );
        db.execute("INSERT OR IGNORE INTO bank_remote_accounts(budget_id,id,connection_id,generation,remote_connection_id,remote_id,institution,name,currency,balance,available_balance,balance_date,diagnostics,tombstone) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'',?9,?10,0,?11,?12)",params![b,remote,connection,generation,row["remote_connection_id"].as_str(),row["remote_id"].as_str(),row["institution"].as_str(),row["name"].as_str(),row["balance"].as_str().unwrap_or(""),row["available_balance"].as_str(),json!([{"code":"imported_metadata","message":"Reconnect and explicitly review this legacy provider mapping. Currency, provider balance date and last successful request window are unknown."}]).to_string(),row["deleted"]==true])?;
        db.execute("INSERT OR IGNORE INTO bank_links(budget_id,id,connection_id,remote_account_id,connection_generation,initial_sync,opening_mode,tombstone) VALUES(?1,?2,?3,?4,?5,0,'none',?6)",params![b,row["id"].as_str(),connection,remote,generation,row["deleted"]==true])?;
    }
    let installed = rows
        .iter()
        .filter_map(|r| r["id"].as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for origin in data["origins"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["account"].as_str().is_some_and(|a| installed.contains(a)))
    {
        let remote:Option<String> = db.query_row("SELECT remote_account_id FROM bank_links WHERE budget_id=?1 AND id=?2 AND connection_id=?3",params![b,origin["account"].as_str(),connection],|r|r.get(0)).optional()?;
        if let Some(remote) = remote {
            db.execute("INSERT OR IGNORE INTO bank_transaction_origins(budget_id,id,connection_id,remote_account_id,tombstone) VALUES(?1,?2,?3,?4,?5)",params![b,origin["id"].as_str(),connection,remote,origin["deleted"]==true])?;
        }
    }
    Ok(())
}
pub(crate) fn import_clock(db: &Connection, b: &str, clock: &Value) -> Result<(), Error> {
    if clock["dataset"] != "accounts"
        || !matches!(
            clock["column"].as_str(),
            Some("bank" | "account_id" | "account_sync_source" | "tombstone")
        )
    {
        return Ok(());
    }
    let Some(a) = clock["row"].as_str() else {
        return Ok(());
    };
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM bank_links WHERE budget_id=?1 AND id=?2) AND NOT EXISTS(SELECT 1 FROM field_versions WHERE budget_id=?1 AND entity='bank_links' AND entity_id=?2 AND field=?3)",
        params![b, a, clock["column"].as_str()],
        |r| r.get(0),
    )?;
    if exists {
        let timestamp = clock["timestamp"]
            .as_str()
            .ok_or_else(|| Error::invalid("Invalid bank mapping clock"))?;
        // Each original selector remains independently inspectable; the compound
        // native mapping guard additionally protects against every selector.
        sync::seed_field_version(
            db,
            b,
            "bank_links",
            a,
            clock["column"].as_str().unwrap(),
            timestamp,
        )?;
    }
    Ok(())
}
pub(crate) fn enrich(db: &Connection, only: Option<&str>) -> Result<(), Error> {
    let mut q=db.prepare("SELECT s.budget_id,s.original FROM staged_imports s JOIN imported_sources i ON i.import_id=s.id AND COALESCE(json_extract(s.report,'$.source_format'),'actual') NOT IN('ynab4','nynab') WHERE s.status='activated' AND (?1 IS NULL OR s.budget_id=?1) UNION SELECT budget_id,original FROM budget_provenance WHERE kind='actual-import' AND original IS NOT NULL AND (?1 IS NULL OR budget_id=?1)")?;
    let originals = q
        .query_map([only], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (b, bytes) in originals {
        let Ok((_, data)) = crate::imports::conversion::inspect(&bytes) else {
            continue;
        };
        install(db, &b, &data["banking"])?;
        for clock in data["clocks"].as_array().into_iter().flatten() {
            import_clock(db, &b, clock)?;
        }
    }
    // This existing bounded, Actual-only installer also refreshes unactivated
    // stages and fills newly supported bank preference cells without rewriting
    // existing user values or original receipts.
    crate::transaction_imports::enrich(db, only)
}
