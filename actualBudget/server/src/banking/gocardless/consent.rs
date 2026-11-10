//! Explicit consent intentions precede non-idempotent provider calls. An unknown
//! authorization outcome is never retried as another remote requisition.
use super::*;
use reqwest::{Method, Url};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Authorize {
    action: String,
    generation: String,
    institution_id: String,
    country: String,
    redirect_origin: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Check {
    action: String,
    generation: String,
}
struct Prepared {
    generation: String,
    tokens: Credentials,
    requisition: Option<String>,
}
struct Consent {
    requisition: String,
    institution: String,
    country: String,
    url: String,
}
fn origin(raw: &str) -> Result<String, Error> {
    let url =
        Url::parse(raw).map_err(|_| Error::invalid("Choose a valid application return origin"))?;
    if raw.len() > 2048
        || !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::invalid(
            "Use this application's HTTP or HTTPS origin for consent return",
        ));
    }
    Ok(url.origin().ascii_serialization())
}
fn handoff(raw: &Value) -> Result<String, Error> {
    let raw = raw.as_str().filter(|v| v.len() <= 8192).ok_or_else(|| {
        transport::failure(
            "provider_invalid_response",
            "GoCardless did not return an authorization link",
        )
    })?;
    let url = Url::parse(raw).map_err(|_| {
        transport::failure(
            "provider_invalid_response",
            "GoCardless returned an invalid authorization link",
        )
    })?;
    if url.scheme() != "https"
        || !matches!(
            url.host_str(),
            Some("ob.gocardless.com" | "bankaccountdata.gocardless.com" | "ob.nordigen.com")
        )
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(transport::failure(
            "provider_invalid_response",
            "GoCardless returned an unrecognized authorization destination",
        ));
    }
    Ok(raw.to_owned())
}
fn authorize(
    client: &mut protocol::Client,
    body: &Authorize,
    generation: &str,
) -> Result<Consent, Error> {
    client.authenticate()?;
    let institution = client.request(
        Method::GET,
        &format!("/api/v2/institutions/{}/", body.institution_id),
        None,
        true,
    )?;
    if institution["id"] != body.institution_id
        || !institution["countries"]
            .as_array()
            .is_some_and(|r| r.iter().any(|v| v == &body.country))
    {
        return Err(transport::failure(
            "provider_invalid_response",
            "The institution does not support this country",
        ));
    }
    let access = protocol::days(&institution["max_access_valid_for_days"])?;
    let features = institution["supported_features"].as_array();
    let separate =
        features.is_some_and(|f| f.iter().any(|v| v == "separate_continuous_history_consent"));
    let history = if separate {
        90
    } else {
        protocol::days(&institution["transaction_total_days"])?
    };
    let agreement=client.request(Method::POST,"/api/v2/agreements/enduser/",Some(json!({"institution_id":body.institution_id,"max_historical_days":history,"access_valid_for_days":access,"access_scope":["balances","details","transactions"]})),true)?;
    let agreement = protocol::identifier(&agreement["id"])?;
    let response=client.request(Method::POST,"/api/v2/requisitions/",Some(json!({"redirect":format!("{}/?banking_return={generation}",origin(&body.redirect_origin)?),"institution_id":body.institution_id,"agreement":agreement,"reference":generation,"user_language":"en","redirect_immediate":false,"account_selection":features.is_some_and(|f|f.iter().any(|v|v=="account_selection"))})),true)?;
    Ok(Consent {
        requisition: protocol::identifier(&response["id"])?,
        institution: body.institution_id.clone(),
        country: body.country.clone(),
        url: handoff(&response["link"])?,
    })
}
pub(in crate::banking) async fn authorization(
    State(store): State<Store>,
    Path((b, id)): Path<(String, String)>,
) -> Result<(HeaderMap, Json<Value>), Error> {
    models::validate_id(&b)?;
    models::validate_id(&id)?;
    let data=store.run(move|db| {
        let c=connection(db,&b,&id)?;
        if c["provider"]!="gocardless" || c["deleted"]==true {return Err(Error::missing());}
        Ok(db.query_row("SELECT authorization_url,consent_status,requisition_id,institution_id,country FROM bank_gocardless_credentials WHERE budget_id=?1 AND connection_id=?2 AND generation=?3",params![b,id,c["generation"].as_str()],|r|Ok(json!({"url":r.get::<_,Option<String>>(0)?,"status":r.get::<_,Option<String>>(1)?,"requisition_id":r.get::<_,Option<String>>(2)?,"institution_id":r.get::<_,Option<String>>(3)?,"country":r.get::<_,Option<String>>(4)?}))).optional()?.unwrap_or(json!({"url":null,"status":null,"requisition_id":null,"institution_id":null,"country":null})))
    }).await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        "no-store".parse().unwrap(),
    );
    Ok((headers, Json(json!({"data":data}))))
}
pub(in crate::banking) async fn command(
    store: Store,
    b: String,
    id: String,
    headers: HeaderMap,
    value: Value,
) -> Result<(StatusCode, Json<Value>), Error> {
    let is_authorize = value["action"] == "authorize";
    let (generation, auth) = if is_authorize {
        let body: Authorize = serde_json::from_value(value.clone()).map_err(|_| {
            Error::invalid("Choose valid institution, country and consent return fields")
        })?;
        let _ = &body.action;
        models::validate_id(&body.generation)?;
        protocol::identifier(&json!(body.institution_id))?;
        if body.country.len() != 2 || !body.country.bytes().all(|c| c.is_ascii_uppercase()) {
            return Err(Error::invalid("Choose a two-letter uppercase country code"));
        }
        origin(&body.redirect_origin)?;
        if let Some(sent) = headers.get(axum::http::header::ORIGIN)
            && sent.to_str().ok() != Some(body.redirect_origin.as_str())
        {
            return Err(Error::invalid(
                "Consent return must match this browser's origin",
            ));
        }
        (body.generation.clone(), Some(body))
    } else {
        let body: Check = serde_json::from_value(value.clone())
            .map_err(|_| Error::invalid("Choose a valid authorization status command"))?;
        if body.action != "check_authorization" {
            return Err(Error::invalid("Unsupported GoCardless connection action"));
        }
        models::validate_id(&body.generation)?;
        (body.generation, None)
    };
    let key = api::idempotency_key(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    if epoch.is_none() {
        return Err(Error::invalid(
            "Bank consent requires the original budget epoch",
        ));
    }
    let lookup = sync::receipt_only_header(&headers)?;
    let cancel = sync::cancel_header(&headers)?;
    let payload = json!({"command":value,"epoch":epoch}).to_string();
    let scope = format!("POST /budgets/{b}/banking/connections/{id}/commands");
    if lookup {
        return store
            .run(move |db| sync::receipt(db, &scope, &key, &payload, None, StatusCode::OK))
            .await;
    }
    if cancel {
        return store.run(move|db|sync::write(db,&scope,&key,&payload,None,StatusCode::OK,|db| {
            let result=sync::canceled(db,&b)?;
            let changed=if is_authorize {
                db.execute("UPDATE bank_gocardless_credentials SET intent_id=NULL,intent_fingerprint=NULL,authorization_url=NULL,requisition_id=NULL,consent_status=NULL WHERE budget_id=?1 AND connection_id=?2 AND intent_id=?3",params![b,id,key])?
            }else {
                db.execute("UPDATE bank_gocardless_credentials SET intent_id=NULL,intent_fingerprint=NULL WHERE budget_id=?1 AND connection_id=?2 AND intent_id=?3",params![b,id,key])?
            };
            if changed>0 && is_authorize {
                db.execute("UPDATE bank_connections SET state='claim_pending',revision=revision+1 WHERE budget_id=?1 AND id=?2",params![b,id])?;sync::mark(db,&b,"bank_connections",&id)?;
            }
            Ok(result)
        })).await;
    }
    let (bb, ii, ss, kk, pp, ee, gg) = (
        b.clone(),
        id.clone(),
        scope.clone(),
        key.clone(),
        payload.clone(),
        epoch.clone(),
        generation.clone(),
    );
    let prepared=store.run(move|db| {
        if crate::store::receipt(db,&ss,&kk,&pp)?.is_some() {return Ok(None);}
        let tx=db.transaction()?;
        crate::lifecycle::require_epoch(&tx,&bb,ee.as_deref())?;
        let current=connection(&tx,&bb,&ii)?;
        if current["provider"]!="gocardless" || current["generation"]!=gg || current["deleted"]==true {return Err(Error::conflict("Connection changed. Review the current consent"));}
        let (intent,fingerprint,requisition):(Option<String>,Option<String>,Option<String>)=tx.query_row("SELECT intent_id,intent_fingerprint,requisition_id FROM bank_gocardless_credentials WHERE budget_id=?1 AND connection_id=?2 AND generation=?3",params![bb,ii,gg],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?.ok_or_else(||Error::conflict("Credentials are missing. Replace user secrets"))?;
        if let Some(intent)=intent && (is_authorize || intent!=kk || fingerprint.as_deref()!=Some(&pp)) {return Err(Error::conflict("Another consent request may still be finishing. Check or cancel its original identity; do not repeat authorization"));}
        if !is_authorize && requisition.is_none() {return Err(Error::conflict("Prepare an authorization link first"));}
        let generation=if is_authorize {uuid::Uuid::new_v4().to_string()}else{gg};
        let tokens=credentials(&tx,&bb,&ii,current["generation"].as_str().unwrap())?;
        if is_authorize {
            tx.execute("UPDATE bank_connections SET generation=?3,state='claim_inflight',error_code=NULL,message=NULL,revision=revision+1 WHERE budget_id=?1 AND id=?2",params![bb,ii,generation])?;
            tx.execute("UPDATE bank_gocardless_credentials SET generation=?3,requisition_id=NULL,institution_id=NULL,country=NULL,authorization_url=NULL,consent_status=NULL WHERE budget_id=?1 AND connection_id=?2",params![bb,ii,generation])?;
            sync::mark(&tx,&bb,"bank_connections",&ii)?;
        }
        tx.execute("UPDATE bank_gocardless_credentials SET intent_id=?3,intent_fingerprint=?4 WHERE budget_id=?1 AND connection_id=?2",params![bb,ii,kk,pp])?;
        let permit=transport::permit()?;
        tx.commit()?;
        Ok(Some((Prepared{generation,tokens,requisition},permit)))
    }).await?;
    let Some((prepared, permit)) = prepared else {
        return store
            .run(move |db| sync::receipt(db, &scope, &key, &payload, None, StatusCode::OK))
            .await;
    };
    let operation_generation = prepared.generation.clone();
    let (tokens, result) = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut client = protocol::Client::new(prepared.tokens);
        let result = if let Some(auth) = auth {
            authorize(&mut client, &auth, &prepared.generation).map(Ok)
        } else {
            client
                .authenticate()
                .and_then(|_| {
                    client.request(
                        Method::GET,
                        &format!(
                            "/api/v2/requisitions/{}/",
                            prepared.requisition.as_deref().unwrap()
                        ),
                        None,
                        true,
                    )
                })
                .map(Err)
        };
        (client.tokens, result)
    })
    .await
    .map_err(|_| {
        transport::failure(
            "provider_unavailable",
            "Authorization was interrupted. Review or cancel the original consent request",
        )
    })?;
    store.run(move|db|sync::write(db,&scope,&key,&payload,None,StatusCode::OK,|db| {
        crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;
        let current=connection(db,&b,&id)?;
        if current["generation"]!=operation_generation || current["deleted"]==true {return Err(Error::conflict("Connection changed while the provider was responding"));}
        let intent:Option<String>=db.query_row("SELECT intent_id FROM bank_gocardless_credentials WHERE budget_id=?1 AND connection_id=?2",params![b,id],|r|r.get(0)).optional()?.flatten();
        if intent.as_deref()!=Some(&key) {return Err(Error::conflict("This consent intention was canceled or superseded"));}
        save_tokens(db,&b,&id,&operation_generation,&tokens)?;
        let (state,code,message)=match result {
            Ok(Ok(consent))=> {
                db.execute("UPDATE bank_gocardless_credentials SET requisition_id=?3,institution_id=?4,country=?5,authorization_url=?6,consent_status='CR' WHERE budget_id=?1 AND connection_id=?2",params![b,id,consent.requisition,consent.institution,consent.country,consent.url])?;
                ("claim_pending",None,None)
            },
            Ok(Err(status))=> {
                let phase=status["status"].as_str().filter(|s|matches!(*s,"CR"|"GC"|"UA"|"RJ"|"SA"|"GA"|"LN"|"EX"|"SU")).ok_or_else(||transport::failure("provider_invalid_response","GoCardless returned an unknown consent status"))?;
                let requisition:String=db.query_row("SELECT requisition_id FROM bank_gocardless_credentials WHERE budget_id=?1 AND connection_id=?2",params![b,id],|r|r.get(0))?;
                if status["id"]!=requisition {return Err(transport::failure("provider_invalid_response","GoCardless returned another consent identity"));}
                let institution:String=db.query_row("SELECT institution_id FROM bank_gocardless_credentials WHERE budget_id=?1 AND connection_id=?2",params![b,id],|r|r.get(0))?;
                if status["institution_id"]!=institution {return Err(transport::failure("provider_invalid_response","GoCardless returned another consent institution"));}
                let accounts=status["accounts"].as_array().filter(|r|r.len()<=1000).ok_or_else(||transport::failure("provider_response_limit","Consent account list is invalid or exceeds1000 entries"))?;
                for account in accounts {protocol::identifier(account)?;}

                db.execute("UPDATE bank_gocardless_credentials SET consent_status=?3,authorization_url=CASE WHEN ?3 IN('LN','EX','SU','RJ') THEN NULL ELSE authorization_url END WHERE budget_id=?1 AND connection_id=?2",params![b,id,phase])?;
                match phase {"LN"=>("connected",None,None),"EX"|"SU"|"RJ"=>("claim_failed",Some("provider_consent_expired"),Some("Bank consent expired, was rejected or suspended. Authorize this connection again".to_owned())),_=>("claim_pending",None,None)}
            },
            Err(error)=> {
                let fail=is_authorize || matches!(error.code,"provider_consent_expired"|"provider_consent_suspended"|"provider_credentials_invalid");
                (if fail {"claim_failed"}else{current["state"].as_str().unwrap()},Some(error.code),Some(error.message))
            },
        };
        db.execute("UPDATE bank_gocardless_credentials SET intent_id=NULL,intent_fingerprint=NULL WHERE budget_id=?1 AND connection_id=?2",params![b,id])?;
        db.execute("UPDATE bank_connections SET state=?3,error_code=?4,message=?5,revision=revision+1 WHERE budget_id=?1 AND id=?2",params![b,id,state,code,message])?;
        sync::mark(db,&b,"bank_connections",&id)?;
        Ok(json!({"response_status":200,"response_body":{"data":connection(db,&b,&id)?}}))
    })).await
}
