//! Validated read-only automation queries. Data never becomes SQL identifiers.
mod resources;
use crate::{error::Error, store::Store};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State, rejection::JsonRejection},
    routing::{get, post},
};
use rusqlite::{
    Connection, params_from_iter,
    types::{Value as SqlValue, ValueRef},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
fn normalise(value: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    static DIACRITIC: std::sync::OnceLock<regress::Regex> = std::sync::OnceLock::new();
    let mapped = value
        .to_lowercase()
        .replace('ł', "l")
        .replace('ø', "o")
        .replace('ß', "ss")
        .replace('œ', "oe");
    let decomposed: String = mapped.nfd().collect();
    DIACRITIC
        .get_or_init(|| {
            regress::Regex::with_flags(r"\p{Diacritic}", "u").expect("constant Unicode property")
        })
        .replace_all(&decomposed, "")
}

fn sql_error(error: rusqlite::Error) -> Error {
    if matches!(&error, rusqlite::Error::SqliteFailure(_,Some(message)) if message == "integer overflow")
    {
        return invalid("Aggregate exceeds exact numeric range; narrow the query");
    }
    Error::internal(error)
}
fn invalid(message: impl Into<String>) -> Error {
    Error::new(
        axum::http::StatusCode::BAD_REQUEST,
        "invalid_query",
        message,
    )
}
fn limit() -> usize {
    100
}
fn yes() -> bool {
    true
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    version: u8,
    table: String,
    #[serde(default)]
    select: Vec<Value>,
    #[serde(default)]
    filter: Vec<Value>,
    #[serde(default)]
    group_by: Vec<Value>,
    #[serde(default)]
    order_by: Vec<Value>,
    #[serde(default)]
    calculation: bool,
    #[serde(default)]
    splits: Split,
    #[serde(default)]
    include_deleted: bool,
    #[serde(default = "yes")]
    validate_refs: bool,
    #[serde(default)]
    raw: bool,
    #[serde(default = "limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
    snapshot: Option<String>,
}
#[derive(Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Split {
    #[default]
    Inline,
    Grouped,
    All,
    None,
}
pub fn router() -> Router<Store> {
    Router::new()
        .route("/api/v1/query-capabilities", get(capabilities))
        .route(
            "/api/v1/budgets/{budget}/query",
            post(query).layer(DefaultBodyLimit::max(65536)),
        )
}
async fn capabilities() -> Json<Value> {
    Json(
        json!({"data":{"version":1,"tables":["transactions","accounts","account_groups","payees","categories","category_groups","rules","schedules","notes","budget_cells"],"resource_profiles":resources::catalog(),"splits":["inline","grouped","all","none"],"max_rows":100000,"max_page":1000,"max_depth":8,"max_nodes":256,"max_bound_values":500,"max_reference_depth":4,"max_emitted_rows":10000,"max_emitted_values":100000,"max_response_bytes":4194304,"max_work_steps":2000000,"max_text_work_bytes":67108864,"functions":["$sum","$count","$sumOver","$substr","$lower","$neg","$abs","$idiv","$id","$day","$month","$year","$condition","$nocase","$literal"],"operators":["$eq","$ne","$lt","$lte","$gt","$gte","$oneof","$like","$notlike","$regexp"],"boolean_operators":["$and","$or"],"pagination":"Supply the original snapshot for every continuation to refuse intervening edits. Without snapshot, each page is a new read."}}),
    )
}
async fn query(
    State(store): State<Store>,
    Path(budget): Path<String>,
    input: Result<Json<Query>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&budget)?;
    let Json(input) = input.map_err(crate::api::json_error)?;
    let result = store
        .run(move |db| {
            let tx = db.transaction()?;
            let result = execute(&tx, &budget, &input)?;
            tx.commit()?;
            Ok(result)
        })
        .await?;
    Ok(Json(result))
}
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Text,
    Id,
    Money,
    Integer,
    Boolean,
    Json,
    Date,
    Month,
    Year,
}
#[derive(Clone)]
struct Expr {
    sql: String,
    kind: Kind,
    aggregate: bool,
    window: bool,
}
impl Expr {
    fn new(sql: impl Into<String>, kind: Kind) -> Self {
        Self {
            sql: sql.into(),
            kind,
            aggregate: false,
            window: false,
        }
    }
}
struct Compiler<'a> {
    q: &'a Query,
    values: Vec<SqlValue>,
    orders: String,
    nodes: usize,
    order_aggregate: bool,
}
const JOINS: &str = "FROM transactions t JOIN accounts a ON a.budget_id=t.budget_id AND a.id=t.account_id LEFT JOIN payees p ON p.budget_id=t.budget_id AND p.id=t.payee_id AND p.tombstone=0 LEFT JOIN accounts y ON y.budget_id=p.budget_id AND y.id=p.transfer_account_id LEFT JOIN categories c ON c.budget_id=t.budget_id AND c.id=t.category_id AND c.tombstone=0 LEFT JOIN category_groups g ON g.budget_id=c.budget_id AND g.id=c.group_id AND g.tombstone=0";
fn quote(name: &str) -> Result<String, Error> {
    if name.is_empty()
        || name.len() > 128
        || name.starts_with("__query_")
        || name.chars().any(char::is_control)
    {
        return Err(invalid("Invalid select alias"));
    }
    Ok(format!("\"{}\"", name.replace('"', "\"\"")))
}
impl Compiler<'_> {
    fn charge(&mut self, depth: usize) -> Result<(), Error> {
        self.nodes += 1;
        if depth > 8 || self.nodes > 256 {
            return Err(invalid(
                "Query exceeds depth8 or256 expression nodes; simplify it",
            ));
        }
        Ok(())
    }
    fn bind(&mut self, value: &Value) -> Result<String, Error> {
        if self.values.len() > 500 {
            return Err(invalid("Query exceeds500 bound values"));
        }
        let value = match value {
            Value::Null => SqlValue::Null,
            Value::Bool(b) => SqlValue::Integer(i64::from(*b)),
            Value::Number(n) => SqlValue::Integer(
                n.as_i64()
                    .filter(|v| v.unsigned_abs() <= crate::models::MAX_MONEY as u64)
                    .ok_or_else(|| invalid("Query numbers must be exact safe integers"))?,
            ),
            Value::String(s) if s.len() <= 10000 => SqlValue::Text(s.clone()),
            _ => return Err(invalid("Expected bounded scalar query literal")),
        };
        self.values.push(value);
        Ok(format!("?{}", self.values.len()))
    }
    fn field(&self, name: &str) -> Result<Expr, Error> {
        if self.q.table == "notes" {
            return match name {
                "id" => Ok(Expr::new("t.id", Kind::Id)),
                "note" => Ok(Expr::new("t.note", Kind::Text)),
                _ => Err(invalid(format!("Unknown notes field: {name}"))),
            };
        }

        if self.q.table != "transactions" {
            return resources::field(&self.q.table, name, self.q.validate_refs);
        }
        let (sql, kind) = match name {
            "id" => ("t.id", Kind::Id),
            "account" => ("t.account_id", Kind::Id),
            "payee" => (
                if self.q.validate_refs {
                    "p.id"
                } else {
                    "t.payee_id"
                },
                Kind::Id,
            ),
            "category" => (
                if self.q.validate_refs {
                    "c.id"
                } else {
                    "t.category_id"
                },
                Kind::Id,
            ),
            "amount" => ("t.amount", Kind::Money),
            "date" => ("t.date", Kind::Date),
            "notes" => ("t.notes", Kind::Text),
            "is_parent" => ("t.is_parent", Kind::Boolean),
            "is_child" => ("(t.parent_id IS NOT NULL)", Kind::Boolean),
            "parent_id" => ("t.parent_id", Kind::Id),
            "transfer_id" => ("t.transfer_id", Kind::Id),
            "cleared" => ("t.cleared", Kind::Boolean),
            "reconciled" => ("t.reconciled", Kind::Boolean),
            "starting_balance_flag" => ("t.starting_balance", Kind::Boolean),
            "tombstone" => ("t.tombstone", Kind::Boolean),
            "error" => ("NULL", Kind::Json),
            "imported_payee" => ("t.imported_payee", Kind::Text),
            "imported_id" => (
                "(SELECT im.imported_id FROM transaction_import_metadata im WHERE im.budget_id=t.budget_id AND im.id=t.id AND im.tombstone=0)",
                Kind::Text,
            ),
            "raw_synced_data" => (
                "(SELECT im.raw_synced_data FROM transaction_import_metadata im WHERE im.budget_id=t.budget_id AND im.id=t.id AND im.tombstone=0)",
                Kind::Json,
            ),
            "schedule" => (
                "(SELECT sl.schedule_id FROM schedule_links sl JOIN schedules qs ON qs.budget_id=sl.budget_id AND qs.id=sl.schedule_id AND qs.tombstone=0 WHERE sl.budget_id=t.budget_id AND sl.id=t.id AND sl.tombstone=0)",
                Kind::Id,
            ),
            "account.name" => ("a.name", Kind::Text),
            "account.offbudget" => ("a.off_budget", Kind::Boolean),
            "account.closed" => ("a.closed", Kind::Boolean),
            "payee.name" => ("coalesce(y.name,p.name)", Kind::Text),
            "category.name" => ("c.name", Kind::Text),
            "category.is_income" => ("c.is_income", Kind::Boolean),
            "category.hidden" => ("c.hidden", Kind::Boolean),
            "category.group" => ("c.group_id", Kind::Id),
            "category.group.name" => ("g.name", Kind::Text),
            "category.group.hidden" => ("g.hidden", Kind::Boolean),
            "category.group.is_income" => ("g.is_income", Kind::Boolean),
            _ => {
                if let Some((head, path)) = name.split_once('.') {
                    let resource = match head {
                        "account" => Some(("accounts", "a")),
                        "payee" => Some(("payees", "p")),
                        "category" => Some(("categories", "c")),
                        _ => None,
                    };
                    if head == "schedule" {
                        let expression =
                            resources::resolve("schedules", "qs", path, self.q.validate_refs, 0)?;
                        return Ok(Expr::new(
                            format!(
                                "(SELECT {} FROM schedule_links sl JOIN schedules qs ON qs.budget_id=sl.budget_id AND qs.id=sl.schedule_id AND qs.tombstone=0 WHERE sl.budget_id=t.budget_id AND sl.id=t.id AND sl.tombstone=0)",
                                expression.sql
                            ),
                            expression.kind,
                        ));
                    }
                    if let Some((table, alias)) = resource {
                        return resources::resolve(table, alias, path, self.q.validate_refs, 0);
                    }
                }
                return Err(invalid(format!("Unknown transactions field: {name}")));
            }
        };
        Ok(Expr::new(sql, kind))
    }
    fn expr(&mut self, v: &Value, depth: usize) -> Result<Expr, Error> {
        self.charge(depth)?;
        if let Some(s) = v.as_str().and_then(|s| s.strip_prefix('$')) {
            return self.field(s);
        }
        if let Some(o) = v.as_object() {
            if o.len() != 1 {
                return Err(invalid("An expression requires exactly one function"));
            }
            let (name, args) = o.iter().next().unwrap();
            if name == "$condition" {
                return Ok(Expr::new(self.predicate(args, depth + 1)?, Kind::Boolean));
            }
            if name == "$literal" || name == "literal" {
                let kind = if args.is_boolean() {
                    Kind::Boolean
                } else if args.is_number() {
                    Kind::Integer
                } else {
                    Kind::Text
                };
                return Ok(Expr::new(self.bind(args)?, kind));
            }
            let args = if let Some(a) = args.as_array() {
                a.clone()
            } else {
                vec![args.clone()]
            };
            if name == "$count" && args == vec![json!("*")] {
                return Ok(Expr {
                    sql: "COUNT(*)".into(),
                    kind: Kind::Integer,
                    aggregate: true,
                    window: false,
                });
            }
            let values = args
                .iter()
                .map(|a| self.expr(a, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            let n = values.len();
            let required = match name.as_str() {
                "$substr" => 2..=3,
                "$idiv" => 2..=2,
                _ => 1..=1,
            };
            if !required.contains(&n) {
                return Err(invalid(format!("Wrong argument count for {name}")));
            }
            if matches!(name.as_str(), "$sum" | "$sumOver" | "$count")
                && values.iter().any(|e| e.aggregate || e.window)
            {
                return Err(invalid("Nested aggregate expressions are not supported"));
            }
            let e = &values[0];
            if matches!(name.as_str(), "$day" | "$month" | "$year") && values[0].kind == Kind::Text
            {
                let literal = args[0]
                    .as_str()
                    .filter(|s| !s.starts_with('$'))
                    .or_else(|| args[0].get("$literal").and_then(Value::as_str))
                    .ok_or_else(|| {
                        invalid("Calendar conversion requires a date field or literal date")
                    })?;
                let lengths: &[usize] = match name.as_str() {
                    "$day" => &[10],
                    "$month" => &[7, 10],
                    _ => &[4, 7, 10],
                };
                if !lengths.contains(&literal.len())
                    || literal.bytes().enumerate().any(|(i, c)| {
                        if i == 4 || i == 7 {
                            c != b'-'
                        } else {
                            !c.is_ascii_digit()
                        }
                    })
                {
                    return Err(invalid("Calendar literal has incorrect date format"));
                }
            }

            let mut result = Expr::new("", e.kind);
            result.aggregate = values.iter().any(|e| e.aggregate);
            result.window = values.iter().any(|e| e.window);
            result.sql = match name.as_str() {
                "$sum" | "$sumOver" => {
                    if !matches!(e.kind, Kind::Money | Kind::Integer) {
                        return Err(invalid("Sum requires exact numbers"));
                    }
                    result.aggregate = name == "$sum";
                    result.window = name == "$sumOver";
                    let sum = format!("SUM({})", e.sql);
                    if result.aggregate {
                        format!("coalesce({sum},0)")
                    } else {
                        format!(
                            "{sum} OVER (ORDER BY {} ROWS BETWEEN CURRENT ROW AND UNBOUNDED FOLLOWING)",
                            self.orders
                        )
                    }
                }
                "$count" => {
                    result.kind = Kind::Integer;
                    result.aggregate = true;
                    format!("COUNT({})", e.sql)
                }
                "$abs" | "$neg" | "$idiv" => {
                    if values
                        .iter()
                        .any(|v| !matches!(v.kind, Kind::Money | Kind::Integer))
                    {
                        return Err(invalid("Arithmetic requires exact numbers"));
                    }
                    match name.as_str() {
                        "$abs" => format!("abs({})", e.sql),
                        "$neg" => format!("(-{})", e.sql),
                        _ => format!("({}/{})", e.sql, values[1].sql),
                    }
                }
                "$lower" => {
                    if e.kind != Kind::Text {
                        return Err(invalid("Lower requires text"));
                    }
                    result.kind = Kind::Text;
                    format!("query_lower({})", e.sql)
                }
                "$substr" => {
                    if e.kind != Kind::Text || values[1..].iter().any(|e| e.kind != Kind::Integer) {
                        return Err(invalid("Substring requires text and integer positions"));
                    }
                    result.kind = Kind::Text;
                    format!(
                        "substr({})",
                        values
                            .iter()
                            .map(|e| e.sql.as_str())
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                }
                "$year" | "$month" => {
                    if !matches!(e.kind, Kind::Date | Kind::Month | Kind::Text)
                        && !(name == "$year" && e.kind == Kind::Year)
                    {
                        return Err(invalid("Calendar functions require dates"));
                    }
                    result.kind = if name == "$year" {
                        Kind::Year
                    } else {
                        Kind::Month
                    };
                    format!(
                        "substr({},1,{})",
                        e.sql,
                        if name == "$year" { 4 } else { 7 }
                    )
                }
                "$day" => {
                    if !matches!(e.kind, Kind::Date | Kind::Text) {
                        return Err(invalid("Day requires a calendar date"));
                    }
                    result.kind = Kind::Date;
                    e.sql.clone()
                }
                "$id" => e.sql.clone(),
                "$nocase" => {
                    if e.kind != Kind::Text {
                        return Err(invalid("Nocase requires text"));
                    }
                    format!("({} COLLATE NOCASE)", e.sql)
                }
                _ => return Err(invalid(format!("Unknown query function: {name}"))),
            };
            return Ok(result);
        }
        let kind = if v.is_number() {
            Kind::Integer
        } else if v.is_boolean() {
            Kind::Boolean
        } else {
            Kind::Text
        };
        Ok(Expr::new(self.bind(v)?, kind))
    }
    fn predicate(&mut self, value: &Value, depth: usize) -> Result<String, Error> {
        self.charge(depth)?;
        let o = value
            .as_object()
            .ok_or_else(|| invalid("Filter must be an object"))?;
        let mut terms = vec![];
        for (field, value) in o {
            if field == "$and" || field == "$or" {
                let a = value
                    .as_array()
                    .ok_or_else(|| invalid("Filter junction requires an array"))?;
                let parts = a
                    .iter()
                    .map(|v| self.predicate(v, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                terms.push(if parts.is_empty() {
                    if field == "$and" { "1" } else { "0" }.into()
                } else {
                    format!(
                        "({})",
                        parts.join(if field == "$and" { " AND " } else { " OR " })
                    )
                });
                continue;
            }
            if value.is_array() {
                let parts = value
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| self.comparison(field, v, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                terms.push(if parts.is_empty() {
                    "1".into()
                } else {
                    format!("({})", parts.join(" AND "))
                });
            } else {
                terms.push(self.comparison(field, value, depth + 1)?)
            }
        }
        Ok(if terms.is_empty() {
            "1".into()
        } else {
            format!("({})", terms.join(" AND "))
        })
    }
    fn comparison(&mut self, field: &str, value: &Value, depth: usize) -> Result<String, Error> {
        self.charge(depth)?;
        let mut left = self.field(field)?;
        let (op, right) = if let Some(o) = value.as_object() {
            if let Some(transform) = o.get("$transform") {
                let expr = if let Some(name) = transform.as_str() {
                    json!({name:format!("${field}")})
                } else {
                    return Err(invalid("Transform must name a function"));
                };
                left = self.expr(&expr, depth + 1)?;
                if left.aggregate || left.window {
                    return Err(invalid(
                        "Filter transforms require row-level scalar functions",
                    ));
                }
            }
            let ops = o
                .iter()
                .filter(|(k, _)| k.as_str() != "$transform")
                .collect::<Vec<_>>();
            if ops.len() != 1 {
                return Err(invalid("Each field predicate needs exactly one operator"));
            }
            (ops[0].0.as_str(), ops[0].1)
        } else {
            ("$eq", value)
        };
        if left.kind == Kind::Json && !right.is_null() {
            return Err(invalid(
                "Structured JSON supports null checks only; use typed feature routes for structured predicates",
            ));
        }
        if right.is_null() && left.kind == Kind::Boolean {
            return match op {
                "$eq" => Ok(format!("{}=0", left.sql)),
                "$ne" => Ok(format!("({}!=0 OR {} IS NULL)", left.sql, left.sql)),
                _ => Err(invalid("Null supports equality/inequality only")),
            };
        }
        if right.is_null() {
            return match op {
                "$eq" => Ok(format!("{} IS NULL", left.sql)),
                "$ne" => Ok(format!("{} IS NOT NULL", left.sql)),
                _ => Err(invalid("Null supports equality/inequality only")),
            };
        }
        if op == "$oneof" {
            let a = right
                .as_array()
                .ok_or_else(|| invalid("Membership needs an array"))?;
            let values = a
                .iter()
                .map(|v| {
                    let text = match v {
                        Value::String(s) => s.clone(),
                        Value::Null => "null".into(),
                        Value::Bool(b) => b.to_string(),
                        Value::Number(n) => n.to_string(),
                        _ => return Err(invalid("Membership requires scalar values")),
                    };
                    self.bind(&json!(text))
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(if values.is_empty() {
                "0".into()
            } else {
                format!(
                    "{} IN({})",
                    if left.kind == Kind::Date {
                        format!("replace({},'-','')", left.sql)
                    } else {
                        left.sql.clone()
                    },
                    values.join(",")
                )
            });
        }
        if matches!(op, "$like" | "$notlike" | "$regexp") {
            if left.kind != Kind::Text {
                return Err(invalid("Pattern predicates require text"));
            }
            let pattern = right
                .as_str()
                .ok_or_else(|| invalid("Pattern must be text"))?;
            if op != "$regexp" && pattern.is_empty() {
                return Ok(if op == "$notlike" { "1" } else { "0" }.into());
            }
            let pattern = if op == "$regexp" {
                pattern.to_owned()
            } else {
                crate::register::like_regex(&normalise(pattern))
            };
            let parameter = self.bind(&json!(pattern))?;
            return Ok(format!(
                "{}{}({parameter},coalesce({},'')){}",
                if op == "$notlike" { "(NOT " } else { "" },
                if op == "$regexp" {
                    "register_regex"
                } else {
                    "register_like"
                },
                if op == "$regexp" {
                    left.sql.clone()
                } else {
                    format!("query_normalise({})", left.sql)
                },
                if op == "$notlike" {
                    format!(" OR {} IS NULL)", left.sql)
                } else {
                    String::new()
                }
            ));
        }

        let operator = match op {
            "$eq" => "=",
            "$ne" => "!=",
            "$lt" => "<",
            "$lte" => "<=",
            "$gt" => ">",
            "$gte" => ">=",
            _ => return Err(invalid(format!("Unknown query operator: {op}"))),
        };
        match left.kind {
            Kind::Money | Kind::Integer if right.as_i64().is_none() => {
                return Err(invalid("Numeric predicates require exact integers"));
            }
            Kind::Boolean if !right.is_boolean() => {
                return Err(invalid("Boolean predicates require boolean literals"));
            }
            Kind::Text | Kind::Id if !right.is_string() => {
                return Err(invalid("Text/ID predicates require text literals"));
            }
            Kind::Json => {
                return Err(invalid(
                    "Structured JSON supports null checks only; query its typed feature route for structured predicates",
                ));
            }
            Kind::Date | Kind::Month | Kind::Year => {
                let text = right
                    .as_str()
                    .ok_or_else(|| invalid("Calendar predicates require text dates"))?;
                let len = match left.kind {
                    Kind::Date => 10,
                    Kind::Month => 7,
                    _ => 4,
                };
                if text.len() != len
                    || text.bytes().enumerate().any(|(i, c)| {
                        if i == 4 || i == 7 {
                            c != b'-'
                        } else {
                            !c.is_ascii_digit()
                        }
                    })
                {
                    return Err(invalid("Calendar literal has incorrect date granularity"));
                }
            }
            _ => {}
        }
        let parameter = self.bind(right)?;
        Ok(if op == "$ne" {
            format!("({} != {parameter} OR {} IS NULL)", left.sql, left.sql)
        } else {
            format!("{} {operator} {parameter}", left.sql)
        })
    }
    fn order(&mut self) -> Result<String, Error> {
        let mut terms = vec![];
        for v in &self.q.order_by {
            let (expr, direction) = if let Some(o) = v.as_object() {
                if o.len() == 1
                    && o.values()
                        .next()
                        .is_some_and(|v| matches!(v.as_str(), Some("asc" | "desc")))
                {
                    let (k, v) = o.iter().next().unwrap();
                    (self.field(k)?, v.as_str().unwrap())
                } else {
                    (self.expr(v, 1)?, "asc")
                }
            } else {
                (
                    self.field(
                        v.as_str()
                            .ok_or_else(|| invalid("Order requires field/function"))?,
                    )?,
                    "asc",
                )
            };
            if expr.window || expr.kind == Kind::Json {
                return Err(invalid(
                    "Order expressions cannot use windows or structured JSON",
                ));
            }
            self.order_aggregate |= expr.aggregate;
            terms.push(format!("{} {direction}", expr.sql));
        }
        if terms.is_empty() && self.q.table != "transactions" {
            return Ok("t.id ASC".into());
        }
        if terms.is_empty() {
            terms = vec![
                "t.date DESC".into(),
                format!("{} DESC", crate::android_upgrade::order::SQL),
            ]
        }
        terms.push("t.id ASC".into());
        Ok(terms.join(","))
    }
}
fn value(row: &rusqlite::Row<'_>, index: usize, kind: Kind, raw: bool) -> Result<Value, Error> {
    Ok(match row.get_ref(index)? {
        ValueRef::Null => {
            if kind == Kind::Boolean && !raw {
                json!(false)
            } else {
                Value::Null
            }
        }
        ValueRef::Integer(n) => {
            if n.unsigned_abs() > crate::models::MAX_MONEY as u64 {
                return Err(invalid("Aggregate exceeds exact safe numeric range"));
            }
            if kind == Kind::Boolean && !raw {
                json!(n != 0)
            } else {
                json!(n)
            }
        }
        ValueRef::Real(_) => return Err(invalid("Result is not an exact integer")),
        ValueRef::Text(t) => {
            let s = std::str::from_utf8(t).map_err(Error::internal)?;
            if kind == Kind::Json && !raw {
                serde_json::from_str(s).map_err(Error::internal)?
            } else {
                json!(s)
            }
        }
        ValueRef::Blob(_) => return Err(invalid("Binary fields are not queryable")),
    })
}
/// One cumulative allowance for roots and all expanded children. Charge JSON
/// bytes while serializing into a counter, before retaining each record.
#[derive(Default)]
struct OutputBudget {
    rows: usize,
    values: usize,
    bytes: usize,
}
impl std::io::Write for OutputBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes = self.bytes.saturating_add(bytes.len());
        if self.bytes > 4 * 1024 * 1024 - 1024 {
            return Err(std::io::Error::other("Query response output exceeds4MiB"));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl OutputBudget {
    fn record(&mut self, record: &serde_json::Map<String, Value>) -> Result<(), Error> {
        self.rows += 1;
        // Reserve commas and the optional subtransactions key/array framing.
        self.bytes += 32;
        self.values += record.len();
        if self.rows > 10000 || self.values > 100000 {
            return Err(invalid(
                "Query output exceeds10000 emitted rows or100000 values; narrow fields/page",
            ));
        }
        serde_json::to_writer(self, record)
            .map_err(|_| invalid("Query response output exceeds4MiB; narrow fields/page"))
    }
}

fn run(
    db: &Connection,
    b: &str,
    q: &Query,
    regex: &crate::register_regex::Budget,
) -> Result<Value, Error> {
    crate::store::require_budget(db, b)?;
    crate::concurrency::check_snapshot(db, b, q.snapshot.as_deref())?;
    if q.version != 1 || !resources::supported(&q.table) {
        return Err(invalid("Unsupported query version/table"));
    }
    if q.limit == 0
        || q.limit > 1000
        || q.offset > 100000
        || q.select.len() > 32
        || q.group_by.len() > 8
        || q.order_by.len() > 8
    {
        return Err(invalid(
            "Narrow query fields/groups/order or page1..1000/offset0..100000",
        ));
    }
    let joins = if q.table == "notes" {
        crate::notes::QUERY_FROM.to_owned()
    } else if q.table == "transactions" {
        JOINS.to_owned()
    } else {
        resources::from(&q.table)?
    };
    let mut c = Compiler {
        q,
        values: vec![SqlValue::Text(b.into())],
        orders: String::new(),
        nodes: 0,
        order_aggregate: false,
    };
    c.orders = c.order()?;
    if q.calculation && q.select.len() != 1 {
        return Err(invalid("Calculation requires exactly one named expression"));
    }
    let mut projected = vec![];
    let mut aliases = BTreeSet::new();
    let selection = if q.select.is_empty() {
        vec![json!("id")]
    } else {
        q.select.clone()
    };
    let selection = selection
        .into_iter()
        .flat_map(|v| {
            if v.as_str() == Some("*") {
                resources::names(&q.table)
                    .into_iter()
                    .map(|s| json!(s))
                    .collect::<Vec<_>>()
            } else {
                vec![v]
            }
        })
        .collect::<Vec<_>>();
    if selection.len() > 32 {
        return Err(invalid("Expanded selection exceeds32 fields"));
    }
    for v in selection {
        let (name, expr) = if let Some(field) = v.as_str() {
            (field.to_owned(), c.field(field)?)
        } else {
            let o = v
                .as_object()
                .filter(|o| o.len() == 1)
                .ok_or_else(|| invalid("Each select alias requires one expression"))?;
            let (name, expr) = o.iter().next().unwrap();
            (name.clone(), c.expr(expr, 1)?)
        };
        let alias = quote(&name)?;
        if !aliases.insert(name.clone()) {
            return Err(invalid("Duplicate select alias"));
        }
        projected.push((name, alias, expr));
    }
    let aggregate = projected.iter().any(|(_, _, e)| e.aggregate) || !q.group_by.is_empty();
    if c.order_aggregate && !aggregate {
        return Err(invalid(
            "Aggregate ordering requires an aggregate/grouped query",
        ));
    }
    let filters = q
        .filter
        .iter()
        .map(|v| c.predicate(v, 1))
        .collect::<Result<Vec<_>, _>>()?;
    let filter = if filters.is_empty() {
        "1".into()
    } else {
        filters.join(" AND ")
    };
    let live = if q.include_deleted && q.table != "notes" {
        "1"
    } else {
        "t.tombstone=0"
    };
    let group = q.table == "transactions" && q.splits == Split::Grouped && !aggregate;
    let base = format!("t.budget_id=?1 AND {live}");
    let matched = format!("SELECT t.id,t.parent_id {joins} WHERE {base} AND ({filter})");
    let where_sql = if group {
        format!(
            "{base} AND t.parent_id IS NULL AND (t.id IN (SELECT id FROM matched) OR t.id IN (SELECT parent_id FROM matched))"
        )
    } else {
        format!(
            "{base} AND ({filter}) AND {}",
            if q.table != "transactions" {
                "1"
            } else {
                match q.splits {
                    Split::Inline | Split::Grouped => "t.is_parent=0",
                    Split::None => "t.parent_id IS NULL",
                    Split::All => "1",
                }
            }
        )
    };
    let groups = q
        .group_by
        .iter()
        .map(|v| {
            if let Some(s) = v.as_str() {
                c.field(s)
            } else {
                c.expr(v, 1)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    if groups
        .iter()
        .any(|e| e.aggregate || e.window || e.kind == Kind::Json)
    {
        return Err(invalid(
            "Group expressions require row-level scalar functions",
        ));
    }
    let group_sql = if groups.is_empty() {
        String::new()
    } else {
        format!(
            " GROUP BY {}",
            groups
                .iter()
                .map(|e| e.sql.as_str())
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let cte = if group {
        format!("WITH matched AS({matched}) ")
    } else {
        String::new()
    };
    let bound_count = c.values.len();
    let count_sql = format!(
        "{cte}SELECT COUNT(*) FROM(SELECT t.id {joins} WHERE {where_sql} AND (?{bound_count} IS ?{bound_count}){group_sql} LIMIT 100001)"
    );
    let count: i64 = db
        .query_row(&count_sql, params_from_iter(c.values.iter()), |r| r.get(0))
        .map_err(|e| regex.error().unwrap_or_else(|| Error::internal(e)))?;
    if count > 100000 {
        return Err(invalid(
            "Query matches more than100000 rows; narrow account/date filters",
        ));
    }
    let select = projected
        .iter()
        .map(|(_, alias, e)| format!("{} AS {alias}", e.sql))
        .collect::<Vec<_>>()
        .join(",");
    let match_column = if group {
        ",EXISTS(SELECT 1 FROM matched m WHERE m.id=t.id) AS __query_matched"
    } else {
        ""
    };
    let sql = format!(
        "{cte}SELECT {select},t.id AS __query_id{match_column} {joins} WHERE {where_sql}{group_sql} ORDER BY {} LIMIT {} OFFSET {}",
        c.orders, q.limit, q.offset
    );
    let mut statement = db.prepare(&sql)?;
    let mut rows = statement
        .query(params_from_iter(c.values.iter()))
        .map_err(sql_error)?;
    let mut data = vec![];
    let mut output = OutputBudget::default();
    while let Some(row) = rows.next().map_err(sql_error)? {
        let mut record = serde_json::Map::new();
        for (index, (name, _, expr)) in projected.iter().enumerate() {
            record.insert(name.clone(), value(row, index, expr.kind, q.raw)?);
        }
        if group {
            if !row.get::<_, bool>(projected.len() + 1)? {
                record.insert("_unmatched".into(), json!(true));
            }
            let id: String = row.get(projected.len())?;
            let child_sql = format!(
                "{cte}SELECT {select},EXISTS(SELECT 1 FROM matched m WHERE m.id=t.id) AS __query_matched {joins} WHERE t.budget_id=?1 AND t.parent_id=?{} AND {live} ORDER BY {} LIMIT 100001",
                c.values.len() + 1,
                c.orders
            );
            let mut values = c.values.clone();
            values.push(SqlValue::Text(id));
            let mut stmt = db.prepare(&child_sql)?;
            let mut children = stmt.query(params_from_iter(values)).map_err(sql_error)?;
            let mut nested = vec![];
            while let Some(row) = children.next().map_err(sql_error)? {
                if nested.len() >= 100000 {
                    return Err(invalid("Grouped split exceeds100000 children"));
                }
                let mut item = serde_json::Map::new();
                for (i, (name, _, expr)) in projected.iter().enumerate() {
                    item.insert(name.clone(), value(row, i, expr.kind, q.raw)?);
                }
                if !row.get::<_, bool>(projected.len())? {
                    item.insert("_unmatched".into(), json!(true));
                }
                output.record(&item)?;
                nested.push(Value::Object(item));
            }
            output.record(&record)?;
            record.insert("subtransactions".into(), json!(nested));
        }
        if !group {
            output.record(&record)?;
        }
        data.push(Value::Object(record));
    }
    if let Some(error) = regex.error() {
        return Err(error);
    }
    let result_count = if aggregate && groups.is_empty() {
        1
    } else {
        count
    };
    let data = if q.calculation {
        data.first()
            .and_then(Value::as_object)
            .and_then(|o| o.values().next())
            .cloned()
            .unwrap_or(json!(0))
    } else {
        json!(data)
    };
    Ok(
        json!({"data":data,"matched_count":result_count,"next_offset":if !q.calculation&&q.offset+q.limit<result_count as usize{Some(q.offset+q.limit)}else{None},"snapshot":crate::concurrency::cursor(db,b)?,"epoch":crate::lifecycle::epoch(db,b)?}),
    )
}
pub fn execute(db: &Connection, b: &str, q: &Query) -> Result<Value, Error> {
    use rusqlite::functions::FunctionFlags;
    const TEXT_WORK: usize = 64 * 1024 * 1024;
    let text_work = Arc::new(AtomicUsize::new(0));
    for (name, normalised) in [("query_lower", false), ("query_normalise", true)] {
        let work = text_work.clone();
        db.create_scalar_function(
            name,
            1,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            move |ctx| {
                let input = ctx.get::<Option<String>>(0)?;
                let Some(input) = input else {
                    return Ok(None::<String>);
                };
                let failed = || {
                    rusqlite::Error::UserFunctionError(Box::new(std::io::Error::other(
                        "Query text work limit",
                    )))
                };
                if work
                    .fetch_add(input.len(), Ordering::Relaxed)
                    .saturating_add(input.len())
                    > TEXT_WORK
                {
                    return Err(failed());
                }
                let value = if normalised {
                    normalise(&input)
                } else {
                    input.to_lowercase()
                };
                if work
                    .fetch_add(value.len(), Ordering::Relaxed)
                    .saturating_add(value.len())
                    > TEXT_WORK
                {
                    return Err(failed());
                }
                Ok(Some(value))
            },
        )?;
    }
    let regex = crate::register_regex::install(db)?;
    let steps = Arc::new(AtomicUsize::new(0));
    let progress = steps.clone();
    db.progress_handler(
        1000,
        Some(move || progress.fetch_add(1000, Ordering::Relaxed) >= 2_000_000),
    )?;
    let result = run(db, b, q, &regex);
    db.progress_handler(0, None::<fn() -> bool>)?;
    if text_work.load(Ordering::Relaxed) > TEXT_WORK {
        return Err(invalid(
            "Query text work limit reached; narrow account/date/fields",
        ));
    }
    if let Some(error) = regex.error() {
        return Err(error);
    }
    if steps.load(Ordering::Relaxed) >= 2_000_000 {
        return Err(invalid(
            "Query work limit reached; narrow fields/account/date",
        ));
    }
    result
}
