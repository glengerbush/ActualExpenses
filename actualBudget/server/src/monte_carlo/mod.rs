//! Server-owned simulations. Financial data is captured briefly, then computation
//! runs without the store lock; one bounded worker serves the whole router.
mod input;
mod runtime;
use crate::{
    concurrency,
    error::Error,
    store::{self, Store},
};
use axum::{
    Extension, Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    http::StatusCode,
    routing::{delete, post},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u8,
    request_id: String,
    config: Value,
    #[serde(default = "seed")]
    seed: u32,
    #[serde(default, rename = "captureRunDetail")]
    capture_run: Option<u32>,
    #[serde(default, rename = "deflateToTodaysMoney")]
    deflate: bool,
}
fn seed() -> u32 {
    1234
}
type JobKey = (String, String);
#[derive(Clone)]
struct Service {
    jobs: Arc<Mutex<HashMap<JobKey, Arc<AtomicBool>>>>,
    worker: Arc<Semaphore>,
    cache: Arc<Mutex<crate::reports::ReportCache>>,
}
struct Job {
    service: Service,
    key: JobKey,
    cancel: Arc<AtomicBool>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Ok(mut jobs) = self.service.jobs.lock() {
            jobs.remove(&self.key);
        }
    }
}
impl Service {
    fn begin(&self, key: JobKey) -> Result<(Job, OwnedSemaphorePermit), Error> {
        let mut jobs = self.jobs.lock().map_err(Error::internal)?;
        if jobs.contains_key(&key) {
            return Err(Error::conflict("Simulation request is already running"));
        }
        let permit = self.worker.clone().try_acquire_owned().map_err(|_| {
            Error::new(
                StatusCode::TOO_MANY_REQUESTS,
                "simulation_busy",
                "Another simulation is running; retry after it completes or is cancelled",
            )
        })?;
        let cancel = Arc::new(AtomicBool::new(false));
        jobs.insert(key.clone(), cancel.clone());
        Ok((
            Job {
                service: self.clone(),
                key,
                cancel,
            },
            permit,
        ))
    }
}
fn cancelled() -> Error {
    Error::new(
        StatusCode::CONFLICT,
        "simulation_cancelled",
        "Monte Carlo simulation cancelled",
    )
}
pub(crate) fn router() -> Router<Store> {
    Router::new()
        .route("/api/v1/budgets/{budget}/reports/monte-carlo", post(run))
        .route(
            "/api/v1/budgets/{budget}/reports/monte-carlo/{request_id}",
            delete(cancel).get(status),
        )
        .layer(Extension(Service {
            jobs: Default::default(),
            worker: Arc::new(Semaphore::new(1)),
            cache: Default::default(),
        }))
}
async fn status(
    Path((budget, id)): Path<(String, String)>,
    Extension(service): Extension<Service>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&budget)?;
    crate::models::validate_id(&id)?;
    let jobs = service.jobs.lock().map_err(Error::internal)?;
    let flag = jobs.get(&(budget, id.clone())).ok_or_else(Error::missing)?;
    Ok(Json(
        json!({"data":{"request_id":id,"status":if flag.load(Ordering::Relaxed) {"cancelling"} else {"running"}}}),
    ))
}
async fn cancel(
    Path((budget, id)): Path<(String, String)>,
    Extension(service): Extension<Service>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&budget)?;
    crate::models::validate_id(&id)?;
    let jobs = service.jobs.lock().map_err(Error::internal)?;
    let flag = jobs.get(&(budget, id.clone())).ok_or_else(Error::missing)?;
    flag.store(true, Ordering::Relaxed);
    Ok(Json(
        json!({"data":{"request_id":id,"cancellation_requested":true}}),
    ))
}
async fn run(
    State(store): State<Store>,
    Path(budget): Path<String>,
    Extension(service): Extension<Service>,
    body: Result<Json<Request>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&budget)?;
    let Json(request) = body.map_err(crate::api::json_error)?;
    crate::models::validate_id(&request.request_id)?;
    if request.version != 1 {
        return Err(Error::invalid("Unsupported Monte Carlo version"));
    }
    input::validate(&request.config)?;
    if request.capture_run.is_some_and(|index| index >= 10000) {
        return Err(Error::invalid("Captured run must be below10000"));
    }
    let (job, permit) = service.begin((budget.clone(), request.request_id.clone()))?;
    let captured_budget = budget.clone();
    let mut config = request.config.clone();
    let (config, snapshot, epoch, warnings) = store.run(move |db| {
        let tx = db.transaction()?;
        store::require_budget(&tx,&captured_budget)?;
        let preferences = crate::preferences::get_preferences(&tx,&captured_budget)?;
        if preferences["experimental"]["monteCarloReport"] != true {
            return Err(Error::new(StatusCode::CONFLICT,"feature_disabled","Enable Monte Carlo in experimental settings before calculating"));
        }
        let mut warnings = vec![];
        if let Some(pots) = config.get_mut("pots").and_then(Value::as_array_mut) {
            for pot in pots {
                if let Some(id) = pot["accountId"].as_str() {
                    crate::models::validate_id(id)?;
                    let balance: Option<i64> = tx.query_row("SELECT COALESCE(t.total,0) FROM accounts a LEFT JOIN ledger_totals t ON t.budget_id=a.budget_id AND t.account_id=a.id WHERE a.budget_id=?1 AND a.id=?2 AND a.tombstone=0",params![captured_budget,id],|row|row.get(0)).optional()?;
                    if let Some(balance) = balance { pot["startingBalance"] = json!(balance.max(0)); }
                    else { warnings.push(json!({"code":"account_unavailable","pot_id":pot["id"],"message":"Linked account unavailable; using the saved starting balance"})); }
                }
            }
        }
        Ok((config, concurrency::cursor(&tx,&captured_budget)?, crate::lifecycle::epoch(&tx,&captured_budget)?, warnings))
    }).await?;
    if job.cancel.load(Ordering::Relaxed) {
        return Err(cancelled());
    }
    let input = json!({"config":config,"seed":request.seed,"captureRunDetail":request.capture_run,"deflateToTodaysMoney":request.deflate});
    let key = format!("{budget}:{epoch}:{input}");
    let revision = format!("mc1:{:x}", Sha256::digest(key.as_bytes()));
    let cached = service.cache.lock().map_err(Error::internal)?.get(&key);
    let hit = cached.is_some();
    let value = if let Some(cached) = cached {
        cached
    } else {
        let flag = job.cancel.clone();
        let result = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            runtime::calculate(&input, flag)
        })
        .await
        .map_err(Error::internal)?;
        if job.cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        let value = result?;
        service
            .cache
            .lock()
            .map_err(Error::internal)?
            .put(key, value.clone());
        value
    };
    if job.cancel.load(Ordering::Relaxed) {
        return Err(cancelled());
    }
    Ok(Json(
        json!({"data":{"config":value["config"],"result":value["result"],"snapshot":snapshot,"epoch":epoch,"recorded":false,"cache":{"hit":hit,"input_revision":revision},"warnings":warnings,"assumptions":["Experimental planning estimates; outcomes are not guaranteed.","Normal-model pot returns are independent. Historical returns use the pinned US1928–2025 series.","Spending precedes annual investment returns; tax and fee assumptions are user inputs."]}}),
    ))
}
