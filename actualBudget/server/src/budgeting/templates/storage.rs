use super::*;
use rusqlite::{OptionalExtension, params};
pub fn convert(source: &Value) -> Result<Value, Error> {
    let mut rows = vec![];
    let mut pools = vec![];
    for c in source["categories"].as_array().into_iter().flatten() {
        let id = c["id"]
            .as_str()
            .ok_or_else(|| Error::invalid("Missing template category"))?;
        let mut row = json!({"id":id,"tombstone":c["tombstone"]==1});
        for field in ["goal_def", "cleanup_def", "template_settings"] {
            row[field] = if let Some(s) = c[field].as_str() {
                serde_json::from_str(s)
                    .map_err(|_| Error::invalid(format!("Invalid preserved {field}")))?
            } else if c[field].is_null() {
                Value::Null
            } else {
                return Err(Error::invalid(format!("Invalid preserved {field}")));
            };
        }
        if row["template_settings"].is_null() {
            row["template_settings"] = json!({"source":"notes"});
        }
        validate_config(&row)?;
        rows.push(row);
    }
    for c in source["cleanup_groups"].as_array().into_iter().flatten() {
        crate::models::validate_id(
            c["id"]
                .as_str()
                .ok_or_else(|| Error::invalid("Missing cleanup pool identity"))?,
        )?;
        let name = c["name"]
            .as_str()
            .ok_or_else(|| Error::invalid("Missing cleanup pool name"))?;
        crate::models::validate_name(name)?;
        pools.push(json!({"id":c["id"],"name":name,"tombstone":c["tombstone"]==1}));
    }
    let currency = source["preferences"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["id"] == "defaultCurrencyCode")
        .and_then(|p| p["value"].as_str());
    Ok(json!({"rows":rows,"pools":pools,"currency":currency}))
}
pub(super) fn validate_config(row: &Value) -> Result<(), Error> {
    if !matches!(
        row["template_settings"]["source"].as_str(),
        Some("notes" | "ui")
    ) {
        return Err(Error::invalid("Template source must be notes or ui"));
    }
    for field in ["goal_def", "cleanup_def"] {
        if !row[field].is_null() && row[field].as_array().is_none_or(|a| a.len() > 256) {
            return Err(Error::invalid(format!(
                "{field} must contain at most256 directives"
            )));
        }
    }
    if row.to_string().len() > 64000 {
        return Err(Error::invalid(
            "Category automation configuration exceeds limit",
        ));
    }
    Ok(())
}
pub fn install(db: &Connection, b: &str, data: &Value) -> Result<(), Error> {
    if let Some(currency) = data["currency"].as_str() {
        db.execute("UPDATE budget_settings SET template_currency=?2,template_currency_review=?3 WHERE budget_id=?1 AND template_currency IS NULL",params![b,currency,data["currency_review"].as_bool().unwrap_or(false)])?;
    }
    for row in data["rows"].as_array().into_iter().flatten() {
        validate_config(row)?;
        db.execute("INSERT OR IGNORE INTO budget_automations(budget_id,id,goal_def,cleanup_def,template_settings,tombstone) VALUES(?1,?2,?3,?4,?5,?6)",params![b,row["id"].as_str(),if row["goal_def"].is_null(){None}else{Some(row["goal_def"].to_string())},if row["cleanup_def"].is_null(){None}else{Some(row["cleanup_def"].to_string())},row["template_settings"].to_string(),row["tombstone"].as_bool().unwrap_or(false)])?;
    }
    for row in data["pools"].as_array().into_iter().flatten() {
        db.execute("INSERT OR IGNORE INTO budget_cleanup_groups(budget_id,id,name,tombstone) VALUES(?1,?2,?3,?4)",params![b,row["id"].as_str(),row["name"].as_str(),row["tombstone"].as_bool().unwrap_or(false)])?;
    }
    Ok(())
}
pub fn clock(db: &Connection, b: &str, row: &Value) -> Result<(), Error> {
    let dataset = row["dataset"].as_str().unwrap_or("");
    let field = row["column"].as_str().unwrap_or("");
    let id = row["row"].as_str().unwrap_or("");
    if dataset == "preferences" && id == "defaultCurrencyCode" && field == "value" {
        return crate::concurrency::seed_field_version(
            db,
            b,
            "budget_settings",
            "method",
            "template_currency",
            row["timestamp"]
                .as_str()
                .unwrap_or(crate::concurrency::ZERO),
        );
    }
    let entity = if dataset == "categories"
        && ["goal_def", "cleanup_def", "template_settings"].contains(&field)
    {
        "budget_automations"
    } else if dataset == "cleanup_groups" && ["name", "tombstone"].contains(&field) {
        "budget_cleanup_groups"
    } else {
        return Ok(());
    };
    crate::concurrency::seed_field_version(
        db,
        b,
        entity,
        id,
        field,
        row["timestamp"]
            .as_str()
            .unwrap_or(crate::concurrency::ZERO),
    )
}
pub(super) fn config(db: &Connection, b: &str, id: &str) -> Result<Value, Error> {
    let row:Option<(Option<String>,Option<String>,String)>=db.query_row("SELECT goal_def,cleanup_def,template_settings FROM budget_automations WHERE budget_id=?1 AND id=?2 AND tombstone=0",params![b,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let Some((goal, cleanup, settings)) = row else {
        return Ok(
            json!({"goal_def":null,"cleanup_def":null,"template_settings":{"source":"notes"}}),
        );
    };
    Ok(
        json!({"goal_def":goal.map(|s|serde_json::from_str::<Value>(&s)).transpose().map_err(Error::internal)?,"cleanup_def":cleanup.map(|s|serde_json::from_str::<Value>(&s)).transpose().map_err(Error::internal)?,"template_settings":serde_json::from_str::<Value>(&settings).map_err(Error::internal)?}),
    )
}
pub fn record(db: &Connection, b: &str, entity: &str, id: &str) -> Result<Value, Error> {
    if entity == "budget_cleanup_groups" {
        return db
            .query_row(
                "SELECT name,tombstone FROM budget_cleanup_groups WHERE budget_id=?1 AND id=?2",
                params![b, id],
                |r| Ok(json!({"id":id,"name":r.get::<_,String>(0)?,"deleted":r.get::<_,bool>(1)?})),
            )
            .map_err(Error::from);
    }
    let mut row = config(db, b, id)?;
    row["id"] = json!(id);
    row["deleted"] = json!(db.query_row(
        "SELECT tombstone FROM budget_automations WHERE budget_id=?1 AND id=?2",
        params![b, id],
        |r| r.get::<_, bool>(0)
    )?);
    Ok(row)
}
