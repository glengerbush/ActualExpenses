//! Bounded financial prefetch for trusted custom functions; never supply a local ledger.
use crate::{budgeting, error::Error};
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter, types::Value as SqlValue};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
const JOINS: &str = "FROM transactions t JOIN accounts a ON a.budget_id=t.budget_id AND a.id=t.account_id LEFT JOIN payees p ON p.budget_id=t.budget_id AND p.id=t.payee_id AND p.tombstone=0 LEFT JOIN accounts y ON y.budget_id=t.budget_id AND y.id=p.transfer_account_id LEFT JOIN categories c ON c.budget_id=t.budget_id AND c.id=t.category_id LEFT JOIN category_groups g ON g.budget_id=c.budget_id AND g.id=c.group_id";
pub struct Context<'a> {
    pub db: &'a Connection,
    pub budget: &'a str,
    pub queries: &'a Value,
    pub today: NaiveDate,
    pub work: usize,
    pub prefetch_values: usize,
    pub matcher: &'a crate::register_regex::Budget,
}
fn names(requests: &Value, key: &str) -> Result<Vec<String>, Error> {
    let values = requests[key].as_array().cloned().unwrap_or_default();
    if values.len() > 32 {
        return Err(Error::invalid("Maximum32 formula requests per function"));
    }
    values
        .into_iter()
        .map(|v| {
            v.as_str()
                .filter(|s| s.len() <= 512)
                .map(str::to_owned)
                .ok_or_else(|| Error::invalid("Invalid formula query name"))
        })
        .collect()
}
fn exact(value: i128) -> Result<i64, Error> {
    if value.abs() > 9007199254740991 {
        Err(Error::invalid(
            "Formula aggregate exceeds exact numeric range",
        ))
    } else {
        Ok(value as i64)
    }
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::String(v) => !v.is_empty(),
        Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
        _ => true,
    }
}
impl Context<'_> {
    fn frame(&self, config: &Value) -> Result<Value, Error> {
        crate::reports::formula_range(self.db, self.budget, config, self.today)
    }
    fn transaction_total(&self, config: &Value, count: bool) -> Result<i64, Error> {
        if !config.is_object() {
            return Err(Error::invalid("Invalid named query configuration"));
        }
        let mut values = vec![SqlValue::Text(self.budget.into())];
        // Formula transaction queries apply named conditions; report graphs skip
        // them. Strip only presentation metadata at this adapter boundary.
        let mut transaction_config = config.clone();
        if let Some(conditions) = transaction_config["conditions"].as_array_mut() {
            for condition in conditions {
                if let Some(condition) = condition.as_object_mut() {
                    condition.remove("customName");
                }
            }
        }
        let mut filter = crate::reports::formula_predicate(
            self.db,
            self.budget,
            &transaction_config,
            &mut values,
        )?;
        if truthy(&config["timeFrame"]["mode"]) {
            let frame = self.frame(&config["timeFrame"])?;
            let normal = |v: &Value, end: bool| -> Result<String, Error> {
                let s = v
                    .as_str()
                    .ok_or_else(|| Error::invalid("Invalid formula timeframe"))?;
                if s.len() == 7 {
                    let m = budgeting::month(s)?;
                    if end {
                        let next = budgeting::month_name(m + 1)?;
                        let d = NaiveDate::parse_from_str(&(next + "-01"), "%Y-%m-%d")
                            .map_err(Error::internal)?;
                        Ok(d.pred_opt()
                            .ok_or_else(|| Error::invalid("Invalid ending date"))?
                            .to_string())
                    } else {
                        Ok(format!("{s}-01"))
                    }
                } else {
                    crate::models::validate_date(s)?;
                    Ok(s.into())
                }
            };
            values.push(SqlValue::Text(normal(&frame[0], false)?));
            let a = values.len();
            values.push(SqlValue::Text(normal(&frame[1], true)?));
            let z = values.len();
            filter = format!("({filter}) AND t.date>=?{a} AND t.date<=?{z}");
        }
        let aggregate = if count {
            "count(*)"
        } else {
            "coalesce(sum(t.amount),0)"
        };
        let sql = format!(
            "SELECT {aggregate} {JOINS} WHERE t.budget_id=?1 AND t.tombstone=0 AND t.is_parent=0 AND ({filter})"
        );
        self.db
            .query_row(&sql, params_from_iter(values), |r| r.get(0))
            .map_err(|e| {
                if matches!(&e,rusqlite::Error::SqliteFailure(_,Some(m))if m=="integer overflow") {
                    Error::invalid("Formula sum exceeds exact integer range")
                } else {
                    e.into()
                }
            })
    }
    fn category_ids(&mut self, config: &Value) -> Result<Vec<String>, Error> {
        let conditions = config["conditions"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|c| {
                !truthy(&c["customName"])
                    && matches!(c["field"].as_str(), Some("category" | "category_group"))
            })
            .collect::<Vec<_>>();
        if conditions.len() > 20 {
            return Err(Error::invalid("Maximum20 category conditions"));
        }
        let mut statement=self.db.prepare("SELECT c.id,c.name,c.group_id,c.is_income,0,g.name FROM categories c LEFT JOIN category_groups g ON g.budget_id=c.budget_id AND g.id=c.group_id WHERE c.budget_id=?1 AND c.tombstone=0 ORDER BY c.sort_order,c.id LIMIT 10001")?;
        let rows = statement
            .query_map([self.budget], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, bool>(3)?,
                    r.get::<_, bool>(4)?,
                    r.get::<_, Option<String>>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if rows.len() > 10000 {
            return Err(Error::invalid(
                "Formula category extraction exceeds10000 references",
            ));
        }
        self.work = self
            .work
            .checked_sub(rows.len().saturating_mul(conditions.len().max(1)))
            .ok_or_else(|| Error::invalid("Formula category comparison work limit reached"))?;
        let mut sets = vec![];
        for condition in &conditions {
            let group = condition["field"] == "category_group";
            let pattern = if condition["op"] == "matches" {
                match crate::register_regex::Pattern::compile(
                    condition["value"]
                        .as_str()
                        .ok_or_else(|| Error::invalid("Category pattern must be text"))?,
                    true,
                ) {
                    Ok(pattern) => Some(pattern),
                    Err(crate::register_regex::Failure::Invalid) => None,
                    Err(error) => return Err(error.api_error()),
                }
            } else {
                None
            };
            let mut ids = BTreeSet::new();
            for (id, name, group_id, _, _, group_name) in &rows {
                let key = if group {
                    group_id.as_deref().unwrap_or("")
                } else {
                    id
                };
                let text = if group {
                    group_name.as_deref().unwrap_or(key)
                } else {
                    name
                };
                let value = &condition["value"];
                let matches = match condition["op"].as_str() {
                    Some("is") => value.as_str() == Some(key),
                    Some("isNot") => value.as_str() != Some(key),
                    Some("oneOf") => value
                        .as_array()
                        .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(key))),
                    Some("notOneOf") => value
                        .as_array()
                        .is_some_and(|a| !a.iter().any(|v| v.as_str() == Some(key))),
                    Some("contains") => text.to_lowercase().contains(
                        &value
                            .as_str()
                            .ok_or_else(|| Error::invalid("Category text filter requires text"))?
                            .to_lowercase(),
                    ),
                    Some("doesNotContain") => !text.to_lowercase().contains(
                        &value
                            .as_str()
                            .ok_or_else(|| Error::invalid("Category text filter requires text"))?
                            .to_lowercase(),
                    ),
                    Some("matches") => match &pattern {
                        Some(pattern) => pattern
                            .matches(text, self.matcher)
                            .map_err(|e| e.api_error())?,
                        None => false,
                    },
                    _ => false,
                };
                if matches {
                    ids.insert(id.clone());
                }
            }
            sets.push(ids);
        }
        if sets.is_empty() {
            return Ok(rows
                .into_iter()
                .filter(|r| !r.3 && !r.4)
                .map(|r| r.0)
                .collect());
        }
        if config["conditionsOp"] == "or" {
            // Source union retains first condition's category order, then new IDs from
            // subsequent conditions, not a global category sort.
            let mut result = vec![];
            let mut seen = BTreeSet::new();
            for set in sets {
                for row in &rows {
                    if set.contains(&row.0) && seen.insert(row.0.clone()) {
                        result.push(row.0.clone());
                    }
                }
            }
            Ok(result)
        } else {
            Ok(rows
                .into_iter()
                .filter(|r| sets.iter().all(|set| set.contains(&r.0)))
                .map(|r| r.0)
                .collect())
        }
    }
    fn budget(&mut self, request: &Value) -> Result<f64, Error> {
        let dimension = request["dimension"].as_str().unwrap_or("");
        if !["budgeted", "spent", "goal", "balance_start", "balance_end"].contains(&dimension) {
            return Err(Error::invalid("Invalid BUDGET_QUERY dimension"));
        }
        let start = request["startMonth"]
            .as_str()
            .ok_or_else(|| Error::invalid("Budget start month required"))?;
        let end = request["endMonth"]
            .as_str()
            .ok_or_else(|| Error::invalid("Budget end month required"))?;
        let first = budgeting::month(start)?;
        let last = budgeting::month(end)?;
        if last < first {
            return Ok(0.0);
        }
        if last - first > 239 {
            return Err(Error::invalid(
                "Formula budget queries permit at most240 months",
            ));
        }
        let ids = request["categoryIds"]
            .as_array()
            .ok_or_else(|| Error::invalid("Budget categories required"))?;
        if ids.len() > 1000 {
            return Err(Error::invalid("Maximum1000 formula budget categories"));
        }
        let ids = ids
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| Error::invalid("Invalid formula category identity"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let scope = ids.iter().cloned().collect::<BTreeSet<_>>();
        let cells = budgeting::formula_cells(
            self.db,
            self.budget,
            &budgeting::month_name(first - 1)?,
            (last - first + 2) as usize,
            &scope,
            &mut self.work,
        )?;
        let mut months = BTreeMap::new();
        for (month, data) in cells["months"].as_object().unwrap() {
            let rows = data["categories"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| (r["id"].as_str().unwrap().to_owned(), r.clone()))
                .collect::<BTreeMap<_, _>>();
            months.insert(month.clone(), rows);
        }
        let amount = |row: Option<&Value>, field: &str| -> Result<i128, Error> {
            let Some(row) = row else { return Ok(0) };
            let value = &row[field];
            if value.is_null() {
                return Ok(0);
            };
            let value = if field == "goal" && value.is_string() {
                serde_json::from_str::<Value>(value.as_str().unwrap())
                    .map_err(|_| Error::invalid("Invalid preserved formula goal"))?
            } else {
                value.clone()
            };
            value
                .as_i64()
                .map(i128::from)
                .ok_or_else(|| Error::invalid("Formula budget cell is not an integer"))
        };
        let mut sum = 0i128;
        let mut running = 0i128;
        let previous = &months[&budgeting::month_name(first - 1)?];
        for id in &ids {
            let row = previous.get(id);
            let value = amount(row, "balance")?;
            if value > 0 || value < 0 && row.is_some_and(|r| r["carryover"] == true) {
                running += value;
            }
        }
        let initial = running;
        let mut ending = running;
        for m in first..=last {
            let rows = &months[&budgeting::month_name(m)?];
            let mut allocated = 0i128;
            let mut spent = 0i128;
            let mut carry = 0i128;
            for id in &ids {
                let row = rows.get(id);
                allocated += amount(row, "budgeted")?;
                spent += amount(row, "actual")?;
                if dimension == "goal" {
                    sum += amount(row, "goal")?;
                }
                let balance = amount(row, "balance")?;
                if balance > 0 || balance < 0 && row.is_some_and(|r| r["carryover"] == true) {
                    carry += balance;
                }
            }
            if dimension == "budgeted" {
                sum += allocated;
            } else if dimension == "spent" {
                sum += spent;
            }
            ending = allocated + spent + running;
            running = carry;
        }
        let value = match dimension {
            "balance_start" => initial,
            "balance_end" => ending,
            _ => sum,
        };
        Ok(exact(value)? as f64 / 100.0)
    }
    pub fn prefetch(&mut self, requests: &Value) -> Result<Value, Error> {
        let mut output = json!({});
        for (requested, target, count) in [
            ("queryNames", "querySumPrefetch", false),
            ("queryCountNames", "queryCountPrefetch", true),
        ] {
            for name in names(requests, requested)? {
                let value = match self.queries.get(&name) {
                    None => 0.0,
                    Some(config) => {
                        let value = exact(self.transaction_total(config, count)? as i128)?;
                        if count {
                            value as f64
                        } else {
                            value as f64 / 100.0
                        }
                    }
                };
                self.prefetch_values = self.prefetch_values.saturating_add(1);
                if self.prefetch_values > 20000 {
                    return Err(Error::invalid(
                        "Formula prefetch exceeds20000 values; narrow selected categories/queries",
                    ));
                }
                if output[target].is_null() {
                    output[target] = json!([])
                }
                output[target]
                    .as_array_mut()
                    .unwrap()
                    .push(json!([name, value]));
            }
        }
        for (requested, target) in [
            (
                "queryExtractCategoryNames",
                "queryExtractCategoriesPrefetch",
            ),
            (
                "queryExtractTimeframeStartNames",
                "queryExtractTimeframeStartPrefetch",
            ),
            (
                "queryExtractTimeframeEndNames",
                "queryExtractTimeframeEndPrefetch",
            ),
        ] {
            for name in names(requests, requested)? {
                let config = self.queries.get(&name);
                let value = if requested == "queryExtractCategoryNames" {
                    match config {
                        Some(config) => json!(self.category_ids(config)?),
                        None => json!([]),
                    }
                } else {
                    let frame = config
                        .and_then(|c| c.get("timeFrame"))
                        .filter(|v| !v.is_null());
                    match frame {
                        Some(frame) => self.frame(frame)?
                            [usize::from(requested == "queryExtractTimeframeEndNames")]
                        .clone(),
                        None => json!(self.today.format("%Y-%m").to_string()),
                    }
                };
                self.prefetch_values = self
                    .prefetch_values
                    .saturating_add(value.as_array().map_or(1, Vec::len));
                if self.prefetch_values > 20000 {
                    return Err(Error::invalid(
                        "Formula prefetch exceeds20000 values; narrow selected categories/queries",
                    ));
                }
                if output[target].is_null() {
                    output[target] = json!([])
                }
                output[target]
                    .as_array_mut()
                    .unwrap()
                    .push(json!([name, value]));
            }
        }
        for name in names(requests, "balanceOfNames")? {
            let account:Option<String>=self.db.query_row("SELECT id FROM accounts WHERE budget_id=?1 AND tombstone=0 AND (id=?2 OR name=?2) ORDER BY id<>?2,sort_order,name,id LIMIT 1",params![self.budget,name],|r|r.get(0)).optional()?;
            let value = match account {
                Some(id) => {
                    let value:i64=self.db.query_row("SELECT coalesce(sum(amount),0) FROM transactions WHERE budget_id=?1 AND account_id=?2 AND tombstone=0 AND is_parent=0",params![self.budget,id],|r|r.get(0))?;
                    exact(value as i128)? as f64 / 100.0
                }
                None => 0.0,
            };
            if output["balanceOfPrefetch"].is_null() {
                output["balanceOfPrefetch"] = json!([])
            }
            output["balanceOfPrefetch"]
                .as_array_mut()
                .unwrap()
                .push(json!([name, value]));
        }
        let requests = requests["budgetQueryRequests"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if requests.len() > 32 {
            return Err(Error::invalid("Maximum32 formula budget requests"));
        }
        for entry in requests {
            let key = entry[0]
                .as_str()
                .filter(|s| s.len() <= 32768)
                .ok_or_else(|| Error::invalid("Invalid budget request"))?;
            let value = self.budget(&entry[1])?;
            if output["budgetQueryPrefetch"].is_null() {
                output["budgetQueryPrefetch"] = json!([])
            }
            output["budgetQueryPrefetch"]
                .as_array_mut()
                .unwrap()
                .push(json!([key, value]));
        }
        Ok(output)
    }
}
