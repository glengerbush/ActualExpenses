//! Payee identity and note-tag management. Tombstones retain provenance; aliases
//! resolve before ordinary transaction writes. Bulk intents commit atomically.
use crate::{
    api::{budget_id, idempotency_key, json_error},
    concurrency,
    error::Error,
    models::{validate_id, validate_name},
    store::{self, Store},
};
use axum::{
    Json, Router,
    extract::{
        OriginalUri, Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, Method, StatusCode},
    routing::{get, post},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub fn router() -> Router<Store> {
    Router::new()
        .route("/api/v1/budgets/{budget}/payees", get(list).post(mutate))
        .route(
            "/api/v1/budgets/{budget}/payees/{id}",
            get(read).patch(mutate),
        )
        .route("/api/v1/budgets/{budget}/payees/batch", post(mutate))
        .route("/api/v1/budgets/{budget}/tags", get(list).post(mutate))
        .route(
            "/api/v1/budgets/{budget}/tags/{id}",
            get(read).patch(mutate),
        )
        .route("/api/v1/budgets/{budget}/tags/batch", post(mutate))
        .route("/api/v1/budgets/{budget}/tags/discover", post(mutate))
}
fn domain(uri: &str) -> &'static str {
    if uri.contains("/tags") {
        "tags"
    } else {
        "payees"
    }
}
fn fields(body: &Value, allowed: &[&str]) -> Result<(), Error> {
    let fields = body
        .as_object()
        .ok_or_else(|| Error::invalid("Expected an object"))?;
    if fields.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(Error::invalid("Unknown management field"));
    }
    Ok(())
}
fn text<'a>(body: &'a Value, key: &str) -> Result<&'a str, Error> {
    body[key]
        .as_str()
        .ok_or_else(|| Error::invalid(format!("{key} must be text")))
}
pub fn versions(db: &Connection, b: &str, entity: &str, id: &str) -> Result<Value, Error> {
    let mut q = db.prepare(
        "SELECT field,clock FROM field_versions WHERE budget_id=?1 AND entity=?2 AND entity_id=?3",
    )?;
    Ok(Value::Object(
        q.query_map(params![b, entity, id], |r| {
            Ok((r.get::<_, String>(0)?, json!(r.get::<_, String>(1)?)))
        })?
        .collect::<Result<_, _>>()?,
    ))
}
pub fn record(db: &Connection, b: &str, entity: &str, id: &str) -> Result<Value, Error> {
    let mut value=if entity=="payees"{db.query_row("SELECT COALESCE(a.name,p.name),p.favorite,p.tombstone,p.transfer_account_id,p.category_id,p.merged_into,p.learn_categories,(SELECT count(*) FROM transactions t WHERE t.budget_id=p.budget_id AND t.payee_id=p.id AND t.tombstone=0) FROM payees p LEFT JOIN accounts a ON a.budget_id=p.budget_id AND a.id=p.transfer_account_id WHERE p.budget_id=?1 AND p.id=?2",params![b,id],|r|Ok(json!({"id":id,"name":r.get::<_,String>(0)?,"favorite":r.get::<_,bool>(1)?,"deleted":r.get::<_,bool>(2)?,"transfer_account_id":r.get::<_,Option<String>>(3)?,"category_id":r.get::<_,Option<String>>(4)?,"merged_into":r.get::<_,Option<String>>(5)?,"learn_categories":r.get::<_,bool>(6)?,"transaction_count":r.get::<_,i64>(7)?}))).optional()?}
 else{db.query_row("SELECT tag,color,description,hidden,tombstone FROM tags WHERE budget_id=?1 AND id=?2",params![b,id],|r|Ok(json!({"id":id,"tag":r.get::<_,String>(0)?,"color":r.get::<_,Option<String>>(1)?,"description":r.get::<_,Option<String>>(2)?,"hidden":r.get::<_,bool>(3)?,"deleted":r.get::<_,bool>(4)?}))).optional()?}.ok_or_else(Error::missing)?;
    value["field_versions"] = versions(db, b, entity, id)?;
    Ok(value)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    #[serde(default = "limit")]
    limit: u32,
    #[serde(default)]
    offset: u32,
    snapshot: Option<String>,
    q: Option<String>,
    #[serde(default)]
    include_deleted: bool,
    #[serde(default)]
    include_transfers: bool,
    #[serde(default)]
    include_hidden: bool,
    #[serde(default)]
    unused: bool,
}
fn limit() -> u32 {
    100
}
async fn list(
    State(s): State<Store>,
    p: Result<Path<String>, PathRejection>,
    OriginalUri(uri): OriginalUri,
    q: Result<Query<Page>, QueryRejection>,
) -> Result<Json<Value>, Error> {
    let b = budget_id(p)?;
    let q = q.map_err(|_| Error::invalid("Invalid management query"))?.0;
    if !(1..=1000).contains(&q.limit)
        || q.offset > 1000000
        || q.q.as_ref().is_some_and(|v| v.len() > 1000)
    {
        return Err(Error::invalid("Invalid pagination or search"));
    }
    let entity = domain(uri.path());
    Ok(Json(s.run(move|db|{concurrency::check_snapshot(db,&b,q.snapshot.as_deref())?;let sql=if entity=="payees"{"SELECT p.id FROM payees p LEFT JOIN accounts a ON a.budget_id=p.budget_id AND a.id=p.transfer_account_id WHERE p.budget_id=?1 AND (?2 OR p.tombstone=0) AND (?3 OR p.transfer_account_id IS NULL) AND (p.transfer_account_id IS NULL OR a.tombstone=0) AND (?4 IS NULL OR instr(lower(COALESCE(a.name,p.name)),lower(?4))>0) AND (?5=0 OR NOT EXISTS(SELECT 1 FROM transactions t WHERE t.budget_id=p.budget_id AND t.payee_id=p.id AND t.tombstone=0)) ORDER BY p.name,p.id LIMIT ?6 OFFSET ?7"}else{"SELECT id FROM tags WHERE budget_id=?1 AND (?2 OR tombstone=0) AND (?3 OR hidden=0) AND (?4 IS NULL OR instr(lower(tag),lower(?4))>0) AND (?5=0 OR 1=1) ORDER BY tag,id LIMIT ?6 OFFSET ?7"};let mut query=db.prepare(sql)?;let ids=query.query_map(params![b,q.include_deleted,if entity=="payees"{q.include_transfers}else{q.include_hidden},q.q,q.unused,q.limit+1,q.offset],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;let more=ids.len()>q.limit as usize;let rows=ids.iter().take(q.limit as usize).map(|id|record(db,&b,entity,id)).collect::<Result<Vec<_>,_>>()?;Ok(json!({"data":rows,"next_offset":if more{Some(q.offset+q.limit)}else{None}}))}).await?))
}
async fn read(
    State(s): State<Store>,
    p: Result<Path<(String, String)>, PathRejection>,
    OriginalUri(uri): OriginalUri,
) -> Result<Json<Value>, Error> {
    let Path((b, id)) = p.map_err(|_| Error::invalid("Invalid reference path"))?;
    validate_id(&b)?;
    validate_id(&id)?;
    let entity = domain(uri.path());
    Ok(Json(
        json!({"data":s.run(move|db|{store::require_budget(db,&b)?;record(db,&b,entity,&id)}).await?}),
    ))
}
async fn mutate(
    State(s): State<Store>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let parts = uri.path().split('/').collect::<Vec<_>>();
    let b = parts[4].to_owned();
    validate_id(&b)?;
    let entity = domain(uri.path());
    let action = parts.get(6).copied().unwrap_or("create").to_owned();
    let Json(body) = input.map_err(json_error)?;
    let key = idempotency_key(&headers)?;
    let owner = crate::history::owner_header(&headers)?;
    let proposed = concurrency::header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let cancel = concurrency::cancel_header(&headers)?;
    let scope = format!("{} {}", method, uri.path().trim_start_matches("/api/v1"));
    let created = action == "create";
    // Preserve schema-1 payee POST receipt payload bytes for old queued callers.
    #[derive(serde::Serialize)]
    struct LegacyPayee<'a> {
        id: &'a str,
        name: &'a str,
        is_income: Option<bool>,
    }
    let payload = if created && entity == "payees" {
        fields(&body, &["id", "name", "is_income"])?;
        if !body["is_income"].is_null() {
            return Err(Error::invalid("Payees cannot specify is_income"));
        }
        serde_json::to_string(&LegacyPayee {
            id: text(&body, "id")?,
            name: text(&body, "name")?,
            is_income: None,
        })
        .map_err(Error::internal)?
    } else {
        body.to_string()
    };
    s.run(move |db| {
        concurrency::write_owned(
            db,
            &scope,
            &key,
            &payload,
            proposed.as_deref(),
            if created {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            },
            owner.as_ref(),
            |db| {
                if cancel {crate::lifecycle::metadata(db,&b,true)?;return Ok(json!({"response_status":409,"response_body":{"error":{"code":"operation_canceled","message":"This change was canceled before it was saved."}}}));}
                crate::lifecycle::require_epoch(db, &b, epoch.as_deref())?;
                let clock = concurrency::clock(db, &b, proposed.as_deref())?;
                match action.as_str() {
                    "create" => create(db, &b, entity, &body, &clock),
                    "batch" => batch(db, &b, entity, &body, &clock),
                    "discover" if entity == "tags" => {
                        fields(&body, &[])?;
                        discover(db, &b, &clock)
                    }
                    id => {
                        validate_id(id)?;
                        patch(db, &b, entity, id, &body, &clock)
                    }
                }
            },
        )
    })
    .await
}
fn active(value: &Value) -> Result<(), Error> {
    if value["deleted"] == true {
        Err(Error::conflict(
            "This record was removed. Refresh before continuing.",
        ))
    } else if !value["transfer_account_id"].is_null() {
        Err(Error::invalid(
            "Transfer payees are managed with their accounts",
        ))
    } else {
        Ok(())
    }
}
pub(crate) fn duplicate(
    db: &Connection,
    b: &str,
    entity: &str,
    id: &str,
    name: &str,
) -> Result<(), Error> {
    let column = if entity == "payees" { "name" } else { "tag" };
    let exists: bool = db.query_row(
        &format!(
            "SELECT EXISTS(SELECT 1 FROM {entity} WHERE budget_id=?1 AND id<>?2 AND {column}=?3{})",
            if entity == "payees" {
                " AND tombstone=0 AND transfer_account_id IS NULL"
            } else {
                ""
            }
        ),
        params![b, id, name],
        |r| r.get(0),
    )?;
    if exists {
        Err(Error::conflict(
            "A record with this name already exists. Merge payees instead.",
        ))
    } else {
        Ok(())
    }
}
pub fn validate_tag(tag: &str) -> Result<(), Error> {
    if tag.is_empty()
        || tag.chars().count() > 1000
        || tag.chars().any(|c| js_whitespace(c) || c == '#')
    {
        Err(Error::invalid("A tag must be one word without #"))
    } else {
        Ok(())
    }
}
fn create(
    db: &Connection,
    b: &str,
    entity: &str,
    body: &Value,
    clock: &str,
) -> Result<Value, Error> {
    fields(
        body,
        if entity == "payees" {
            &["id", "name", "is_income"]
        } else {
            &["id", "tag", "color", "description"]
        },
    )?;
    let id = text(body, "id")?;
    validate_id(id)?;
    let column = if entity == "payees" { "name" } else { "tag" };
    let name = text(body, column)?.trim_matches(js_whitespace);
    if entity == "payees" {
        validate_name(name)?
    } else {
        validate_tag(name)?
    }
    if entity == "tags" {
        let existing: Option<String> = db
            .query_row(
                "SELECT id FROM tags WHERE budget_id=?1 AND tag=?2",
                params![b, name],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            let collision: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM tags WHERE budget_id=?1 AND id=?2 AND tag<>?3)",
                params![b, id, name],
                |r| r.get(0),
            )?;
            if collision {
                return Err(Error::conflict("Reference ID already exists"));
            }
            let before = record(db, b, entity, &existing)?;
            if before["field_versions"]["tombstone"]
                .as_str()
                .unwrap_or(concurrency::ZERO)
                >= clock
            {
                return Err(Error::conflict(
                    "A newer tag removal exists. Refresh first.",
                ));
            }
            db.execute(
                "UPDATE tags SET tombstone=0 WHERE budget_id=?1 AND id=?2",
                params![b, existing],
            )?;
            concurrency::seed_field_version(db, b, entity, &existing, "tombstone", clock)?;
            self::patch(
                db,
                b,
                entity,
                &existing,
                &json!({"color":body.get("color").unwrap_or(&Value::Null),"description":body.get("description").unwrap_or(&Value::Null)}),
                clock,
            )?;
            return record(db, b, entity, &existing);
        }
    }
    duplicate(db, b, entity, id, name)?;
    if db.query_row(
        &format!("SELECT EXISTS(SELECT 1 FROM {entity} WHERE budget_id=?1 AND id=?2)"),
        params![b, id],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Error::conflict("Reference ID already exists"));
    }
    db.execute(
        &format!("INSERT INTO {entity}(budget_id,id,{column}) VALUES(?1,?2,?3)"),
        params![b, id, name],
    )?;
    concurrency::seed_field_version(db, b, entity, id, column, clock)?;
    if entity == "tags" {
        let mut patch = body.clone();
        patch.as_object_mut().unwrap().remove("id");
        patch.as_object_mut().unwrap().remove("tag");
        if !patch.as_object().unwrap().is_empty() {
            self::patch(db, b, entity, id, &patch, clock)?;
        }
    }
    record(db, b, entity, id)
}
fn patch(
    db: &Connection,
    b: &str,
    entity: &str,
    id: &str,
    body: &Value,
    clock: &str,
) -> Result<Value, Error> {
    fields(
        body,
        if entity == "payees" {
            &["name", "favorite", "learn_categories"]
        } else {
            &["tag", "color", "description", "hidden"]
        },
    )?;
    if body.as_object().unwrap().is_empty() {
        return Err(Error::invalid("Supply a field to update"));
    }
    let before = record(db, b, entity, id)?;
    active(&before)?;
    let mut conflicts = vec![];
    for (field, value) in body.as_object().unwrap() {
        let value = match field.as_str() {
            "name" | "tag" => {
                let v = value
                    .as_str()
                    .ok_or_else(|| Error::invalid("Name must be text"))?
                    .trim_matches(js_whitespace);
                if field == "name" {
                    validate_name(v)?
                } else {
                    validate_tag(v)?
                }
                json!(v)
            }
            "favorite" | "hidden" | "learn_categories" => {
                if !value.is_boolean() {
                    return Err(Error::invalid("Expected a boolean"));
                }
                value.clone()
            }
            "color" => {
                if !value.is_null()
                    && !value.as_str().is_some_and(|v| {
                        v.len() == 7
                            && v.starts_with('#')
                            && v[1..].bytes().all(|c| c.is_ascii_hexdigit())
                    })
                {
                    return Err(Error::invalid("color must be #RRGGBB or null"));
                }
                value.clone()
            }
            "description" => {
                if !value.is_null() && !value.as_str().is_some_and(|v| v.chars().count() <= 10000) {
                    return Err(Error::invalid("description must be text or null"));
                }
                value.clone()
            }
            _ => unreachable!(),
        };
        let old = before["field_versions"][field]
            .as_str()
            .unwrap_or(concurrency::ZERO);
        if clock <= old {
            conflicts.push(json!({"field":field,"winning_clock":old,"submitted_clock":clock}));
            continue;
        }
        if matches!(field.as_str(), "name" | "tag") {
            duplicate(db, b, entity, id, value.as_str().unwrap())?;
        }
        if field == "tag" && value != before["tag"] {
            rename_notes(
                db,
                b,
                before["tag"].as_str().unwrap(),
                value.as_str().unwrap(),
                clock,
            )?;
        }
        let sql = match value {
            Value::Null => rusqlite::types::Value::Null,
            Value::String(v) => rusqlite::types::Value::Text(v),
            Value::Bool(v) => rusqlite::types::Value::Integer(i64::from(v)),
            _ => unreachable!(),
        };
        db.execute(
            &format!("UPDATE {entity} SET {field}=?3 WHERE budget_id=?1 AND id=?2"),
            params![b, id, sql],
        )?;
        concurrency::seed_field_version(db, b, entity, id, field, clock)?;
    }
    Ok(
        json!({"response_status":200,"response_body":{"data":record(db,b,entity,id)?,"conflicts":conflicts,"cursor":concurrency::cursor(db,b)?}}),
    )
}
fn batch(
    db: &Connection,
    b: &str,
    entity: &str,
    body: &Value,
    clock: &str,
) -> Result<Value, Error> {
    fields(body, &["action", "ids", "target_id"])?;
    let action = text(body, "action")?;
    let ids = body["ids"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 1000)
        .ok_or_else(|| Error::invalid("Select 1–1000 records"))?;
    let mut seen = BTreeSet::new();
    let target = if action == "merge" && entity == "payees" {
        let target = text(body, "target_id")?;
        validate_id(target)?;
        active(&record(db, b, entity, target)?)?;
        Some(target)
    } else {
        if body.get("target_id").is_some() {
            return Err(Error::invalid("target_id is only valid for merging payees"));
        }
        None
    };
    if !(entity == "payees" && ["merge", "remove", "favorite", "unfavorite"].contains(&action)
        || entity == "tags" && ["hide", "show", "remove"].contains(&action))
    {
        return Err(Error::invalid("Unsupported management action"));
    }
    for id in ids {
        let id = id.as_str().ok_or_else(|| Error::invalid("Expected UUID"))?;
        validate_id(id)?;
        if !seen.insert(id) || Some(id) == target {
            return Err(Error::invalid(
                "Select distinct source records, excluding the merge target",
            ));
        }
        let value = record(db, b, entity, id)?;
        active(&value)?;
        if value["field_versions"]
            .as_object()
            .unwrap()
            .values()
            .any(|v| v.as_str().unwrap_or(concurrency::ZERO) > clock)
        {
            return Err(Error::conflict(
                "Newer changes exist. Refresh before managing these records.",
            ));
        }
    }
    for id in &seen {
        let field = match action {
            "merge" | "remove" => "tombstone",
            "favorite" | "unfavorite" => "favorite",
            _ => "hidden",
        };
        let value = !matches!(action, "show" | "unfavorite");
        db.execute(
            &format!("UPDATE {entity} SET {field}=?3 WHERE budget_id=?1 AND id=?2"),
            params![b, id, value],
        )?;
        concurrency::seed_field_version(db, b, entity, id, field, clock)?;
        if let Some(target) = target {
            db.execute(
                "UPDATE payees SET merged_into=?3 WHERE budget_id=?1 AND (id=?2 OR merged_into=?2)",
                params![b, id, target],
            )?;
            db.execute(
                "UPDATE transactions SET payee_id=?3 WHERE budget_id=?1 AND payee_id=?2",
                params![b, id, target],
            )?;
            concurrency::seed_field_version(db, b, entity, id, "merged_into", clock)?;
        }
    }
    Ok(json!({"ids":seen,"action":action,"target_id":target}))
}
/// Exact pinned notes token boundaries, including escaped ## and adjacent tags.
pub fn tags_in(notes: &str) -> Vec<(usize, usize, &str)> {
    let mut tags = vec![];
    let mut previous = None;
    for (i, c) in notes.char_indices() {
        if c == '#' && previous != Some('#') {
            let start = i + 1;
            let end = notes[start..]
                .char_indices()
                .find(|(_, c)| *c == '#' || js_whitespace(*c))
                .map(|(n, _)| start + n)
                .unwrap_or(notes.len());
            if end > start {
                tags.push((i, end, &notes[start..end]));
            }
        }
        previous = Some(c);
    }
    tags
}
fn rename_notes(db: &Connection, b: &str, old: &str, new: &str, clock: &str) -> Result<(), Error> {
    let mut q=db.prepare("SELECT id,notes FROM transactions WHERE budget_id=?1 AND tombstone=0 AND instr(notes,'#')>0")?;
    let rows = q
        .query_map([b], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, notes) in rows {
        let matching = tags_in(&notes)
            .into_iter()
            .filter(|(_, _, tag)| *tag == old)
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        let versions = concurrency::versions(db, b, &id)?;
        if versions["notes"].as_str().unwrap_or(concurrency::ZERO) > clock {
            return Err(Error::conflict(
                "Newer transaction notes prevent this rename. Refresh first.",
            ));
        }
        let mut renamed = String::new();
        let mut from = 0;
        for (start, end, _) in matching {
            renamed.push_str(&notes[from..start]);
            renamed.push('#');
            renamed.push_str(new);
            from = end;
        }
        renamed.push_str(&notes[from..]);
        if renamed.chars().count() > 10000 {
            return Err(Error::invalid("Renamed notes exceed 10000 characters"));
        }
        db.execute(
            "UPDATE transactions SET notes=?3 WHERE budget_id=?1 AND id=?2",
            params![b, id, renamed],
        )?;
        concurrency::seed_field_version(db, b, "transactions", &id, "notes", clock)?;
    }
    Ok(())
}
fn discover(db: &Connection, b: &str, clock: &str) -> Result<Value, Error> {
    let mut q = db.prepare(
        "SELECT notes FROM transactions WHERE budget_id=?1 AND tombstone=0 AND instr(notes,'#')>0",
    )?;
    let notes = q
        .query_map([b], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut found = BTreeSet::new();
    for note in &notes {
        for (_, _, tag) in tags_in(note) {
            found.insert(tag);
        }
    }
    let mut created = vec![];
    for tag in found {
        validate_tag(tag)?;
        let existing: Option<String> = db
            .query_row(
                "SELECT id FROM tags WHERE budget_id=?1 AND tag=?2 AND tombstone=0",
                params![b, tag],
                |r| r.get(0),
            )
            .optional()?;
        if existing.is_none() {
            let id = uuid::Uuid::new_v4().to_string();
            created.push(create(db, b, "tags", &json!({"id":id,"tag":tag}), clock)?);
        }
    }
    Ok(json!({"created":created}))
}
pub fn canonical(db: &Connection, b: &str, id: &str) -> Result<String, Error> {
    let mut id = id.to_owned();
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(id.clone()) {
            return Err(Error::invalid("Cyclic payee aliases"));
        }
        let v = record(db, b, "payees", &id)?;
        match v["merged_into"].as_str() {
            Some(next) => id = next.into(),
            None => return Ok(id),
        }
    }
}
pub fn validate_archive(db: &Connection, b: &str) -> Result<(), Error> {
    let mut q = db.prepare("SELECT id FROM payees WHERE budget_id=?1")?;
    for id in q.query_map([b], |r| r.get::<_, String>(0))? {
        let id = id?;
        canonical(db, b, &id)?;
        let p = record(db, b, "payees", &id)?;
        if !p["merged_into"].is_null()
            && (p["deleted"] != true || !p["transfer_account_id"].is_null())
        {
            return Err(Error::invalid("Invalid merged payee identity"));
        }
        if let Some(a) = p["transfer_account_id"].as_str()
            && !db.query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE budget_id=?1 AND id=?2)",
                params![b, a],
                |r| r.get::<_, bool>(0),
            )?
        {
            return Err(Error::invalid("Missing transfer account"));
        }
        if let Some(c) = p["category_id"].as_str()
            && !db.query_row(
                "SELECT EXISTS(SELECT 1 FROM categories WHERE budget_id=?1 AND id=?2)",
                params![b, c],
                |r| r.get::<_, bool>(0),
            )?
        {
            return Err(Error::invalid("Missing payee category"));
        }
    }
    let mut q = db.prepare("SELECT id,tag FROM tags WHERE budget_id=?1")?;
    for row in q.query_map([b], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (id, tag) = row?;
        validate_id(&id)?;
        validate_tag(&tag)?;
    }
    Ok(())
}

// ECMAScript \s, as used by pinned Actual (Rust's Unicode whitespace differs).
pub(crate) fn js_whitespace(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
