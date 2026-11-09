//! Category organization. Deleted rows retain notes and redirect stale references.
use crate::{
    api::{budget_id, idempotency_key, json_error},
    concurrency as sync,
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
    routing::get as route_get,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
const CATEGORY_FIELDS: &[&str] = &[
    "name",
    "group_id",
    "is_income",
    "hidden",
    "notes",
    "sort_order",
];
const GROUP_FIELDS: &[&str] = &["name", "is_income", "hidden", "notes", "sort_order"];
pub fn router() -> Router<Store> {
    Router::new()
        .route(
            "/api/v1/budgets/{budget}/categories",
            route_get(list).post(create),
        )
        .route(
            "/api/v1/budgets/{budget}/category-groups",
            route_get(list).post(create),
        )
        .route(
            "/api/v1/budgets/{budget}/categories/{id}/transactions",
            route_get(transactions),
        )
        .route(
            "/api/v1/budgets/{budget}/categories/{id}",
            route_get(detail).patch(edit).delete(remove),
        )
        .route(
            "/api/v1/budgets/{budget}/category-groups/{id}",
            route_get(detail).patch(edit).delete(remove),
        )
}
fn table(uri: &str) -> &'static str {
    if uri.contains("/category-groups") {
        "category_groups"
    } else {
        "categories"
    }
}
pub fn validate_group_id(id: &str) -> Result<(), Error> {
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| Error::invalid("Invalid group ID"))?;
    if parsed.to_string() != id.to_ascii_lowercase() {
        return Err(Error::invalid("Invalid group ID"));
    }
    Ok(())
}
fn fields(table: &str) -> &'static [&'static str] {
    if table == "categories" {
        CATEGORY_FIELDS
    } else {
        GROUP_FIELDS
    }
}
pub fn get(db: &Connection, budget: &str, table: &str, id: &str) -> Result<Value, Error> {
    let columns = if table == "categories" {
        "name,is_income,hidden,notes,sort_order,tombstone,group_id,replacement_id"
    } else {
        "name,is_income,hidden,notes,sort_order,tombstone,NULL,NULL"
    };
    let mut row=db.query_row(&format!("SELECT {columns} FROM {table} WHERE budget_id=?1 AND id=?2"),params![budget,id],|r|Ok(json!({"id":id,"name":r.get::<_,String>(0)?,"is_income":r.get::<_,bool>(1)?,"hidden":r.get::<_,bool>(2)?,"notes":r.get::<_,String>(3)?,"sort_order":r.get::<_,Option<f64>>(4)?,"deleted":r.get::<_,bool>(5)?,"group_id":r.get::<_,Option<String>>(6)?,"replacement_id":r.get::<_,Option<String>>(7)?}))).optional()?.ok_or_else(Error::missing)?;
    let mut versions = serde_json::Map::new();
    let mut q = db.prepare(
        "SELECT field,clock FROM field_versions WHERE budget_id=?1 AND entity=?2 AND entity_id=?3",
    )?;
    for v in q.query_map(params![budget, table, id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (k, v) = v?;
        versions.insert(k, json!(v));
    }
    row["field_versions"] = json!(versions);
    if table == "categories" {
        row["group_hidden"] = json!(
            db.query_row(
                "SELECT hidden FROM category_groups WHERE budget_id=?1 AND id=?2",
                params![budget, row["group_id"].as_str()],
                |r| r.get::<_, bool>(0)
            )
            .optional()?
            .unwrap_or(false)
        );
    }
    Ok(row)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    #[serde(default = "limit")]
    limit: u32,
    #[serde(default)]
    offset: u32,
    #[serde(default)]
    include_deleted: bool,
    snapshot: Option<String>,
}
fn limit() -> u32 {
    100
}
async fn list(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    OriginalUri(uri): OriginalUri,
    query: Result<Query<Page>, QueryRejection>,
) -> Result<Json<Value>, Error> {
    let budget = budget_id(path)?;
    let Query(page) = query.map_err(|_| Error::invalid("Invalid category pagination"))?;
    if page.limit == 0 || page.limit > 1000 || page.offset > 1000000 {
        return Err(Error::invalid("Invalid category pagination"));
    }
    let table = table(uri.path());
    Ok(Json(store.run(move|db|{sync::check_snapshot(db,&budget,page.snapshot.as_deref())?;let mut q=db.prepare(&format!("SELECT id FROM {table} WHERE budget_id=?1 AND (?4 OR tombstone=0) ORDER BY sort_order,name,id LIMIT ?2 OFFSET ?3"))?;let ids=q.query_map(params![budget,page.limit+1,page.offset,page.include_deleted],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;let more=ids.len()>page.limit as usize;let data=ids.iter().take(page.limit as usize).map(|id|get(db,&budget,table,id)).collect::<Result<Vec<_>,_>>()?;Ok(json!({"data":data,"next_offset":if more{Some(page.offset+page.limit)}else{None}}))}).await?))
}
async fn detail(
    State(store): State<Store>,
    Path((budget, id)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
) -> Result<Json<Value>, Error> {
    validate_id(&budget)?;
    let table = table(uri.path());
    if table == "category_groups" {
        validate_group_id(&id)?;
    } else {
        validate_id(&id)?;
    }
    Ok(Json(
        store
            .run(move |db| {
                store::require_budget(db, &budget)?;
                Ok(json!({"data":get(db,&budget,table,&id)?}))
            })
            .await?,
    ))
}
fn validate(value: &Value, table: &str, create: bool) -> Result<(), Error> {
    let map = value
        .as_object()
        .ok_or_else(|| Error::invalid("Expected category object"))?;
    if map.is_empty() {
        return Err(Error::invalid("No changes supplied"));
    }
    for (k, v) in map {
        if create && k == "id" {
            validate_id(v.as_str().ok_or_else(|| Error::invalid("Invalid ID"))?)?;
            continue;
        }
        if !fields(table).contains(&k.as_str()) {
            return Err(Error::invalid(format!("Unknown category field: {k}")));
        }
        match k.as_str() {
            "name" => validate_name(
                v.as_str()
                    .ok_or_else(|| Error::invalid("Name must be text"))?,
            )?,
            "notes" => {
                if v.as_str().is_none_or(|s| s.chars().count() > 10000) {
                    return Err(Error::invalid(
                        "Notes must contain at most 10000 characters",
                    ));
                }
            }
            "hidden" | "is_income" => {
                if !v.is_boolean() && !(create && k == "is_income" && v.is_null()) {
                    return Err(Error::invalid("Expected boolean"));
                }
            }
            "sort_order" if !v.is_null() => {
                if v.as_f64()
                    .is_none_or(|n| !n.is_finite() || n.abs() > 9007199254740991.0)
                {
                    return Err(Error::invalid("Invalid category order"));
                }
            }
            "group_id" if !v.is_null() => {
                validate_group_id(
                    v.as_str()
                        .ok_or_else(|| Error::invalid("Invalid group ID"))?,
                )?;
            }
            _ => {}
        }
    }
    if create {
        validate_id(
            value["id"]
                .as_str()
                .ok_or_else(|| Error::invalid("ID required"))?,
        )?;
        validate_name(
            value["name"]
                .as_str()
                .ok_or_else(|| Error::invalid("Name required"))?,
        )?;
    }
    Ok(())
}
fn membership(db: &Connection, budget: &str, value: &Value, table: &str) -> Result<(), Error> {
    if table == "categories" {
        if let Some(group) = value["group_id"].as_str() {
            let g = get(db, budget, "category_groups", group)?;
            if g["deleted"] == true || g["is_income"] != value["is_income"] {
                return Err(Error::invalid(
                    "Category and active group must have the same income/expense type",
                ));
            }
        }
    }
    Ok(())
}
fn sqlvalue(value: &Value) -> rusqlite::types::Value {
    match value {
        Value::Null => rusqlite::types::Value::Null,
        Value::Bool(v) => rusqlite::types::Value::Integer(i64::from(*v)),
        Value::Number(v) => rusqlite::types::Value::Real(v.as_f64().unwrap()),
        Value::String(v) => rusqlite::types::Value::Text(v.clone()),
        _ => unreachable!(),
    }
}
async fn create(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let budget = budget_id(path)?;
    let Json(input) = input.map_err(json_error)?;
    let table = table(uri.path());
    validate(&input, table, true)?;
    write(store, budget, table, None, Method::POST, headers, input).await
}
async fn edit(
    State(store): State<Store>,
    Path((budget, id)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let Json(input) = input.map_err(json_error)?;
    let table = table(uri.path());
    validate(&input, table, false)?;
    write(
        store,
        budget,
        table,
        Some(id),
        Method::PATCH,
        headers,
        input,
    )
    .await
}
async fn remove(
    State(store): State<Store>,
    Path((budget, id)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let Json(input) = input.map_err(json_error)?;
    let table = table(uri.path());
    if input
        .as_object()
        .is_none_or(|m| m.keys().any(|k| k != "replacement_id"))
    {
        return Err(Error::invalid(
            "Supply replacement_id or null to uncategorize history",
        ));
    }
    if let Some(v) = input.get("replacement_id") {
        if !v.is_null() {
            validate_id(
                v.as_str()
                    .ok_or_else(|| Error::invalid("Invalid replacement ID"))?,
            )?;
        }
    } else {
        return Err(Error::invalid("Explicit replacement_id is required"));
    }
    write(
        store,
        budget,
        table,
        Some(id),
        Method::DELETE,
        headers,
        input,
    )
    .await
}
async fn write(
    store: Store,
    budget: String,
    table: &'static str,
    id: Option<String>,
    method: Method,
    headers: HeaderMap,
    input: Value,
) -> Result<(StatusCode, Json<Value>), Error> {
    validate_id(&budget)?;
    if let Some(id) = &id {
        if table == "category_groups" {
            validate_group_id(id)?;
        } else {
            validate_id(id)?;
        }
    }
    let key = idempotency_key(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let proposed = sync::header(&headers)?;
    let payload = if method == Method::POST
        && table == "categories"
        && input
            .as_object()
            .unwrap()
            .keys()
            .all(|k| ["id", "name", "is_income"].contains(&k.as_str()))
    {
        #[derive(serde::Serialize)]
        struct Legacy<'a> {
            id: &'a Value,
            name: &'a Value,
            is_income: &'a Value,
        }
        serde_json::to_string(&Legacy {
            id: &input["id"],
            name: &input["name"],
            is_income: &input["is_income"],
        })
        .map_err(Error::internal)?
    } else {
        input.to_string()
    };
    let resource = if table == "categories" {
        "categories"
    } else {
        "category-groups"
    };
    let scope = format!(
        "{method} /budgets/{budget}/{resource}{}",
        id.as_ref().map(|id| format!("/{id}")).unwrap_or_default()
    );
    let status = if method == Method::POST {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    store
        .run(move |db| {
            sync::write(
                db,
                &scope,
                &key,
                &payload,
                proposed.as_deref(),
                status,
                |db| {
                    execute(
                        db,
                        &budget,
                        table,
                        id,
                        Intent {
                            method,
                            input: &input,
                            epoch: epoch.as_deref(),
                            proposed: proposed.as_deref(),
                        },
                    )
                },
            )
        })
        .await
}
struct Intent<'a> {
    method: Method,
    input: &'a Value,
    epoch: Option<&'a str>,
    proposed: Option<&'a str>,
}
fn execute(
    db: &Connection,
    budget: &str,
    table: &str,
    id: Option<String>,
    intent: Intent<'_>,
) -> Result<Value, Error> {
    let Intent {
        method,
        input,
        epoch,
        proposed,
    } = intent;
    crate::lifecycle::require_epoch(db, budget, epoch)?;
    let clock = sync::clock(db, budget, proposed)?;
    let id = id.unwrap_or_else(|| input["id"].as_str().unwrap().to_owned());
    let mut conflicts = vec![];
    let mut changed = true;
    if method == Method::POST {
        if db.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE budget_id=?1 AND id=?2)"),
            params![budget, id],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(Error::conflict("Category identity already exists"));
        }
        let mut value = json!({"name":input["name"],"is_income":false,"hidden":false,"notes":"","sort_order":0,"group_id":null});
        for (k, v) in input.as_object().unwrap() {
            value[k] = v.clone();
        }
        if input["is_income"].is_null() {
            value["is_income"] = json!(false);
            if let Some(group) = value["group_id"].as_str() {
                value["is_income"] =
                    get(db, budget, "category_groups", group)?["is_income"].clone();
            }
        }
        membership(db, budget, &value, table)?;
        db.execute(&format!("INSERT INTO {table}(budget_id,id,name,is_income,hidden,notes,sort_order) VALUES(?1,?2,?3,?4,?5,?6,?7)"),params![budget,id,value["name"].as_str(),value["is_income"].as_bool(),value["hidden"].as_bool(),value["notes"].as_str(),value["sort_order"].as_f64()])?;
        if table == "categories" {
            db.execute(
                "UPDATE categories SET group_id=?3 WHERE budget_id=?1 AND id=?2",
                params![budget, id, value["group_id"].as_str()],
            )?;
        }
        for field in fields(table) {
            sync::seed_field_version(db, budget, table, &id, field, &clock)?;
        }
    } else {
        let previous = get(db, budget, table, &id)?;
        if previous["deleted"] == true {
            return Err(Error::conflict(
                "Category or group was deleted; reload current state",
            ));
        }
        if method == Method::DELETE {
            delete(db, budget, table, &id, input, &clock, &previous)?;
        } else {
            let mut value = previous.clone();
            let mut accepted = vec![];
            for (k, v) in input.as_object().unwrap() {
                let winning = previous["field_versions"][k].as_str().unwrap_or(sync::ZERO);
                if clock.as_str() > winning {
                    value[k] = v.clone();
                    accepted.push(k);
                } else {
                    conflicts
                        .push(json!({"field":k,"submitted_clock":clock,"winning_clock":winning}));
                }
            }
            if value["group_id"] != previous["group_id"] {
                membership(db, budget, &value, table)?;
            }
            if value["is_income"] != previous["is_income"] {
                return Err(Error::invalid(
                    "Income/expense type is immutable; move within a matching group",
                ));
            }
            changed = !accepted.is_empty();
            for field in accepted {
                db.execute(
                    &format!("UPDATE {table} SET {field}=?3 WHERE budget_id=?1 AND id=?2"),
                    params![budget, id, sqlvalue(&value[field])],
                )?;
                sync::seed_field_version(db, budget, table, &id, field, &clock)?;
            }
        }
    }
    if changed {
        sync::mark(db, budget, table, &id)?;
    }
    Ok(
        json!({"response_status":if method==Method::POST{201}else{200},"response_body":{"data":get(db,budget,table,&id)?,"edit_clock":clock,"conflicts":conflicts,"cursor":sync::cursor(db,budget)?}}),
    )
}

fn delete(
    db: &Connection,
    budget: &str,
    table: &str,
    id: &str,
    input: &Value,
    clock: &str,
    previous: &Value,
) -> Result<(), Error> {
    if previous["field_versions"]
        .as_object()
        .unwrap()
        .values()
        .any(|v| v.as_str().is_some_and(|v| v > clock))
    {
        return Err(Error::conflict(
            "A newer edit superseded deletion; review current state",
        ));
    }
    let replacement = input["replacement_id"].as_str();
    if let Some(target) = replacement {
        let dest = get(db, budget, "categories", target)?;
        if dest["deleted"] == true
            || dest["is_income"] != previous["is_income"]
            || target == id
            || (table == "category_groups" && dest["group_id"] == id)
        {
            return Err(Error::invalid(
                "Choose an active category of the same type outside the deleted category/group",
            ));
        }
    }
    let ids = if table == "categories" {
        vec![id.to_owned()]
    } else {
        let mut q = db.prepare(
            "SELECT id FROM categories WHERE budget_id=?1 AND group_id=?2 AND tombstone=0",
        )?;
        q.query_map(params![budget, id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };
    let structural_clock = sync::clock(db, budget, None)?;
    for category in ids {
        let source = get(db, budget, "categories", &category)?;
        if source["field_versions"]
            .as_object()
            .unwrap()
            .values()
            .any(|v| v.as_str().is_some_and(|v| v > clock))
        {
            return Err(Error::conflict(
                "A category was edited after this group deletion; review current state",
            ));
        }
        if let Some(target) = replacement {
            if get(db, budget, "categories", target)?["is_income"] != source["is_income"] {
                return Err(Error::invalid(
                    "Group contains different category types; reassign categories separately",
                ));
            }
        }
        let mut q =
            db.prepare("SELECT id FROM transactions WHERE budget_id=?1 AND category_id=?2")?;
        let transactions = q
            .query_map(params![budget, category], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        db.execute(
            "UPDATE transactions SET category_id=?3 WHERE budget_id=?1 AND category_id=?2",
            params![budget, category, replacement],
        )?;
        // Structural reassignment must win over delayed transaction references to retired IDs.
        for tx in transactions {
            sync::seed_field_version(
                db,
                budget,
                "transactions",
                &tx,
                "category_id",
                &structural_clock,
            )?;
        }
        db.execute(
            "UPDATE categories SET tombstone=1,replacement_id=?3 WHERE budget_id=?1 AND id=?2",
            params![budget, category, replacement],
        )?;
        sync::seed_field_version(db, budget, "categories", &category, "tombstone", clock)?;
        sync::mark(db, budget, "categories", &category)?;
    }
    db.execute(
        &format!("UPDATE {table} SET tombstone=1 WHERE budget_id=?1 AND id=?2"),
        params![budget, id],
    )?;
    sync::seed_field_version(db, budget, table, id, "tombstone", clock)?;
    Ok(())
}
/// Resolve retired category IDs for delayed clients without resurrecting retired categories.
pub fn resolve(db: &Connection, budget: &str, id: Option<String>) -> Result<Option<String>, Error> {
    let mut current = id;
    for _ in 0..128 {
        let Some(id) = &current else { return Ok(None) };
        let row = get(db, budget, "categories", id).map_err(|error| {
            if error.code == "not_found" {
                Error::invalid("Unknown category reference in this budget")
            } else {
                error
            }
        })?;
        if row["deleted"] != true {
            return Ok(current);
        }
        current = row["replacement_id"].as_str().map(str::to_owned);
    }
    Err(Error::invalid("Category redirect cycle"))
}
pub fn validate_archive(db: &Connection, budget: &str) -> Result<(), Error> {
    for table in ["categories", "category_groups"] {
        let mut q = db.prepare(&format!("SELECT id FROM {table} WHERE budget_id=?1"))?;
        for id in q.query_map([budget], |r| r.get::<_, String>(0))? {
            let id = id?;
            let row = get(db, budget, table, &id)?;
            if row["notes"].as_str().unwrap().chars().count() > 10000 {
                return Err(Error::invalid("Category notes exceed limit"));
            }
            if row["deleted"] != true && table == "categories" {
                if let Some(group) = row["group_id"].as_str() {
                    if get(db, budget, "category_groups", group)?["deleted"] == true {
                        return Err(Error::invalid("Active category references a deleted group"));
                    }
                }
            }
            if table == "categories" {
                resolve(db, budget, Some(id))?;
            }
        }
    }
    if db.query_row("SELECT EXISTS(SELECT 1 FROM transactions t JOIN categories c ON c.budget_id=t.budget_id AND c.id=t.category_id WHERE t.budget_id=?1 AND c.tombstone=1)",[budget],|r|r.get::<_,bool>(0))?{return Err(Error::invalid("Transaction references a deleted category"));}
    Ok(())
}
async fn transactions(
    State(store): State<Store>,
    Path((budget, id)): Path<(String, String)>,
    query: Result<Query<Page>, QueryRejection>,
) -> Result<Json<Value>, Error> {
    validate_id(&budget)?;
    if id != "uncategorized" {
        validate_id(&id)?;
    }
    let Query(page) = query.map_err(|_| Error::invalid("Invalid pagination"))?;
    if page.limit == 0 || page.limit > 1000 || page.offset > 1000000 {
        return Err(Error::invalid("Invalid pagination"));
    }
    Ok(Json(store.run(move|db|{sync::check_snapshot(db,&budget,page.snapshot.as_deref())?;let category=if id=="uncategorized"{None}else{Some(id.as_str())};
 if let Some(id)=category{get(db,&budget,"categories",id)?;}
 let mut q=db.prepare("SELECT id,amount FROM transactions WHERE budget_id=?1 AND category_id IS ?2 AND tombstone=0 ORDER BY date DESC,sort_order DESC,id")?;
 let mut total=0i128;let mut data=vec![];let mut count=0;
 for row in q.query_map(params![budget,category],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?)))?{let(id,amount)=row?;total+=amount as i128;if count>=page.offset&&count<page.offset+page.limit{data.push(crate::ledger::get(db,&budget,&id)?);}count+=1;}
 if total.abs()>9007199254740991{return Err(Error::invalid("Category total exceeds exact money range"));}
 Ok(json!({"data":data,"total":total as i64,"count":count,"next_offset":if count>page.offset+page.limit{Some(page.offset+page.limit)}else{None},"snapshot":sync::cursor(db,&budget)?}))}).await?))
}
pub fn stamp_new(db: &Connection, budget: &str, id: &str) -> Result<(), Error> {
    let clock = sync::clock(db, budget, None)?;
    for field in CATEGORY_FIELDS {
        sync::seed_field_version(db, budget, "categories", id, field, &clock)?;
    }
    Ok(())
}
/// Upgrade only metadata absent before category management. Financial records and
/// existing names/types/field winners are never reconstructed from old source files.
pub fn enrich_legacy_imports(db: &Connection, only_budget: Option<&str>) -> Result<(), Error> {
    let mut q=db.prepare("SELECT i.budget_id,s.original FROM imported_sources i JOIN staged_imports s ON s.id=i.import_id WHERE ?1 IS NULL OR i.budget_id=?1")?;
    let sources = q
        .query_map([only_budget], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (budget, bytes) in sources {
        let (_, data) = crate::imports::conversion::inspect(&bytes)?;
        for group in data["category_groups"].as_array().unwrap() {
            db.execute("INSERT OR IGNORE INTO category_groups(budget_id,id,name,is_income,hidden,notes,sort_order,tombstone) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![budget,group["id"].as_str(),group["name"].as_str(),group["is_income"].as_bool(),group["hidden"].as_bool(),group["notes"].as_str(),group["sort_order"].as_f64(),group["tombstone"].as_bool()])?;
        }
        for category in data["categories"].as_array().unwrap() {
            for field in ["group_id", "hidden", "notes", "sort_order"] {
                db.execute(&format!("UPDATE categories SET {field}=?3 WHERE budget_id=?1 AND id=?2 AND NOT EXISTS(SELECT 1 FROM field_versions WHERE budget_id=?1 AND entity='categories' AND entity_id=?2 AND field=?4)"),params![budget,category["id"].as_str(),sqlvalue(&category[field]),field])?;
            }
        }
        // Raw imported clocks remain original evidence. Add ordering only for new fields.
        for row in data["clocks"].as_array().unwrap() {
            let dataset = row["dataset"].as_str().unwrap();
            let field = row["column"].as_str().unwrap();
            let mapped = match (dataset, field) {
                ("categories", "cat_group") => Some(("categories", "group_id")),
                ("categories", "hidden" | "sort_order") => Some(("categories", field)),
                (
                    "category_groups",
                    "name" | "is_income" | "hidden" | "sort_order" | "tombstone",
                ) => Some(("category_groups", field)),
                _ => None,
            };
            if let Some((entity, field)) = mapped {
                sync::seed_field_version(
                    db,
                    &budget,
                    entity,
                    row["row"].as_str().unwrap(),
                    field,
                    row["timestamp"].as_str().unwrap(),
                )?;
            }
        }
        validate_archive(db, &budget)?;
    }
    Ok(())
}
