//! Retained bank files, read-only review, and receipt-owned ledger application.
mod matching;
mod settings;
mod storage;
mod title;
pub(crate) use storage::{
    archive_tables, convert, decorate_many, enrich, import_clock, imported_preference, install,
    record, validate,
};
mod parse;
use crate::{concurrency as sync, error::Error, store::Store};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{
        DefaultBodyLimit, Path, Query, State,
        rejection::{BytesRejection, JsonRejection},
    },
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
const MAX_FILE: usize = 16 * 1024 * 1024;
pub fn router() -> Router<Store> {
    Router::new()
        .route(
            "/api/v1/budgets/{budget}/accounts/{account}/transaction-import-settings",
            get(settings::read),
        )
        .route(
            "/api/v1/budgets/{budget}/accounts/{account}/transaction-imports",
            post(upload).layer(DefaultBodyLimit::max(MAX_FILE)),
        )
        .route(
            "/api/v1/budgets/{budget}/transaction-imports/{id}/original",
            get(original),
        )
        .route(
            "/api/v1/budgets/{budget}/transaction-imports/{id}",
            get(read),
        )
        .route(
            "/api/v1/budgets/{budget}/transaction-imports/{id}/parse",
            post(parse_file),
        )
        .route(
            "/api/v1/budgets/{budget}/transaction-imports/{id}/preview",
            post(preview),
        )
        .route(
            "/api/v1/budgets/{budget}/transaction-imports/{id}/commit",
            post(commit),
        )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Upload {
    filename: String,
}
async fn upload(
    State(store): State<Store>,
    Path((b, a)): Path<(String, String)>,
    Query(query): Query<Upload>,
    headers: HeaderMap,
    input: Result<Bytes, BytesRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    crate::models::validate_id(&b)?;
    crate::models::validate_id(&a)?;
    crate::models::validate_name(&query.filename)?;
    let bytes = input.map_err(|_| Error::invalid("Upload a file at most16 MiB"))?;
    if bytes.is_empty() {
        return Err(Error::invalid("The import file is empty"));
    }
    let key = crate::api::idempotency_key(&headers)?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let payload = json!({"filename":query.filename,"sha256":hash}).to_string();
    store.run(move|db|{sync::write(db,&format!("POST /budgets/{b}/accounts/{a}/transaction-imports"),&key,&payload,None,StatusCode::CREATED,|db|{crate::ledger::account_exists(db,&b,&a)?;if !query.filename.to_ascii_lowercase().ends_with(".csv")&&!query.filename.to_ascii_lowercase().ends_with(".tsv"){return Err(Error::invalid("Choose a CSV or TSV file"));}let id=uuid::Uuid::new_v4().to_string();db.execute("INSERT INTO transaction_import_files(budget_id,id,account_id,filename,sha256,original) VALUES(?1,?2,?3,?4,?5,?6)",params![b,id,a,query.filename,hash,bytes.as_ref()])?;Ok(json!({"id":id,"account_id":a,"filename":query.filename,"sha256":hash,"bytes":bytes.len()}))})}).await
}
fn file(db: &Connection, b: &str, id: &str) -> Result<(String, String, Vec<u8>), Error> {
    crate::store::require_budget(db, b)?;
    db.query_row("SELECT account_id,filename,original FROM transaction_import_files WHERE budget_id=?1 AND id=?2",params![b,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?.ok_or_else(Error::missing)
}
async fn read(
    State(store): State<Store>,
    Path((b, id)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    crate::models::validate_id(&id)?;
    store.run(move|db|{let(a,filename,bytes)=file(db,&b,&id)?;Ok(json!({"data":{"id":id,"account_id":a,"filename":filename,"bytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes))}}))}).await.map(Json)
}
async fn original(
    State(store): State<Store>,
    Path((b, id)): Path<(String, String)>,
) -> Result<impl axum::response::IntoResponse, Error> {
    crate::models::validate_id(&b)?;
    crate::models::validate_id(&id)?;
    let (filename, bytes) = store
        .run(move |db| {
            let (_, filename, bytes) = file(db, &b, &id)?;
            Ok((filename, bytes))
        })
        .await?;
    let encoded = filename
        .as_bytes()
        .iter()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() {
                (*byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect::<String>();
    Ok((
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/octet-stream".to_owned(),
            ),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename*=UTF-8''{encoded}"),
            ),
            (axum::http::header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        bytes,
    ))
}

async fn parse_file(
    State(store): State<Store>,
    Path((b, id)): Path<(String, String)>,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    crate::models::validate_id(&id)?;
    let Json(body) = input.map_err(crate::api::json_error)?;
    if body
        .as_object()
        .is_none_or(|o| o.keys().any(|k| k != "options"))
    {
        return Err(Error::invalid(
            "Only file options belong in this parse request",
        ));
    }
    let raw = body.get("options").cloned().unwrap_or(json!({}));
    let mut options: parse::Options = serde_json::from_value(raw.clone())
        .map_err(|e| Error::invalid(format!("Invalid file options: {e}")))?;
    // Raw parsing has the source parser's defaults; the mapping workflow sends
    // explicit captured modal defaults instead.
    if raw.get("has_header_row").is_none() {
        options.has_header_row = false;
    }
    if raw.get("import_notes").is_none() {
        options.import_notes = false;
    }
    store.run(move|db|{let(_,filename,bytes)=file(db,&b,&id)?;let result=parse::csv(&bytes,&options);Ok(json!({"data":match result{Ok(rows)=>json!({"transactions":rows,"errors":[],"format":if filename.to_lowercase().ends_with(".tsv"){"tsv"}else{"csv"}}),Err(e)=>json!({"transactions":[],"errors":[{"message":e.message}]})}}))}).await.map(Json)
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Choice {
    row: usize,
    action: String,
    match_id: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Review {
    snapshot: String,
    #[serde(default)]
    mapping: parse::Mapping,
    #[serde(default)]
    options: parse::Options,
    #[serde(default)]
    rows: Option<Vec<Choice>>,
    #[serde(default)]
    remember: bool,
}
fn identity(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    let mut bytes: [u8; 16] = hasher.finalize()[..16].try_into().unwrap();
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    uuid::Uuid::from_bytes(bytes).to_string()
}
fn plan(
    db: &Connection,
    b: &str,
    id: &str,
    body: &Review,
) -> Result<(Value, crate::rules::Work), Error> {
    sync::check_snapshot(db, b, Some(&body.snapshot))?;
    let (account, filename, bytes) = file(db, b, id)?;
    crate::ledger::account_exists(db, b, &account)?;
    let parsed = parse::csv(&bytes, &body.options)?;
    let mut rows = vec![];
    let mut errors = vec![];

    let mut payees = BTreeMap::new();
    let mut q =
        db.prepare("SELECT id,name FROM payees WHERE budget_id=?1 AND tombstone=0 ORDER BY rowid")?;
    for row in q.query_map([b], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (id, name) = row?;
        payees.entry(name.to_lowercase()).or_insert((id, name));
    }
    let mut q = db.prepare(
        "SELECT id,name FROM categories WHERE budget_id=?1 AND tombstone=0 ORDER BY rowid",
    )?;
    let categories = q
        .query_map([b], |r| {
            Ok((r.get::<_, String>(1)?, r.get::<_, String>(0)?))
        })?
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let existing_payees = payees
        .values()
        .map(|p| p.0.clone())
        .collect::<BTreeSet<_>>();
    let mut runner = crate::rules::import_runner(db, b, crate::rules::Work::default())?;
    for (index, source) in parsed.into_iter().enumerate() {
        let mut transaction = match parse::mapped(&source, &body.mapping, &body.options) {
            Ok(t) => t,
            Err(e) => {
                errors.push(json!({"row":index,"message":e.message}));
                continue;
            }
        };
        let name = transaction["payee_name"]
            .as_str()
            .map(|s| s.trim_matches(crate::payees_tags::js_whitespace))
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        transaction["payee_id"] = name
            .as_deref()
            .and_then(|n| payees.get(&n.to_lowercase()).map(|p| json!(p.0)))
            .unwrap_or(Value::Null);
        transaction["id"] = json!(identity(&[
            b,
            id,
            &body.snapshot,
            "row",
            &index.to_string()
        ]));
        transaction["is_parent"] = json!(false);
        transaction["parent_id"] = Value::Null;
        if transaction["payee_id"].is_null()
            && let Some(name) = name.as_deref()
        {
            let name = title::title(name);
            let key = name.to_lowercase();
            let (payee, name) = payees
                .entry(key.clone())
                .or_insert_with(|| (identity(&[b, id, "payee", &key]), name));
            transaction["payee_id"] = json!(payee);
            transaction["payee_name"] = json!(name);
            transaction["new_payee"] = json!({"id":payee,"name":name});
        }
        if let Some(payee) = transaction["payee_id"].as_str()
            && !existing_payees.contains(payee)
            && let Some((_, name)) = payees.values().find(|p| p.0 == payee)
        {
            transaction["new_payee"] = json!({"id":payee,"name":name});
        }
        transaction["imported_payee"] = name.as_deref().map(|n| json!(n)).unwrap_or(Value::Null);
        transaction["account_id"] = json!(account);
        transaction["category_id"] = transaction["category_name"]
            .as_str()
            .and_then(|n| categories.get(n))
            .map(|id| json!(id))
            .unwrap_or(Value::Null);
        let (mut transaction, matched) = runner.evaluate(db, b, &transaction)?;
        if transaction["new_payee"].is_object()
            && transaction["payee_id"] != transaction["new_payee"]["id"]
        {
            transaction.as_object_mut().unwrap().remove("new_payee");
        }
        rows.push(json!({"row":index,"source":source,"transaction":transaction,"matched_rule_ids":matched,"match_id":null,"action":"add"}));
    }
    let mut work = runner.into_work();
    matching::apply(
        db,
        b,
        &account,
        &mut rows,
        &body.options,
        &mut work.remaining,
    )?;
    Ok((
        json!({"id":id,"filename":filename,"account_id":account,"rows":rows,"errors":errors,"snapshot":sync::cursor(db,b)?,"epoch":crate::lifecycle::epoch(db,b)?}),
        work,
    ))
}
async fn preview(
    State(store): State<Store>,
    Path((b, id)): Path<(String, String)>,
    input: Result<Json<Review>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    crate::models::validate_id(&id)?;
    let Json(body) = input.map_err(crate::api::json_error)?;
    store
        .run(move |db| {
            let tx = db.transaction()?;
            Ok(json!({"data":plan(&tx,&b,&id,&body)?.0}))
        })
        .await
        .map(Json)
}
fn guard(
    db: &Connection,
    b: &str,
    entity: &str,
    id: &str,
    field: &str,
    clock: &str,
) -> Result<(), Error> {
    let newer:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM field_versions WHERE budget_id=?1 AND entity=?2 AND entity_id=?3 AND field=?4 AND clock>=?5)",params![b,entity,id,field,clock],|r|r.get(0))?;
    if newer {
        return Err(Error::conflict(
            "A newer field edit exists. Keep the original import and review a fresh preview.",
        ));
    }
    Ok(())
}
fn remember(
    db: &Connection,
    b: &str,
    account: &str,
    body: &Review,
    clock: &str,
) -> Result<(), Error> {
    let pairs = [
        (
            "csv-mappings",
            json!(serde_json::to_string(&body.mapping).map_err(Error::internal)?),
        ),
        ("csv-delimiter", json!(body.options.delimiter)),
        ("csv-encoding", json!(body.options.encoding)),
        (
            "csv-has-header",
            json!(body.options.has_header_row.to_string()),
        ),
        (
            "csv-skip-start-lines",
            json!(body.options.skip_start_lines.to_string()),
        ),
        (
            "csv-skip-end-lines",
            json!(body.options.skip_end_lines.to_string()),
        ),
    ];
    for (key, value) in pairs {
        let id = format!("{key}-{account}");
        let value = value.to_string();
        let old:Option<String>=db.query_row("SELECT value FROM transaction_import_settings WHERE budget_id=?1 AND id=?2 AND tombstone=0",params![b,id],|r|r.get(0)).optional()?;
        if old.as_deref() == Some(&value) {
            continue;
        }
        guard(db, b, "transaction_import_settings", &id, "value", clock)?;
        guard(
            db,
            b,
            "transaction_import_settings",
            &id,
            "tombstone",
            clock,
        )?;
        if key == "csv-skip-start-lines" {
            guard(
                db,
                b,
                "transaction_import_settings",
                &format!("csv-skip-lines-{account}"),
                "value",
                clock,
            )?;
        }
        db.execute("INSERT INTO transaction_import_settings(budget_id,id,account_id,option_key,value) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(budget_id,id) DO UPDATE SET value=excluded.value,tombstone=0",params![b,id,account,key,value])?;
        sync::seed_field_version(db, b, "transaction_import_settings", &id, "value", clock)?;
        sync::seed_field_version(
            db,
            b,
            "transaction_import_settings",
            &id,
            "tombstone",
            clock,
        )?;
        sync::mark(db, b, "transaction_import_settings", &id)?;
    }
    Ok(())
}
async fn commit(
    State(store): State<Store>,
    Path((b, id)): Path<(String, String)>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    crate::models::validate_id(&b)?;

    crate::models::validate_id(&id)?;

    let Json(body) = input.map_err(crate::api::json_error)?;

    let payload = body.to_string();

    let key = crate::api::idempotency_key(&headers)?;

    let proposed = sync::header(&headers)?;

    let epoch = crate::lifecycle::epoch_header(&headers)?;

    let owner = crate::history::owner_header(&headers)?;

    let lookup = sync::receipt_only_header(&headers)?;

    let cancel = sync::cancel_header(&headers)?;

    store.run(move|db|{let scope=format!("POST /budgets/{b}/transaction-imports/{id}/commit");
if lookup{return sync::receipt_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref());
}sync::write_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref(),|db|{if cancel{return sync::canceled(db,&b);
}crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;
let body:Review=serde_json::from_value(body).map_err(|e|Error::invalid(format!("Invalid import command: {e}")))?;
let (plan,mut work)=plan(db,&b,&id,&body)?;
if !plan["errors"].as_array().unwrap().is_empty(){return Err(Error::invalid("Repair invalid import rows before committing"));
}let choices=body.rows.as_ref().ok_or_else(||Error::invalid("Select the reviewed rows to import"))?;
if choices.is_empty()||choices.len()>10000{return Err(Error::invalid("Select1–10000 rows per atomic import"));
}let reviewed=plan["rows"].as_array().unwrap().iter().map(|r|(r["row"].as_u64().unwrap() as usize,r)).collect::<BTreeMap<_,_>>();
let mut selected=BTreeSet::new();
let clock=sync::clock(db,&b,proposed.as_deref())?;
let mut updated=vec![];
let mut added=vec![];
let mut ignored=vec![];

 for choice in choices{if !selected.insert(choice.row){return Err(Error::invalid("A source row was selected twice"));
}let row=reviewed.get(&choice.row).copied().ok_or_else(||Error::invalid("Unknown source row"))?;
let transaction=&row["transaction"];
if choice.action=="skip"||transaction["deleted"]==true{continue;
}
if choice.action=="merge"{let target=choice.match_id.as_deref().ok_or_else(||Error::invalid("A merge requires its reviewed match"))?;
if row["match_id"]!=target{return Err(Error::conflict("The selected match differs from the captured preview"));
}let before=crate::ledger::get(db,&b,target)?;
if before["reconciled"]==true{ignored.push(target.to_owned());
continue;
}let mut patch=serde_json::Map::new();
for field in ["payee_id","category_id","notes"]{let empty=before[field].is_null()||before[field]=="";
if empty&&!transaction[field].is_null()&&before[field]!=transaction[field]{patch.insert(field.into(),transaction[field].clone());
}}
if before["cleared"]!=true&&transaction["cleared"]==true{patch.insert("cleared".into(),json!(true));
}
if patch.get("payee_id").is_some() && transaction["new_payee"].is_object(){patch.insert("payee_id".into(),Value::Null);patch.insert("new_payee".into(),transaction["new_payee"].clone());}
for field in patch.keys().filter(|field|field.as_str()!="new_payee"){guard(db,&b,"transactions",target,field,&clock)?;
}
if !patch.is_empty(){let response=sync::execute(db,&b,Some(&clock),false,epoch.as_deref(),sync::Mutation::Patch(target.into(),Value::Object(patch)))?;
if response["response_status"].as_u64().unwrap_or(500)>=400{return Err(Error::conflict("The matched transaction requires review before import"));
}updated.push(target.to_owned());
}
if before["imported_payee"]!=transaction["imported_payee"]{guard(db,&b,"transactions",target,"imported_payee",&clock)?;
db.execute("UPDATE transactions SET imported_payee=?3 WHERE budget_id=?1 AND id=?2",params![b,target,transaction["imported_payee"].as_str()])?;
sync::seed_field_version(db,&b,"transactions",target,"imported_payee",&clock)?;
sync::mark(db,&b,"transactions",target)?;
}
 }else if choice.action=="add"{
let new_id=transaction["id"].as_str().unwrap().to_owned();
let matched=serde_json::from_value(row["matched_rule_ids"].clone()).map_err(Error::internal)?;
let runner=crate::rules::import_runner(db,&b,work)?;
let result=crate::rules::save_import(db,&b,transaction.clone(),matched,&clock,epoch.as_deref(),runner)?;
work=result.0;
if result.1{
if !transaction["imported_payee"].is_null(){db.execute("UPDATE transactions SET imported_payee=?3 WHERE budget_id=?1 AND id=?2",params![b,new_id,transaction["imported_payee"].as_str()])?;sync::seed_field_version(db,&b,"transactions",&new_id,"imported_payee",&clock)?;}
added.push(new_id);
}
}else{return Err(Error::invalid("Choose add, merge or skip"));
}}
 if body.remember{remember(db,&b,plan["account_id"].as_str().unwrap(),&body,&clock)?;
}
 Ok(json!({"response_status":200,"response_body":{"data":{"added":added,"updated":updated,"ignored":ignored},"edit_clock":clock,"cursor":sync::cursor(db,&b)?}}))})}).await
}
