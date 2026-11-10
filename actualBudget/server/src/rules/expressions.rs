//! Source Action conversions run in the bounded trusted engine. Database reads
//! prepare context from the original transaction, never from a mutated amount.
use super::*;
use std::time::{Duration, Instant};

pub struct Budget {
    remaining: Duration,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            remaining: Duration::from_secs(10),
        }
    }
}
impl Budget {
    fn run(&mut self, input: &Value) -> Result<Value, Error> {
        if self.remaining.is_zero() || input.to_string().len() > 262144 {
            return Err(Error::invalid("Calculated rule work limit reached"));
        }
        let start = Instant::now();
        let result = crate::formulas::rule_actions(input);
        let elapsed = start.elapsed();
        let exhausted = elapsed > self.remaining;
        self.remaining = self.remaining.saturating_sub(elapsed);
        if exhausted {
            return Err(Error::invalid("Calculated rule work limit reached"));
        }
        result
    }
}
pub fn calculated(a: &Value) -> bool {
    a["options"]["template"]
        .as_str()
        .is_some_and(|s| !s.is_empty())
        || a["options"]["formula"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
        || a["op"] == "set-split-amount" && a["options"]["method"] == "formula"
}
fn input(preferences: Value) -> Value {
    json!({"preferences":preferences,"locale":"en-US","locale_format":"comma-dot",
        "local_clock_ms":chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).timestamp_millis()})
}
pub fn validate(rule: &Rule) -> Result<(), Error> {
    let actions: Vec<_> = rule.actions.iter().filter(|a| calculated(a)).collect();
    if actions.is_empty() {
        return Ok(());
    }
    let mut value = input(json!({"experimental":{}}));
    value["actions"] = json!(actions);
    value["validate"] = json!(true);
    Budget::default().run(&value)?;
    Ok(())
}
pub struct Context {
    stored: bool,
    input: Value,
    original: Value,
    helpers: Value,
    balances: std::collections::BTreeMap<String, i64>,
    balance: Option<i64>,
}
impl Context {
    pub fn new(db: &Connection, b: &str, original: &Value, stored: bool) -> Result<Self, Error> {
        let mut helpers = json!({});
        if let Some(id) = original["account_id"].as_str() {
            let account = crate::accounts::get(db, b, id)?;
            helpers["_account_name"] = account["name"].clone();
            // Only the source account fields; native computed summaries are not
            // rule variables and must not change template context accidentally.
            helpers["_account"] = json!({"id":id,"name":account["name"],
                "offbudget":account["off_budget"],"closed":account["closed"],
                "sort_order":account["sort_order"]});
        }
        if let Some(id) = original["category_id"].as_str() {
            let category = crate::categories::get(db, b, "categories", id)?;
            helpers["_category_name"] = category["name"].clone();
        }
        if let Some(id) = original["payee_id"].as_str()
            && let Some(name) = db
                .query_row(
                    "SELECT name FROM payees WHERE budget_id=?1 AND id=?2",
                    params![b, id],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
        {
            helpers["payee_name"] = json!(name);
        }
        let mut original = original.clone();
        if stored
            && let Some(id) = original["id"].as_str()
            && let Some(order) = db
                .query_row(
                    "SELECT source_order FROM android_upgrade_order WHERE budget_id=?1 AND id=?2",
                    params![b, id],
                    |r| r.get::<_, Option<f64>>(0),
                )
                .optional()?
        {
            original["sort_order"] = json!(order);
        }
        if stored && let Some(id) = original["id"].as_str() {
            let metadata=db.query_row("SELECT imported_id,raw_synced_data FROM transaction_import_metadata WHERE budget_id=?1 AND id=?2 AND tombstone=0",params![b,id],|r|Ok((r.get::<_,Option<String>>(0)?,r.get::<_,Option<String>>(1)?))).optional()?;
            let (imported, raw) = metadata.unwrap_or_default();
            helpers["imported_id"] = json!(imported);
            helpers["raw_synced_data"] = json!(raw);
        }
        Ok(Self {
            stored,
            input: input(crate::preferences::stored_preferences(db, b)?),
            original,
            helpers,
            balances: Default::default(),
            balance: None,
        })
    }
    pub fn original_order(&self) -> Option<f64> {
        self.original["sort_order"].as_f64()
    }
    pub fn begin_rule(
        &mut self,
        db: &Connection,
        b: &str,
        row: &Value,
        rule: &Rule,
    ) -> Result<(), Error> {
        self.helpers
            .as_object_mut()
            .unwrap()
            .remove("category_group");
        if let Some(id) = row["category_id"].as_str()
            && let Some(group) = db
                .query_row(
                    "SELECT group_id FROM categories WHERE budget_id=?1 AND id=?2",
                    params![b, id],
                    |r| r.get::<_, Option<String>>(0),
                )
                .optional()?
                .flatten()
        {
            self.helpers["category_group"] = json!(group);
        }
        for action in &rule.actions {
            self.prepare_action(db, b, action, true)?;
        }
        Ok(())
    }
    pub fn prefetch<'a>(
        &mut self,
        db: &Connection,
        b: &str,
        rules: impl IntoIterator<Item = &'a Rule>,
    ) -> Result<(), Error> {
        for rule in rules {
            for action in &rule.actions {
                self.prepare_action(db, b, action, false)?;
            }
        }
        Ok(())
    }
    fn running_balance(&self, db: &Connection, b: &str, account: &str) -> Result<i64, Error> {
        let today = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
            .date_naive()
            .to_string();
        let date = self.original["date"].as_str().unwrap_or(&today);
        let id = self.original["id"].as_str();
        let order = self.original["sort_order"].as_f64();
        let mut q=db.prepare("WITH rows AS (SELECT t.id,t.amount,t.date,CASE WHEN o.id IS NULL THEN t.sort_order ELSE o.source_order END AS sort_order FROM transactions t LEFT JOIN android_upgrade_order o ON o.budget_id=t.budget_id AND o.id=t.id WHERE t.budget_id=?1 AND t.account_id=?2 AND t.tombstone=0 AND t.is_parent=0) SELECT amount FROM rows WHERE (?3 IS NULL OR id<>?3) AND (date<?4 OR (date=?4 AND ((?5 IS NOT NULL AND sort_order<?5) OR (?5 IS NULL AND (sort_order IS NOT NULL OR id<?3)))))")?;
        let values = q.query_map(params![b, account, id, date, order], |r| r.get::<_, i64>(0))?;
        let mut total = 0i128;
        for value in values {
            total += i128::from(value?);
        }
        super::actions::money(total)
    }
    fn prepare_action(
        &mut self,
        db: &Connection,
        b: &str,
        a: &Value,
        current: bool,
    ) -> Result<(), Error> {
        // Exactly the source literal recognizer, including its escaped quotes.
        static LITERALS: std::sync::OnceLock<regress::Regex> = std::sync::OnceLock::new();
        static BALANCE: std::sync::OnceLock<regress::Regex> = std::sync::OnceLock::new();
        let literals = LITERALS.get_or_init(|| {
            regress::Regex::with_flags(r#"BALANCE_OF\s*\(\s*"((?:[^"\\]|\\.)*)"\s*\)"#, "i")
                .unwrap()
        });
        let balance =
            BALANCE.get_or_init(|| regress::Regex::with_flags(r"\bbalance\b", "i").unwrap());
        for text in [
            a["options"]["template"].as_str(),
            a["options"]["formula"].as_str(),
        ]
        .into_iter()
        .flatten()
        {
            if current && balance.find(text).is_some() && self.balance.is_none() {
                self.balance = Some(match self.original["account_id"].as_str() {
                    Some(account) => self.running_balance(db, b, account)?,
                    None => 0,
                });
            }
        }
        if let Some(formula) = a["options"]["formula"].as_str() {
            for matched in literals.find_iter(formula) {
                let name = formula[matched.group(1).unwrap()]
                    .replace("\\\"", "\"")
                    .replace("\\\\", "\\");
                if !self.balances.contains_key(&name) {
                    if self.balances.len() >= 128 {
                        return Err(Error::invalid("Rule account lookup limit reached"));
                    }
                    let account: Option<String>=db.query_row("SELECT id FROM accounts WHERE budget_id=?1 AND tombstone=0 AND (id=?2 OR name=?2) ORDER BY (id=?2) DESC,sort_order,id LIMIT 1",params![b,name],|r|r.get(0)).optional()?;
                    let amount = match account {
                        Some(id) => self.running_balance(db, b, &id)?,
                        None => 0,
                    };
                    self.balances.insert(name, amount);
                }
            }
        }
        Ok(())
    }
    pub fn apply(
        &mut self,
        row: &mut Value,
        a: &Value,
        parent: Option<&Value>,
        budget: &mut Budget,
    ) -> Result<(), Error> {
        let mut transaction = source_row(row, self.stored && parent.is_none());
        if self.stored && parent.is_none() {
            transaction["sort_order"] = self.original["sort_order"].clone();
        }
        if let Some(name) = row.get("_rule_payee_name") {
            transaction["payee_name"] = name.clone();
            transaction["payee"] = json!("new");
        }
        if let Some(parent) = parent {
            for (source, native) in [
                ("parent_amount", "amount"),
                ("parent_notes", "notes"),
                ("parent_imported_payee", "imported_payee"),
                ("parent_payee", "payee_id"),
                ("parent_account", "account_id"),
            ] {
                transaction[source] = parent
                    .get(native)
                    .filter(|v| !v.is_null())
                    .cloned()
                    .unwrap_or(json!(""));
            }
        } else {
            for (key, value) in self.helpers.as_object().unwrap() {
                if transaction.get(key).is_none() {
                    transaction[key] = value.clone();
                }
            }
        }
        transaction["balance"] = json!(self.balance.unwrap_or(0));
        if let Some(errors) = row.get("rule_errors") {
            transaction["_ruleErrors"] = errors.clone();
        }
        let mut input = self.input.clone();
        input["transaction"] = transaction;
        input["actions"] = json!([a]);
        input["balances"] = json!(self.balances.iter().collect::<Vec<_>>());
        let result = budget.run(&input)?;
        if let Some(errors) = result.get("_ruleErrors") {
            row["rule_errors"] = errors.clone();
        }
        if a["op"] == "set-split-amount" {
            row["amount"] = result["amount"].clone();
        } else if a["op"] == "set" {
            let field = a["field"].as_str().unwrap();
            if matches!(field, "account" | "payee" | "category")
                && a["options"]["formula"]
                    .as_str()
                    .is_some_and(|v| !v.is_empty())
            {
                return Ok(()); // Source formula actions do not assign ID fields.
            }
            let target = match field {
                "account" => "account_id",
                "payee" => "payee_id",
                "category" => "category_id",
                "payee_name" => "_rule_payee_name",
                v => v,
            };
            // Failed payee-name formulas leave payee unchanged in source.
            if (field != "payee_name" || result["payee"] == "new")
                && let Some(value) = result.get(field)
            {
                row[target] = value.clone();
            }
        }
        Ok(())
    }
}

// The source indexers union the imported-payee first-character and payee-ID
// candidates. They include wildcard buckets even when a condition won't match.
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
fn source_row(row: &Value, stored: bool) -> Value {
    let mut value = json!({});
    for field in [
        "id",
        "amount",
        "date",
        "notes",
        "cleared",
        "reconciled",
        "sort_order",
        "imported_payee",
        "imported_id",
        "raw_synced_data",
        "transfer_id",
        "is_parent",
        "is_child",
        "parent_id",
        "payee_name",
        "starting_balance_flag",
        "tombstone",
        "forceUpcoming",
        "_unmatched",
        "_deleted",
        "error",
    ] {
        if let Some(v) = row.get(field) {
            value[field] = v.clone();
        }
    }
    for (source, native) in [
        ("account", "account_id"),
        ("payee", "payee_id"),
        ("category", "category_id"),
        ("schedule", "schedule_id"),
        ("starting_balance_flag", "starting_balance"),
        ("tombstone", "deleted"),
    ] {
        if let Some(v) = row.get(native) {
            value[source] = v.clone();
        }
    }
    if stored {
        value["is_parent"] = json!(row["is_parent"] == true);
    }
    if stored || !row["parent_id"].is_null() {
        value["is_child"] = json!(!row["parent_id"].is_null());
    }
    if stored {
        value["tombstone"] = json!(row["deleted"] == true);
        value["error"] = row["error"].clone();
        for field in ["transfer_id", "parent_id", "schedule"] {
            value
                .as_object_mut()
                .unwrap()
                .entry(field)
                .or_insert(Value::Null);
        }
    }
    if let Some(children) = row["children"].as_array() {
        value["subtransactions"] = json!(
            children
                .iter()
                .map(|c| source_row(c, stored))
                .collect::<Vec<_>>()
        );
    }
    value
}
