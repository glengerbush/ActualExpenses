//! Explicit public projection. Never publish arbitrary provider diagnostics,
//! URLs, credentials or unknown fields from captured remote JSON.
use super::*;
fn label(value: &Value, key: &str, max: usize) -> Result<String, Error> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control))
        .map(str::to_owned)
        .ok_or_else(|| {
            transport::failure(
                "provider_invalid_response",
                "Provider account fields are missing or invalid. No data was applied",
            )
        })
}
fn decimal(value: &Value, key: &str, required: bool) -> Result<Option<String>, Error> {
    if !required && value.get(key).is_none() {
        return Ok(None);
    }
    let s = label(value, key, 64)?;
    let rest = s.strip_prefix('-').unwrap_or(&s);
    let pieces = rest.split('.').collect::<Vec<_>>();
    if pieces.len() > 2
        || pieces
            .iter()
            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
        || !s.parse::<f64>().is_ok_and(f64::is_finite)
    {
        return Err(transport::failure(
            "provider_invalid_response",
            "Provider balances must be finite numeric strings",
        ));
    }
    Ok(Some(s))
}
fn identity(connection: &str, remote_connection: &str, remote: &str) -> String {
    let mut h = Sha256::new();
    for s in ["simplefin-remote-v1", connection, remote_connection, remote] {
        h.update((s.len() as u64).to_be_bytes());
        h.update(s.as_bytes());
    }
    let mut bytes: [u8; 16] = h.finalize()[..16].try_into().unwrap();
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    uuid::Uuid::from_bytes(bytes).to_string()
}
pub(super) fn remote(db: &Connection, b: &str, id: &str) -> Result<Value, Error> {
    db.query_row("SELECT id,connection_id,generation,remote_connection_id,remote_id,institution,name,currency,balance,available_balance,balance_date,diagnostics,tombstone FROM bank_remote_accounts WHERE budget_id=?1 AND id=?2",params![b,id],|r|Ok(json!({"id":r.get::<_,String>(0)?,"connection_id":r.get::<_,String>(1)?,"generation":r.get::<_,String>(2)?,"remote_connection_id":r.get::<_,String>(3)?,"remote_id":r.get::<_,String>(4)?,"institution":r.get::<_,String>(5)?,"name":r.get::<_,String>(6)?,"currency":r.get::<_,String>(7)?,"balance":r.get::<_,String>(8)?,"available_balance":r.get::<_,Option<String>>(9)?,"balance_date":r.get::<_,i64>(10)?,"diagnostics":serde_json::from_str::<Value>(&r.get::<_,String>(11)?).map_err(|e|rusqlite::Error::FromSqlConversionFailure(11,rusqlite::types::Type::Text,Box::new(e)))?,"deleted":r.get::<_,bool>(12)?}))).optional()?.ok_or_else(Error::missing)
}
pub(super) fn links(db: &Connection, b: &str) -> Result<Vec<Value>, Error> {
    db.prepare("SELECT id,connection_id,remote_account_id,connection_generation,revision,initial_sync,opening_mode,opening_amount,opening_date,last_sync_end,tombstone FROM bank_links WHERE budget_id=?1 ORDER BY id")?.query_map([b],|r|Ok(json!({"id":r.get::<_,String>(0)?,"connection_id":r.get::<_,String>(1)?,"remote_account_id":r.get::<_,String>(2)?,"connection_generation":r.get::<_,String>(3)?,"revision":r.get::<_,i64>(4)?,"initial_sync":r.get::<_,bool>(5)?,"opening_mode":r.get::<_,String>(6)?,"opening_amount":r.get::<_,Option<i64>>(7)?,"opening_date":r.get::<_,Option<String>>(8)?,"last_sync_end":r.get::<_,Option<String>>(9)?,"deleted":r.get::<_,bool>(10)?})))?.collect::<Result<Vec<_>,_>>().map_err(Error::from)
}
pub(super) fn accounts(db: &Connection, b: &str) -> Result<Vec<Value>, Error> {
    let ids = db
        .prepare("SELECT id FROM bank_remote_accounts WHERE budget_id=?1 ORDER BY rowid")?
        .query_map([b], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    ids.iter().map(|id| remote(db, b, id)).collect()
}
pub(super) fn install_remote(
    db: &Connection,
    b: &str,
    connection: &str,
    generation: &str,
    raw: &Value,
) -> Result<(Vec<Value>, Vec<Value>), Error> {
    if ["next", "next_cursor", "continuation", "has_more"]
        .iter()
        .any(|k| {
            raw.get(k)
                .is_some_and(|v| !v.is_null() && v != false && v != "")
        })
    {
        return Err(transport::failure(
            "provider_pagination_unsupported",
            "The provider returned a continuation that SimpleFIN does not define. No data was applied",
        ));
    }
    let source = raw["accounts"]
        .as_array()
        .filter(|rows| rows.len() <= 500)
        .ok_or_else(|| {
            transport::failure(
                "provider_response_limit",
                "Provider must return at most 500 accounts",
            )
        })?;
    let mut errors = vec![];
    if let Some(list) = raw.get("errlist") {
        let list = list.as_array().filter(|l| l.len() <= 1000).ok_or_else(|| {
            transport::failure(
                "provider_invalid_response",
                "Provider diagnostics are invalid",
            )
        })?;
        for e in list {
            let code = e["code"].as_str().unwrap_or("gen.");
            let (public, message) = if code == "gen.auth" || code == "con.auth" {
                (
                    "provider_attention",
                    "Reauthorize this connection at SimpleFIN",
                )
            } else if code == "act.missingdata" {
                (
                    "provider_incomplete",
                    "The provider returned incomplete transactions. Retry later",
                )
            } else {
                (
                    "provider_attention",
                    "SimpleFIN reported a connection or account problem. Review the provider and retry",
                )
            };
            // Scope IDs are copied only when they correspond to an actual account;
            // arbitrary diagnostic text is never copied into public state or receipts.
            let account = e["account_id"]
                .as_str()
                .and_then(|id| source.iter().find(|a| a["id"] == id))
                .and_then(|a| a["id"].as_str());
            let conn = e["conn_id"]
                .as_str()
                .filter(|id| source.iter().any(|a| a["conn_id"] == *id));
            errors.push(json!({"code":public,"message":message,"remote_id":account,"remote_connection_id":conn}));
        }
    }
    if let Some(legacy) = raw.get("errors") {
        let legacy = legacy
            .as_array()
            .filter(|l| l.len() <= 1000)
            .ok_or_else(|| {
                transport::failure(
                    "provider_invalid_response",
                    "Provider diagnostics are invalid",
                )
            })?;
        if !legacy.is_empty() {
            errors.push(json!({"code":"provider_attention","message":"SimpleFIN reported a warning. Review your provider connection before importing","remote_id":null,"remote_connection_id":null}));
        }
    }
    let mut prepared = vec![];
    let mut seen = std::collections::BTreeSet::new();
    for account in source {
        let remote_id = label(account, "id", 1024)?;
        let remote_connection = account["conn_id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| account["org"]["id"].as_str())
            .unwrap_or("legacy")
            .to_owned();
        if remote_connection.len() > 1024
            || !seen.insert((remote_connection.clone(), remote_id.clone()))
        {
            return Err(transport::failure(
                "provider_invalid_response",
                "Provider account identities must be distinct within each institution",
            ));
        }
        let institution = account["conn_name"]
            .as_str()
            .or_else(|| {
                raw["connections"]
                    .as_array()
                    .and_then(|rows| rows.iter().find(|r| r["conn_id"] == remote_connection))
                    .and_then(|r| r["name"].as_str())
            })
            .or_else(|| account["org"]["name"].as_str())
            .unwrap_or("SimpleFIN");
        if institution.len() > 512 || institution.chars().any(char::is_control) {
            return Err(transport::failure(
                "provider_invalid_response",
                "Provider institution name is invalid",
            ));
        }
        let institution = institution.to_owned();
        let name = label(account, "name", 512)?;
        let currency = label(account, "currency", 512)?;
        let balance = decimal(account, "balance", true)?.unwrap();
        let available = decimal(account, "available-balance", false)?;
        let date = account["balance-date"]
            .as_i64()
            .filter(|v| (-62135596800..=253402300799).contains(v))
            .ok_or_else(|| {
                transport::failure(
                    "provider_invalid_response",
                    "Provider balance date is invalid",
                )
            })?;
        prepared.push((
            identity(connection, &remote_connection, &remote_id),
            remote_connection,
            remote_id,
            institution,
            name,
            currency,
            balance,
            available,
            date,
        ));
    }
    // Validation above is complete before the first metadata write. SQL failures
    // propagate to the surrounding receipt transaction and roll everything back.
    let current: i64 = db.query_row(
        "SELECT count(*) FROM bank_remote_accounts WHERE budget_id=?1",
        [b],
        |r| r.get(0),
    )?;
    let mut additions = 0;
    for (id, ..) in &prepared {
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM bank_remote_accounts WHERE budget_id=?1 AND id=?2)",
            params![b, id],
            |r| r.get(0),
        )?;
        if !exists {
            additions += 1;
        }
    }
    if current + additions > 5000 {
        return Err(transport::failure(
            "provider_response_limit",
            "This budget exceeds the 5000 retained remote account limit. No data was applied",
        ));
    }
    let clock = sync::clock(db, b, None)?;
    let old=db.prepare("SELECT id FROM bank_remote_accounts WHERE budget_id=?1 AND connection_id=?2 AND tombstone=0")?.query_map(params![b,connection],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
    if errors.is_empty() {
        for id in old {
            db.execute(
                "UPDATE bank_remote_accounts SET tombstone=1 WHERE budget_id=?1 AND id=?2",
                params![b, id],
            )?;
            sync::mark(db, b, "bank_remote_accounts", &id)?;
        }
    }
    let mut accounts = vec![];
    for (id, conn, remote_id, institution, name, currency, balance, available, date) in prepared {
        let mut diagnostics = errors
            .iter()
            .filter(|e| {
                (e["remote_id"].is_null() || e["remote_id"] == remote_id)
                    && (e["remote_connection_id"].is_null() || e["remote_connection_id"] == conn)
            })
            .cloned()
            .collect::<Vec<_>>();
        if diagnostics.len() > 8 {
            let extra = diagnostics.len() - 7;
            diagnostics.truncate(7);
            diagnostics.push(json!({"code":"provider_additional_errors","message":"Additional provider diagnostics are omitted; this account remains blocked","count":extra}));
        }
        db.execute("INSERT INTO bank_remote_accounts(budget_id,id,connection_id,generation,remote_connection_id,remote_id,institution,name,currency,balance,available_balance,balance_date,diagnostics) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13) ON CONFLICT(budget_id,id) DO UPDATE SET generation=excluded.generation,institution=excluded.institution,name=excluded.name,currency=excluded.currency,balance=excluded.balance,available_balance=excluded.available_balance,balance_date=excluded.balance_date,diagnostics=excluded.diagnostics,tombstone=0",params![b,id,connection,generation,conn,remote_id,institution,name,currency,balance,available,date,serde_json::to_string(&diagnostics).map_err(Error::internal)?])?;
        sync::seed_field_version(db, b, "bank_remote_accounts", &id, "metadata", &clock)?;
        sync::mark(db, b, "bank_remote_accounts", &id)?;
        accounts.push(remote(db, b, &id)?);
    }
    Ok((accounts, errors))
}
pub(super) fn tables() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            "bank_connections",
            vec![
                "budget_id",
                "id",
                "provider",
                "name",
                "state",
                "generation",
                "revision",
                "error_code",
                "message",
                "last_success",
                "tombstone",
            ],
        ),
        (
            "bank_remote_accounts",
            vec![
                "budget_id",
                "id",
                "connection_id",
                "generation",
                "remote_connection_id",
                "remote_id",
                "institution",
                "name",
                "currency",
                "balance",
                "available_balance",
                "balance_date",
                "diagnostics",
                "tombstone",
            ],
        ),
        (
            "bank_links",
            vec![
                "budget_id",
                "id",
                "connection_id",
                "remote_account_id",
                "connection_generation",
                "revision",
                "initial_sync",
                "opening_mode",
                "opening_amount",
                "opening_date",
                "last_sync_end",
                "tombstone",
            ],
        ),
        (
            "bank_transaction_origins",
            vec![
                "budget_id",
                "id",
                "connection_id",
                "remote_account_id",
                "tombstone",
            ],
        ),
    ]
}
