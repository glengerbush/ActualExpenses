//! Source calendar state is selected using Actual's local/base generation rule.
use super::*;
fn flag(row: &Value, field: &str) -> Result<bool, Error> {
    match row[field].as_i64() {
        Some(0) | None if row[field].is_null() || row[field] == 0 => Ok(false),
        Some(1) => Ok(true),
        _ => Err(Error::invalid("Invalid source schedule flag")),
    }
}
fn date(value: &Value) -> Result<Option<String>, Error> {
    if value.is_null() {
        return Ok(None);
    }
    let text = if let Some(n) = value.as_i64() {
        format!("{n:08}")
    } else {
        return Err(Error::invalid("Invalid source schedule date"));
    };
    if text.len() != 8 {
        return Err(Error::invalid("Invalid source schedule date"));
    }
    let date = format!("{}-{}-{}", &text[..4], &text[4..6], &text[6..]);
    crate::models::validate_date(&date)?;
    Ok(Some(date))
}
pub fn convert(source: &Value, rules: &Value) -> Result<Value, Error> {
    let mut rows = vec![];
    let mut links = vec![];
    let mut next_states = vec![];
    let mut ids = std::collections::BTreeSet::new();
    for row in source["schedules"].as_array().into_iter().flatten() {
        let id = row["id"]
            .as_str()
            .ok_or_else(|| Error::invalid("Source schedule ID required"))?;
        crate::models::validate_id(id)?;
        if !ids.insert(id) {
            return Err(Error::invalid("Duplicate source schedule identity"));
        }
        let rule = row["rule"]
            .as_str()
            .ok_or_else(|| Error::invalid("Source schedule rule required"))?;
        crate::models::validate_id(rule)?;
        let dead = flag(row, "tombstone")?;
        if !dead
            && !rules["rules"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|r| r["rule"]["id"] == rule && r["tombstone"] == false)
        {
            return Err(Error::invalid("Missing live schedule rule"));
        }
        let states = source["schedules_next_date"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| r["schedule_id"] == id && r["tombstone"] != 1)
            .collect::<Vec<_>>();
        if states.len() > 1 || states.is_empty() && !dead {
            return Err(Error::invalid(
                "Ambiguous or missing source next-date state",
            ));
        }
        if let Some(state) = states.first() {
            next_states.push(json!({"id":state["id"],"schedule_id":id,"field":if state["local_next_date_ts"]==state["base_next_date_ts"]{"local_next_date"}else{"base_next_date"}}));
        }
        let next = if let Some(state) = states.first() {
            date(
                &state[if state["local_next_date_ts"] == state["base_next_date_ts"] {
                    "local_next_date"
                } else {
                    "base_next_date"
                }],
            )?
        } else {
            None
        };
        let name = row["name"]
            .as_str()
            .map(str::trim)
            .filter(|v| !v.is_empty());
        if let Some(name) = name {
            crate::models::validate_name(name)?;
        }
        if let Some(horizon) = row["custom_upcoming_length"].as_str() {
            view::horizon_days(horizon, "2026-01-01")?;
        }
        rows.push(json!({"id":id,"rule_id":rule,"name":name,"posts_transaction":flag(row,"posts_transaction")?,"completed":flag(row,"completed")?,"next_date":next,"custom_upcoming_length":row["custom_upcoming_length"],"tombstone":dead}));
    }
    for row in source["transactions"].as_array().into_iter().flatten() {
        if let Some(schedule) = row["schedule"].as_str() {
            if !ids.contains(schedule) {
                return Err(Error::invalid("Transaction points to a missing schedule"));
            }
            links.push(
                json!({"id":row["id"],"schedule_id":schedule,"tombstone":flag(row,"tombstone")?}),
            );
        } else if !row["schedule"].is_null() {
            return Err(Error::invalid("Invalid source schedule link"));
        }
    }
    let horizon = source["preferences"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["id"] == "upcomingScheduledTransactionLength")
        .and_then(|p| p["value"].as_str())
        .unwrap_or("7");
    view::horizon_days(horizon, "2026-01-01")?;
    Ok(json!({"rows":rows,"links":links,"horizon":horizon,"next_states":next_states}))
}
pub fn install(db: &Connection, b: &str, data: &Value) -> Result<(), Error> {
    for row in data["rows"].as_array().into_iter().flatten() {
        db.execute("INSERT OR IGNORE INTO schedules(budget_id,id,rule_id,name,posts_transaction,completed,next_date,custom_upcoming_length,tombstone) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![b,row["id"].as_str(),row["rule_id"].as_str(),row["name"].as_str(),row["posts_transaction"].as_bool(),row["completed"].as_bool(),row["next_date"].as_str(),row["custom_upcoming_length"].as_str(),row["tombstone"].as_bool()])?;
    }
    for row in data["links"].as_array().into_iter().flatten() {
        db.execute("INSERT OR IGNORE INTO schedule_links(budget_id,id,schedule_id,tombstone) VALUES(?1,?2,?3,?4)",params![b,row["id"].as_str(),row["schedule_id"].as_str(),row["tombstone"].as_bool()])?;
    }
    if let Some(horizon) = data["horizon"].as_str() {
        db.execute("UPDATE schedule_settings SET value=?2 WHERE budget_id=?1 AND NOT EXISTS(SELECT 1 FROM field_versions WHERE budget_id=?1 AND entity='schedule_settings')",params![b,horizon])?;
    }
    Ok(())
}
pub fn import_clock(db: &Connection, b: &str, data: &Value, row: &Value) -> Result<(), Error> {
    let dataset = row["dataset"].as_str().unwrap_or("");
    let id = row["row"].as_str().unwrap_or("");
    let field = row["column"].as_str().unwrap_or("");
    let clock = row["timestamp"].as_str().unwrap_or(sync::ZERO);
    let target = if dataset == "schedules"
        && matches!(
            field,
            "rule"
                | "name"
                | "posts_transaction"
                | "completed"
                | "custom_upcoming_length"
                | "tombstone"
        ) {
        Some((
            "schedules",
            id.to_owned(),
            if field == "rule" { "rule_id" } else { field },
        ))
    } else if dataset == "transactions" && field == "schedule" {
        Some(("schedule_links", id.to_owned(), "schedule_id"))
    } else if dataset == "preferences"
        && id == "upcomingScheduledTransactionLength"
        && field == "value"
    {
        Some(("schedule_settings", "horizon".into(), "value"))
    } else {
        None
    };
    if let Some((entity, id, field)) = target {
        sync::seed_field_version(db, b, entity, &id, field, clock)?;
    }
    if dataset == "schedules_next_date" {
        if let Some(state) = data["next_states"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|s| {
                s["id"] == id
                    && (s["field"] == field
                        || matches!(field, "local_next_date_ts" | "base_next_date_ts"))
            })
        {
            sync::seed_field_version(
                db,
                b,
                "schedules",
                state["schedule_id"].as_str().unwrap(),
                "next_date",
                clock,
            )?;
        }
    }
    Ok(())
}
pub fn enrich(db: &Connection, budget: Option<&str>) -> Result<(), Error> {
    let mut q=db.prepare("SELECT id,original,archive_sha256 FROM staged_imports WHERE status<>'activated' AND (?1 IS NULL OR budget_id=?1)")?;
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
        let (mut report, data) = crate::imports::conversion::inspect(&bytes)?;
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
    let mut q=db.prepare("SELECT s.budget_id,s.original FROM staged_imports s JOIN imported_sources i ON i.import_id=s.id WHERE s.status='activated' AND (?1 IS NULL OR s.budget_id=?1) UNION SELECT budget_id,original FROM budget_provenance WHERE kind='actual-import' AND original IS NOT NULL AND (?1 IS NULL OR budget_id=?1)")?;
    let sources = q
        .query_map([budget], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (b, bytes) in sources {
        let (_, data) = crate::imports::conversion::inspect(&bytes)?;
        install(db, &b, &data["schedules"])?;
        for row in data["clocks"].as_array().unwrap() {
            import_clock(db, &b, &data["schedules"], row)?;
        }
    }
    Ok(())
}
