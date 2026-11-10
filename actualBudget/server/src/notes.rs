//! SDK note identity: existing domain fields remain authoritative; arbitrary IDs
//! have durable budget-scoped storage and the same clock/receipt/history rules.
use crate::{
    concurrency as sync,
    error::Error,
    store::{self, Store},
};
use axum::{
    Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
fn validate(id: &str, note: &str) -> Result<(), Error> {
    if id.is_empty()
        || id.chars().count() > 512
        || id.contains('\0')
        || note.chars().count() > 10000
    {
        return Err(Error::invalid(
            "Note identity must have1–512 characters and note at most10000 characters",
        ));
    }
    Ok(())
}
/// Replacement clients may reserve explicit domain UUIDs. Never let that
/// extension silently hide a previously written standalone note.
pub fn guard_domain_id(db: &Connection, b: &str, id: &str) -> Result<(), Error> {
    let retained:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM notes WHERE budget_id=?1 AND id=?2 AND tombstone=0 AND note<>'')",params![b,id],|r|r.get(0))?;
    if retained {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "note_identity_conflict",
            "This identity has a standalone note. Choose a different ID, or explicitly clear that note before creating this reference.",
        ));
    }
    Ok(())
}

fn target(
    db: &Connection,
    b: &str,
    id: &str,
) -> Result<(&'static str, String, &'static str), Error> {
    for table in ["accounts", "categories", "category_groups"] {
        if db.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE budget_id=?1 AND id=?2)"),
            params![b, id],
            |r| r.get::<_, bool>(0),
        )? {
            return Ok((table, id.into(), "notes"));
        }
    }
    if let Some(month) = id.strip_prefix("budget-")
        && crate::budgeting::month(month).is_ok()
    {
        return Ok(("budget_months", month.into(), "notes"));
    }
    Ok(("notes", id.into(), "note"))
}
pub fn get(db: &Connection, b: &str, id: &str) -> Result<Value, Error> {
    store::require_budget(db, b)?;
    validate(id, "")?;
    let (table, row, field) = target(db, b, id)?;
    let note: Option<String> = db
        .query_row(
            &format!(
                "SELECT {field} FROM {table} WHERE budget_id=?1 AND id=?2{}",
                if matches!(table, "notes" | "budget_months") {
                    " AND tombstone=0"
                } else {
                    ""
                }
            ),
            params![b, row],
            |r| r.get(0),
        )
        .optional()?;
    Ok(note
        .map(|note| json!({"id":id,"note":note}))
        .unwrap_or(Value::Null))
}
pub fn record(db: &Connection, b: &str, id: &str) -> Result<Value, Error> {
    let mut value = db
        .query_row(
            "SELECT note,tombstone FROM notes WHERE budget_id=?1 AND id=?2",
            params![b, id],
            |r| Ok(json!({"id":id,"note":r.get::<_,String>(0)?,"deleted":r.get::<_,bool>(1)?})),
        )
        .optional()?
        .ok_or_else(Error::missing)?;
    let mut q=db.prepare("SELECT field,clock FROM field_versions WHERE budget_id=?1 AND entity='notes' AND entity_id=?2")?;
    value["field_versions"] = json!(
        q.query_map(params![b, id], |r| Ok((
            r.get::<_, String>(0)?,
            json!(r.get::<_, String>(1)?)
        )))?
        .collect::<Result<serde_json::Map<_, _>, _>>()?
    );
    Ok(value)
}

