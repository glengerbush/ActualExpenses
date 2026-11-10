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
    super::expressions::validate(rule)?;
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
    pub expressions: super::expressions::Budget,
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
            expressions: Default::default(),
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
        if let Some(id) = input["payee_id"].as_str()
            && let Some(name) = super::expressions::payee_name(db, b, id)?
        {
            row["payee_name"] = json!(name);
        }
        let mut expressions = None;
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
            if self.indexed && !super::expressions::candidate(rule, input) {
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
                if expressions.is_none() && rule.actions.iter().any(super::expressions::calculated)
                {
                    expressions = Some(super::expressions::Context::new(
                        db,
                        b,
                        input,
                        stored,
                        account_override,
                    )?);
                    expressions.as_mut().unwrap().prefetch(
                        db,
                        b,
                        self.rules.iter().filter(|rule| {
                            !self.indexed || super::expressions::candidate(rule, input)
                        }),
                    )?;
                }
                if let Some(context) = expressions.as_mut() {
                    context.begin_rule(db, b, &row, rule)?;
                }
                row = super::actions::apply(
                    db,
                    b,
                    &row,
                    rule,
                    expressions.as_mut(),
                    &mut self.work.expressions,
                )?;
            }
        }
        Ok((row, matched))
    }
}
