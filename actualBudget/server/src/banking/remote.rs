//! Durable, read-only provider retrieval. A captured response is never refetched
//! under the same operation identity; financial consumers share this boundary.
use super::*;
use std::time::{SystemTime, UNIX_EPOCH};
fn now() -> Result<i64, Error> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(Error::internal)?
        .as_secs() as i64)
}
enum Access {
    Simplefin(String),
    GoCardless(gocardless::DownloadCredentials),
}
enum Prepared {
    Receipt,
    Captured(Capture),
    Fetch(Access, tokio::sync::OwnedSemaphorePermit, i64),
}
pub(super) struct Capture {
    pub result: Result<Value, Error>,
    pub revision: i64,
}
fn captured(
    response: Option<String>,
    code: Option<String>,
    message: Option<String>,
) -> Result<Capture, Error> {
    let envelope:Value=serde_json::from_str(response.as_deref().ok_or_else(||Error::conflict("Captured provider data lacks its ordering witness. Cancel this original request before refreshing again"))?).map_err(Error::internal)?;
    let revision=envelope["revision"].as_i64().filter(|_|envelope["bank_capture_version"]==1).ok_or_else(||Error::conflict("Captured provider data lacks its ordering witness. Cancel this original request before refreshing again"))?;
    let result = if let Some(code) = code {
        Err(Error::new(
            StatusCode::BAD_GATEWAY,
            match code.as_str() {
                "provider_revoked" => "provider_revoked",
                "provider_consent_expired" => "provider_consent_expired",
                "provider_consent_suspended" => "provider_consent_suspended",
                "provider_credentials_invalid" => "provider_credentials_invalid",
                "provider_access_denied" => "provider_access_denied",
                "provider_resource_missing" => "provider_resource_missing",
                "provider_bank_unsupported" => "provider_bank_unsupported",
                "provider_redirect_refused" => "provider_redirect_refused",
                "provider_request_rejected" => "provider_request_rejected",
                "provider_rate_limited" => "provider_rate_limited",
                "provider_payment_required" => "provider_payment_required",
                "provider_invalid_response" => "provider_invalid_response",
                "provider_response_limit" => "provider_response_limit",
                "provider_redirect_limit" => "provider_redirect_limit",
                "provider_pagination_unsupported" => "provider_pagination_unsupported",
                "invalid_request" => "invalid_request",
                _ => "provider_unavailable",
            },
            message.unwrap_or_else(|| "Provider retrieval failed".into()),
        ))
    } else {
        Ok(envelope["data"].clone())
    };
    Ok(Capture { result, revision })
}
#[derive(Clone)]
pub(super) struct Request {
    pub budget: String,
    pub connection: String,
    pub key: String,
    pub payload: String,
    pub epoch: Option<String>,
    pub generation: String,
    pub proposed: Option<String>,
    pub owner: Option<crate::history::Owner>,
}
impl Request {
    pub fn scope(&self) -> String {
        format!(
            "POST /budgets/{}/banking/connections/{}/commands",
            self.budget, self.connection
        )
    }
    pub fn fingerprint(&self) -> String {
        sync::receipt_payload(
            &crate::history::payload(&self.payload, self.owner.as_ref()),
            self.proposed.as_deref(),
        )
    }
    pub async fn receipt(self, store: Store) -> Result<(StatusCode, Json<Value>), Error> {
        store
            .run(move |db| {
                sync::receipt_owned(
                    db,
                    &self.scope(),
                    &self.key,
                    &self.payload,
                    self.proposed.as_deref(),
                    StatusCode::OK,
                    self.owner.as_ref(),
                )
            })
            .await
    }
    pub async fn cancel(self, store: Store) -> Result<(StatusCode, Json<Value>), Error> {
        store.run(move|db|sync::write_owned(db,&self.scope(),&self.key,&self.payload,self.proposed.as_deref(),StatusCode::OK,self.owner.as_ref(),|db|{
    guard_pending(db,&self.budget,&self.connection,&self.key,&self.fingerprint())?;
    let outcome=sync::canceled(db,&self.budget)?;db.execute("DELETE FROM bank_jobs WHERE budget_id=?1 AND connection_id=?2 AND id=?3",params![self.budget,self.connection,self.key])?;Ok(outcome)
 })).await
    }
}

