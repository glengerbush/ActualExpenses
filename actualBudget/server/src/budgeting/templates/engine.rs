use super::*;
use super::{calculators::Environment, context::Context};
use std::collections::BTreeSet;
pub(super) fn compute(db: &Connection, b: &str, body: &Value) -> Result<Value, Error> {
    let m = body["month"]
        .as_str()
        .ok_or_else(|| Error::invalid("month is required"))?;
    super::super::month(m)?;
    let before = super::super::view(db, b, m, 1)?;
    let mut env = Environment::new(db, b, m, before.clone())?;
    let selected = if let Some(raw) = body.get("category_ids") {
        let ids = raw
            .as_array()
            .ok_or_else(|| Error::invalid("category_ids must be an array"))?;
        if ids.is_empty() || ids.len() > 1000 {
            return Err(Error::invalid("Select1–1000 categories"));
        }
        let mut set = BTreeSet::new();
        for id in ids {
            let id = id
                .as_str()
                .ok_or_else(|| Error::invalid("Invalid category identity"))?;
            crate::models::validate_id(id)?;
            if !set.insert(id.to_owned()) {
                return Err(Error::invalid("Duplicate selected category"));
            }
        }
        Some(set)
    } else {
        None
    };
    let force = body["force"] == true || body["action"] == "overwrite" || selected.is_some();
    let projection = body["projection"] == true;
    let mut rows = vec![];
    let mut contexts = vec![];
    let mut errors = vec![];
    let mut priorities = BTreeSet::new();
    let data = &before["months"][m];
    let mut available = data["summary"][if env.method == "tracking" {
        "total_saved"
    } else {
        "to_budget"
    }]
    .as_f64()
    .unwrap();
    let categories = data["categories"].as_array().unwrap();
    if let Some(ids) = &selected
        && ids
            .iter()
            .any(|id| !categories.iter().any(|c| c["id"] == *id))
        {
            return Err(Error::invalid("Choose active template categories"));
        }
    for category in categories {
        env.tick()?;
        let id = category["id"].as_str().unwrap();
        let stored = storage::config(db, b, id)?;
        let templates = if let Some(override_templates) = body["template_overrides"][id].as_array()
        {
            override_templates.clone()
        } else if stored["template_settings"]["source"] == "ui" {
            stored["goal_def"].as_array().cloned().unwrap_or_default()
        } else {
            parse::notes(category["notes"].as_str().unwrap_or(""))?["templates"]
                .as_array()
                .unwrap()
                .clone()
        };
        let chosen = selected.as_ref().map(|ids| ids.contains(id)).unwrap_or(
            category["hidden"] != true
                && category["group_hidden"] != true
                && (env.method == "tracking" || category["is_income"] != true),
        );
        let active = chosen && (force || category["budgeted"] == 0) && !templates.is_empty();
        let clear_goal = chosen && templates.is_empty() && !category["goal"].is_null();
        let row = json!({"id":id,"name":category["name"],"templates":templates,"template_settings":stored["template_settings"],"before_budgeted":category["budgeted"],"budgeted":category["budgeted"],"goal":category["goal"].as_str().and_then(|s|serde_json::from_str::<Value>(s).ok()),"long_goal":category["long_goal"].as_str().and_then(|s|serde_json::from_str::<Value>(s).ok()),"applies":active,"clear_goal":clear_goal,"notes":category["notes"],"per_template":vec![0;templates.len()]});
        rows.push(row);
        if active {
            match Context::init(category.clone(), templates, &mut env) {
                Ok(ctx) => {
                    if !ctx.goal_only() {
                        available += ctx.old;
                    }
                    available += ctx.excess;
                    priorities.extend(ctx.priorities.iter().cloned());
                    contexts.push((rows.len() - 1, ctx));
                }
                Err(e) => errors.push(json!({"id":id,"name":category["name"],"error":e.message})),
            }
        }
    }
    if errors.is_empty() {
        for p in priorities {
            let start = available;
            for (_, ctx) in &mut contexts {
                match ctx.priority(p, available, start, projection, &mut env) {
                    Ok(spent) => available -= spent,
                    Err(e) => errors
                        .push(json!({"id":ctx.row["id"],"name":ctx.row["name"],"error":e.message})),
                }
            }
        }
        while available > 0.0 {
            env.tick()?;
            let weight = contexts
                .iter()
                .filter(|(_, c)| c.weight > 0.0 && !c.met)
                .map(|(_, c)| c.weight)
                .sum::<f64>();
            if weight <= 0.0 {
                break;
            }
            let before = available;
            let per = available / weight;
            for (_, ctx) in &mut contexts {
                if ctx.weight > 0.0 && !ctx.met {
                    available -= ctx.remainder(available, per, &env)?;
                }
            }
            if available == before {
                break;
            }
        }
        for (index, ctx) in &mut contexts {
            ctx.finish(&env)?;
            rows[*index]["budgeted"] = json!(round(ctx.amount)?);
            rows[*index]["goal"] = ctx
                .goal
                .map(round)
                .transpose()?
                .map(|n| json!(n))
                .unwrap_or(Value::Null);
            rows[*index]["long_goal"] = if ctx.long { json!(1) } else { Value::Null };
            rows[*index]["per_template"] = json!(
                ctx.contributions
                    .iter()
                    .map(|n| round(*n))
                    .collect::<Result<Vec<_>, _>>()?
            );
        }
        for row in &mut rows {
            if row["clear_goal"] == true {
                row["goal"] = Value::Null;
                row["long_goal"] = Value::Null;
            }
        }
    }
    Ok(
        json!({"month":m,"method":before["method"],"categories":rows,"errors":errors,"available_after":round(available)?,"snapshot":before["snapshot"],"epoch":before["epoch"]}),
    )
}
pub(super) fn round(n: f64) -> Result<i64, Error> {
    if !n.is_finite() {
        return Err(Error::invalid("Template result must be finite"));
    }
    let lower = n.floor();
    let rounded = if n - lower < 0.5 { lower } else { lower + 1.0 };
    if rounded.abs() > crate::models::MAX_MONEY as f64 {
        return Err(Error::invalid(
            "Template result exceeds exact integer range",
        ));
    }
    Ok(rounded as i64)
}
