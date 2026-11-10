//! Provider data enters the same immutable capture/guarded financial commit as28.
use super::*;
use reqwest::Method;
pub(in crate::banking) struct DownloadCredentials {
    tokens: Credentials,
    requisition: String,
    institution: String,
    remotes: Vec<Value>,
}
pub(in crate::banking) fn prepare_download(
    db: &Connection,
    b: &str,
    id: &str,
    generation: &str,
) -> Result<DownloadCredentials, Error> {
    let tokens = credentials(db, b, id, generation)?;
    let (requisition,institution):(Option<String>,Option<String>)=db.query_row("SELECT requisition_id,institution_id FROM bank_gocardless_credentials WHERE budget_id=?1 AND connection_id=?2 AND generation=?3",params![b,id,generation],|r|Ok((r.get(0)?,r.get(1)?)))?;
    let requisition = requisition
        .ok_or_else(|| Error::conflict("Bank consent is missing. Authorize GoCardless again"))?;
    let institution =
        institution.ok_or_else(|| Error::conflict("Bank consent institution is missing"))?;
    protocol::identifier(&json!(requisition))?;
    protocol::identifier(&json!(institution))?;
    let remotes = records::accounts(db, b)?
        .into_iter()
        .filter(|r| {
            r["connection_id"] == id && r["generation"] == generation && r["deleted"] == false
        })
        .collect();
    Ok(DownloadCredentials {
        tokens,
        requisition,
        institution,
        remotes,
    })
}
pub(in crate::banking) fn save_download(
    db: &Connection,
    b: &str,
    id: &str,
    generation: &str,
    download: &DownloadCredentials,
) -> Result<(), Error> {
    save_tokens(db, b, id, generation, &download.tokens)
}
fn details(client: &mut protocol::Client, id: &str) -> Result<Value, Error> {
    let details = client.request(
        Method::GET,
        &format!("/api/v2/accounts/{id}/details/"),
        None,
        true,
    )?;
    let metadata = client.request(Method::GET, &format!("/api/v2/accounts/{id}/"), None, true)?;
    if metadata["id"] != id {
        return Err(transport::failure(
            "provider_invalid_response",
            "GoCardless returned another account identity",
        ));
    }
    let mut result = details["account"].as_object().cloned().unwrap_or_default();
    let metadata = metadata.as_object().ok_or_else(|| {
        transport::failure(
            "provider_invalid_response",
            "GoCardless account metadata is invalid",
        )
    })?;
    for (key, value) in metadata {
        if !value.is_null() && value != false && value != "" && value != 0 {
            result.insert(key.clone(), value.clone());
        }
    }
    Ok(Value::Object(result))
}
fn perform(
    client: &mut protocol::Client,
    session: &DownloadCredentials,
    download: transport::Download,
) -> Result<Value, Error> {
    client.authenticate()?;
    let req = client.request(
        Method::GET,
        &format!("/api/v2/requisitions/{}/", session.requisition),
        None,
        true,
    )?;
    if req["id"] != session.requisition || req["institution_id"] != session.institution {
        return Err(transport::failure(
            "provider_invalid_response",
            "GoCardless returned another consent identity or institution",
        ));
    }
    if req["status"] != "LN" {
        return Err(transport::failure(
            "provider_consent_expired",
            "Bank consent is no longer linked. Authorize this connection again",
        ));
    }
    let authorized = req["accounts"]
        .as_array()
        .filter(|r| r.len() <= 1000)
        .ok_or_else(|| {
            transport::failure(
                "provider_response_limit",
                "Consent account list exceeds1000 entries or is invalid",
            )
        })?
        .iter()
        .map(protocol::identifier)
        .collect::<Result<Vec<_>, _>>()?;
    if authorized
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != authorized.len()
    {
        return Err(transport::failure(
            "provider_invalid_response",
            "Consent account identities must be distinct",
        ));
    }
    let (selected, range) = match download {
        transport::Download::Balances => {
            if authorized.len() > 20 {
                return Err(transport::failure(
                    "provider_response_limit",
                    "Select a bounded discovery page before loading more than20 accounts",
                ));
            }
            (authorized.clone(), None)
        }
        transport::Download::Transactions {
            accounts,
            start,
            end,
        } => (accounts, Some((start, end))),
    };
    normalization::supported(&session.institution)?;
    let institution = if range.is_none() {
        client.request(
            Method::GET,
            &format!("/api/v2/institutions/{}/", session.institution),
            None,
            true,
        )?
    } else {
        Value::Null
    };
    let mut accounts = vec![];
    let mut errors = vec![];
    for id in selected {
        if !authorized.contains(&id) {
            errors.push(
                json!({"code":"act.missingdata","account_id":id,"conn_id":session.requisition}),
            );
            continue;
        }
        let fetched = (|| {
            let previous = session
                .remotes
                .iter()
                .find(|r| r["remote_id"] == id && r["remote_connection_id"] == session.requisition);
            let metadata = if range.is_none() {
                Some(details(client, &id)?)
            } else {
                None
            };
            let balances = client.request(
                Method::GET,
                &format!("/api/v2/accounts/{id}/balances/"),
                None,
                true,
            )?;
            let primary = normalization::balance(&balances, &session.institution)?;
            let currency = metadata
                .as_ref()
                .and_then(|v| v["currency"].as_str())
                .or_else(|| primary.and_then(|v| v["balanceAmount"]["currency"].as_str()))
                .or_else(|| previous.and_then(|v| v["currency"].as_str()))
                .unwrap_or("");
            if currency.len() != 3 || !currency.bytes().all(|c| c.is_ascii_uppercase()) {
                return Err(transport::failure(
                    "provider_invalid_response",
                    "Provider account currency is missing or invalid",
                ));
            }
            let name = if let Some(metadata) = &metadata {
                normalization::account(metadata, &institution)?
            } else {
                previous
                    .and_then(|v| v["name"].as_str())
                    .unwrap_or("")
                    .to_owned()
            };
            let org = if range.is_none() {
                institution["name"].as_str().unwrap_or("")
            } else {
                previous
                    .and_then(|v| v["institution"].as_str())
                    .unwrap_or("")
            };
            let balance = primary
                .and_then(|v| v["balanceAmount"]["amount"].as_str())
                .unwrap_or("");
            if !balance.is_empty() {
                normalize::minor(balance)?;
            }
            let available = balances["balances"]
                .as_array()
                .and_then(|r| {
                    r.iter()
                        .find(|v| v["balanceType"] == "interimAvailable")
                        .or_else(|| r.iter().find(|v| v["balanceType"] == "forwardAvailable"))
                })
                .and_then(|v| v["balanceAmount"]["amount"].as_str());
            let date = primary
                .and_then(|v| v["referenceDate"].as_str())
                .and_then(|v| chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").ok())
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|d| d.and_utc().timestamp())
                .unwrap_or(0);
            let mut account = json!({"id":id,"conn_id":session.requisition,"name":name,"org":{"id":session.requisition,"name":org},"currency":currency,"balance":balance,"balance-date":date,"transactions":[]});
            if let Some(available) = available {
                account["available-balance"] = json!(available);
            }
            if let Some((start, end)) = range {
                let start = chrono::DateTime::from_timestamp(start, 0)
                    .ok_or_else(|| Error::invalid("Invalid refresh start"))?
                    .format("%Y-%m-%d")
                    .to_string();
                let end = chrono::DateTime::from_timestamp(end - 86400, 0)
                    .ok_or_else(|| Error::invalid("Invalid refresh end"))?
                    .format("%Y-%m-%d")
                    .to_string();
                let raw = client.request(
                    Method::GET,
                    &format!("/api/v2/accounts/{id}/transactions/?date_from={start}&date_to={end}"),
                    None,
                    true,
                )?;
                let rows = normalization::transactions(&raw, &session.institution)?;
                let opening = normalization::opening(&rows, &balances, &session.institution)?;
                account["_gocardless_core"] = json!(rows);
                account["_gocardless_opening"] = json!(opening);
            }
            Ok(account)
        })();
        match fetched {Ok(account)=>accounts.push(account),Err(error)=>errors.push(json!({"code":"act.missingdata","account_id":id,"conn_id":session.requisition,"provider_code":error.code}))}
    }
    Ok(
        json!({"accounts":accounts,"errlist":errors,"connections":[],"_gocardless_authorized":authorized,"_gocardless_requisition":session.requisition}),
    )
}
pub(in crate::banking) fn retrieve(
    mut session: DownloadCredentials,
    download: transport::Download,
) -> (Result<Value, Error>, DownloadCredentials) {
    let mut client = protocol::Client::new(session.tokens.clone());
    let result = perform(&mut client, &session, download);
    session.tokens = client.tokens;
    (result, session)
}

pub(in crate::banking) async fn observe_download_quota(
    store: Store,
    download: &DownloadCredentials,
) -> Result<(), Error> {
    observe_quota(store, &download.tokens).await
}