pub(super) async fn capture(
    store: Store,
    request: Request,
    download: transport::Download,
) -> Result<Option<Capture>, Error> {
    if request.epoch.is_none() {
        return Err(Error::invalid(
            "Provider downloads require the original budget epoch",
        ));
    }
    let Request {
        budget: b,
        connection: id,
        key,
        epoch,
        generation,
        ..
    } = request.clone();
    let payload = request.fingerprint();
    let scope = format!("POST /budgets/{b}/banking/connections/{id}/commands");
    let (bb, ii, ss, kk, pp, ee, gg) = (
        b.clone(),
        id.clone(),
        scope.clone(),
        key.clone(),
        payload.clone(),
        epoch.clone(),
        generation.clone(),
    );
    let ready=store.run(move|db|{
        if crate::store::receipt(db,&ss,&kk,&pp)?.is_some(){return Ok(Prepared::Receipt);}
        let tx=db.transaction()?;
        guard_pending(&tx,&bb,&ii,&kk,&pp)?;
        crate::lifecycle::require_epoch(&tx,&bb,ee.as_deref())?;
        let current=connection(&tx,&bb,&ii)?;
        if current["generation"]!=gg || current["deleted"]==true || current["state"]!="connected" {return Err(Error::conflict("Reconnect this provider before discovering accounts"));}
        let timestamp=now()?;
        let job=tx.query_row("SELECT generation,epoch,fingerprint,phase,response,error_code,error_message,started_at FROM bank_jobs WHERE budget_id=?1 AND connection_id=?2 AND id=?3",params![bb,ii,kk],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,Option<String>>(4)?,r.get::<_,Option<String>>(5)?,r.get::<_,Option<String>>(6)?,r.get::<_,i64>(7)?))).optional()?;
        if let Some((g,e,p,phase,response,code,message,started))=job{
            if g!=gg || Some(e.as_str())!=ee.as_deref() || p!=pp{return Err(Error::conflict("The captured provider request has another identity or generation"));}
            if phase=="captured" {return Ok(Prepared::Captured(captured(response,code,message)?));}
            if timestamp-started<35{return Err(Error::conflict("This provider request is still finishing. Retry its original identity shortly"));}
        }
        tx.execute("DELETE FROM bank_request_log WHERE started_at<?1",[timestamp-86400])?;
        let requests:i64=tx.query_row("SELECT count(*) FROM bank_request_log WHERE budget_id=?1 AND connection_id=?2",params![bb,ii],|r|r.get(0))?;
        if current["provider"]=="simplefin" && requests>=24{return Err(Error::new(StatusCode::TOO_MANY_REQUESTS,"provider_rate_limited","This connection has used its 24 requests in the last 24 hours. Wait before refreshing"));}
        let size:i64=tx.query_row("SELECT COALESCE(SUM(length(CAST(response AS BLOB))),0) FROM bank_jobs WHERE budget_id=?1",[&bb],|r|r.get(0))?;
        if size>=64*1024*1024{return Err(Error::conflict("Finish or cancel captured provider requests before downloading more data"));}
        let access=if current["provider"]=="gocardless" {Access::GoCardless(gocardless::prepare_download(&tx,&bb,&ii,&gg)?)}else{Access::Simplefin(tx.query_row("SELECT access_url FROM bank_credentials WHERE budget_id=?1 AND connection_id=?2 AND generation=?3",params![bb,ii,gg],|r|r.get(0)).optional()?.ok_or_else(||Error::conflict("Credentials are missing. Replace the setup token"))?)};
        let permit=transport::permit()?;
        tx.execute("INSERT INTO bank_jobs(budget_id,connection_id,id,generation,epoch,fingerprint,phase,started_at) VALUES(?1,?2,?3,?4,?5,?6,'fetching',?7) ON CONFLICT(budget_id,connection_id,id) DO UPDATE SET started_at=excluded.started_at",params![bb,ii,kk,gg,ee,pp,timestamp])?;
        if current["provider"]=="simplefin" {tx.execute("INSERT INTO bank_request_log(budget_id,connection_id,started_at) VALUES(?1,?2,?3)",params![bb,ii,timestamp])?;}
        tx.commit()?;
        Ok(Prepared::Fetch(access,permit,current["revision"].as_i64().unwrap()))
    }).await?;
    let result = match ready {
        Prepared::Receipt => return Ok(None),
        Prepared::Captured(value) => return Ok(Some(value)),
        Prepared::Fetch(access, permit, revision) => {
            let (result, tokens) = match tokio::time::timeout(
                std::time::Duration::from_secs(30),
                tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    match access {
                        Access::Simplefin(access) => (transport::accounts(&access, download), None),
                        Access::GoCardless(access) => {
                            let (result, tokens) = gocardless::retrieve(access, download);
                            (result, Some(tokens))
                        }
                    }
                }),
            )
            .await
            {
                Ok(Ok(result)) => result,
                _ => (
                    Err(transport::failure(
                        "provider_unavailable",
                        "Provider retrieval timed out. Retry later",
                    )),
                    None,
                ),
            };
            if let Some(tokens) = &tokens {
                gocardless::observe_download_quota(store.clone(), tokens).await?;
            }
            let (bb, ii, kk, ee, gg) = (
                b.clone(),
                id.clone(),
                key.clone(),
                epoch.clone(),
                generation.clone(),
            );
            let (ss, pp) = (scope.clone(), payload.clone());
            store.run(move|db|{
                let tx=db.transaction()?;
                // Cancellation/another committed outcome wins before any late raw
                // response can consume private recovery capacity or touch metadata.
                if crate::store::receipt(&tx,&ss,&kk,&pp)?.is_some(){
                    tx.execute("DELETE FROM bank_jobs WHERE budget_id=?1 AND connection_id=?2 AND id=?3",params![bb,ii,kk])?;
                    tx.commit()?;return Ok(None);
                }
                crate::lifecycle::require_epoch(&tx,&bb,ee.as_deref())?;
                if connection(&tx,&bb,&ii)?["generation"]!=gg{return Err(Error::conflict("Credentials changed during provider retrieval"));}
                if let Some(tokens)=&tokens {gocardless::save_download(&tx,&bb,&ii,&gg,tokens)?;}
                let(response,code,message)=match &result{Ok(value)=>(value.clone(),None,None),Err(e)=>(Value::Null,Some(e.code.to_owned()),Some(e.message.clone()))};
                let response=Some(json!({"bank_capture_version":1,"revision":revision,"data":response}).to_string());
                let existing:i64=tx.query_row("SELECT COALESCE(SUM(length(CAST(response AS BLOB))),0) FROM bank_jobs WHERE budget_id=?1 AND NOT(connection_id=?2 AND id=?3)",params![bb,ii,kk],|r|r.get(0))?;
                if existing+response.as_ref().map_or(0,|s|s.len()) as i64>64*1024*1024{return Err(Error::conflict("Captured provider data exceeds the 64 MiB recovery limit. Finish existing requests first"));}
                tx.execute("UPDATE bank_jobs SET phase='captured',response=?4,error_code=?5,error_message=?6 WHERE budget_id=?1 AND connection_id=?2 AND id=?3",params![bb,ii,kk,response,code,message])?;tx.commit()?;Ok(Some(Capture{result,revision}))
            }).await?
        }
    };
    Ok(result)
}

