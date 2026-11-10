//! Provider-neutral connection custody and SimpleFIN lifecycle. Only nonsecret
//! metadata crosses client, receipt, change-feed and portable archive boundaries.
mod records;
mod remote;
mod transport;
use crate::{
    api, concurrency as sync,
    error::Error,
    models,
    store::{self, Store},
};
use axum::{
    Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub fn router() -> Router<Store> {
    Router::new()
        .route("/api/v1/budgets/{budget}/banking", get(read))
        .route("/api/v1/budgets/{budget}/banking/connections", post(create))
        .route(
            "/api/v1/budgets/{budget}/banking/connections/{id}/commands",
            post(command),
        )
}
pub(crate) fn connection(db: &Connection, budget: &str, id: &str) -> Result<Value, Error> {
    db.query_row("SELECT id,provider,name,state,generation,revision,error_code,message,last_success,tombstone FROM bank_connections WHERE budget_id=?1 AND id=?2", params![budget,id], |r| Ok(json!({"id":r.get::<_,String>(0)?,"provider":r.get::<_,String>(1)?,"name":r.get::<_,String>(2)?,"state":r.get::<_,String>(3)?,"generation":r.get::<_,String>(4)?,"revision":r.get::<_,i64>(5)?,"error_code":r.get::<_,Option<String>>(6)?,"message":r.get::<_,Option<String>>(7)?,"last_success":r.get::<_,Option<String>>(8)?,"deleted":r.get::<_,bool>(9)?}))).optional()?.ok_or_else(Error::missing)
}
fn state(db: &Connection, budget: &str) -> Result<Value, Error> {
    store::require_budget(db, budget)?;
    let ids = db
        .prepare(
            "SELECT id FROM bank_connections WHERE budget_id=?1 AND tombstone=0 ORDER BY name,id",
        )?
        .query_map([budget], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(
        json!({"connections":ids.iter().map(|id|connection(db,budget,id)).collect::<Result<Vec<_>,_>>()?,"links":records::links(db,budget)?,"accounts":records::accounts(db,budget)?,"epoch":crate::lifecycle::epoch(db,budget)?,"snapshot":sync::cursor(db,budget)?}),
    )
}
async fn read(
    State(store): State<Store>,
    Path(budget): Path<String>,
) -> Result<Json<Value>, Error> {
    models::validate_id(&budget)?;
    Ok(Json(
        json!({"data":store.run(move|db|{let tx=db.transaction()?;state(&tx,&budget)}).await?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewConnection {
    id: String,
    provider: String,
    name: String,
    setup_token: String,
}
async fn create(
    State(store): State<Store>,
    Path(budget): Path<String>,
    headers: HeaderMap,
    input: Result<Json<NewConnection>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    models::validate_id(&budget)?;
    let Json(body) = input.map_err(api::json_error)?;
    models::validate_id(&body.id)?;
    models::validate_name(&body.name)?;
    if body.provider != "simplefin"
        || body.setup_token.is_empty()
        || body.setup_token.len() > 8192
        || !body
            .setup_token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(&b))
    {
        return Err(Error::invalid(
            "Choose SimpleFIN and paste a valid setup token",
        ));
    }
    transport::setup(&body.setup_token)?;
    let key = api::idempotency_key(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    // Raw external secrets must not enter the generic receipt payload. A stable
    // one-way fingerprint still refuses same-key/different-secret substitutions.
    let lookup = sync::receipt_only_header(&headers)?;
    let cancel = sync::cancel_header(&headers)?;
    let payload=json!({"id":body.id,"provider":body.provider,"name":body.name,"setup_token_sha256":format!("{:x}",Sha256::digest(body.setup_token.as_bytes())),"epoch":epoch}).to_string();
    store.run(move|db|{let scope=format!("POST /budgets/{budget}/banking/connections");
    if lookup {return sync::receipt(db,&scope,&key,&payload,None,StatusCode::CREATED);}
    sync::write(db,&scope,&key,&payload,None,StatusCode::CREATED,|db|{
        if cancel{return sync::canceled(db,&budget);}
        crate::lifecycle::require_epoch(db,&budget,epoch.as_deref())?;
        let count:i64=db.query_row("SELECT count(*) FROM bank_connections WHERE budget_id=?1",[&budget],|r|r.get(0))?;
        if count>=100{return Err(Error::invalid("At most 100 connection identities are supported per budget"));}
        let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM bank_connections WHERE budget_id=?1 AND id=?2)",params![budget,body.id],|r|r.get(0))?;
        if exists{return Err(Error::conflict("Connection identity already exists. Replace its credentials explicitly"));}
        let generation=uuid::Uuid::new_v4().to_string();
        db.execute("INSERT INTO bank_connections(budget_id,id,provider,name,generation) VALUES(?1,?2,?3,?4,?5)",params![budget,body.id,body.provider,body.name,generation])?;
        db.execute("INSERT INTO bank_credentials(budget_id,connection_id,generation,setup_token) VALUES(?1,?2,?3,?4)",params![budget,body.id,generation,body.setup_token])?;
        let clock=sync::clock(db,&budget,None)?;
        sync::seed_field_version(db,&budget,"bank_connections",&body.id,"connection",&clock)?;
        sync::mark(db,&budget,"bank_connections",&body.id)?;
        Ok(json!({"response_status":201,"response_body":{"data":connection(db,&budget,&body.id)?}}))
    })}).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Command {
    action: String,
    generation: String,
    setup_token: Option<String>,
}
async fn command(
    State(store): State<Store>,
    Path((b, id)): Path<(String, String)>,
    headers: HeaderMap,
    input: Result<Json<Command>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    models::validate_id(&b)?;
    models::validate_id(&id)?;
    let Json(body) = input.map_err(api::json_error)?;
    if !matches!(
        body.action.as_str(),
        "claim" | "replace" | "remove" | "discover"
    ) {
        return Err(Error::invalid("Unsupported connection action"));
    }
    models::validate_id(&body.generation)?;
    if body.action == "replace" {
        let token = body
            .setup_token
            .as_deref()
            .filter(|v| !v.is_empty() && v.len() <= 8192)
            .ok_or_else(|| Error::invalid("Supply a new setup token"))?;
        transport::setup(token)?;
    } else if body.setup_token.is_some() {
        return Err(Error::invalid(
            "Only credential replacement accepts a setup token",
        ));
    }
    let key = api::idempotency_key(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let mut fingerprint = json!({"action":body.action,"generation":body.generation,"epoch":epoch});
    if let Some(token) = &body.setup_token {
        fingerprint["setup_token_sha256"] =
            json!(format!("{:x}", Sha256::digest(token.as_bytes())));
    }
    let payload = fingerprint.to_string();
    let scope = format!("POST /budgets/{b}/banking/connections/{id}/commands");
    if sync::receipt_only_header(&headers)? {
        return store
            .run(move |db| sync::receipt(db, &scope, &key, &payload, None, StatusCode::OK))
            .await;
    }
    if sync::cancel_header(&headers)? {
        return store
            .run(move |db| {
                sync::write(db, &scope, &key, &payload, None, StatusCode::OK, |db| {
                    let outcome = sync::canceled(db, &b)?;
                    db.execute(
                        "DELETE FROM bank_jobs WHERE budget_id=?1 AND connection_id=?2 AND id=?3",
                        params![b, id, key],
                    )?;
                    Ok(outcome)
                })
            })
            .await;
    }
    if body.action == "discover" {
        return remote::discover(store, b, id, key, payload, epoch, body.generation).await;
    }
    if body.action != "claim" {
        return store.run(move|db|sync::write(db,&scope,&key,&payload,None,StatusCode::OK,|db|{
            crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;
            let current=connection(db,&b,&id)?;
            if current["generation"]!=body.generation || current["deleted"]==true{return Err(Error::conflict("Connection changed. Reload before replacing or removing it"));}
            let generation=uuid::Uuid::new_v4().to_string();
            db.execute("DELETE FROM bank_jobs WHERE budget_id=?1 AND connection_id=?2",params![b,id])?;
            db.execute("DELETE FROM bank_credentials WHERE budget_id=?1 AND connection_id=?2",params![b,id])?;
            if body.action=="replace" {
                db.execute("INSERT INTO bank_credentials(budget_id,connection_id,generation,setup_token) VALUES(?1,?2,?3,?4)",params![b,id,generation,body.setup_token])?;
            }
            db.execute("UPDATE bank_connections SET generation=?3,state=?4,tombstone=?5,error_code=NULL,message=NULL,revision=revision+1 WHERE budget_id=?1 AND id=?2",params![b,id,generation,if body.action=="replace"{"claim_pending"}else{"credentials_missing"},body.action=="remove"])?;
            sync::mark(db,&b,"bank_connections",&id)?;
            Ok(json!({"response_status":200,"response_body":{"data":connection(db,&b,&id)?}}))
        })).await;
    }
    // Probe before taking the one-use token. Replays must never repeat transport.
    let (bb, ii, ss, kk, pp, ee, gg) = (
        b.clone(),
        id.clone(),
        scope.clone(),
        key.clone(),
        payload.clone(),
        epoch.clone(),
        body.generation.clone(),
    );
    let prepared=store.run(move|db|{
        if let Some(result)=crate::store::receipt(db,&ss,&kk,&pp)? {return Ok(Err(result));}
        let tx=db.transaction()?;
        crate::lifecycle::require_epoch(&tx,&bb,ee.as_deref())?;
        let current=connection(&tx,&bb,&ii)?;
        if current["generation"]!=gg || current["deleted"]==true{return Err(Error::conflict("Connection changed. Reload before claiming"));}
        if current["state"]!="claim_pending" {return Err(Error::conflict("This claim may already have been used. Replace the setup token explicitly"));}
        let token:String=tx.query_row("SELECT setup_token FROM bank_credentials WHERE budget_id=?1 AND connection_id=?2 AND generation=?3",params![bb,ii,gg],|r|r.get(0)).optional()?.ok_or_else(||Error::conflict("Credentials are missing. Supply a new setup token"))?;
        let permit=transport::permit()?;
        tx.execute("UPDATE bank_connections SET state='claim_inflight',revision=revision+1 WHERE budget_id=?1 AND id=?2",params![bb,ii])?;
        sync::mark(&tx,&bb,"bank_connections",&ii)?;
        tx.commit()?;
        Ok(Ok((token,permit)))
    }).await?;
    let (token, permit) = match prepared {
        Ok(value) => value,
        Err(_) => {
            return store
                .run(move |db| sync::receipt(db, &scope, &key, &payload, None, StatusCode::OK))
                .await;
        }
    };
    let result = match tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            transport::claim(&token)
        }),
    )
    .await
    {
        Ok(Ok(result)) => result,
        _ => Err(transport::failure(
            "claim_uncertain",
            "Claim transport was interrupted. Replace the setup token",
        )),
    };
    store.run(move|db|sync::write(db,&scope,&key,&payload,None,StatusCode::OK,|db|{
        crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;
        let current=connection(db,&b,&id)?;
        if current["generation"]!=body.generation || current["deleted"]==true{return Err(Error::conflict("Connection changed while the provider was responding"));}
        let (state,code,message)=match result {Ok(access)=>{
            db.execute("UPDATE bank_credentials SET setup_token=NULL,access_url=?3 WHERE budget_id=?1 AND connection_id=?2",params![b,id,access])?;
            ("connected",None,None)
        },Err(error)=>("claim_failed",Some(error.code),Some(error.message))};
        db.execute("UPDATE bank_connections SET state=?3,error_code=?4,message=?5,revision=revision+1 WHERE budget_id=?1 AND id=?2",params![b,id,state,code,message])?;
        sync::mark(db,&b,"bank_connections",&id)?;
        Ok(json!({"response_status":200,"response_body":{"data":connection(db,&b,&id)?}}))
    })).await
}

pub(crate) fn remote_account(db: &Connection, b: &str, id: &str) -> Result<Value, Error> {
    records::remote(db, b, id)
}
pub(crate) fn archive_tables() -> Vec<(&'static str, Vec<&'static str>)> {
    records::tables()
}
