//! Saved rules retain their source expressions. Unsupported dependencies block activation.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
fn parsed(value: &Value) -> Result<Value, Error> {
    if let Some(s) = value.as_str() {
        serde_json::from_str(s).map_err(|_| Error::invalid("Invalid saved rule JSON"))
    } else {
        Ok(value.clone())
    }
}
fn mapped(field: &str) -> &str {
    match field {
        "acct" => "account",
        "description" => "payee",
        "imported_description" => "imported_payee",
        s => s,
    }
}
fn reference(v: &mut Value, map: &BTreeMap<String, String>) -> Result<(), Error> {
    let Some(text) = v.as_str() else {
        return Ok(());
    };
    let mut id = text.to_owned();
    let mut seen = BTreeSet::new();
    while let Some(next) = map.get(&id) {
        if next == &id {
            break;
        }
        if !seen.insert(id.clone()) {
            return Err(Error::invalid("Cyclic rule reference aliases"));
        }
        id = next.clone();
    }
    *v = json!(id);
    Ok(())
}
pub fn convert(
    source: &Value,
    payees: &BTreeMap<String, String>,
    categories: &BTreeMap<String, String>,
) -> Result<Value, Error> {
    let mut rules = vec![];
    let mut requirements = vec![];
    let mut ids = BTreeSet::new();
    for row in source["rules"].as_array().into_iter().flatten() {
        let id = row["id"]
            .as_str()
            .ok_or_else(|| Error::invalid("Saved rule ID required"))?;
        crate::models::validate_id(id)?;
        if !ids.insert(id) {
            return Err(Error::invalid("Duplicate rule identity"));
        }
        let mut conditions = parsed(&row["conditions"])?;
        let mut actions = parsed(&row["actions"])?;
        for item in conditions
            .as_array_mut()
            .ok_or_else(|| Error::invalid("Saved rule conditions must be a list"))?
            .iter_mut()
            .chain(
                actions
                    .as_array_mut()
                    .ok_or_else(|| Error::invalid("Saved rule actions must be a list"))?
                    .iter_mut(),
            )
        {
            if let Some(field) = item["field"].as_str() {
                let field = mapped(field).to_owned();
                item["field"] = json!(field);
                if matches!(field.as_str(), "payee" | "category") {
                    let map = if field == "payee" { payees } else { categories };
                    if let Some(list) = item["value"].as_array_mut() {
                        for value in list {
                            reference(value, map)?;
                        }
                    } else {
                        reference(&mut item["value"], map)?;
                    }
                }
            }
        }
        let mut stage = row["stage"].clone();
        if stage == "cleanup" || stage == "modify" {
            stage = json!("pre");
        }
        let value = json!({"id":id,"stage":stage,"conditionsOp":row["conditions_op"],"conditions":conditions,"actions":actions});
        let rule: Rule = serde_json::from_value(value.clone())
            .map_err(|_| Error::invalid("Invalid saved rule shape"))?;
        let retired = rule.actions.iter().any(|a| {
            !a["options"]["formula"].is_null()
                || !a["options"]["template"].is_null()
                || a["options"]["method"] == "formula"
        });
        let dead = row["tombstone"] == 1 || row["tombstone"] == true || retired;
        if !dead && let Err(error) = engine::validate(&rule) {
            requirements.push(json!({"feature":"rules","count":1,"blocking":true,"rule_id":id,"reason":error.message,"raw":row}));
        }
        rules.push(json!({"rule":value,"tombstone":dead}));
    }
    let mut learning = true;
    for row in source["preferences"].as_array().into_iter().flatten() {
        if row["id"] == "learn-categories" {
            learning = match &row["value"] {
                Value::Bool(v) => *v,
                Value::String(v) if v == "true" || v == "false" => v == "true",
                _ => return Err(Error::invalid("Invalid category-learning preference")),
            };
        }
    }
    let flags = source["payees"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| {
            let enabled = match p["learn_categories"].as_i64() {
                Some(0) => false,
                Some(1) => true,
                None if p["learn_categories"].is_null() => true,
                _ => return Err(Error::invalid("Invalid payee-learning flag")),
            };
            Ok(json!({"id":p["id"],"enabled":enabled}))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(json!({"rules":rules,"learning":learning,"payees":flags,"requirements":requirements}))
}
pub fn install(db: &Connection, b: &str, data: &Value) -> Result<(), Error> {
    for item in data["rules"].as_array().into_iter().flatten() {
        let r: Rule = serde_json::from_value(item["rule"].clone()).map_err(Error::internal)?;
        db.execute("INSERT OR IGNORE INTO rules(budget_id,id,stage,conditions_op,conditions,actions,tombstone) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![b,r.id,r.stage,r.conditions_op,serde_json::to_string(&r.conditions).map_err(Error::internal)?,serde_json::to_string(&r.actions).map_err(Error::internal)?,item["tombstone"].as_bool()])?;
    }
    for p in data["payees"].as_array().into_iter().flatten() {
        db.execute("UPDATE payees SET learn_categories=?3 WHERE budget_id=?1 AND id=?2 AND NOT EXISTS(SELECT 1 FROM field_versions WHERE budget_id=?1 AND entity='payees' AND entity_id=?2 AND field='learn_categories')",params![b,p["id"].as_str(),p["enabled"].as_bool()])?;
    }
    if let Some(flag) = data["learning"].as_bool() {
        db.execute("UPDATE rule_settings SET enabled=?2 WHERE budget_id=?1 AND id='learning' AND NOT EXISTS(SELECT 1 FROM field_versions WHERE budget_id=?1 AND entity='rule_settings' AND entity_id='learning' AND field='enabled')",params![b,flag])?;
    }
    Ok(())
}
pub fn import_clock(db: &Connection, b: &str, row: &Value) -> Result<(), Error> {
    let dataset = row["dataset"].as_str().unwrap_or("");
    let id = row["row"].as_str().unwrap_or("");
    let field = row["column"].as_str().unwrap_or("");
    let clock = row["timestamp"].as_str().unwrap_or(sync::ZERO);
    let target = if dataset == "rules"
        && matches!(
            field,
            "stage" | "conditions_op" | "conditions" | "actions" | "tombstone"
        ) {
        Some(("rules", id, field))
    } else if dataset == "payees" && field == "learn_categories" {
        Some(("payees", id, field))
    } else if dataset == "preferences" && id == "learn-categories" && field == "value" {
        Some(("rule_settings", "learning", "enabled"))
    } else {
        None
    };
    if let Some((entity, id, field)) = target {
        sync::seed_field_version(db, b, entity, id, field, clock)?;
    }
    Ok(())
}
pub fn enrich(db: &Connection, budget: Option<&str>) -> Result<(), Error> {
    let mut q=db.prepare("SELECT id,original,archive_sha256 FROM staged_imports WHERE COALESCE(json_extract(report,'$.source_format'),'actual') NOT IN('ynab4','nynab') AND status<>'activated' AND (?1 IS NULL OR budget_id=?1)")?;
    let stages = q
        .query_map([budget], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, bytes, hash) in stages {
        let Ok((mut report, data)) = crate::imports::conversion::inspect(&bytes) else {
            continue;
        };
        report["archive_sha256"] = json!(hash);
        let status = if report["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["blocking"] == true)
        {
            "blocked"
        } else {
            "ready"
        };
        db.execute(
            "UPDATE staged_imports SET status=?2,report=?3,conversion=?4 WHERE id=?1",
            params![id, status, report.to_string(), data.to_string()],
        )?;
    }
    let mut q=db.prepare("SELECT s.budget_id,s.original FROM staged_imports s JOIN imported_sources i ON i.import_id=s.id AND COALESCE(json_extract(s.report,'$.source_format'),'actual') NOT IN('ynab4','nynab') WHERE s.status='activated' AND (?1 IS NULL OR s.budget_id=?1) UNION SELECT budget_id,original FROM budget_provenance WHERE kind='actual-import' AND original IS NOT NULL AND (?1 IS NULL OR budget_id=?1)")?;
    let sources = q
        .query_map([budget], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (b, bytes) in sources {
        let Ok((_, data)) = crate::imports::conversion::inspect(&bytes) else {
            continue;
        };
        install(db, &b, &data["rules"])?;
        for row in data["clocks"].as_array().unwrap() {
            import_clock(db, &b, row)?;
        }
    }
    Ok(())
}
pub fn validate(db: &Connection, b: &str) -> Result<(), Error> {
    let valid: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM rule_settings WHERE budget_id=?1 AND id='learning')",
        [b],
        |r| r.get(0),
    )?;
    if !valid {
        return Err(Error::invalid("Missing learning settings"));
    }
    let mut q = db.prepare(
        "SELECT id,stage,conditions_op,conditions,actions,tombstone FROM rules WHERE budget_id=?1",
    )?;
    for row in q.query_map([b], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, bool>(5)?,
        ))
    })? {
        let (id, stage, conditions_op, c, a, dead) = row?;
        let rule = Rule {
            id,
            stage,
            conditions_op,
            conditions: serde_json::from_str(&c)
                .map_err(|_| Error::invalid("Invalid retained rule conditions"))?,
            actions: serde_json::from_str(&a)
                .map_err(|_| Error::invalid("Invalid retained rule actions"))?,
        };
        crate::models::validate_id(&rule.id)?;
        if !dead {
            engine::validate(&rule)?;
        }
    }
    Ok(())
}
