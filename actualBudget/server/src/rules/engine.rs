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
        super::actions::validate(a)?;
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
pub struct Work {
    pub remaining: usize,
    pub regex: crate::register_regex::Budget,
    pub compiled: std::collections::BTreeMap<
        String,
        Result<std::sync::Arc<crate::register_regex::Pattern>, crate::register_regex::Failure>,
    >,
}
impl Default for Work {
    fn default() -> Self {
        Self {
            remaining: 2_000_000,
            regex: crate::register_regex::replacement_budget(),
            compiled: std::collections::BTreeMap::new(),
        }
    }
}
pub struct Runner {
    rules: Vec<Rule>,
    work: Work,
    indexed: bool,
}
impl Runner {
    pub fn for_apply(mut rules: Vec<Rule>) -> Self {
        // Pinned RuleEditor invokes rule-apply-actions, whose parser discards
        // options for these operations. Live rules-run retains their options.
        for rule in &mut rules {
            for action in &mut rule.actions {
                if matches!(
                    action["op"].as_str(),
                    Some("prepend-notes" | "append-notes" | "delete-transaction")
                ) {
                    action.as_object_mut().unwrap().remove("options");
                }
            }
        }
        Self {
            rules,
            work: Work::default(),
            indexed: false,
        }
    }
    pub fn with_work(rules: Vec<Rule>, work: Work) -> Self {
        Self {
            rules,
            work,
            indexed: true,
        }
    }
    pub fn into_work(self) -> Work {
        self.work
    }
    pub fn new(rules: Vec<Rule>) -> Self {
        Self {
            rules,
            work: Work::default(),
            indexed: true,
        }
    }
    pub fn evaluate(
        &mut self,
        db: &Connection,
        b: &str,
        input: &Value,
    ) -> Result<(Value, Vec<String>), Error> {
        self.evaluate_with_account_context(db, b, input, None)
    }
    /// Pure simulations may supply the on/off-budget context of a virtual account.
    pub(crate) fn evaluate_with_account_context(
        &mut self,
        db: &Connection,
        b: &str,
        input: &Value,
        account_override: Option<bool>,
    ) -> Result<(Value, Vec<String>), Error> {
        self.evaluate_context(db, b, input, account_override, false)
    }
    pub fn evaluate_stored(
        &mut self,
        db: &Connection,
        b: &str,
        input: &Value,
    ) -> Result<(Value, Vec<String>), Error> {
        self.evaluate_context(db, b, input, None, true)
    }
    fn evaluate_context(
        &mut self,
        db: &Connection,
        b: &str,
        input: &Value,
        account_override: Option<bool>,
        stored: bool,
    ) -> Result<(Value, Vec<String>), Error> {
        let mut row = input.clone();
        row["cleared"] = json!(true);
        row["reconciled"] = json!(false);
        if stored && let Some(children) = row.get_mut("children").and_then(Value::as_array_mut) {
            // Stored child views carry display names; source child rows do not.
            // Strip once before the chain so explicit later name actions survive.
            for child in children {
                child.as_object_mut().unwrap().remove("payee_name");
            }
        }
        if let Some(id) = input["payee_id"].as_str()
            && let Some(name) = payee_name(db, b, id)?
        {
            row["payee_name"] = json!(name);
        }
        let mut matched = vec![];
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
        let account_off = account_override.or(account_off);
        let context = super::conditions::Context { db, b, account_off };
        let attached = input["schedule_id"]
            .as_str()
            .map(|id| {
                db.query_row(
                    "SELECT rule_id FROM schedules WHERE budget_id=?1 AND id=?2 AND tombstone=0",
                    params![b, id],
                    |r| r.get::<_, String>(0),
                )
                .optional()
            })
            .transpose()?
            .flatten();
        let linked = db
            .prepare("SELECT rule_id,completed FROM schedules WHERE budget_id=?1 AND tombstone=0")?
            .query_map([b], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)))?
            .collect::<Result<std::collections::BTreeMap<_, _>, _>>()?;
        for rule in &self.rules {
            if self.indexed && !candidate(rule, input) {
                continue;
            }
            let forced = attached.as_deref() == Some(rule.id.as_str());
            if linked
                .get(&rule.id)
                .is_some_and(|completed| *completed || attached.is_some() && !forced)
            {
                continue;
            }
            let mut matches = !rule.conditions.is_empty() && rule.conditions_op == "and";
            for c in rule.conditions.iter().filter(|_| !forced) {
                let result = super::conditions::matches(&context, c, &row, &mut self.work)?;
                if rule.conditions_op == "or" && result {
                    matches = true;
                    break;
                }
                if rule.conditions_op == "and" && !result {
                    matches = false;
                    break;
                }
            }
            if matches || forced {
                matched.push(rule.id.clone());
                row = super::actions::apply(
                    db,
                    b,
                    &row,
                    rule,
                    stored
                        && row["children"]
                            .as_array()
                            .and_then(|c| c.first())
                            .is_some_and(|first| {
                                input["children"].as_array().is_some_and(|children| {
                                    children.iter().any(|child| child["id"] == first["id"])
                                })
                            }),
                )?;
            }
        }
        super::numbers::project(&mut row);
        if row["is_parent"] == true {
            row["category_id"] = Value::Null;
        }
        Ok((row, matched))
    }
}

pub(super) fn payee_name(db: &Connection, b: &str, id: &str) -> Result<Option<String>, Error> {
    Ok(db.query_row(
        "SELECT CASE WHEN transfer_account_id IS NULL THEN name ELSE '' END FROM payees WHERE budget_id=?1 AND id=?2",
        params![b,id], |r|r.get(0),
    ).optional()?)
}

pub(super) fn candidate(rule: &Rule, original: &Value) -> bool {
    fn key(value: &Value, first: bool) -> Option<Vec<u16>> {
        let value = value.as_str().filter(|s| !s.is_empty())?;
        if first {
            let unit = value.encode_utf16().next().unwrap();
            Some(
                char::from_u32(u32::from(unit))
                    .map(|c| {
                        c.to_lowercase()
                            .collect::<String>()
                            .encode_utf16()
                            .collect()
                    })
                    .unwrap_or(vec![unit]),
            )
        } else {
            Some(value.to_lowercase().encode_utf16().collect())
        }
    }
    [
        ("imported_payee", "imported_payee", true),
        ("payee", "payee_id", false),
    ]
    .iter()
    .any(|(field, native, first)| {
        let Some(condition) = rule.conditions.iter().find(|c| c["field"] == *field) else {
            return true;
        };
        if !matches!(
            condition["op"].as_str(),
            Some("is" | "isNot" | "oneOf" | "notOneOf")
        ) {
            return true;
        }
        let matches = |value: &Value| {
            // String conditions are parsed to lowercase before indexing; the
            // original transaction is indexed directly. This matters for
            // Unicode case mappings that expand into multiple code units.
            let normalized = if *first {
                value.as_str().map(|s| json!(s.to_lowercase()))
            } else {
                None
            };
            key(normalized.as_ref().unwrap_or(value), *first).is_none_or(|expected| {
                expected == [u16::from(b'*')] || Some(expected) == key(&original[*native], *first)
            })
        };
        match condition["value"].as_array() {
            Some(values) => values
                .iter()
                .filter(|value| !first || value.as_str().is_some_and(|s| !s.is_empty()))
                .any(matches),
            None => matches(&condition["value"]),
        }
    })
}
