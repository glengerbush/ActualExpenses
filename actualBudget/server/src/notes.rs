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
pub fn configure(db: &Connection) -> Result<(), Error> {
    db.execute_batch(include_str!("notes_triggers.sql"))?;
    Ok(())
}
fn identity(db: &Connection, b: &str, id: &str) -> Result<(), Error> {
    db.execute("INSERT INTO notes(budget_id,id,note) VALUES(?1,?2,'') ON CONFLICT(budget_id,id) DO UPDATE SET note='',tombstone=0",params![b,id])?;
    Ok(())
}

pub fn binding(db: &Connection, b: &str, id: &str) -> Result<String, Error> {
    Ok(target(db, b, id)?.0.into())
}
/// Legacy domain fields and a formerly standalone note share one logical HLC.
pub fn decorate_versions(
    db: &Connection,
    b: &str,
    id: &str,
    versions: &mut serde_json::Map<String, Value>,
) -> Result<(), Error> {
    let clock:Option<String>=db.query_row("SELECT MAX(clock) FROM field_versions WHERE budget_id=?1 AND entity='notes' AND entity_id=?2 AND field IN('note','tombstone')",params![b,id],|r|r.get(0))?;
    if let Some(clock) = clock
        && clock.as_str()
            > versions
                .get("notes")
                .and_then(Value::as_str)
                .unwrap_or(sync::ZERO)
    {
        versions.insert("notes".into(), json!(clock));
    }
    Ok(())
}
/// Mirror original metadata clocks with max ordering; never mint clocks.
/// Touch the row before its clock so existing history captures clock-only edits.
pub fn mirror_domain_clock(
    db: &Connection,
    b: &str,
    entity: &str,
    id: &str,
    field: &str,
    clock: &str,
) -> Result<(), Error> {
    if field != "notes" || !["accounts"].contains(&entity) {
        return Ok(());
    }
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='notes')",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(());
    }
    let id = id.to_owned();
    if db.execute(
        "UPDATE notes SET note=note WHERE budget_id=?1 AND id=?2",
        params![b, id],
    )? > 0
    {
        db.execute("INSERT INTO field_versions(budget_id,entity,entity_id,field,clock) VALUES(?1,'notes',?2,'note',?3) ON CONFLICT(budget_id,entity,entity_id,field) DO UPDATE SET clock=excluded.clock WHERE excluded.clock>field_versions.clock",params![b,id,clock])?;
    }
    Ok(())
}

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
    for table in ["accounts"] {
        if db.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE budget_id=?1 AND id=?2)"),
            params![b, id],
            |r| r.get::<_, bool>(0),
        )? {
            return Ok((table, id.into(), "notes"));
        }
    }
    Ok(("notes", id.into(), "note"))
}
pub fn get(db: &Connection, b: &str, id: &str) -> Result<Value, Error> {
    store::require_budget(db, b)?;
    validate(id, "")?;
    if retired_budget_note(id, b) || retired_category_note(db, b, id)? {
        return Ok(Value::Null);
    }
    let (table, row, field) = target(db, b, id)?;
    let note: Option<String> = db
        .query_row(
            &format!(
                "SELECT {field} FROM {table} WHERE budget_id=?1 AND id=?2{}",
                if table == "notes" {
                    " AND tombstone=0"
                } else {
                    ""
                }
            ),
            params![b, row],
            |r| r.get(0),
        )
        .optional()?;
    let marker: Option<bool> = db
        .query_row(
            "SELECT tombstone FROM notes WHERE budget_id=?1 AND id=?2",
            params![b, id],
            |r| r.get(0),
        )
        .optional()?;
    if marker == Some(true) {
        return Ok(Value::Null);
    }
    Ok(note
        .filter(|note| !note.is_empty() || marker == Some(false))
        .map(|note| json!({"id":id,"note":note}))
        .unwrap_or(Value::Null))
}
pub fn record(db: &Connection, b: &str, id: &str) -> Result<Value, Error> {
    let retained: Option<String> = db
        .query_row(
            "SELECT note FROM notes WHERE budget_id=?1 AND id=?2",
            params![b, id],
            |r| r.get(0),
        )
        .optional()?;
    let retained = retained.ok_or_else(Error::missing)?;
    let canonical = get(db, b, id)?;
    let mut value = if canonical.is_null() {
        json!({"id":id,"note":retained,"deleted":true})
    } else {
        let mut v = canonical;
        v["deleted"] = json!(false);
        v
    };
    let (table, row, field) = target(db, b, id)?;
    let mut q=db.prepare("SELECT CASE WHEN entity=?3 AND entity_id=?4 AND field=?5 THEN 'note' ELSE field END,MAX(clock) FROM field_versions WHERE budget_id=?1 AND ((entity='notes' AND entity_id=?2) OR (entity=?3 AND entity_id=?4 AND field=?5)) GROUP BY 1")?;
    value["field_versions"] = json!(
        q.query_map(params![b, id, table, row, field], |r| Ok((
            r.get::<_, String>(0)?,
            json!(r.get::<_, String>(1)?)
        )))?
        .collect::<Result<serde_json::Map<_, _>, _>>()?
    );
    Ok(value)
}
/// Domain text is authoritative; marker rows distinguish empty from absent.
/// Versioned backfill retains native domain notes written before marker support.
pub const QUERY_FROM: &str = "FROM (SELECT n.budget_id,n.id,CASE WHEN a.id IS NOT NULL THEN a.notes ELSE n.note END AS note,n.tombstone FROM notes n LEFT JOIN accounts a ON a.budget_id=n.budget_id AND a.id=n.id WHERE n.id NOT GLOB 'budget-[0-9][0-9][0-9][0-9]-[0-9][0-9]' AND n.id<>n.budget_id AND NOT EXISTS(SELECT 1 FROM categories c WHERE c.budget_id=n.budget_id AND c.id=n.id) AND NOT EXISTS(SELECT 1 FROM category_groups g WHERE g.budget_id=n.budget_id AND g.id=n.id)) t";
/// Once per migration or explicit portable restore, never a startup ledger scan.
pub fn domain_identities(db: &Connection, only: Option<&str>) -> Result<(), Error> {
    for table in ["accounts"] {
        let id = "id";
        let live = "";
        let mut q=db.prepare(&format!("INSERT OR IGNORE INTO notes(budget_id,id,note) SELECT budget_id,{id},'' FROM {table} WHERE (?1 IS NULL OR budget_id=?1) AND notes<>''{live} RETURNING budget_id,id"))?;
        let inserted = q
            .query_map([only], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (b, id) in inserted {
            sync::mark(db, &b, "notes", &id)?;
        }
    }
    Ok(())
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
 let id=body["id"].as_str().ok_or_else(||Error::invalid("Provide note identity"))?;let note=body["note"].as_str().ok_or_else(||Error::invalid("Provide note text"))?;validate(id,note)?;if retired_budget_note(id,&b) || retired_category_note(db,&b,id)? {return Err(Error::invalid("Budget and category notes are no longer supported"));}
 sync::check_snapshot(db,&b,Some(body["snapshot"].as_str().ok_or_else(||Error::invalid("Provide current snapshot"))?))?;
 let(table,row,field)=target(db,&b,id)?;let clock=sync::clock(db,&b,proposed.as_deref())?;crate::rules::guard(db,&b,table,&row,field,&clock)?;if table!="notes" {for f in ["note","tombstone"] {crate::rules::guard(db,&b,"notes",id,f,&clock)?;}}
 if table=="notes" && db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE budget_id=?1 AND id=?2 AND tombstone=1)"),params![b,row],|r|r.get::<_,bool>(0))? {crate::rules::guard(db,&b,table,&row,"tombstone",&clock)?;}
 if table=="notes"{db.execute("INSERT INTO notes(budget_id,id,note) VALUES(?1,?2,?3) ON CONFLICT(budget_id,id) DO UPDATE SET note=excluded.note,tombstone=0",params![b,row,note])?;}else{db.execute(&format!("UPDATE {table} SET {field}=?3 WHERE budget_id=?1 AND id=?2"),params![b,row,note])?;}
 if table!="notes" {identity(db,&b,id)?;sync::seed_field_version(db,&b,"notes",id,"note",&clock)?;sync::mark(db,&b,"notes",id)?;}
 sync::seed_field_version(db,&b,table,&row,field,&clock)?;sync::mark(db,&b,table,&row)?;
 Ok(json!({"response_status":200,"response_body":{"data":get(db,&b,id)?,"snapshot":sync::cursor(db,&b)?,"edit_clock":clock}}))
 })}).await
}
fn retired_category_note(db: &Connection, b: &str, id: &str) -> Result<bool, Error> {
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM categories WHERE budget_id=?1 AND id=?2) OR EXISTS(SELECT 1 FROM category_groups WHERE budget_id=?1 AND id=?2)", params![b,id], |r| r.get(0))?)
}
fn retired_budget_note(id: &str, budget: &str) -> bool {
    id == budget
        || id
            .strip_prefix("budget-")
            .is_some_and(|m| crate::budgeting::month(m).is_ok())
}
pub fn convert(source: &Value) -> Result<Value, Error> {
    let mut rows = vec![];
    for row in source["notes"].as_array().into_iter().flatten() {
        let id = row["id"]
            .as_str()
            .ok_or_else(|| Error::invalid("Invalid source note identity"))?;
        if retired_budget_note(id, "")
            || ["categories", "category_groups"].iter().any(|table| {
                source[*table]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|r| r["id"] == id)
            })
        {
            continue;
        }
        let note = row["note"].as_str().unwrap_or("");
        validate(id, note)?;
        let known = ["accounts"].iter().any(|table| {
            source[*table]
                .as_array()
                .into_iter()
                .flatten()
                .any(|r| r["id"] == id)
        });
        rows.push(json!({"id":id,"note":if known {""}else{note}}));
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
    domain_identities(db, only)?;
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
