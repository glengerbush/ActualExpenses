//! Closed resource profiles. SQL identifiers and reference paths come only from
//! this registry; caller strings never become table/column identifiers.
use super::{Error, Expr, Kind, invalid};
use serde_json::{Value, json};
const TABLES: &[&str] = &[
    "transactions",
    "accounts",
    "account_groups",
    "payees",
    "categories",
    "category_groups",
    "rules",
    "schedules",
    "notes",
    "budget_cells",
];
struct Field {
    name: &'static str,
    sql: &'static str,
    kind: Kind,
    reference: Option<&'static str>,
}
macro_rules! f {
    ($n:literal,$s:literal,$k:ident) => {
        Field {
            name: $n,
            sql: $s,
            kind: Kind::$k,
            reference: None,
        }
    };
    ($n:literal,$s:literal,$r:literal) => {
        Field {
            name: $n,
            sql: $s,
            kind: Kind::Id,
            reference: Some($r),
        }
    };
}
fn fields(table: &str) -> &'static [Field] {
    match table {
        "accounts" => &[
            f!("id", "{a}.id", Id),
            f!("name", "{a}.name", Text),
            f!("offbudget", "{a}.off_budget", Boolean),
            f!("closed", "{a}.closed", Boolean),
            f!("account_group_id", "{a}.group_id", "account_groups"),
        ],
        "account_groups" => &[f!("id", "{a}.id", Id), f!("name", "{a}.name", Text)],
        "payees" => &[
            f!("id", "{a}.id", Id),
            f!(
                "name",
                "coalesce((SELECT pa.name FROM accounts pa WHERE pa.budget_id={a}.budget_id AND pa.id={a}.transfer_account_id AND pa.tombstone=0),{a}.name)",
                Text
            ),
            f!("transfer_acct", "{a}.transfer_account_id", "accounts"),
            f!("favorite", "{a}.favorite", Boolean),
            f!("learn_categories", "{a}.learn_categories", Boolean),
        ],
        "categories" => &[
            f!("id", "{a}.id", Id),
            f!("name", "{a}.name", Text),
            f!("is_income", "{a}.is_income", Boolean),
            f!("hidden", "{a}.hidden", Boolean),
            f!("group", "{a}.group_id", "category_groups"),
            f!(
                "goal_def",
                "(SELECT qa.goal_def FROM budget_automations qa WHERE qa.budget_id={a}.budget_id AND qa.id={a}.id AND qa.tombstone=0)",
                Json
            ),
            f!(
                "cleanup_def",
                "(SELECT qa.cleanup_def FROM budget_automations qa WHERE qa.budget_id={a}.budget_id AND qa.id={a}.id AND qa.tombstone=0)",
                Json
            ),
            f!(
                "template_settings",
                "(SELECT qa.template_settings FROM budget_automations qa WHERE qa.budget_id={a}.budget_id AND qa.id={a}.id AND qa.tombstone=0)",
                Json
            ),
        ],
        "category_groups" => &[
            f!("id", "{a}.id", Id),
            f!("name", "{a}.name", Text),
            f!("is_income", "{a}.is_income", Boolean),
            f!("hidden", "{a}.hidden", Boolean),
        ],
        "rules" => &[
            f!("id", "{a}.id", Id),
            f!("stage", "{a}.stage", Text),
            f!("conditions_op", "{a}.conditions_op", Text),
            f!("conditions", "{a}.conditions", Json),
            f!("actions", "{a}.actions", Json),
        ],
        "schedules" => &[
            f!("id", "{a}.id", Id),
            f!("name", "{a}.name", Text),
            f!("rule", "{a}.rule_id", "rules"),
            f!("next_date", "{a}.next_date", Date),
            f!("posts_transaction", "{a}.posts_transaction", Boolean),
            f!("completed", "{a}.completed", Boolean),
            f!("custom_upcoming_length", "{a}.custom_upcoming_length", Text),
        ],
        "budget_cells" => &[
            f!("id", "{a}.id", Id),
            f!("method", "{a}.method", Text),
            f!("month", "{a}.month", Month),
            f!("category", "{a}.category_id", "categories"),
            f!("amount", "{a}.amount", Money),
            f!("carryover", "{a}.carryover", Boolean),
            f!("goal", "{a}.goal", Json),
            f!("long_goal", "{a}.long_goal", Json),
        ],
        _ => &[],
    }
}
pub(super) fn supported(table: &str) -> bool {
    TABLES.contains(&table)
}
pub(super) fn from(table: &str) -> Result<String, Error> {
    if supported(table) && !matches!(table, "transactions" | "notes") {
        Ok(format!("FROM {table} t"))
    } else {
        Err(invalid("Unsupported resource profile"))
    }
}
pub(super) fn field(table: &str, path: &str, validate_refs: bool) -> Result<Expr, Error> {
    resolve(table, "t", path, validate_refs, 0)
}
pub(super) fn resolve(
    table: &str,
    alias: &str,
    path: &str,
    validate_refs: bool,
    depth: usize,
) -> Result<Expr, Error> {
    if depth > 4 {
        return Err(invalid("Reference paths exceed4 joins"));
    }
    let (name, rest) = path
        .split_once('.')
        .map_or((path, None), |(a, b)| (a, Some(b)));
    let field = fields(table)
        .iter()
        .find(|f| f.name == name)
        .ok_or_else(|| invalid(format!("Unknown {table} field: {name}")))?;
    let sql = field.sql.replace("{a}", alias);
    if let Some(rest) = rest {
        let target = field
            .reference
            .ok_or_else(|| invalid("Only advertised references support joined fields"))?;
        let next = format!("qr{depth}");
        let expression = resolve(target, &next, rest, validate_refs, depth + 1)?;
        Ok(Expr::new(
            format!(
                "(SELECT {} FROM {target} {next} WHERE {next}.budget_id={alias}.budget_id AND {next}.id={sql}{})",
                expression.sql,
                if validate_refs {
                    format!(" AND {next}.tombstone=0")
                } else {
                    String::new()
                }
            ),
            expression.kind,
        ))
    } else if validate_refs && let Some(target) = field.reference {
        Ok(Expr::new(
            format!(
                "(SELECT qref.id FROM {target} qref WHERE qref.budget_id={alias}.budget_id AND qref.id={sql} AND qref.tombstone=0)"
            ),
            field.kind,
        ))
    } else {
        Ok(Expr::new(sql, field.kind))
    }
}
pub(super) fn type_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Text => "text",
        Kind::Id => "id",
        Kind::Money => "money",
        Kind::Integer => "integer",
        Kind::Boolean => "boolean",
        Kind::Json => "json",
        Kind::Date => "date",
        Kind::Month => "month",
        Kind::Year => "year",
    }
}
pub(super) fn names(table: &str) -> Vec<&'static str> {
    if table == "notes" {
        return vec!["id", "note"];
    }
    if table == "transactions" {
        return vec![
            "id",
            "account",
            "payee",
            "category",
            "amount",
            "date",
            "notes",
            "is_parent",
            "is_child",
            "parent_id",
            "transfer_id",
            "cleared",
            "reconciled",
            "starting_balance_flag",
            "tombstone",
            "error",
            "imported_payee",
            "imported_id",
            "raw_synced_data",
            "schedule",
        ];
    }
    fields(table).iter().map(|f| f.name).collect()
}
fn descriptor(table: &str, name: &str) -> Value {
    let kind = match name {
        "amount" => Kind::Money,
        "date" => Kind::Date,
        "cleared"
        | "reconciled"
        | "is_parent"
        | "is_child"
        | "starting_balance_flag"
        | "tombstone" => Kind::Boolean,
        "error" | "raw_synced_data" => Kind::Json,
        "notes" | "note" | "imported_payee" | "imported_id" => Kind::Text,
        _ => Kind::Id,
    };
    let reference = match name {
        "account" => Some("accounts"),
        "payee" => Some("payees"),
        "category" => Some("categories"),
        "schedule" => Some("schedules"),
        _ => None,
    };
    let field = fields(table).iter().find(|f| f.name == name);
    json!({"name":name,"type":type_name(field.map_or(kind,|f|f.kind)),"reference":field.map_or(reference,|f|f.reference)})
}
pub(super) fn catalog() -> Value {
    Value::Array(TABLES.iter().map(|table| {
        let descriptors=names(table).into_iter().map(|name|descriptor(table,name)).collect::<Vec<_>>();
        json!({
            "table":table,"fields":descriptors,
            "default_order":if *table=="transactions" {"date descending, stable display position descending, id ascending"}else{"id ascending"},
            "representation":match *table {
                "category_groups"=>"flat; expand children with categories filter group=ID",
                "budget_cells"=>"business key method/month/category; opaque native id; ISO month",
                "categories"=>"automation definitions are structured JSON",
                _=>"typed resource profile"
            }
        })
    }).collect())
}
