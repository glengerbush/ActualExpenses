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
enum Prepared {
    Receipt,
    Captured(Result<Value, Error>),
    Fetch(String, tokio::sync::OwnedSemaphorePermit),
}
fn captured(
    response: Option<String>,
    code: Option<String>,
    message: Option<String>,
) -> Result<Value, Error> {
    if let Some(code) = code {
        return Err(Error::new(
            StatusCode::BAD_GATEWAY,
            match code.as_str() {
                "provider_revoked" => "provider_revoked",
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
        ));
    }
    serde_json::from_str(
        &response.ok_or_else(|| Error::internal("Missing captured provider response"))?,
    )
    .map_err(Error::internal)
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
    let epoch =
        Some(epoch.ok_or_else(|| {
            Error::invalid("Provider downloads require the original budget epoch")
        })?);
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
        crate::lifecycle::require_epoch(&tx,&bb,ee.as_deref())?;
        let current=connection(&tx,&bb,&ii)?;
        if current["generation"]!=gg || current["deleted"]==true || current["state"]!="connected" {return Err(Error::conflict("Reconnect this provider before discovering accounts"));}
        let timestamp=now()?;
        let job=tx.query_row("SELECT generation,epoch,fingerprint,phase,response,error_code,error_message,started_at FROM bank_jobs WHERE budget_id=?1 AND connection_id=?2 AND id=?3",params![bb,ii,kk],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,Option<String>>(4)?,r.get::<_,Option<String>>(5)?,r.get::<_,Option<String>>(6)?,r.get::<_,i64>(7)?))).optional()?;
        if let Some((g,e,p,phase,response,code,message,started))=job{
            if g!=gg || Some(e.as_str())!=ee.as_deref() || p!=pp{return Err(Error::conflict("The captured provider request has another identity or generation"));}
            if phase=="captured" {return Ok(Prepared::Captured(captured(response,code,message)));}
            if timestamp-started<35{return Err(Error::conflict("This provider request is still finishing. Retry its original identity shortly"));}
        }
        tx.execute("DELETE FROM bank_request_log WHERE started_at<?1",[timestamp-86400])?;
        let requests:i64=tx.query_row("SELECT count(*) FROM bank_request_log WHERE budget_id=?1 AND connection_id=?2",params![bb,ii],|r|r.get(0))?;
        if requests>=24{return Err(Error::new(StatusCode::TOO_MANY_REQUESTS,"provider_rate_limited","This connection has used its 24 requests in the last 24 hours. Wait before refreshing"));}
        let size:i64=tx.query_row("SELECT COALESCE(SUM(length(response)),0) FROM bank_jobs WHERE budget_id=?1",[&bb],|r|r.get(0))?;
        if size>=64*1024*1024{return Err(Error::conflict("Finish or cancel captured provider requests before downloading more data"));}
        let access:String=tx.query_row("SELECT access_url FROM bank_credentials WHERE budget_id=?1 AND connection_id=?2 AND generation=?3",params![bb,ii,gg],|r|r.get(0)).optional()?.ok_or_else(||Error::conflict("Credentials are missing. Replace the setup token"))?;
        let permit=transport::permit()?;
        tx.execute("INSERT INTO bank_jobs(budget_id,connection_id,id,generation,epoch,fingerprint,phase,started_at) VALUES(?1,?2,?3,?4,?5,?6,'fetching',?7) ON CONFLICT(budget_id,connection_id,id) DO UPDATE SET started_at=excluded.started_at",params![bb,ii,kk,gg,ee,pp,timestamp])?;
        tx.execute("INSERT INTO bank_request_log(budget_id,connection_id,started_at) VALUES(?1,?2,?3)",params![bb,ii,timestamp])?;
        tx.commit()?;
        Ok(Prepared::Fetch(access,permit))
    }).await?;
    let result = match ready {
        Prepared::Receipt => {
            return store
                .run(move |db| sync::receipt(db, &scope, &key, &payload, None, StatusCode::OK))
                .await;
        }
        Prepared::Captured(value) => value,
        Prepared::Fetch(access, permit) => {
            let result = match tokio::time::timeout(
                std::time::Duration::from_secs(30),
                tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    transport::accounts(&access)
                }),
            )
            .await
            {
                Ok(Ok(result)) => result,
                _ => Err(transport::failure(
                    "provider_unavailable",
                    "Provider retrieval timed out. Retry later",
                )),
            };
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
                    tx.commit()?;return Ok(result);
                }
                crate::lifecycle::require_epoch(&tx,&bb,ee.as_deref())?;
                if connection(&tx,&bb,&ii)?["generation"]!=gg{return Err(Error::conflict("Credentials changed during provider retrieval"));}
                let(response,code,message)=match &result{Ok(value)=>(Some(value.to_string()),None,None),Err(e)=>(None,Some(e.code.to_owned()),Some(e.message.clone()))};
                let existing:i64=tx.query_row("SELECT COALESCE(SUM(length(response)),0) FROM bank_jobs WHERE budget_id=?1 AND NOT(connection_id=?2 AND id=?3)",params![bb,ii,kk],|r|r.get(0))?;
                if existing+response.as_ref().map_or(0,|s|s.len()) as i64>64*1024*1024{return Err(Error::conflict("Captured provider data exceeds the 64 MiB recovery limit. Finish existing requests first"));}
                tx.execute("UPDATE bank_jobs SET phase='captured',response=?4,error_code=?5,error_message=?6 WHERE budget_id=?1 AND connection_id=?2 AND id=?3",params![bb,ii,kk,response,code,message])?;tx.commit()?;Ok(result)
            }).await?
        }
    };
    store.run(move|db|sync::write(db,&scope,&key,&payload,None,StatusCode::OK,|db|{
        crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;
        let current=connection(db,&b,&id)?;
        if current["generation"]!=generation || current["deleted"]==true{return Err(Error::conflict("Connection changed before captured accounts could be saved"));}
        let (accounts,errors)=match result {
            Ok(raw)=>match super::records::install_remote(db,&b,&id,&generation,&raw){Ok(value)=>value,Err(e) if e.status==StatusCode::INTERNAL_SERVER_ERROR=>return Err(e),Err(e)=>(vec![],vec![json!({"code":e.code,"message":e.message})])},
            Err(e)=>{db.execute("UPDATE bank_connections SET error_code=?3,message=?4,state=CASE WHEN ?3='provider_revoked' THEN 'credentials_missing' ELSE state END WHERE budget_id=?1 AND id=?2",params![b,id,e.code,e.message])?;(vec![],vec![json!({"code":e.code,"message":e.message})])}
        };
        db.execute("UPDATE bank_connections SET error_code=?3,message=?4,last_success=CASE WHEN ?3 IS NULL THEN strftime('%Y-%m-%dT%H:%M:%fZ','now') ELSE last_success END,revision=revision+1 WHERE budget_id=?1 AND id=?2",params![b,id,errors.first().and_then(|v|v["code"].as_str()),errors.first().and_then(|v|v["message"].as_str())])?;
        sync::mark(db,&b,"bank_connections",&id)?;
        db.execute("DELETE FROM bank_jobs WHERE budget_id=?1 AND connection_id=?2 AND id=?3",params![b,id,key])?;
        Ok(json!({"response_status":200,"response_body":{"data":{"connection":connection(db,&b,&id)?,"accounts":accounts,"errors":errors}}}))
    })).await
}
