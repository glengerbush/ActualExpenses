//! Lossless Actual allocation domains; calculations remain independent of import.
use super::*;
use rusqlite::OptionalExtension;
pub fn convert(source: &Value) -> Result<Value, Error> {
    let mut cells = vec![];
    let mut ids = std::collections::BTreeSet::new();
    for (table, kind) in [
        ("reflect_budgets", "tracking"),
        ("zero_budgets", "envelope"),
    ] {
        for row in source[table].as_array().into_iter().flatten() {
            let raw = row["month"]
                .as_i64()
                .ok_or_else(|| Error::invalid("Invalid allocation month"))?;
            let m = format!("{:04}-{:02}", raw / 100, raw % 100);
            month(&m)?;
            let category = row["category"]
                .as_str()
                .ok_or_else(|| Error::invalid("Missing allocation category"))?;
            crate::models::validate_id(category)?;
            if !source["categories"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["id"] == category)
            {
                return Err(Error::invalid("Unknown allocation category"));
            }
            let amount = if row["amount"].is_null() {
                0
            } else {
                row["amount"]
                    .as_i64()
                    .ok_or_else(|| Error::invalid("Invalid allocation amount"))?
            };
            money(amount as i128)?;
            let carry = match row["carryover"].as_i64() {
                Some(0) | None if row["carryover"].is_null() || row["carryover"] == 0 => false,
                Some(1) => true,
                _ => return Err(Error::invalid("Invalid allocation carryover")),
            };
            let deleted = source["categories"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["id"] == category)
                .is_some_and(|c| c["tombstone"] == 1);
            if !ids.insert(format!("{kind}:{m}:{category}")) {
                return Err(Error::invalid("Duplicate allocation identity"));
            }
            cells.push(json!({"id":format!("{kind}:{m}:{category}"),"source_id":row["id"],"method":kind,"month":m,"category_id":category,"amount":amount,"carryover":carry,"goal":row["goal"],"long_goal":row["long_goal"],"tombstone":deleted}));
        }
    }
    Ok(json!({"cells":cells,"months":[]}))
}
pub fn install(db: &Connection, b: &str, data: &Value) -> Result<(), Error> {
    for row in data["cells"].as_array().into_iter().flatten() {
        db.execute("INSERT OR IGNORE INTO budget_cells(budget_id,id,method,month,category_id,amount,carryover,goal,long_goal,tombstone) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![b,row["id"].as_str(),row["method"].as_str(),row["month"].as_str(),row["category_id"].as_str(),row["amount"].as_i64(),row["carryover"].as_bool(),if row["goal"].is_null(){None}else{Some(row["goal"].to_string())},if row["long_goal"].is_null(){None}else{Some(row["long_goal"].to_string())},row["tombstone"].as_bool().unwrap_or(false)])?;
    }
    Ok(())
}
pub fn clock(db: &Connection, b: &str, data: &Value, row: &Value) -> Result<(), Error> {
    let dataset = row["dataset"].as_str().unwrap_or("");
    let source_id = row["row"].as_str().unwrap_or("");
    let field = row["column"].as_str().unwrap_or("");
    let clock = row["timestamp"].as_str().unwrap_or(sync::ZERO);
    let target = if ["zero_budgets", "reflect_budgets"].contains(&dataset)
        && ["amount", "carryover", "goal", "long_goal"].contains(&field)
    {
        data["cells"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|c| {
                c["source_id"] == source_id
                    && c["method"]
                        == if dataset == "zero_budgets" {
                            "envelope"
                        } else {
                            "tracking"
                        }
            })
            .map(|c| ("budget_cells", c["id"].as_str().unwrap(), field))
    } else if dataset == "preferences" && source_id == "budgetType" && field == "value" {
        Some(("budget_settings", "method", "method"))
    } else {
        None
    };
    if let Some((table, id, field)) = target {
        sync::seed_field_version(db, b, table, id, field, clock)?;
    }
    Ok(())
}
pub fn validate(db: &Connection, b: &str) -> Result<(), Error> {
    let settings:Option<(String,String)>=db.query_row("SELECT s.method,b.budget_type FROM budget_settings s JOIN budgets b ON b.id=s.budget_id WHERE s.budget_id=?1 AND s.id='method'",[b],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    if settings.is_none_or(|(a, z)| a != z) {
        return Err(Error::invalid("Budget method settings are inconsistent"));
    }
    let invalid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM budget_cells c LEFT JOIN categories k ON k.budget_id=c.budget_id AND k.id=c.category_id WHERE c.budget_id=?1 AND (k.id IS NULL OR (c.tombstone=0 AND k.tombstone=1)))",[b],|r|r.get(0))?;
    if invalid {
        return Err(Error::invalid("Budget plan references a removed category"));
    }
    let mut q = db.prepare(
        "SELECT id,method,month,category_id,goal,long_goal FROM budget_cells WHERE budget_id=?1",
    )?;
    for row in q.query_map([b], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<String>>(5)?,
        ))
    })? {
        let (id, method, m, c, goal, long) = row?;
        month(&m)?;
        if id != format!("{method}:{m}:{c}") {
            return Err(Error::invalid("Invalid allocation identity"));
        }
        for text in [goal, long].into_iter().flatten() {
            serde_json::from_str::<Value>(&text)
                .map_err(|_| Error::invalid("Invalid preserved goal"))?;
        }
    }
    let mut q = db.prepare("SELECT id,notes FROM budget_months WHERE budget_id=?1")?;
    for row in q.query_map([b], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (m, notes) = row?;
        month(&m)?;
        if notes.chars().count() > 10000 {
            return Err(Error::invalid("Budget notes exceed limit"));
        }
    }
    Ok(())
}

/// Upgrade converter metadata from retained originals, never financial receipt bytes.
/// Only pre16 budgets use this path, so newly introduced planning rows cannot
/// replace a native allocation. Existing rows still win during archive staging.
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
    let originals = q
        .query_map([budget], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (b, bytes) in originals {
        let Ok((_, mut data)) = crate::imports::conversion::inspect(&bytes) else {
            continue;
        };
        for cell in data["budgeting"]["cells"].as_array_mut().unwrap() {
            let dead: Option<bool> = db
                .query_row(
                    "SELECT tombstone FROM categories WHERE budget_id=?1 AND id=?2",
                    params![b, cell["category_id"].as_str()],
                    |r| r.get(0),
                )
                .optional()?;
            if dead != Some(false) {
                cell["tombstone"] = json!(true);
            }
        }
        install(db, &b, &data["budgeting"])?;
        for row in data["clocks"].as_array().unwrap() {
            clock(db, &b, &data["budgeting"], row)?;
        }
    }
    Ok(())
}
