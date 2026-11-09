//! Conditions follow the pinned evaluator, including its string normalization.
use super::*;
use crate::register_regex::{Budget, Failure, Pattern};
use chrono::NaiveDate;
fn kind(field: &str) -> Option<&'static str> {
    Some(match field {
        "date" => "date",
        "account" | "payee" | "category" | "category_group" => "id",
        "notes" | "imported_payee" | "payee_name" => "string",
        "amount" => "number",
        "cleared" | "reconciled" | "transfer" | "parent" => "boolean",
        _ => return None,
    })
}
pub fn validate(c: &Value) -> Result<(), Error> {
    let field = c["field"]
        .as_str()
        .ok_or_else(|| Error::invalid("Condition field required"))?;
    let kind = kind(field).ok_or_else(|| Error::invalid("Invalid condition field"))?;
    let op = c["op"].as_str().unwrap_or("");
    if c.as_object().is_none_or(|v| {
        v.keys()
            .any(|k| !["op", "field", "value", "options", "type"].contains(&k.as_str()))
    }) {
        return Err(Error::invalid("Unknown condition field"));
    }
    let allowed = match kind {
        "date" => matches!(op, "is" | "isapprox" | "gt" | "gte" | "lt" | "lte"),
        "number" => matches!(
            op,
            "is" | "isapprox" | "isbetween" | "gt" | "gte" | "lt" | "lte"
        ),
        "boolean" => op == "is",
        "id" => {
            matches!(
                op,
                "is" | "isNot" | "oneOf" | "notOneOf" | "contains" | "doesNotContain" | "matches"
            ) || field == "account" && matches!(op, "onBudget" | "offBudget")
        }
        _ => matches!(
            op,
            "is" | "isNot"
                | "oneOf"
                | "notOneOf"
                | "contains"
                | "doesNotContain"
                | "matches"
                | "hasTags"
                | "hasAnyTag"
        ),
    };
    if !allowed
        || field == "notes" && matches!(op, "oneOf" | "notOneOf")
        || field == "imported_payee" && matches!(op, "hasTags" | "hasAnyTag")
    {
        return Err(Error::invalid(
            "Operator is unavailable for this condition field",
        ));
    }
    let v = &c["value"];
    match kind {
        "date" => {
            if v.is_object() {
                crate::register_recurrence::dates(v)?;
                if !matches!(op, "is" | "isapprox") {
                    return Err(Error::invalid("Date comparisons require a calendar date"));
                }
            } else {
                let text = v
                    .as_str()
                    .ok_or_else(|| Error::invalid("Date condition requires a date"))?;
                match text.len() {
                    10 => crate::models::validate_date(text)?,
                    7 if op == "is" => {
                        crate::budgeting::month(text)?;
                    }
                    4 if op == "is"
                        && text.bytes().all(|c| c.is_ascii_digit())
                        && text != "0000" => {}
                    _ => return Err(Error::invalid("Invalid date condition")),
                }
            }
        }
        "number" => {
            if op == "isbetween" {
                for n in [&v["num1"], &v["num2"]] {
                    if n.as_f64()
                        .is_none_or(|x| !x.is_finite() || x.abs() > crate::models::MAX_MONEY as f64)
                    {
                        return Err(Error::invalid("Invalid amount range"));
                    }
                }
            } else if v
                .as_f64()
                .is_none_or(|x| !x.is_finite() || x.abs() > crate::models::MAX_MONEY as f64)
            {
                return Err(Error::invalid("Invalid amount condition"));
            }
        }
        "boolean" => {
            if !v.is_boolean() {
                return Err(Error::invalid("Boolean condition requires true or false"));
            }
        }
        _ => {
            if matches!(op, "oneOf" | "notOneOf") {
                if v.as_array().is_none_or(|a| {
                    a.len() > 100
                        || a.iter()
                            .any(|v| !v.is_string() && !(kind == "id" && v.is_null()))
                }) {
                    return Err(Error::invalid("Membership condition requires a list"));
                }
            } else if matches!(op, "onBudget" | "offBudget") {
            } else if !(kind == "id" && v.is_null())
                && v.as_str().is_none_or(|s| {
                    s.chars().count() > 2000
                        || matches!(
                            op,
                            "contains" | "doesNotContain" | "matches" | "hasTags" | "hasAnyTag"
                        ) && s.is_empty()
                })
            {
                return Err(Error::invalid("Condition requires text"));
            }
        }
    }
    if !c["options"].is_null()
        && c["options"].as_object().is_none_or(|o| {
            o.keys()
                .any(|k| !["inflow", "outflow"].contains(&k.as_str()))
                || o.values().any(|v| !v.is_boolean())
        })
    {
        return Err(Error::invalid("Invalid condition options"));
    }
    Ok(())
}
fn key(field: &str) -> &str {
    match field {
        "account" => "account_id",
        "payee" => "payee_id",
        "category" => "category_id",
        f => f,
    }
}
fn regex(pattern: &str, text: &str, budget: &Budget) -> Result<bool, Error> {
    match Pattern::compile(pattern, false) {
        Ok(p) => p.matches(text, budget).map_err(|e| e.api_error()),
        Err(Failure::Invalid) => Ok(false),
        Err(e) => Err(e.api_error()),
    }
}
pub fn matches(
    db: &Connection,
    b: &str,
    c: &Value,
    row: &Value,
    account_off: Option<bool>,
    budget: &Budget,
    work: &mut usize,
) -> Result<bool, Error> {
    *work = work
        .checked_sub(1)
        .ok_or_else(|| Error::invalid("Rule evaluation work limit reached; narrow the preview"))?;
    let field = c["field"].as_str().unwrap();
    let op = c["op"].as_str().unwrap();
    let kind = kind(field).unwrap();
    let mut v = row[key(field)].clone();
    if field == "category_group" {
        v = if let Some(id) = row["category_id"].as_str() {
            db.query_row(
                "SELECT group_id FROM categories WHERE budget_id=?1 AND id=?2",
                params![b, id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten()
            .map_or(Value::Null, Value::String)
        } else {
            Value::Null
        };
    }
    if field == "parent" {
        v = json!(row["is_parent"] == true);
    }
    if field == "transfer" {
        v = json!(!row["transfer_id"].is_null());
    }
    if matches!(op, "onBudget" | "offBudget") {
        return Ok(account_off.is_some_and(|v| v == (op == "offBudget")));
    }
    if kind == "string" && v.is_null() {
        v = json!("");
    }
    if let Some(text) = v.as_str() {
        v = json!(text.to_lowercase());
    }
    let mut expected = c["value"].clone();
    if kind == "string" {
        if let Some(text) = expected.as_str() {
            if !matches!(op, "hasTags" | "hasAnyTag") {
                expected = json!(text.to_lowercase());
            }
        } else if let Some(values) = expected.as_array() {
            expected = json!(
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_lowercase)
                    .collect::<Vec<_>>()
            );
        }
    }
    if kind == "date" {
        let Some(actual) = v.as_str() else {
            return Ok(false);
        };
        if expected.is_object() {
            return crate::register_recurrence::rule_matches(
                &expected,
                actual,
                op == "isapprox",
                work,
            );
        }
        let target = expected.as_str().unwrap();
        if op == "is" {
            return Ok(actual.starts_with(target));
        }
        let actual = NaiveDate::parse_from_str(actual, "%Y-%m-%d")
            .map_err(|_| Error::invalid("Invalid transaction date"))?;
        let target = NaiveDate::parse_from_str(target, "%Y-%m-%d").map_err(Error::internal)?;
        return Ok(match op {
            "isapprox" => (actual - target).num_days().abs() <= 2,
            "gt" => actual > target,
            "gte" => actual >= target,
            "lt" => actual < target,
            "lte" => actual <= target,
            _ => false,
        });
    }
    if kind == "number" {
        let Some(mut n) = v.as_f64() else {
            return Ok(false);
        };
        if c["options"]["outflow"] == true {
            if n > 0.0 {
                return Ok(false);
            }
            n = -n;
        } else if c["options"]["inflow"] == true && n < 0.0 {
            return Ok(false);
        }
        if op == "isbetween" {
            let a = expected["num1"].as_f64().unwrap();
            let z = expected["num2"].as_f64().unwrap();
            return Ok(n >= a.min(z) && n <= a.max(z));
        }
        let target = expected.as_f64().unwrap();
        return Ok(match op {
            "is" => n == target,
            "isapprox" => (n - target).abs() <= (target.abs() * 0.075).round(),
            "gt" => n > target,
            "gte" => n >= target,
            "lt" => n < target,
            "lte" => n <= target,
            _ => false,
        });
    }
    Ok(match op {
        "is" => v == expected,
        "isNot" => v != expected,
        "oneOf" => !v.is_null() && expected.as_array().unwrap().contains(&v),
        "notOneOf" => !v.is_null() && !expected.as_array().unwrap().contains(&v),
        "contains" => {
            !v.is_null()
                && v.as_str()
                    .unwrap_or("")
                    .contains(expected.as_str().unwrap_or(""))
        }
        "doesNotContain" => {
            !v.is_null()
                && !v
                    .as_str()
                    .unwrap_or("")
                    .contains(expected.as_str().unwrap_or(""))
        }
        "matches" => {
            !v.is_null() && regex(expected.as_str().unwrap(), v.as_str().unwrap_or(""), budget)?
        }
        "hasTags" | "hasAnyTag" => {
            let mut matches = vec![];
            for tag in expected
                .as_str()
                .unwrap()
                .split(|c: char| c == '#' || c.is_whitespace())
                .filter(|s| !s.is_empty())
            {
                matches.push(regex(
                    &format!(
                        "(?<!#)#{}([\\s#]|$)",
                        crate::register_regex::escape(&tag.to_lowercase())
                    ),
                    v.as_str().unwrap_or(""),
                    budget,
                )?);
            }
            if op == "hasTags" {
                matches.into_iter().all(|v| v)
            } else {
                matches.into_iter().any(|v| v)
            }
        }
        _ => false,
    })
}
