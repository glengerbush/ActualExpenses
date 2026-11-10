//! Source-compatible directive rendering. Representation losses are detected by
//! migration planning rather than silently accepted as changed financial intent.
use super::*;
use rusqlite::{OptionalExtension, params};
fn n(t: &Value, key: &str) -> Result<String, Error> {
    t[key]
        .as_f64()
        .filter(|n| n.is_finite())
        .map(|n| n.to_string())
        .ok_or_else(|| Error::invalid(format!("Template {key} is required")))
}
fn word<'a>(t: &'a Value, key: &str) -> Result<&'a str, Error> {
    t[key]
        .as_str()
        .ok_or_else(|| Error::invalid(format!("Template {key} is required")))
}
fn prefix(t: &Value) -> String {
    match t["priority"].as_i64() {
        Some(p) if p != 0 => format!("#template-{p}"),
        _ => "#template".into(),
    }
}
fn limit(t: &Value) -> Result<String, Error> {
    let mut line = format!("up to {}", n(t, "amount")?);
    match t["period"].as_str() {
        Some("weekly") => line.push_str(&format!(" per week starting {}", word(t, "start")?)),
        Some("daily") => line.push_str(" per day"),
        Some("monthly") | None => {}
        _ => return Err(Error::invalid("Invalid limit period")),
    }
    if t["hold"] == true {
        line.push_str(" hold");
    }
    Ok(line)
}
fn adjustment(t: &Value) -> Result<String, Error> {
    let Some(value) = t["adjustment"].as_f64() else {
        return Ok(String::new());
    };
    Ok(format!(
        " [{} {}{}]",
        if value >= 0.0 { "increase" } else { "decrease" },
        value.abs(),
        if t["adjustmentType"] == "percent" {
            "%"
        } else {
            ""
        }
    ))
}
fn name(db: &Connection, b: &str, table: &str, id: &str) -> Result<Option<String>, Error> {
    db.query_row(
        &format!("SELECT name FROM {table} WHERE budget_id=?1 AND id=?2 AND tombstone=0"),
        params![b, id],
        |r| r.get(0),
    )
    .optional()
    .map_err(Error::from)
}
pub(super) fn templates(db: &Connection, b: &str, templates: &[Value]) -> Result<String, Error> {
    let refill = templates.iter().find(|t| t["type"] == "refill");
    let mut lines = vec![];
    for t in templates {
        let p = prefix(t);
        let mut line = match word(t, "type")? {
            "refill" => continue,
            "goal" => format!("#goal {}", n(t, "amount")?),
            "simple" => {
                let mut line = p;
                if !t["monthly"].is_null() {
                    line.push_str(&format!(" {}", n(t, "monthly")?));
                }
                if !t["limit"].is_null() {
                    line.push_str(&format!(" {}", limit(&t["limit"])?));
                }
                line
            }
            "limit" => format!(
                "{} {}{}",
                refill.map(prefix).unwrap_or(p),
                if refill.is_some() { "" } else { "0 " },
                limit(t)?
            ),
            "percentage" => {
                let raw = word(t, "category")?;
                let category = name(db, b, "categories", raw)?.unwrap_or(raw.into());
                format!(
                    "{p} {}% of {}{category}",
                    n(t, "percent")?,
                    if t["previous"] == true {
                        "previous "
                    } else {
                        ""
                    }
                )
            }
            "schedule" => {
                let schedule = if let Some(id) = t["scheduleId"].as_str() {
                    name(db,b,"schedules",id)?.ok_or_else(||Error::invalid("The schedule reference is unavailable; keep visual definitions or repair it"))?
                } else {
                    word(t, "name")?.to_owned()
                };
                format!(
                    "{p} schedule {}{schedule}{}",
                    if t["full"] == true { "full " } else { "" },
                    adjustment(t)?
                )
            }
            "periodic" => {
                let mut line = format!(
                    "{p} {} repeat every {} {}s starting {}",
                    n(t, "amount")?,
                    n(&t["period"], "amount")?,
                    word(&t["period"], "period")?,
                    word(t, "starting")?
                );
                if !t["limit"].is_null() {
                    line.push_str(&format!(" {}", limit(&t["limit"])?));
                }
                line
            }
            "by" | "spend" => {
                let mut line = format!("{p} {} by {}", n(t, "amount")?, word(t, "month")?);
                if t["type"] == "spend" {
                    line.push_str(&format!(" spend from {}", word(t, "from")?));
                }
                if !t["annual"].is_null() {
                    let annual = t["annual"] == true;
                    let repeat = t["repeat"].as_i64().unwrap_or(1);
                    line.push_str(&format!(
                        " repeat every {}{}{}",
                        if repeat == 1 {
                            String::new()
                        } else {
                            format!("{repeat} ")
                        },
                        if annual { "year" } else { "month" },
                        if repeat == 1 { "" } else { "s" }
                    ));
                }
                line
            }
            "remainder" => {
                let mut line = format!("{p} remainder");
                if t["weight"].as_f64().is_some_and(|w| w != 1.0) {
                    line.push_str(&format!(" {}", n(t, "weight")?));
                }
                if !t["limit"].is_null() {
                    line.push_str(&format!(" {}", limit(&t["limit"])?));
                }
                line
            }
            "average" => format!(
                "{p} average {} months{}",
                n(t, "numMonths")?,
                adjustment(t)?
            ),
            "copy" => format!("{p} copy from {} months ago", n(t, "lookBack")?),
            _ => {
                return Err(Error::invalid(
                    "A retained directive cannot be represented in notes. Keep its current source and repair it first.",
                ));
            }
        };
        if let Some(description) = t["description"].as_str() {
            lines.extend(
                description
                    .lines()
                    .map(str::trim_end)
                    .filter(|l| !l.is_empty())
                    .map(str::to_owned),
            );
        }
        line = line.trim().into();
        lines.push(line);
    }
    Ok(lines.join("\n"))
}
pub(super) fn cleanup(cleanup: &[Value], pools: &[Value]) -> Result<String, Error> {
    let mut lines = vec![];
    for row in cleanup {
        let group = if let Some(id) = row["groupId"].as_str() {
            Some(pools.iter().find(|p|p["id"]==id&&p["deleted"]!=true).and_then(|p|p["name"].as_str()).ok_or_else(||Error::invalid("A cleanup pool is unavailable. Keep visual definitions or repair the pool before exporting."))?)
        } else {
            None
        };
        lines.push(match row["role"].as_str() {
            Some("source") => format!(
                "#cleanup {}source",
                group.map(|s| format!("{s} ")).unwrap_or_default()
            ),
            Some("sink") => format!(
                "#cleanup {}sink{}",
                group.map(|s| format!("{s} ")).unwrap_or_default(),
                if row["weight"].as_f64() == Some(1.0) {
                    String::new()
                } else {
                    format!(" {}", n(row, "weight")?)
                }
            ),
            Some("overspend") => format!(
                "#cleanup {}",
                group
                    .ok_or_else(|| Error::invalid("Overspending cleanup requires a named pool"))?
            ),
            _ => return Err(Error::invalid("Unknown cleanup role")),
        });
    }
    Ok(lines.join("\n"))
}
/// Retain ordinary prose, including inline prefixes, but do not reactivate stale
/// directives from the inactive notes source. The full original is also returned
/// alongside this proposal and remains in engine history/provenance.
pub(super) fn merge(original: &str, rendered: &str) -> String {
    let mut prose = vec![];
    for line in original.split('\n') {
        if let Some(index) = line.find('#') {
            let directive = line[index..].trim();
            if directive.starts_with("#template") || directive.starts_with("#goal") {
                if index > 0 {
                    prose.push(line[..index].to_owned());
                }
                continue;
            }
        }
        if line.trim().starts_with("#cleanup ") {
            continue;
        }
        prose.push(line.to_owned());
    }
    let base = prose.join("\n");
    let base = base.trim_end();
    if rendered.is_empty() {
        return base.into();
    }
    if base.is_empty() {
        return rendered.into();
    }
    format!("{base}\n\n{rendered}")
}