pub fn router() -> Router<Store> {
    Router::new()
        .route("/api/v1/budgets/{budget}/notes/read", post(read))
        .route("/api/v1/budgets/{budget}/notes/commands", post(command))
}
async fn read(
    State(store): State<Store>,
    Path(b): Path<String>,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    let Json(body) = input.map_err(crate::api::json_error)?;
    crate::models::validate_id(&b)?;
    let id = body["id"]
        .as_str()
        .filter(|_| body.as_object().is_some_and(|o| o.len() == 1))
        .ok_or_else(|| Error::invalid("Read note requires only its identity"))?
        .to_owned();
    store
        .run(move |db| Ok(Json(json!({"data":get(db,&b,&id)?}))))
        .await
}
async fn command(
    State(store): State<Store>,
    Path(b): Path<String>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let Json(body) = input.map_err(crate::api::json_error)?;
    crate::models::validate_id(&b)?;
    let key = crate::api::idempotency_key(&headers)?;
    let owner = crate::history::owner_header(&headers)?;
    let proposed = sync::header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let cancel = sync::cancel_header(&headers)?;
    let lookup = sync::receipt_only_header(&headers)?;
    let payload = json!({"request":body,"epoch":epoch}).to_string();
    store.run(move|db|{let scope=format!("POST /budgets/{b}/notes/commands");if lookup{return sync::receipt_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref());}
 sync::write_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref(),|db|{
 if cancel{return sync::canceled(db,&b);}crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;
 if body.as_object().is_none_or(|o|o.len()!=3||o.keys().any(|k|!["id","note","snapshot"].contains(&k.as_str()))){return Err(Error::invalid("Note command requires id,note,snapshot"));}
 let id=body["id"].as_str().ok_or_else(||Error::invalid("Provide note identity"))?;let note=body["note"].as_str().ok_or_else(||Error::invalid("Provide note text"))?;validate(id,note)?;
 sync::check_snapshot(db,&b,Some(body["snapshot"].as_str().ok_or_else(||Error::invalid("Provide current snapshot"))?))?;
 let(table,row,field)=target(db,&b,id)?;let clock=sync::clock(db,&b,proposed.as_deref())?;crate::rules::guard(db,&b,table,&row,field,&clock)?;
 if matches!(table,"notes"|"budget_months") && db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE budget_id=?1 AND id=?2 AND tombstone=1)"),params![b,row],|r|r.get::<_,bool>(0))? {crate::rules::guard(db,&b,table,&row,"tombstone",&clock)?;}
 if table=="notes"{db.execute("INSERT INTO notes(budget_id,id,note) VALUES(?1,?2,?3) ON CONFLICT(budget_id,id) DO UPDATE SET note=excluded.note,tombstone=0",params![b,row,note])?;}else if table=="budget_months"{db.execute("INSERT INTO budget_months(budget_id,id,notes) VALUES(?1,?2,?3) ON CONFLICT(budget_id,id) DO UPDATE SET notes=excluded.notes,tombstone=0",params![b,row,note])?;}else{db.execute(&format!("UPDATE {table} SET {field}=?3 WHERE budget_id=?1 AND id=?2"),params![b,row,note])?;}
 sync::seed_field_version(db,&b,table,&row,field,&clock)?;sync::mark(db,&b,table,&row)?;
 Ok(json!({"response_status":200,"response_body":{"data":get(db,&b,id)?,"snapshot":sync::cursor(db,&b)?,"edit_clock":clock}}))
 })}).await
}
pub fn convert(source: &Value) -> Result<Value, Error> {
    let mut rows = vec![];
    for row in source["notes"].as_array().into_iter().flatten() {
        let id = row["id"]
            .as_str()
            .ok_or_else(|| Error::invalid("Invalid source note identity"))?;
        let note = row["note"].as_str().unwrap_or("");
        validate(id, note)?;
        let known = ["accounts", "categories", "category_groups"]
            .iter()
            .any(|table| {
                source[*table]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|r| r["id"] == id)
            })
            || id
                .strip_prefix("budget-")
                .is_some_and(|m| crate::budgeting::month(m).is_ok());
        if !known {
            rows.push(json!({"id":id,"note":note}));
        }
    }
    Ok(json!(rows))
}
pub fn install(db: &Connection, b: &str, rows: &Value) -> Result<(), Error> {
    for row in rows.as_array().into_iter().flatten() {
        let id = row["id"]
            .as_str()
            .ok_or_else(|| Error::invalid("Invalid note ID"))?;
        let note = row["note"]
            .as_str()
            .ok_or_else(|| Error::invalid("Invalid note text"))?;
        validate(id, note)?;
        let inserted = db.execute(
            "INSERT OR IGNORE INTO notes(budget_id,id,note) VALUES(?1,?2,?3)",
            params![b, id, note],
        )?;
        if inserted > 0 {
            sync::mark(db, b, "notes", id)?;
        }
    }
    Ok(())
}
pub fn import_clock(db: &Connection, b: &str, row: &Value) -> Result<(), Error> {
    if row["dataset"] == "notes" && row["column"] == "note" {
        let id = row["row"].as_str().unwrap_or("");
        if db.query_row(
            "SELECT EXISTS(SELECT 1 FROM notes WHERE budget_id=?1 AND id=?2)",
            params![b, id],
            |r| r.get::<_, bool>(0),
        )? {
            sync::seed_field_version(
                db,
                b,
                "notes",
                id,
                "note",
                row["timestamp"].as_str().unwrap_or(""),
            )?;
        }
    }
    Ok(())
}
pub fn enrich(db: &Connection, only: Option<&str>) -> Result<(), Error> {
    let mut q=db.prepare("SELECT i.budget_id,s.original FROM imported_sources i JOIN staged_imports s ON s.id=i.import_id WHERE (?1 IS NULL OR i.budget_id=?1) AND coalesce(json_extract(s.report,'$.source_format'),'actual') NOT IN('ynab4','nynab')")?;
    let sources = q
        .query_map([only], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (b, bytes) in sources {
        let (_, data) = crate::imports::conversion::inspect(&bytes)?;
        install(db, &b, &data["notes"])?;
        for row in data["clocks"].as_array().into_iter().flatten() {
            import_clock(db, &b, row)?;
        }
    }
    // Conversion advances the staged review, never its immutable upload receipt.
    let mut stages=db.prepare("SELECT id,original FROM staged_imports WHERE status<>'activated' AND coalesce(json_extract(report,'$.source_format'),'actual') NOT IN('ynab4','nynab') AND coalesce(json_extract(conversion,'$.notes_version'),0)<>1")?;
    let stages = stages
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, bytes) in stages {
        let (mut report, data) = crate::imports::conversion::inspect(&bytes)?;
        report["archive_sha256"] = json!(crate::lifecycle::archive::hash(&bytes));
        let blocked = report["requirements"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|r| r["blocking"] == true);
        db.execute(
            "UPDATE staged_imports SET conversion=?2,report=?3,status=?4 WHERE id=?1",
            params![
                id,
                data.to_string(),
                report.to_string(),
                if blocked { "blocked" } else { "ready" }
            ],
        )?;
    }
    Ok(())
}
pub fn validate_archive(db: &Connection, b: &str) -> Result<(), Error> {
    let mut q = db.prepare("SELECT id,note,tombstone FROM notes WHERE budget_id=?1")?;
    for row in q.query_map([b], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, bool>(2)?,
        ))
    })? {
        let (id, note, deleted) = row?;
        validate(&id, &note)?;
        if !deleted && !note.is_empty() && target(db, b, &id)?.0 != "notes" {
            return Err(Error::invalid(
                "Standalone note conflicts with an authoritative domain note identity",
            ));
        }
    }
    Ok(())
}
