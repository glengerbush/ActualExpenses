use super::*;
use rusqlite::params;
pub(super) fn category(db: &Connection, b: &str, id: &str) -> Result<Value, Error> {
    let row = crate::categories::get(db, b, "categories", id)?;
    if row["deleted"] == true {
        return Err(Error::missing());
    }
    let config = storage::config(db, b, id)?;
    let notes = row["notes"].as_str().unwrap_or("");
    let parsed = parse::notes(notes)?;
    let ui = config["template_settings"]["source"] == "ui";
    Ok(
        json!({"id":id,"name":row["name"],"notes":notes,"source":if ui{"ui"}else{"notes"},"templates":if ui{config["goal_def"].as_array().cloned().unwrap_or_default()}else{parsed["templates"].as_array().unwrap().clone()},"cleanup":config["cleanup_def"].as_array().cloned().unwrap_or_default(),"notes_templates":parsed["templates"],"parse_errors":parsed["errors"],"template_settings":config["template_settings"],"field_versions":crate::payees_tags::versions(db,b,"budget_automations",id)?}),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Read {
    month: String,
}
pub(crate) async fn read(
    State(store): State<Store>,
    Path(b): Path<String>,
    input: Result<axum::extract::Query<Read>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, Error> {
    let axum::extract::Query(query) = input.map_err(|e| Error::invalid(e.body_text()))?;
    crate::models::validate_id(&b)?;
    super::super::month(&query.month)?;
    store.run(move|db|{let tx=db.transaction()?;let view=super::super::view(&tx,&b,&query.month,1)?;let mut categories=vec![];for row in view["months"][&query.month]["categories"].as_array().unwrap(){let mut def=category(&tx,&b,row["id"].as_str().unwrap())?;for field in ["is_income","hidden","group_hidden","group_id","group_name"]{def[field]=row[field].clone();}categories.push(def);}
 let mut q=tx.prepare("SELECT id,name FROM budget_cleanup_groups WHERE budget_id=?1 AND tombstone=0 ORDER BY name,id")?;let pools=q.query_map([&b],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?})))?.collect::<Result<Vec<_>,_>>()?;
 Ok(json!({"data":{"month":query.month,"method":view["method"],"categories":categories,"pools":pools,"snapshot":view["snapshot"],"epoch":view["epoch"],"experimental":crate::preferences::get_preferences(&tx,&b)?["experimental"]}}))}).await.map(Json)
}
pub(super) fn configure(
    db: &Connection,
    b: &str,
    body: &Value,
    clock: &str,
) -> Result<Value, Error> {
    let id = body["category_id"]
        .as_str()
        .ok_or_else(|| Error::invalid("category_id is required"))?;
    crate::models::validate_id(id)?;
    let old = category(db, b, id)?;
    let source = body["source"]
        .as_str()
        .filter(|s| matches!(*s, "notes" | "ui"))
        .ok_or_else(|| Error::invalid("Choose notes or ui source"))?;
    let templates = if source == "ui" {
        body["templates"]
            .as_array()
            .ok_or_else(|| Error::invalid("Visual definitions require a template array"))?
            .clone()
    } else {
        old["notes_templates"].as_array().unwrap().clone()
    };
    if source == "ui" {
        validation::templates(&templates)?;
    }
    let mut config = storage::config(db, b, id)?;
    config["goal_def"] = json!(templates);
    config["template_settings"] = json!({"source":source});
    if let Some(cleanup) = body.get("cleanup") {
        config["cleanup_def"] = cleanup.clone();
    }
    storage::validate_config(&config)?;
    let mut fields = vec!["goal_def", "template_settings"];
    if body.get("cleanup").is_some() {
        fields.push("cleanup_def");
    }
    for field in fields.iter().chain(std::iter::once(&"tombstone")) {
        super::super::actions::guard_field(db, b, "budget_automations", id, field, clock)?;
    }
    db.execute("INSERT INTO budget_automations(budget_id,id,goal_def,cleanup_def,template_settings) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(budget_id,id) DO UPDATE SET goal_def=excluded.goal_def,cleanup_def=excluded.cleanup_def,template_settings=excluded.template_settings,tombstone=0",params![b,id,config["goal_def"].to_string(),if config["cleanup_def"].is_null(){None}else{Some(config["cleanup_def"].to_string())},config["template_settings"].to_string()])?;
    for field in fields {
        crate::concurrency::seed_field_version(db, b, "budget_automations", id, field, clock)?;
    }
    crate::concurrency::seed_field_version(db, b, "budget_automations", id, "tombstone", clock)?;
    crate::concurrency::mark(db, b, "budget_automations", id)?;
    Ok(
        json!({"category":category(db,b,id)?,"snapshot":crate::concurrency::cursor(db,b)?,"epoch":crate::lifecycle::epoch(db,b)?}),
    )
}
