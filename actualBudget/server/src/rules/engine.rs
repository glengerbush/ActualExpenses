//! Rule evaluation is pure: referenced names are read, never created by previews.
use super::*;
pub fn validate(rule: &Rule) -> Result<(), Error> {
    crate::models::validate_id(&rule.id)?;
    if rule
        .stage
        .as_deref()
        .is_some_and(|s| !matches!(s, "pre" | "post"))
        || !matches!(rule.conditions_op.as_str(), "and" | "or")
        || rule.conditions.len() > 32
        || rule.actions.len() > 64
    {
        return Err(Error::invalid("Invalid rule stage, combination or size"));
    }
    for c in &rule.conditions {
        super::conditions::validate(c)?;
    }
    for a in &rule.actions {
        if !matches!(a["op"].as_str(), Some("set" | "append-notes"))
            || a["op"] == "set" && !matches!(a["field"].as_str(), Some("category" | "payee"))
        {
            return Err(Error::invalid("Invalid rule action"));
        }
    }
    Ok(())
}
fn score(rule: &Rule) -> i32 {
    let scores = rule
        .conditions
        .iter()
        .map(|c| match c["op"].as_str().unwrap_or("") {
            "is" | "isNot" => 10,
            "oneOf" | "notOneOf" => 9,
            "isapprox" | "isbetween" => 5,
            "gt" | "gte" | "lt" | "lte" => 1,
            _ => 0,
        })
        .sum::<i32>();
    if rule.conditions.iter().all(|c| {
        matches!(
            c["op"].as_str(),
            Some("is" | "isNot" | "isapprox" | "oneOf" | "notOneOf")
        )
    }) {
        scores * 2
    } else {
        scores
    }
}
pub fn rank(rules: &mut [Rule]) {
    rules.sort_by_key(|r| {
        (
            match r.stage.as_deref() {
                Some("pre") => 0,
                Some("post") => 2,
                _ => 1,
            },
            score(r),
            r.id.clone(),
        )
    });
}
fn key(field: &str) -> &str {
    match field {
        "account" => "account_id",
        "payee" => "payee_id",
        "category" => "category_id",
        f => f,
    }
}
pub fn evaluate(
    db: &Connection,
    b: &str,
    input: &Value,
    rules: &[Rule],
) -> Result<(Value, Vec<String>), Error> {
    let mut row = input.clone();
    let mut matched = vec![];
    let budget = crate::register_regex::replacement_budget();
    let mut work = 2_000_000usize;
    let account_off = if let Some(id) = input["account_id"].as_str() {
        db.query_row(
            "SELECT off_budget FROM accounts WHERE budget_id=?1 AND id=?2",
            params![b, id],
            |r| r.get::<_, bool>(0),
        )
        .optional()?
    } else {
        None
    };
    for rule in rules {
        let mut matches = !rule.conditions.is_empty() && rule.conditions_op == "and";
        for c in &rule.conditions {
            let result =
                super::conditions::matches(db, b, c, &row, account_off, &budget, &mut work)?;
            if rule.conditions_op == "or" && result {
                matches = true;
                break;
            }
            if rule.conditions_op == "and" && !result {
                matches = false;
                break;
            }
        }
        if !matches {
            continue;
        }
        matched.push(rule.id.clone());
        for a in &rule.actions {
            match a["op"].as_str() {
                Some("set") => row[key(a["field"].as_str().unwrap())] = a["value"].clone(),
                Some("append-notes") => {
                    row["notes"] = json!(format!(
                        "{}{}",
                        row["notes"].as_str().unwrap_or(""),
                        a["value"].as_str().unwrap_or("")
                    ))
                }
                _ => unreachable!(),
            }
        }
    }
    Ok((row, matched))
}
