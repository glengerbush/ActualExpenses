//! GoCardless consent custody. Submitted secrets never enter generic receipts.
use super::*;
mod consent;
mod protocol;
use axum::extract::Query;
pub(super) use consent::{authorization, command};

#[derive(Clone, serde::Serialize, Deserialize)]
struct Credentials {
    secret_id: String,
    secret_key: String,
    access_token: Option<String>,
    refresh_token: Option<String>,
    access_expires: Option<i64>,
}
fn credentials(db: &Connection, b: &str, id: &str, generation: &str) -> Result<Credentials, Error> {
    db.query_row("SELECT secret_id,secret_key,access_token,refresh_token,access_expires FROM bank_gocardless_credentials WHERE budget_id=?1 AND connection_id=?2 AND generation=?3",params![b,id,generation], |r| Ok(Credentials { secret_id:r.get(0)?,secret_key:r.get(1)?,access_token:r.get(2)?,refresh_token:r.get(3)?,access_expires:r.get(4)? })).optional()?.ok_or_else(||Error::conflict("GoCardless credentials are missing. Replace user secrets before authorizing"))
}
fn save_tokens(
    db: &Connection,
    b: &str,
    id: &str,
    generation: &str,
    tokens: &Credentials,
) -> Result<(), Error> {
    if db.execute("UPDATE bank_gocardless_credentials SET access_token=?4,refresh_token=?5,access_expires=?6 WHERE budget_id=?1 AND connection_id=?2 AND generation=?3",params![b,id,generation,tokens.access_token,tokens.refresh_token,tokens.access_expires])? != 1 { return Err(Error::conflict("Credentials changed while the provider was responding")); }
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Country {
    country: String,
}
pub(super) async fn institutions(
    State(store): State<Store>,
    Path((b, id)): Path<(String, String)>,
    input: Result<Query<Country>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, Error> {
    let Query(query) = input.map_err(|_| Error::invalid("Choose a valid country query"))?;
    models::validate_id(&b)?;
    models::validate_id(&id)?;
    if query.country.len() != 2 || !query.country.bytes().all(|c| c.is_ascii_uppercase()) {
        return Err(Error::invalid("Choose a two-letter uppercase country code"));
    }
    let (bb, ii) = (b.clone(), id.clone());
    let (generation, epoch, tokens) = store
        .run(move |db| {
            let current = connection(db, &bb, &ii)?;
            if current["provider"] != "gocardless" || current["deleted"] == true {
                return Err(Error::missing());
            }
            let generation = current["generation"].as_str().unwrap().to_owned();
            Ok((
                generation.clone(),
                crate::lifecycle::epoch(db, &bb)?,
                credentials(db, &bb, &ii, &generation)?,
            ))
        })
        .await?;
    let permit = transport::permit()?;
    let (tokens, result) = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut client = protocol::Client::new(tokens);
        let result = client.institutions(&query.country);
        (client.tokens, result)
    })
    .await
    .map_err(|_| {
        transport::failure("provider_unavailable", "Provider retrieval was interrupted")
    })?;
    store
        .run(move |db| {
            let tx = db.transaction()?;
            crate::lifecycle::require_epoch(&tx, &b, Some(&epoch))?;
            save_tokens(&tx, &b, &id, &generation, &tokens)?;
            tx.commit()?;
            result.map(|data| Json(json!({"data":data})))
        })
        .await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    id: String,
    provider: String,
    name: String,
    secret_id: Option<String>,
    secret_key: Option<String>,
    credentials_sha256: Option<String>,
}
fn fingerprint(
    id: Option<&str>,
    key: Option<&str>,
    digest: Option<&str>,
    recovery: bool,
) -> Result<String, Error> {
    match (id, key, digest) {
        (Some(id), Some(key), None)
            if [id, key]
                .iter()
                .all(|v| !v.is_empty() && v.len() <= 4096 && !v.chars().any(char::is_control)) =>
        {
            let bytes = serde_json::to_vec(&[id, key]).map_err(Error::internal)?;
            Ok(format!("{:x}", Sha256::digest(bytes)))
        }
        (None, None, Some(digest))
            if recovery
                && digest.len() == 64
                && digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) =>
        {
            Ok(digest.to_owned())
        }
        _ => Err(Error::invalid(
            "Supply both GoCardless user secrets; the credentials fingerprint is accepted only for original receipt lookup or cancellation",
        )),
    }
}
pub(super) async fn create(
    store: Store,
    budget: String,
    headers: HeaderMap,
    value: Value,
) -> Result<(StatusCode, Json<Value>), Error> {
    let body: Create = serde_json::from_value(value)
        .map_err(|_| Error::invalid("Choose a valid GoCardless connection request"))?;
    models::validate_id(&body.id)?;
    models::validate_name(&body.name)?;
    if body.provider != "gocardless" {
        return Err(Error::invalid("Choose GoCardless"));
    }
    let key = api::idempotency_key(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let lookup = sync::receipt_only_header(&headers)?;
    let cancel = sync::cancel_header(&headers)?;
    let digest = fingerprint(
        body.secret_id.as_deref(),
        body.secret_key.as_deref(),
        body.credentials_sha256.as_deref(),
        lookup || cancel,
    )?;
    let payload = json!({"id":body.id,"provider":body.provider,"name":body.name,"credentials_sha256":digest,"epoch":epoch}).to_string();
    store.run(move |db| {
        let scope = format!("POST /budgets/{budget}/banking/connections");
        if lookup { return sync::receipt(db, &scope, &key, &payload, None, StatusCode::CREATED); }
        sync::write(db, &scope, &key, &payload, None, StatusCode::CREATED, |db| {
            if cancel { return sync::canceled(db, &budget); }
            crate::lifecycle::require_epoch(db, &budget, epoch.as_deref())?;
            let count: i64 = db.query_row("SELECT count(*) FROM bank_connections WHERE budget_id=?1", [&budget], |r|r.get(0))?;
            if count >= 100 { return Err(Error::invalid("At most 100 connection identities are supported per budget")); }
            if db.query_row("SELECT EXISTS(SELECT 1 FROM bank_connections WHERE budget_id=?1 AND id=?2)", params![budget,body.id], |r|r.get::<_,bool>(0))? { return Err(Error::conflict("Connection identity already exists. Replace its credentials explicitly")); }
            let generation = uuid::Uuid::new_v4().to_string();
            db.execute("INSERT INTO bank_connections(budget_id,id,provider,name,generation) VALUES(?1,?2,'gocardless',?3,?4)",params![budget,body.id,body.name,generation])?;
            db.execute("INSERT INTO bank_gocardless_credentials(budget_id,connection_id,generation,secret_id,secret_key) VALUES(?1,?2,?3,?4,?5)",params![budget,body.id,generation,body.secret_id,body.secret_key])?;
            let clock = sync::clock(db,&budget,None)?;
            sync::seed_field_version(db,&budget,"bank_connections",&body.id,"connection",&clock)?;
            sync::mark(db,&budget,"bank_connections",&body.id)?;
            Ok(json!({"response_status":201,"response_body":{"data":connection(db,&budget,&body.id)?}}))
        })
    }).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Replacement {
    action: String,
    generation: String,
    secret_id: Option<String>,
    secret_key: Option<String>,
    credentials_sha256: Option<String>,
}
pub(super) async fn custody(
    store: Store,
    b: String,
    id: String,
    headers: HeaderMap,
    value: Value,
) -> Result<(StatusCode, Json<Value>), Error> {
    let body: Replacement = serde_json::from_value(value)
        .map_err(|_| Error::invalid("Choose a valid GoCardless credential command"))?;
    models::validate_id(&body.generation)?;
    if !matches!(body.action.as_str(), "replace" | "remove") {
        return Err(Error::invalid("Choose credential replacement or removal"));
    }
    let key = api::idempotency_key(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let lookup = sync::receipt_only_header(&headers)?;
    let cancel = sync::cancel_header(&headers)?;
    let mut canonical = json!({"action":body.action,"generation":body.generation,"epoch":epoch});
    if body.action == "replace" {
        canonical["credentials_sha256"] = json!(fingerprint(
            body.secret_id.as_deref(),
            body.secret_key.as_deref(),
            body.credentials_sha256.as_deref(),
            lookup || cancel
        )?);
    } else if body.secret_id.is_some()
        || body.secret_key.is_some()
        || body.credentials_sha256.is_some()
    {
        return Err(Error::invalid(
            "Only credential replacement accepts user secrets or their fingerprint",
        ));
    }
    let payload = canonical.to_string();
    store.run(move|db| {
        let scope=format!("POST /budgets/{b}/banking/connections/{id}/commands");
        if lookup {return sync::receipt(db,&scope,&key,&payload,None,StatusCode::OK);}
        sync::write(db,&scope,&key,&payload,None,StatusCode::OK,|db| {
            guard_pending(db,&b,&id,&key,&payload)?;
            if cancel {return sync::canceled(db,&b);}
            crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;
            let current=connection(db,&b,&id)?;
            if current["provider"]!="gocardless" || current["generation"]!=body.generation || current["deleted"]==true {return Err(Error::conflict("Connection changed. Review it before replacing credentials or removing it"));}
            let generation=uuid::Uuid::new_v4().to_string();
            db.execute("DELETE FROM bank_jobs WHERE budget_id=?1 AND connection_id=?2",params![b,id])?;
            db.execute("DELETE FROM bank_gocardless_credentials WHERE budget_id=?1 AND connection_id=?2",params![b,id])?;
            if body.action=="replace" {
                db.execute("INSERT INTO bank_gocardless_credentials(budget_id,connection_id,generation,secret_id,secret_key) VALUES(?1,?2,?3,?4,?5)",params![b,id,generation,body.secret_id,body.secret_key])?;
            }
            db.execute("UPDATE bank_connections SET generation=?3,state=?4,tombstone=?5,error_code=NULL,message=NULL,revision=revision+1 WHERE budget_id=?1 AND id=?2",params![b,id,generation,if body.action=="replace"{"claim_pending"}else{"credentials_missing"},body.action=="remove"])?;
            sync::mark(db,&b,"bank_connections",&id)?;
            Ok(json!({"response_status":200,"response_body":{"data":connection(db,&b,&id)?}}))
        })
    }).await
}
