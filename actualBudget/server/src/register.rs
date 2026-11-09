//! Validated register queries. Pages and exact aggregates share one read snapshot.
use crate::{
    error::Error,
    ledger,
    models::{MAX_MONEY, validate_date},
    store,
};
use rusqlite::{Connection, params_from_iter, types::Value as SqlValue};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Query {
    pub search: String,
    pub account_scope: AccountScope,
    pub sort: Sort,
    pub direction: Direction,
    pub hide_cleared: bool,
    pub hide_reconciled: bool,
    pub filter: Filter,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountScope {
    #[default]
    All,
    Onbudget,
    Offbudget,
    Closed,
    Uncategorized,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    #[default]
    Date,
    Amount,
    Payee,
    Category,
    Account,
    Notes,
    Cleared,
    Reconciled,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Asc,
    #[default]
    Desc,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Filter {
    pub op: Junction,
    pub conditions: Vec<Condition>,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Junction {
    #[default]
    And,
    Or,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub field: String,
    pub op: String,
    pub value: Value,
    #[serde(default)]
    pub options: Options,
}
fn bind(values: &mut Vec<SqlValue>, value: SqlValue) -> String {
    values.push(value);
    format!("?{}", values.len())
}
fn column(field: &str) -> Result<(&'static str, &'static str), Error> {
    Ok(match field {
        "category_group" => ("c.group_id", "id"),
        "account" => ("t.account_id", "id"),
        "payee" => ("t.payee_id", "id"),
        "category" => ("t.category_id", "id"),
        "account_name" => ("a.name", "name"),
        "payee_name" => ("coalesce(y.name,p.name)", "name"),
        "category_name" => ("c.name", "name"),
        "category_group_name" => ("g.name", "name"),
        "transfer" => ("(t.transfer_id IS NOT NULL)", "bool"),
        "parent" => ("t.is_parent", "bool"),
        "notes" => ("t.notes", "text"),
        "date" => ("t.date", "date"),
        "amount" => ("t.amount", "money"),
        "cleared" => ("t.cleared", "bool"),
        "reconciled" => ("t.reconciled", "bool"),
        _ => return Err(Error::invalid("Unsupported register filter field")),
    })
}
fn scalar(value: &Value, kind: &str) -> Result<SqlValue, Error> {
    match kind {
        "money" => value
            .as_i64()
            .filter(|n| (-MAX_MONEY..=MAX_MONEY).contains(n))
            .map(SqlValue::Integer)
            .ok_or_else(|| Error::invalid("Filter amount must be exact minor units")),
        "bool" => value
            .as_bool()
            .map(|b| SqlValue::Integer(b as i64))
            .ok_or_else(|| Error::invalid("Filter status must be boolean")),
        _ => {
            let s = value
                .as_str()
                .filter(|s| s.len() <= 1000)
                .ok_or_else(|| Error::invalid("Invalid filter text"))?;
            if kind == "date" {
                validate_date(s)?;
            }
            if kind == "id" {
                crate::models::validate_id(s)?;
            }
            Ok(SqlValue::Text(s.into()))
        }
    }
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    pub inflow: bool,
    pub outflow: bool,
    pub month: bool,
    pub year: bool,
}
fn condition_sql(condition: &Condition, values: &mut Vec<SqlValue>) -> Result<String, Error> {
    let (raw, kind) = column(&condition.field)?;
    let op = condition.op.as_str();
    if matches!(op, "onBudget" | "offBudget") && condition.field == "account" {
        return Ok(format!("a.off_budget={}", u8::from(op == "offBudget")));
    }
    let mut col = if kind == "text" {
        format!("register_lower(coalesce({raw},''))")
    } else {
        raw.to_string()
    };
    if kind == "money" && condition.options.outflow {
        col = format!("(-{col})");
    }
    let scalar_value = |v: &Value| -> Result<SqlValue, Error> {
        let value = scalar(v, if kind == "name" { "text" } else { kind })?;
        Ok(match value {
            SqlValue::Text(s) if kind == "text" => SqlValue::Text(s.to_lowercase()),
            v => v,
        })
    };
    let finish = |expression: String| -> String {
        if kind == "money" && op != "isbetween" {
            if condition.options.outflow {
                return format!("(t.amount<0 AND ({expression}))");
            }
            if condition.options.inflow {
                return format!("(t.amount>0 AND ({expression}))");
            }
        }
        expression
    };
    if condition.value.is_null() && matches!(op, "eq" | "ne") && kind == "id" {
        return Ok(format!(
            "{col} IS {}NULL",
            if op == "ne" { "NOT " } else { "" }
        ));
    }
    if kind == "date" && matches!(op, "eq" | "isapprox") {
        if condition.value.is_object() {
            let dates = crate::register_recurrence::dates(&condition.value)?;
            let expressions = dates
                .iter()
                .map(|date| date_match(date, op, values))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(if expressions.is_empty() {
                "0".into()
            } else {
                format!("({})", expressions.join(" OR "))
            });
        }
        return date_match(
            condition
                .value
                .as_str()
                .ok_or_else(|| Error::invalid("Invalid filter date"))?,
            op,
            values,
        );
    }
    if kind == "money" && matches!(op, "isbetween" | "isapprox") {
        let (low, high) = if op == "isbetween" {
            let a = scalar(&condition.value["num1"], "money")?;
            let b = scalar(&condition.value["num2"], "money")?;
            let (SqlValue::Integer(a), SqlValue::Integer(b)) = (a, b) else {
                unreachable!()
            };
            (a.min(b), a.max(b))
        } else {
            let SqlValue::Integer(n) = scalar_value(&condition.value)? else {
                unreachable!()
            };
            let threshold = ((n as i128).abs() * 75 + 500) / 1000;
            (
                (n as i128 - threshold) as i64,
                (n as i128 + threshold) as i64,
            )
        };
        let a = bind(values, SqlValue::Integer(low));
        let b = bind(values, SqlValue::Integer(high));
        let target = if op == "isbetween" { raw } else { &col };
        return Ok(finish(format!("{target} BETWEEN {a} AND {b}")));
    }
    if matches!(op, "in" | "not_in") {
        let array = condition
            .value
            .as_array()
            .filter(|a| a.len() <= 100)
            .ok_or_else(|| Error::invalid("Membership permits at most 100 values"))?;
        if array.is_empty() {
            return Ok("0".into());
        }
        let parameters = array
            .iter()
            .map(|v| {
                if v.is_null() && kind == "id" {
                    Ok(SqlValue::Null)
                } else {
                    scalar_value(v)
                }
            })
            .map(|r| r.map(|v| bind(values, v)))
            .collect::<Result<Vec<_>, _>>()?;
        let comparisons = parameters
            .iter()
            .map(|p| format!("{col} IS {}{p}", if op == "not_in" { "NOT " } else { "" }))
            .collect::<Vec<_>>();
        return Ok(finish(format!(
            "({})",
            comparisons.join(if op == "in" { " OR " } else { " AND " })
        )));
    }
    if matches!(op, "hasTags" | "hasAnyTag") && condition.field == "notes" {
        let text = condition
            .value
            .as_str()
            .filter(|s| s.len() <= 1000)
            .ok_or_else(|| Error::invalid("Invalid tags"))?;
        let tags = text
            .split(|c: char| c == '#' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        if tags.len() > 100 {
            return Err(Error::invalid("At most 100 tags"));
        }
        let expressions = tags
            .iter()
            .map(|tag| {
                let parameter = bind(
                    values,
                    SqlValue::Text(format!(
                        "(?<!#)#{}([\\s#]|$)",
                        crate::register_regex::escape(tag)
                    )),
                );
                format!("register_regex({parameter},t.notes)")
            })
            .collect::<Vec<_>>();
        return Ok(if expressions.is_empty() {
            "0".into()
        } else {
            format!(
                "({})",
                expressions.join(if op == "hasTags" { " AND " } else { " OR " })
            )
        });
    }
    if matches!(op, "contains" | "not_contains") && matches!(kind, "text" | "name") {
        let SqlValue::Text(pattern) = scalar_value(&condition.value)? else {
            unreachable!()
        };
        let parameter = bind(values, SqlValue::Text(like_regex(&pattern)));
        return Ok(format!(
            "{}register_like({parameter},coalesce({col},''))",
            if op == "not_contains" { "NOT " } else { "" }
        ));
    }
    let parameter = bind(values, scalar_value(&condition.value)?);
    let expression = match op {
        "eq" => format!("{col}={parameter}"),
        "ne" => format!("{col} IS NOT {parameter}"),
        "lt" | "lte" | "gt" | "gte" if matches!(kind, "money" | "date") => format!(
            "{col}{}{parameter}",
            match op {
                "lt" => "<",
                "lte" => "<=",
                "gt" => ">",
                _ => ">=",
            }
        ),
        "matches" if matches!(kind, "text" | "name") => {
            compile_regex(condition.value.as_str().unwrap())?;
            format!("register_regex({parameter},coalesce({col},''))")
        }
        _ => return Err(Error::invalid("Unsupported operator for register field")),
    };
    Ok(finish(expression))
}
fn date_match(date: &str, op: &str, values: &mut Vec<SqlValue>) -> Result<String, Error> {
    if date.len() == 4 || date.len() == 7 {
        if op != "eq" {
            return Err(Error::invalid("Approximate dates require a calendar day"));
        }
        validate_date(&format!(
            "{date}{}",
            if date.len() == 4 { "-01-01" } else { "-01" }
        ))?;
        let p = bind(values, SqlValue::Text(date.into()));
        return Ok(format!("substr(t.date,1,{})={p}", date.len()));
    }
    validate_date(date)?;
    if op == "isapprox" {
        let day = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(Error::internal)?;
        let lo = bind(
            values,
            SqlValue::Text((day - chrono::TimeDelta::days(2)).to_string()),
        );
        let hi = bind(
            values,
            SqlValue::Text((day + chrono::TimeDelta::days(2)).to_string()),
        );
        Ok(format!("t.date BETWEEN {lo} AND {hi}"))
    } else {
        Ok(format!(
            "t.date={}",
            bind(values, SqlValue::Text(date.into()))
        ))
    }
}
// Actual 26.10 unicodeLike: unanchored, case-insensitive, % and ? wildcards;
// underscore is literal and backslash escapes %, ? and backslash only.
fn like_regex(pattern: &str) -> String {
    let mut result = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek().is_some_and(|c| matches!(c, '%' | '?' | '\\')) {
            result.push_str(&crate::register_regex::escape(
                &chars.next().unwrap().to_string(),
            ));
        } else {
            match ch {
                '%' => result.push_str(".*"),
                '?' => result.push('.'),
                _ => result.push_str(&crate::register_regex::escape(&ch.to_string())),
            }
        }
    }
    result
}
fn compile_regex(pattern: &str) -> Result<crate::register_regex::Pattern, Error> {
    crate::register_regex::Pattern::compile(pattern, false).map_err(|e| e.api_error())
}
fn install_functions(db: &Connection) -> Result<crate::register_regex::Budget, Error> {
    use rusqlite::functions::FunctionFlags;
    db.create_scalar_function(
        "register_lower",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| Ok(ctx.get::<String>(0)?.to_lowercase()),
    )?;
    db.create_scalar_function(
        "register_has_tag",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let notes = ctx.get::<String>(0)?;
            let tag = ctx.get::<String>(1)?;
            Ok(crate::payees_tags::tags_in(&notes)
                .iter()
                .any(|(_, _, value)| *value == tag))
        },
    )?;
    crate::register_regex::install(db)
}

impl Query {
    pub fn validate(&self) -> Result<(), Error> {
        self.sql(&mut Vec::new(), None).map(|_| ())
    }
    fn sql(
        &self,
        values: &mut Vec<SqlValue>,
        preferences: Option<&Value>,
    ) -> Result<String, Error> {
        if self.search.chars().count() > 500 || self.filter.conditions.len() > 20 {
            return Err(Error::invalid(
                "Search permits 500 characters and 20 conditions",
            ));
        }
        let mut clauses = Vec::new();
        match self.account_scope {
            AccountScope::All => {}
            AccountScope::Onbudget => clauses.push("a.off_budget=0 AND a.closed=0".into()),
            AccountScope::Offbudget => clauses.push("a.off_budget=1 AND a.closed=0".into()),
            AccountScope::Closed => clauses.push("a.closed=1".into()),
            AccountScope::Uncategorized => {
                clauses.push("a.off_budget=0 AND t.category_id IS NULL AND t.is_parent=0 AND (p.transfer_account_id IS NULL OR y.off_budget=1)".into())
            }
        };
        if !self.search.is_empty() {
            let p = bind(values, SqlValue::Text(self.search.clone()));
            let extra = search_extras(&self.search, preferences, values);
            clauses.push(format!("(instr(register_lower(coalesce(y.name,p.name,'')),register_lower({p}))>0 OR instr(register_lower(t.notes),register_lower({p}))>0 OR instr(register_lower(coalesce(c.name,'')),register_lower({p}))>0 OR instr(register_lower(a.name),register_lower({p}))>0 OR instr(t.date,{p})>0{extra})"));
        }
        if self.hide_cleared {
            clauses.push("t.cleared=0".into());
        }
        if self.hide_reconciled {
            clauses.push("t.reconciled=0".into());
        }
        let mut conditions = Vec::new();
        for condition in &self.filter.conditions {
            conditions.push(condition_sql(condition, values)?);
        }
        if !conditions.is_empty() {
            clauses.push(format!(
                "({})",
                conditions.join(match self.filter.op {
                    Junction::And => " AND ",
                    Junction::Or => " OR ",
                })
            ));
        }
        Ok(if clauses.is_empty() {
            "1".into()
        } else {
            clauses.join(" AND ")
        })
    }
    fn order(&self) -> String {
        let col = match self.sort {
            Sort::Date => "t.date",
            Sort::Amount => "t.amount",
            Sort::Payee => "coalesce(y.name,p.name,'') COLLATE NOCASE",
            Sort::Category => "coalesce(c.name,'') COLLATE NOCASE",
            Sort::Account => "a.name COLLATE NOCASE",
            Sort::Notes => "t.notes COLLATE NOCASE",
            Sort::Cleared => "t.cleared",
            Sort::Reconciled => "t.reconciled",
        };
        let dir = match self.direction {
            Direction::Asc => "ASC",
            Direction::Desc => "DESC",
        };
        if matches!(self.sort, Sort::Date) {
            format!(
                "t.date {dir},t.starting_balance {},t.sort_order {dir},t.id {}",
                if matches!(self.direction, Direction::Asc) {
                    "DESC"
                } else {
                    "ASC"
                },
                if matches!(self.direction, Direction::Asc) {
                    "DESC"
                } else {
                    "ASC"
                }
            )
        } else {
            format!("{col} {dir},t.date DESC,t.starting_balance ASC,t.sort_order DESC,t.id ASC")
        }
    }
}
// Saved queries retain stable source IDs. Resolve aliases when executing, without
// rejecting a missing reference which may be restored by a later import.
fn canonical_query(db: &Connection, budget: &str, query: &Query) -> Result<Query, Error> {
    let mut query = query.clone();
    for condition in &mut query.filter.conditions {
        if condition.field != "payee" {
            continue;
        }
        let values: Vec<&mut Value> = match &mut condition.value {
            Value::Array(values) => values.iter_mut().collect(),
            value => vec![value],
        };
        for value in values {
            if let Some(id) = value.as_str() {
                let exists: bool = db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM payees WHERE budget_id=?1 AND id=?2)",
                    rusqlite::params![budget, id],
                    |r| r.get(0),
                )?;
                if exists {
                    *value = json!(crate::payees_tags::canonical(db, budget, id)?);
                }
            }
        }
    }
    Ok(query)
}
fn search_extras(search: &str, preferences: Option<&Value>, values: &mut Vec<SqlValue>) -> String {
    use chrono::Datelike;
    let format = preferences
        .and_then(|p| p["number_format"].as_str())
        .unwrap_or("comma-dot");
    let grouping = if format == "dot-comma" { '.' } else { ',' };
    let text = search.replace('−', "-");
    let separator = text
        .rfind([',', '.'])
        .filter(|i| text.as_bytes()[*i] as char != grouping || *i + 4 > text.len());
    let (whole, fraction) = if let Some(i) = separator {
        (&text[..i], &text[i + 1..])
    } else {
        (text.as_str(), "")
    };
    let whole = whole
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '-')
        .collect::<String>();
    let fraction = fraction
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    let mut extra = String::new();
    if let Ok(number) = format!("{whole}.{fraction}").parse::<f64>() {
        // Pinned Actual has zero-decimal IRR/JPY/KRW; the additional whole-unit
        // search still uses its historical /100 expression (queries/index.ts).
        let factor = if preferences.is_some_and(|p| {
            p["experimental"]["currency"] == true
                && ["IRR", "JPY", "KRW"].contains(&p["currency_code"].as_str().unwrap_or(""))
        }) {
            1.0
        } else {
            100.0
        };
        if number.is_finite() && number.abs() <= MAX_MONEY as f64 / factor {
            let amount = (number * factor + 0.5).floor() as i64;
            let p = bind(values, SqlValue::Integer(amount));
            extra += &format!(" OR abs(t.amount)={p}");
            if number.fract() == 0.0 {
                let p = bind(values, SqlValue::Integer(number as i64));
                extra += &format!(" OR abs(t.amount/100)={p}");
            }
        }
    }
    let date_format = preferences
        .and_then(|p| p["date_format"].as_str())
        .unwrap_or("MM/dd/yyyy");
    let pattern = date_format
        .replace("yyyy", "%Y")
        .replace("MM", "%m")
        .replace("dd", "%d");
    let date = chrono::NaiveDate::parse_from_str(search, &pattern)
        .ok()
        .or_else(|| chrono::NaiveDate::parse_from_str(search, &pattern.replace("%Y", "%y")).ok())
        .or_else(|| {
            let year = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).year();
            let short = pattern
                .replace("/%Y", "")
                .replace(".%Y", "")
                .replace("-%Y", "");
            if short == pattern {
                return None;
            }
            chrono::NaiveDate::parse_from_str(&format!("{search}/{year}"), &format!("{short}/%Y"))
                .ok()
        });
    if let Some(date) = date {
        let p = bind(values, SqlValue::Text(date.to_string()));
        extra += &format!(" OR t.date={p}");
    }
    extra
}
fn exact(value: i128) -> Result<i64, Error> {
    if value.abs() > MAX_MONEY as i128 {
        Err(Error::invalid(
            "Register aggregate exceeds exact integer range",
        ))
    } else {
        Ok(value as i64)
    }
}
/// The common newest-first, unfiltered page uses cached financial totals and
/// indexed bounded rows. Only an index COUNT and preceding-page amounts scale.
fn set_running(row: &mut Value, amount: i128) {
    match exact(amount) {
        Ok(value) => row["running_balance"] = json!(value),
        Err(_) => {
            row["running_balance"] = Value::Null;
            row["running_balance_error"] = json!("exact_money_range");
        }
    }
}
fn latest(
    db: &Connection,
    budget: &str,
    account: Option<&str>,
    limit: u32,
    offset: u32,
    snapshot: &str,
) -> Result<Value, Error> {
    let summary = if let Some(account) = account {
        serde_json::to_value(ledger::cached_balances(db, budget, account)?)
            .map_err(Error::internal)?
    } else {
        let mut sums = [0i128; 3];
        let mut statement=db.prepare("SELECT l.total,l.cleared,l.uncleared FROM ledger_totals l JOIN accounts a ON a.budget_id=l.budget_id AND a.id=l.account_id WHERE l.budget_id=?1 AND a.tombstone=0")?;
        for row in statement.query_map([budget], |r| {
            Ok([
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ])
        })? {
            for (sum, value) in sums.iter_mut().zip(row?) {
                *sum += value as i128;
            }
        }
        json!({"total":exact(sums[0])?,"cleared":exact(sums[1])?,"uncleared":exact(sums[2])?})
    };
    let mut values = vec![SqlValue::Text(budget.into())];
    let mut filter = "budget_id=?1 AND tombstone=0 AND parent_id IS NULL".to_string();
    if let Some(account) = account {
        filter += &format!(
            " AND account_id={}",
            bind(&mut values, SqlValue::Text(account.into()))
        );
    }
    let order = "date DESC,starting_balance ASC,sort_order DESC,id ASC";
    let count: i64 = db.query_row(
        &format!("SELECT count(*) FROM transactions WHERE {filter}"),
        params_from_iter(values.iter()),
        |r| r.get(0),
    )?;
    let mut statement = db.prepare(&format!(
        "SELECT id FROM transactions WHERE {filter} ORDER BY {order} LIMIT {} OFFSET {offset}",
        limit + 1
    ))?;
    let mut ids = statement
        .query_map(params_from_iter(values.iter()), |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let more = ids.len() > limit as usize;
    ids.truncate(limit as usize);
    let mut running = summary["total"].as_i64().unwrap() as i128;
    if offset > 0 {
        let mut preceding = db.prepare(&format!(
            "SELECT amount FROM transactions WHERE {filter} ORDER BY {order} LIMIT {offset}"
        ))?;
        for amount in
            preceding.query_map(params_from_iter(values.iter()), |r| r.get::<_, i64>(0))?
        {
            running -= amount? as i128;
        }
    }
    let mut data = Vec::with_capacity(ids.len());
    let mut hydrated = hydrate(db, budget, &ids)?;
    for id in ids {
        let mut row = hydrated.remove(&id).unwrap();
        set_running(&mut row, running);
        running -= row["amount"].as_i64().unwrap() as i128;
        data.push(row);
    }
    Ok(
        json!({"data":data,"balances":summary,"filtered_balances":summary,"filtered_count":count,"as_of":Value::Null,"snapshot":snapshot,"next_offset":if more{Some(offset+limit)}else{None}}),
    )
}
fn hydrate(db: &Connection, budget: &str, ids: &[String]) -> Result<HashMap<String, Value>, Error> {
    let mut data = ledger::get_many(db, budget, ids)?
        .into_iter()
        .map(|row| (row["id"].as_str().unwrap().to_string(), row))
        .collect::<HashMap<_, _>>();
    let placeholders = (2..=ids.len() + 1)
        .map(|n| format!("?{n}"))
        .collect::<Vec<_>>()
        .join(",");
    let mut values = vec![budget.to_string()];
    values.extend(ids.iter().cloned());
    if !ids.is_empty() {
        let mut statement=db.prepare(&format!("SELECT t.id,g.name FROM transactions t LEFT JOIN categories c ON c.budget_id=t.budget_id AND c.id=t.category_id LEFT JOIN category_groups g ON g.budget_id=c.budget_id AND g.id=c.group_id WHERE t.budget_id=?1 AND t.id IN ({placeholders})"))?;
        for row in statement.query_map(params_from_iter(values.iter()), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })? {
            let (id, group) = row?;
            data.get_mut(&id).unwrap()["category_group_name"] = json!(group);
        }
    }
    Ok(data)
}
#[allow(clippy::too_many_arguments)]
pub fn list(
    db: &Connection,
    budget: &str,
    filters: ledger::RegisterFilters<'_>,
    limit: u32,
    offset: u32,
    snapshot: Option<&str>,
    query: &Query,
) -> Result<Value, Error> {
    let matcher = install_functions(db)?;
    let result = list_inner(db, budget, filters, limit, offset, snapshot, query);
    if let Some(error) = matcher.error() {
        return Err(error);
    }
    result
}
#[allow(clippy::too_many_arguments)]
fn list_inner(
    db: &Connection,
    budget: &str,
    filters: ledger::RegisterFilters<'_>,
    limit: u32,
    offset: u32,
    snapshot: Option<&str>,
    query: &Query,
) -> Result<Value, Error> {
    let ledger::RegisterFilters {
        account,
        as_of,
        payee,
        tag,
    } = filters;
    store::require_budget(db, budget)?;
    if let Some(account) = account {
        ledger::account_exists(db, budget, account)?;
    }
    let current = crate::concurrency::cursor(db, budget)?;
    if snapshot.is_some_and(|s| s != current) {
        return Err(Error::new(
            axum::http::StatusCode::CONFLICT,
            "register_changed",
            "Register changed; reload the first page",
        ));
    }
    if as_of.is_none()
        && payee.is_none()
        && tag.is_none()
        && query.search.is_empty()
        && query.filter.conditions.is_empty()
        && !query.hide_cleared
        && !query.hide_reconciled
        && matches!(query.account_scope, AccountScope::All)
        && matches!(query.sort, Sort::Date)
        && matches!(query.direction, Direction::Desc)
    {
        return latest(db, budget, account, limit, offset, &current);
    }
    let mut values = vec![SqlValue::Text(budget.into())];
    let mut base =
        "t.budget_id=?1 AND t.tombstone=0 AND t.parent_id IS NULL AND a.tombstone=0".to_string();
    if let Some(account) = account {
        base += &format!(
            " AND t.account_id={}",
            bind(&mut values, SqlValue::Text(account.into()))
        );
    }
    if let Some(date) = as_of {
        base += &format!(
            " AND t.date<={}",
            bind(&mut values, SqlValue::Text(date.into()))
        );
    }
    let preferences = crate::preferences::get_preferences(db, budget)?;
    let query = canonical_query(db, budget, query)?;
    let mut filter = query.sql(&mut values, Some(&preferences))?;
    // Legacy filters remain additional AND clauses outside the user's OR group.
    // Grouping happens only after the complete row predicate is assembled.
    if let Some(payee) = payee {
        let payee = crate::payees_tags::canonical(db, budget, payee)?;
        let p = bind(&mut values, SqlValue::Text(payee));
        filter = format!("({filter}) AND t.payee_id={p}");
    }
    if let Some(tag) = tag {
        let p = bind(&mut values, SqlValue::Text(tag.to_owned()));
        filter = format!("({filter}) AND register_has_tag(t.notes,{p})");
    }
    let child_filter = filter
        .replace("y.", "child_destination.")
        .replace("t.", "child.")
        .replace("p.", "child_payee.")
        .replace("c.", "child_category.")
        .replace("g.", "child_group.");
    // Actual's grouped executor applies every clause to ONE row before grouping.
    // Separate EXISTS per condition would incorrectly combine unrelated children.
    let filter = format!(
        "(({filter}) OR (t.is_parent=1 AND EXISTS(SELECT 1 FROM transactions child LEFT JOIN payees child_payee ON child_payee.budget_id=child.budget_id AND child_payee.id=child.payee_id AND child_payee.tombstone=0 LEFT JOIN accounts child_destination ON child_destination.budget_id=child.budget_id AND child_destination.id=child_payee.transfer_account_id LEFT JOIN categories child_category ON child_category.budget_id=child.budget_id AND child_category.id=child.category_id LEFT JOIN category_groups child_group ON child_group.budget_id=child_category.budget_id AND child_group.id=child_category.group_id WHERE child.budget_id=t.budget_id AND child.parent_id=t.id AND child.tombstone=0 AND ({child_filter}))))"
    );
    let joins = "FROM transactions t JOIN accounts a ON a.budget_id=t.budget_id AND a.id=t.account_id LEFT JOIN payees p ON p.budget_id=t.budget_id AND p.id=t.payee_id AND p.tombstone=0 LEFT JOIN accounts y ON y.budget_id=t.budget_id AND y.id=p.transfer_account_id LEFT JOIN categories c ON c.budget_id=t.budget_id AND c.id=t.category_id LEFT JOIN category_groups g ON g.budget_id=c.budget_id AND g.id=c.group_id";
    let mut stmt = db.prepare(&format!(
        "SELECT t.id {joins} WHERE {base} AND ({filter}) ORDER BY {} LIMIT {} OFFSET {}",
        query.order(),
        limit + 1,
        offset
    ))?;
    let mut ids = stmt
        .query_map(params_from_iter(values.iter()), |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let more = ids.len() > limit as usize;
    ids.truncate(limit as usize);
    let mut running = ids
        .iter()
        .map(|id| (id.clone(), 0i128))
        .collect::<HashMap<_, _>>();
    let mut sums = [0i128; 3];
    let mut filtered = [0i128; 3];
    let mut count = 0u64;
    // Stream integers on the server, keeping only the requested page. i128 avoids
    // SQLite's intermediate SUM overflow and floating-point TOTAL rounding.
    let mut stmt=db.prepare(&format!("SELECT t.id,t.amount,t.cleared,coalesce(({filter}),0) {joins} WHERE {base} ORDER BY t.date ASC,t.starting_balance DESC,t.sort_order ASC,t.id DESC"))?;
    let mut rows = stmt.query(params_from_iter(values.iter()))?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let amount: i64 = row.get(1)?;
        let cleared: bool = row.get(2)?;
        let matched: bool = row.get(3)?;
        sums[0] += amount as i128;
        sums[if cleared { 1 } else { 2 }] += amount as i128;
        if matched {
            count += 1;
            filtered[0] += amount as i128;
            filtered[if cleared { 1 } else { 2 }] += amount as i128;
        }
        if let Some(balance) = running.get_mut(&id) {
            *balance = sums[0];
        }
    }
    let balances = |s: [i128; 3]| -> Result<Value, Error> {
        Ok(json!({"total":exact(s[0])?,"cleared":exact(s[1])?,"uncleared":exact(s[2])?}))
    };
    let mut hydrated = hydrate(db, budget, &ids)?;
    let data = ids
        .iter()
        .map(|id| {
            let mut row = hydrated.remove(id).unwrap();
            if payee.is_some() || tag.is_some() {
                row["running_balance"] = Value::Null;
            } else {
                set_running(&mut row, running[id]);
            }
            Ok(row)
        })
        .collect::<Result<Vec<Value>, Error>>()?;
    Ok(
        json!({"data":data,"balances":balances(sums)?,"filtered_balances":balances(filtered)?,"filtered_count":count,"as_of":as_of,"snapshot":current,"next_offset":if more{Some(offset+limit)}else{None}}),
    )
}