pub(super) async fn discover(
    store: Store,
    b: String,
    id: String,
    key: String,
    payload: String,
    epoch: Option<String>,
    generation: String,
) -> Result<(StatusCode, Json<Value>), Error> {
    let request = Request {
        budget: b.clone(),
        connection: id.clone(),
        key: key.clone(),
        payload: payload.clone(),
        epoch: epoch.clone(),
        generation: generation.clone(),
        proposed: None,
        owner: None,
    };
    let scope = request.scope();
    let Some(capture) = capture(
        store.clone(),
        request.clone(),
        transport::Download::Balances,
    )
    .await?
    else {
        return request.receipt(store).await;
    };
    store.run(move|db|sync::write(db,&scope,&key,&payload,None,StatusCode::OK,|db|{
        crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;
        let current=connection(db,&b,&id)?;
        if current["generation"]!=generation || current["deleted"]==true{return Err(Error::conflict("Connection changed before captured accounts could be saved"));}
        if current["revision"]!=capture.revision{return Err(Error::new(StatusCode::CONFLICT,"provider_capture_stale","Newer provider metadata was saved. Cancel this captured request and refresh again"));}
        let (accounts,errors)=match capture.result {
            Ok(raw)=>match super::records::install_remote(db,&b,&id,&generation,&raw,true){Ok(value)=>value,Err(e) if e.status==StatusCode::INTERNAL_SERVER_ERROR=>return Err(e),Err(e)=>(vec![],vec![json!({"code":e.code,"message":e.message})])},
            Err(e)=>{db.execute("UPDATE bank_connections SET error_code=?3,message=?4,state=CASE WHEN ?3 IN('provider_revoked','provider_credentials_invalid') THEN 'credentials_missing' WHEN ?3 IN('provider_consent_expired','provider_consent_suspended') THEN 'claim_failed' ELSE state END WHERE budget_id=?1 AND id=?2",params![b,id,e.code,e.message])?;(vec![],vec![json!({"code":e.code,"message":e.message})])}
        };
        db.execute("UPDATE bank_connections SET error_code=?3,message=?4,last_success=CASE WHEN ?3 IS NULL THEN strftime('%Y-%m-%dT%H:%M:%fZ','now') ELSE last_success END,revision=revision+1 WHERE budget_id=?1 AND id=?2",params![b,id,errors.first().and_then(|v|v["code"].as_str()),errors.first().and_then(|v|v["message"].as_str())])?;
        sync::mark(db,&b,"bank_connections",&id)?;
        db.execute("DELETE FROM bank_jobs WHERE budget_id=?1 AND connection_id=?2 AND id=?3",params![b,id,key])?;
        Ok(json!({"response_status":200,"response_body":{"data":{"connection":connection(db,&b,&id)?,"accounts":accounts,"errors":errors}}}))
    })).await
}
