//! Retained transaction files, read-only review, and receipt-owned ledger application.
mod apply;
mod export;
mod formats;
mod matching;
mod merge;
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
            "/api/v1/budgets/{budget}/transactions/export",
            post(export::download),
        )
        .route(
            "/api/v1/budgets/{budget}/transactions/merge/preview",
            post(merge::preview),
        )
        .route(
            "/api/v1/budgets/{budget}/transactions/merge",
            post(merge::commit),
        )
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
    store.run(move|db|{sync::write(db,&format!("POST /budgets/{b}/accounts/{a}/transaction-imports"),&key,&payload,None,StatusCode::CREATED,|db|{crate::ledger::account_exists(db,&b,&a)?;if !matches!(parse::format(&query.filename.to_ascii_lowercase()),"csv"|"tsv"|"ofx"|"qfx"|"qif"|"xml"){return Err(Error::invalid("Choose a CSV, TSV, OFX, QFX, QIF or CAMT XML file"));}let id=uuid::Uuid::new_v4().to_string();db.execute("INSERT INTO transaction_import_files(budget_id,id,account_id,filename,sha256,original) VALUES(?1,?2,?3,?4,?5,?6)",params![b,id,a,query.filename,hash,bytes.as_ref()])?;Ok(json!({"id":id,"account_id":a,"filename":query.filename,"sha256":hash,"bytes":bytes.len()}))})}).await
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
    store.run(move|db|{let(_,filename,bytes)=file(db,&b,&id)?;let result=parse::file(&filename,&bytes,&options);Ok(json!({"data":match result{Ok(rows)=>json!({"transactions":rows,"errors":[],"format":parse::format(&filename.to_ascii_lowercase())}),Err(e)=>json!({"transactions":[],"errors":[{"message":e.message}]})}}))}).await.map(Json)
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
    plan_input(db, b, id, body, crate::rules::Work::default())
}
fn plan_input(
    db: &Connection,
    b: &str,
    id: &str,
    body: &Review,
    mut work: crate::rules::Work,
) -> Result<(Value, crate::rules::Work), Error> {
    sync::check_snapshot(db, b, Some(&body.snapshot))?;
    let (account, filename, bytes) = file(db, b, id)?;
    let parsed = parse::file(&filename, &bytes, &body.options)?;
    crate::ledger::account_exists(db, b, &account)?;
    let kind = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let selected = if let Some(choices) = &body.rows {
        if choices.len() > 10000 {
            return Err(Error::invalid("Select at most10000 rows per atomic import"));
        }
        let mut selected = BTreeSet::new();
        let mut seen = BTreeSet::new();
        for choice in choices {
            if choice.row >= parsed.len()
                || !seen.insert(choice.row)
                || !matches!(choice.action.as_str(), "auto" | "merge" | "add" | "skip")
            {
                return Err(Error::invalid(
                    "Choose a distinct valid source row and auto, merge, add or skip",
                ));
            }
            if choice.action != "skip" {
                selected.insert(choice.row);
            }
        }
        Some(selected)
    } else {
        None
    };
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
    work.remaining = work
        .remaining
        .checked_sub(payees.len() + categories.len() + parsed.len())
        .ok_or_else(|| Error::invalid("File reference work limit reached. Select fewer rows"))?;
    let mut runner = crate::rules::import_runner(db, b, work)?;
    let mut provisional_payees = BTreeMap::new();
    for (index, source) in parsed.into_iter().enumerate() {
        if selected.as_ref().is_some_and(|rows| !rows.contains(&index)) {
            continue;
        }
        let mut transaction = match if matches!(kind.as_str(), "csv" | "tsv") {
            parse::mapped(&source, &body.mapping, &body.options)
        } else {
            parse::structured(&source, &kind, &body.options)
        } {
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
            provisional_payees.insert(payee.clone(), transaction["new_payee"].clone());
        }
        if let Some(payee) = transaction["payee_id"].as_str()
            && !existing_payees.contains(payee)
            && let Some(reference) = provisional_payees.get(payee)
        {
            transaction["new_payee"] = reference.clone();
        }
        transaction["imported_payee"] = name.as_deref().map(|n| json!(n)).unwrap_or(Value::Null);
        transaction["account_id"] = json!(account);
        transaction["category_id"] = transaction["category_name"]
            .as_str()
            .and_then(|name| categories.get(name))
            .map(|id| json!(id))
            .unwrap_or(Value::Null);
        // Source import normalization keeps the proposed display name outside
        // the rule transaction. Only already-persisted payees enrich rule
        // context; a lazy payee is not visible to templates until saved.
        let display_payee = transaction.as_object_mut().unwrap().remove("payee_name");
        let (mut transaction, matched) = runner.evaluate(db, b, &transaction)?;
        if transaction["deleted"] != true
            && let Err(error) = crate::rules::validate_calculated_transaction(&transaction)
        {
            errors.push(json!({"row":index,"message":format!("Rule result needs review: {}", error.message)}));
            continue;
        }
        if let Some(name) = display_payee {
            transaction
                .as_object_mut()
                .unwrap()
                .entry("payee_name")
                .or_insert(name);
        }
        if transaction["new_payee"].is_object()
            && transaction["payee_id"] != transaction["new_payee"]["id"]
            && !transaction["children"].as_array().is_some_and(|children| {
                children
                    .iter()
                    .any(|child| child["payee_id"] == transaction["new_payee"]["id"])
            })
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
        matching::Mode {
            options: &body.options,
            strict_ids: true,
        },
        &mut work.remaining,
    )?;
    let mut existing = BTreeMap::new();
    for row in &mut rows {
        if let Some(target) = row["match_id"].as_str().map(str::to_owned) {
            if !existing.contains_key(&target) {
                let mut before = crate::ledger::get_any(db, b, &target)?;
                let count = before["children"].as_array().map_or(0, Vec::len);
                work.remaining = work.remaining.checked_sub(count + 1).ok_or_else(|| {
                    Error::invalid("Import graph work limit reached. Select fewer rows")
                })?;
                before.as_object_mut().unwrap().remove("children");
                before["child_count"] = json!(count);
                existing.insert(target.clone(), before);
            }
            let before = &existing[&target];
            row["ignored"] = json!(apply::ignored(before, &row["transaction"]));
            row["existing"] = before.clone();
        } else {
            row["ignored"] = json!(false);
            if row["transaction"]["deleted"] == true {
                row["action"] = json!("skip");
            }
        }
    }
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
pub(crate) fn guard(
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
    filename: &str,
    body: &Review,
    clock: &str,
) -> Result<(), Error> {
    let extension = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let kind = if extension == "tsv" {
        "csv"
    } else {
        &extension
    };
    let mut pairs = vec![(
        format!("import-reimport-deleted-{account}"),
        json!(body.options.reimport_deleted.to_string()),
    )];
    if matches!(kind, "csv" | "qif") {
        for (prefix, value) in [
            ("parse-date", body.options.date_format.clone()),
            ("flip-amount", body.options.flip_amount.to_string()),
            ("import-notes", body.options.import_notes.to_string()),
        ] {
            pairs.push((format!("{prefix}-{account}-{kind}"), json!(value)));
        }
    }
    if kind == "csv" {
        let mut mapping = serde_json::to_value(&body.mapping).map_err(Error::internal)?;
        let map = mapping.as_object_mut().unwrap();
        if let Some(value) = map.remove("in_out") {
            map.insert("inOut".into(), value);
        }
        for (key, value) in [
            (
                "csv-mappings",
                serde_json::to_string(&mapping).map_err(Error::internal)?,
            ),
            ("csv-delimiter", body.options.delimiter.clone()),
            ("csv-encoding", body.options.encoding.clone()),
            ("csv-has-header", body.options.has_header_row.to_string()),
            (
                "csv-skip-start-lines",
                body.options.skip_start_lines.to_string(),
            ),
            (
                "csv-skip-end-lines",
                body.options.skip_end_lines.to_string(),
            ),
            ("csv-in-out-mode", body.options.in_out_mode.to_string()),
            ("csv-out-value", body.options.out_value.clone()),
        ] {
            pairs.push((format!("{key}-{account}"), json!(value)));
        }
    }
    if matches!(kind, "ofx" | "qfx") {
        pairs.push((
            format!("ofx-fallback-missing-payee-{account}"),
            json!(body.options.fallback_missing_payee_to_memo.to_string()),
        ));
    }
    let swap = match kind {
        "ofx" | "qfx" => Some("ofx-swap-payee-memo"),
        "qif" => Some("qif-swap-payee-memo"),
        "xml" => Some("camt-swap-payee-memo"),
        _ => None,
    };
    if let Some(prefix) = swap {
        pairs.push((
            format!("{prefix}-{account}"),
            json!(body.options.swap_payee_and_memo.to_string()),
        ));
    }
    for (id, value) in pairs {
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
        if id == format!("csv-skip-start-lines-{account}") {
            guard(
                db,
                b,
                "transaction_import_settings",
                &format!("csv-skip-lines-{account}"),
                "value",
                clock,
            )?;
        }
        let (_, key) = storage::preference_parts(&id)
            .ok_or_else(|| Error::internal("Unsupported saved import preference"))?;
        db.execute("INSERT INTO transaction_import_settings(budget_id,id,account_id,option_key,value) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(budget_id,id) DO UPDATE SET value=excluded.value,tombstone=0",params![b,id,account,key,value])?;
        for field in ["value", "tombstone"] {
            sync::seed_field_version(db, b, "transaction_import_settings", &id, field, clock)?;
        }
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

    store
        .run(move |db| {
            let scope = format!("POST /budgets/{b}/transaction-imports/{id}/commit");
            if lookup {
                return sync::receipt_owned(
                    db,
                    &scope,
                    &key,
                    &payload,
                    proposed.as_deref(),
                    StatusCode::OK,
                    owner.as_ref(),
                );
            }
            sync::write_owned(
                db,
                &scope,
                &key,
                &payload,
                proposed.as_deref(),
                StatusCode::OK,
                owner.as_ref(),
                |db| {
                    if cancel {
                        return sync::canceled(db, &b);
                    }
                    crate::lifecycle::require_epoch(db, &b, epoch.as_deref())?;
                    let body: Review = serde_json::from_value(body)
                        .map_err(|e| Error::invalid(format!("Invalid import command: {e}")))?;
                    let (plan, work) = plan(db, &b, &id, &body)?;
                    if !plan["errors"].as_array().unwrap().is_empty() {
                        return Err(Error::invalid(
                            "Repair invalid import rows before committing",
                        ));
                    }
                    apply_plan(
                        db,
                        &b,
                        &plan,
                        &body,
                        WriteIntent {
                            proposed: proposed.as_deref(),
                            epoch: epoch.as_deref(),
                            update_dates: false,
                        },
                        work,
                    )
                    .map(|(value, _)| value)
                },
            )
        })
        .await
}

struct WriteIntent<'a> {
    proposed: Option<&'a str>,
    epoch: Option<&'a str>,
    update_dates: bool,
}
fn apply_plan(
    db: &Connection,
    b: &str,
    plan: &Value,
    body: &Review,
    intent: WriteIntent<'_>,
    mut work: crate::rules::Work,
) -> Result<(Value, crate::rules::Work), Error> {
    let choices = body
        .rows
        .as_ref()
        .ok_or_else(|| Error::invalid("Select the reviewed rows to import"))?;
    if choices.is_empty() || choices.len() > 10000 {
        return Err(Error::invalid("Select1–10000 rows per atomic import"));
    }
    let mut choices = choices.iter().collect::<Vec<_>>();
    choices.sort_by_key(|choice| choice.row);
    let reviewed = plan["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["row"].as_u64().unwrap() as usize, r))
        .collect::<BTreeMap<_, _>>();
    let mut last_merge = BTreeMap::new();
    for choice in &choices {
        if choice.action == "skip" {
            continue;
        }
        let row = reviewed
            .get(&choice.row)
            .copied()
            .ok_or_else(|| Error::invalid("Unknown selected source row"))?;
        let action = if choice.action == "auto" {
            row["action"].as_str().unwrap()
        } else {
            choice.action.as_str()
        };
        if action == "merge" {
            if choice.action == "merge" && choice.match_id.is_none() {
                return Err(Error::invalid("A merge requires its captured match ID"));
            }
            let target = row["match_id"]
                .as_str()
                .ok_or_else(|| Error::conflict("Review the selected match again"))?;
            if choice.match_id.as_deref().is_some_and(|id| id != target) {
                return Err(Error::conflict(
                    "The selected match differs from the captured preview",
                ));
            }
            if row["ignored"] != true {
                last_merge.insert(target.to_owned(), choice.row);
            }
        }
    }
    let order_base: i64 = db.query_row(
        "SELECT COALESCE(MAX(sort_order),0) FROM transactions WHERE budget_id=?1",
        [b],
        |r| r.get(0),
    )?;
    let clock = sync::clock(db, b, intent.proposed)?;
    let mut updated = vec![];
    let mut updated_ids = BTreeSet::new();
    let mut added = vec![];
    let mut ignored = vec![];
    let mut created_payees = BTreeSet::new();

    for choice in &choices {
        if choice.action == "skip" {
            continue;
        }
        let row = reviewed
            .get(&choice.row)
            .copied()
            .ok_or_else(|| Error::invalid("Unknown source row"))?;
        let mut transaction = row["transaction"].clone();
        if transaction["new_payee"]["id"]
            .as_str()
            .is_some_and(|id| created_payees.contains(id))
        {
            transaction.as_object_mut().unwrap().remove("new_payee");
        }
        let action = if choice.action == "auto" {
            row["action"].as_str().unwrap()
        } else {
            choice.action.as_str()
        };
        if action == "skip" || action == "add" && transaction["deleted"] == true {
            continue;
        }
        if action == "merge" {
            let target = row["match_id"]
                .as_str()
                .ok_or_else(|| Error::invalid("A merge requires its reviewed match"))?;
            if row["ignored"] == true {
                ignored.push(target.to_owned());
                continue;
            }
            if last_merge.get(target) != Some(&choice.row) {
                continue;
            }
            if row["match_id"] != target {
                return Err(Error::conflict(
                    "The selected match differs from the captured preview",
                ));
            }
            let before = crate::ledger::get_any(db, b, target)?;
            if before["reconciled"] == true {
                ignored.push(target.to_owned());
                continue;
            }
            let mut patch = serde_json::Map::new();
            for field in ["payee_id", "category_id", "notes"] {
                let empty = before[field].is_null() || before[field] == "";
                if empty && !transaction[field].is_null() && before[field] != transaction[field] {
                    patch.insert(field.into(), transaction[field].clone());
                }
            }
            if intent.update_dates && before["date"] != transaction["date"] {
                patch.insert("date".into(), transaction["date"].clone());
            }
            if before["cleared"] != true && transaction["cleared"] == true {
                patch.insert("cleared".into(), json!(true));
            }
            if patch.get("payee_id").is_some() && transaction["new_payee"].is_object() {
                patch.insert("payee_id".into(), Value::Null);
                patch.insert("new_payee".into(), transaction["new_payee"].clone());
            }
            for field in patch.keys().filter(|field| field.as_str() != "new_payee") {
                guard(db, b, "transactions", target, field, &clock)?;
            }
            let creating_payee = patch
                .get("new_payee")
                .and_then(|p| p["id"].as_str())
                .map(str::to_owned);
            if !patch.is_empty() {
                let response = apply::patch(
                    db,
                    b,
                    target,
                    &before,
                    Value::Object(patch),
                    apply::Intent {
                        clock: &clock,
                        epoch: intent.epoch,
                        work: &mut work,
                    },
                )?;
                if response["response_status"].as_u64().unwrap_or(500) >= 400 {
                    return Err(Error::conflict(
                        "The matched transaction requires review before import",
                    ));
                }
                if let Some(id) = creating_payee {
                    created_payees.insert(id);
                }
                if updated_ids.insert(target.to_owned()) {
                    updated.push(target.to_owned());
                }
            }
            if before["imported_payee"] != transaction["imported_payee"] {
                guard(db, b, "transactions", target, "imported_payee", &clock)?;
                db.execute(
                    "UPDATE transactions SET imported_payee=?3 WHERE budget_id=?1 AND id=?2",
                    params![b, target, transaction["imported_payee"].as_str()],
                )?;
                sync::seed_field_version(db, b, "transactions", target, "imported_payee", &clock)?;
                sync::mark(db, b, "transactions", target)?;
            }
            if storage::save(db, b, target, &transaction, &clock)?
                && updated_ids.insert(target.to_owned())
            {
                updated.push(target.to_owned());
            }
        } else if action == "add" {
            let new_id = transaction["id"].as_str().unwrap().to_owned();
            let matched =
                serde_json::from_value(row["matched_rule_ids"].clone()).map_err(Error::internal)?;
            let runner = crate::rules::import_runner(db, b, work)?;
            let result = crate::rules::save_import(
                db,
                b,
                transaction.clone(),
                matched,
                &clock,
                intent.epoch,
                runner,
            )?;
            work = result.0;
            if result.1 {
                if let Some(id) = transaction["new_payee"]["id"].as_str() {
                    created_payees.insert(id.to_owned());
                }
                if !transaction["imported_payee"].is_null() {
                    db.execute(
                        "UPDATE transactions SET imported_payee=?3 WHERE budget_id=?1 AND id=?2",
                        params![b, new_id, transaction["imported_payee"].as_str()],
                    )?;
                    sync::seed_field_version(
                        db,
                        b,
                        "transactions",
                        &new_id,
                        "imported_payee",
                        &clock,
                    )?;
                }
                storage::save(db, b, &new_id, &transaction, &clock)?;
                added.push(new_id);
            }
        } else {
            return Err(Error::invalid("Choose add, merge or skip"));
        }
    }
    apply::order(db, b, &added, order_base, &clock, &mut work)?;
    if body.remember {
        remember(
            db,
            b,
            plan["account_id"].as_str().unwrap(),
            plan["filename"].as_str().unwrap(),
            body,
            &clock,
        )?;
    }
    Ok((
        json!({"response_status":200,"response_body":{"data":{"added":added,"updated":updated,"ignored":ignored},"edit_clock":clock,"cursor":sync::cursor(db,b)?}}),
        work,
    ))
}
