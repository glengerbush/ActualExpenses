//! Current ledger/budget aggregates; graph presentation never reads full history.
use super::super::{JOINS, predicate, truthy};
use super::{Options, graph::*};
use crate::error::Error;
use rusqlite::{
    Connection,
    functions::{Aggregate, Context, FunctionFlags},
    params_from_iter,
    types::Value as SqlValue,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Category {
    pub id: String,
    pub name: String,
    pub income: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Group {
    pub id: String,
    pub name: String,
    pub income: bool,
    pub categories: Vec<Category>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Entry {
    pub group: String,
    pub group_name: String,
    pub category: String,
    pub category_name: String,
    pub income: bool,
    pub value: i64,
    pub negative: bool,
    pub account: String,
    pub account_name: String,
    pub payee: String,
    pub payee_name: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Transfer {
    pub from: String,
    pub from_name: String,
    pub to: String,
    pub to_name: String,
    pub amount: i64,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct BudgetContext {
    pub to_budget: i64,
    pub previous: i64,
    pub overspent: i64,
    pub next: i64,
    pub start: String,
    pub end: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Base {
    pub groups: Vec<Group>,
    pub entries: Vec<Entry>,
    pub transfers: Vec<Transfer>,
    pub context: Option<BudgetContext>,
}
struct Sum;
impl Aggregate<i128, String> for Sum {
    fn init(&self, _: &mut Context<'_>) -> rusqlite::Result<i128> {
        Ok(0)
    }
    fn step(&self, ctx: &mut Context<'_>, sum: &mut i128) -> rusqlite::Result<()> {
        *sum += ctx.get::<i64>(0)? as i128;
        Ok(())
    }
    fn finalize(&self, _: &mut Context<'_>, sum: Option<i128>) -> rusqlite::Result<String> {
        Ok(sum.unwrap_or(0).to_string())
    }
}
fn groups(db: &Connection, b: &str) -> Result<Vec<Group>, Error> {
    let mut output: Vec<Group> = vec![];
    let mut categories = 0usize;
    let mut stmt=db.prepare("SELECT g.id,g.name,g.is_income,c.id,c.name,c.is_income FROM category_groups g LEFT JOIN categories c ON c.budget_id=g.budget_id AND c.group_id=g.id AND c.tombstone=0 WHERE g.budget_id=?1 AND g.tombstone=0 ORDER BY g.is_income,g.sort_order,g.id,c.sort_order,c.id LIMIT 10001")?;
    let mut rows = stmt.query([b])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        if output.last().is_none_or(|group| group.id != id) {
            output.push(Group {
                id,
                name: row.get(1)?,
                income: row.get(2)?,
                categories: vec![],
            });
        }
        if let Some(id) = row.get::<_, Option<String>>(3)? {
            categories += 1;
            output.last_mut().unwrap().categories.push(Category {
                id,
                name: row.get(4)?,
                income: row.get(5)?,
            });
        }
        if categories > 5000 || output.len() > 5000 {
            return Err(Error::invalid(
                "Sankey permits5000 categories and groups; narrow the dataset",
            ));
        }
    }
    Ok(output)
}
pub(super) fn clauses(
    db: &Connection,
    b: &str,
    options: &Options,
) -> Result<(Vec<SqlValue>, String), Error> {
    let mut values = vec![
        SqlValue::Text(b.into()),
        SqlValue::Text(options.start_date[..7].to_owned() + "-01"),
        SqlValue::Text(options.end_date[..7].to_owned() + "-31"),
    ];
    // The source removes truthy named conditions, then calls its SQL translator.
    // Remaining nonfunctional name markers do not affect SQL translation.
    let conditions = options
        .conditions
        .iter()
        .filter(|condition| !truthy(&condition["customName"]))
        .cloned()
        .map(|mut condition| {
            condition.as_object_mut().unwrap().remove("customName");
            condition
        })
        .collect::<Vec<_>>();
    let filter = predicate(
        db,
        b,
        &json!({"conditions":conditions,"conditionsOp":options.conditions_op}),
        &mut values,
    )?;
    Ok((
        values,
        format!(
            "t.budget_id=?1 AND t.tombstone=0 AND t.is_parent=0 AND t.date>=?2 AND t.date<=?3 AND ({filter})"
        ),
    ))
}
pub(super) fn input_bound(
    db: &Connection,
    values: &[SqlValue],
    where_clause: &str,
) -> Result<(), Error> {
    let count: i64 = db.query_row(
        &format!("SELECT count(*) FROM (SELECT t.id {JOINS} WHERE {where_clause} LIMIT 100001)"),
        params_from_iter(values),
        |r| r.get(0),
    )?;
    if count > 100000 {
        Err(Error::invalid(
            "Sankey exceeds100000 matching entries; narrow dates or conditions",
        ))
    } else {
        Ok(())
    }
}
fn spent(
    db: &Connection,
    b: &str,
    options: &Options,
    groups: &[Group],
) -> Result<(Vec<Entry>, Vec<Transfer>), Error> {
    db.create_aggregate_function(
        "sankey_exact_sum",
        1,
        FunctionFlags::SQLITE_DETERMINISTIC,
        Sum,
    )?;
    let (values, where_clause) = clauses(db, b, options)?;
    let relevant = format!(
        "{where_clause} AND ((c.tombstone=0 AND g.tombstone=0) OR ({} AND t.transfer_id IS NOT NULL AND a.off_budget=0))",
        i32::from(options.show_transfers)
    );
    if options.show_transfers || groups.iter().any(|group| !group.categories.is_empty()) {
        input_bound(db, &values, &relevant)?;
    }
    let mut entries = vec![];
    for group in groups {
        for category in &group.categories {
            let mut values = values.clone();
            values.push(SqlValue::Text(category.id.clone()));
            let payee_group = if group.income { ",payee_id" } else { "" };
            // The pinned source category/date index visits a category in ascending date
            // and insertion order. Bare selected fields and default group ordering use
            // that first representative row. Our native history index is descending;
            // materialize this source order explicitly rather than depend on its plan.
            let sql = format!(
                "WITH selected AS MATERIALIZED (SELECT a.id AS account_id,a.name AS account_name,t.amount,p.id AS payee_id,coalesce(y.name,p.name,'') AS payee_name,t.date,t.starting_balance,t.sort_order,t.id {JOINS} WHERE {where_clause} AND c.id=?{} ORDER BY t.date ASC,t.rowid ASC) SELECT account_id,account_name,sankey_exact_sum(amount),payee_id,payee_name FROM selected GROUP BY account_id{payee_group} ORDER BY date DESC,starting_balance,sort_order DESC,id LIMIT 20001",
                values.len()
            );
            let mut stmt = db.prepare(&sql)?;
            let mut rows = stmt.query(params_from_iter(values))?;
            while let Some(row) = rows.next()? {
                let amount = row
                    .get::<_, String>(2)?
                    .parse::<i128>()
                    .map_err(Error::internal)?;
                let amount = exact(amount)?;
                entries.push(Entry {
                    group: group.id.clone(),
                    group_name: group.name.clone(),
                    category: category.id.clone(),
                    category_name: category.name.clone(),
                    income: group.income,
                    value: amount.abs(),
                    negative: amount < 0,
                    account: row.get(0)?,
                    account_name: row.get(1)?,
                    payee: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    payee_name: row.get(4)?,
                });
                if entries.len() > 20000 {
                    return Err(Error::invalid(
                        "Sankey exceeds20000 source aggregate cells; narrow conditions",
                    ));
                }
            }
        }
    }
    let transfers = if options.show_transfers {
        transfers(db, &values, &where_clause)?
    } else {
        vec![]
    };
    Ok((entries, transfers))
}
#[derive(Clone)]
pub(super) struct TransferRow {
    pub id: String,
    pub amount: i64,
    pub account: String,
    pub name: String,
    pub peer: String,
    pub date: String,
}
pub(super) fn transfer_rows(
    db: &Connection,
    values: &[SqlValue],
    where_clause: &str,
) -> Result<Vec<TransferRow>, Error> {
    let mut stmt=db.prepare(&format!("SELECT t.id,t.amount,t.account_id,a.name,t.transfer_id,t.date {JOINS} WHERE {where_clause} AND t.transfer_id IS NOT NULL AND a.off_budget=0 ORDER BY t.date DESC,t.starting_balance,t.sort_order DESC,t.id LIMIT 100001"))?;
    let rows = stmt
        .query_map(params_from_iter(values), |row| {
            Ok(TransferRow {
                id: row.get(0)?,
                amount: row.get(1)?,
                account: row.get(2)?,
                name: row.get(3)?,
                peer: row.get(4)?,
                date: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if rows.len() > 100000 {
        return Err(Error::invalid("Sankey transfer input exceeds100000 rows"));
    }
    Ok(rows)
}
fn transfers(
    db: &Connection,
    values: &[SqlValue],
    where_clause: &str,
) -> Result<Vec<Transfer>, Error> {
    let rows = transfer_rows(db, values, where_clause)?;
    let by_id = rows
        .iter()
        .map(|r| (r.id.clone(), r))
        .collect::<BTreeMap<_, _>>();
    let names = rows
        .iter()
        .map(|r| (r.account.clone(), r.name.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut pairs = Vec::<((String, String), i128)>::new();
    let mut indices = BTreeMap::<(String, String), usize>::new();
    for from in &rows {
        let Some(to) = by_id.get(&from.peer) else {
            continue;
        };
        if from.account > to.account {
            continue;
        }
        let pair = (from.account.clone(), to.account.clone());
        let source = if from.amount < 0 {
            &from.account
        } else {
            &to.account
        };
        let sign = if source == &pair.0 { 1 } else { -1 };
        let value = from.amount.abs() as i128 * sign;
        if let Some(&index) = indices.get(&pair) {
            pairs[index].1 += value;
        } else {
            indices.insert(pair.clone(), pairs.len());
            pairs.push((pair, value));
        }
    }
    pairs
        .into_iter()
        .filter(|(_, amount)| *amount != 0)
        .map(|((a, b), amount)| {
            let (from, to) = if amount > 0 { (a, b) } else { (b, a) };
            Ok(Transfer {
                from_name: names.get(&from).cloned().unwrap_or_default(),
                to_name: names.get(&to).cloned().unwrap_or_default(),
                from,
                to,
                amount: exact(amount.abs())?,
            })
        })
        .collect()
}
fn manual_match(groups: &[Group], options: &Options) -> Result<Vec<Group>, Error> {
    let budget = crate::register_regex::replacement_budget();
    let mut conditions = vec![];
    for condition in &options.conditions {
        if !matches!(
            condition["field"].as_str(),
            Some("category" | "category_group")
        ) {
            continue;
        }
        let pattern = if condition["op"] == "matches" {
            condition["value"]
                .as_str()
                .filter(|value| value.encode_utf16().count() <= 256)
                .and_then(|value| {
                    let source = if value.starts_with('/')
                        && value.rfind('/').is_some_and(|index| index > 0)
                    {
                        &value[1..value.rfind('/').unwrap()]
                    } else {
                        value
                    };
                    match crate::register_regex::Pattern::compile(source, true) {
                        Ok(pattern) => Some(Ok(pattern)),
                        Err(crate::register_regex::Failure::Invalid) => None,
                        Err(error) => Some(Err(error.api_error())),
                    }
                })
                .transpose()?
        } else {
            None
        };
        conditions.push((condition, pattern));
    }
    let mut output = vec![];
    for group in groups {
        let mut selected = group.clone();
        selected.categories.clear();
        for category in &group.categories {
            let mut matches = vec![];
            for (condition, pattern) in &conditions {
                let (id, name) = if condition["field"] == "category" {
                    (&category.id, &category.name)
                } else {
                    (&group.id, &group.name)
                };
                let value = &condition["value"];
                matches.push(match condition["op"].as_str().unwrap_or("") {
                    "is" => *value == json!(id),
                    "isNot" => *value != json!(id),
                    "oneOf" => value.as_array().is_some_and(|v| v.contains(&json!(id))),
                    "notOneOf" => value.as_array().is_none_or(|v| !v.contains(&json!(id))),
                    "contains" => value
                        .as_str()
                        .is_some_and(|value| name.to_lowercase().contains(&value.to_lowercase())),
                    "doesNotContain" => value
                        .as_str()
                        .is_some_and(|value| !name.to_lowercase().contains(&value.to_lowercase())),
                    "matches" => pattern
                        .as_ref()
                        .map(|pattern| {
                            pattern
                                .matches(name, &budget)
                                .map_err(|error| error.api_error())
                        })
                        .transpose()?
                        .unwrap_or(false),
                    _ => false,
                });
            }
            if conditions.is_empty()
                || if options.conditions_op == "or" {
                    matches.iter().any(|value| *value)
                } else {
                    matches.iter().all(|value| *value)
                }
            {
                selected.categories.push(category.clone());
            }
        }
        if !selected.categories.is_empty() {
            output.push(selected);
        }
    }
    Ok(output)
}
fn budgeted(
    db: &Connection,
    b: &str,
    options: &Options,
    groups: &[Group],
) -> Result<(Vec<Entry>, BudgetContext), Error> {
    let start = crate::budgeting::month(&options.start_date[..7])?;
    let end = crate::budgeting::month(&options.end_date[..7])?;
    let count = (end - start + 1) as usize;
    let data = crate::budgeting::sankey_months(db, b, &options.start_date[..7], count + 1)?;
    let months = data["months"]
        .as_object()
        .ok_or_else(|| Error::internal("Missing Sankey budget month cells"))?;
    let selected = manual_match(groups, options)?;
    let mut sums = BTreeMap::<String, i128>::new();
    let mut overspent = 0i128;
    for month in start..=end {
        let name = crate::budgeting::month_name(month)?;
        let view = &months[&name];
        overspent += view["summary"]["last_month_overspent"]
            .as_i64()
            .unwrap_or(0) as i128;
        let rows = view["categories"]
            .as_array()
            .ok_or_else(|| Error::internal("Missing Sankey category cells"))?;
        let by_id = rows
            .iter()
            .map(|row| {
                row["id"]
                    .as_str()
                    .map(|id| (id, row))
                    .ok_or_else(|| Error::internal("Missing Sankey category identity"))
            })
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        for group in &selected {
            for category in &group.categories {
                if let Some(row) = by_id.get(category.id.as_str()) {
                    *sums.entry(category.id.clone()).or_default() +=
                        row[if group.income { "actual" } else { "budgeted" }]
                            .as_i64()
                            .unwrap_or(0) as i128;
                }
            }
        }
    }
    let mut entries = vec![];
    for group in &selected {
        for category in &group.categories {
            let value = exact(sums.get(&category.id).copied().unwrap_or(0))?;
            if value != 0 {
                entries.push(Entry {
                    group: group.id.clone(),
                    group_name: group.name.clone(),
                    category: category.id.clone(),
                    category_name: category.name.clone(),
                    income: group.income,
                    value,
                    negative: value < 0,
                    account: String::new(),
                    account_name: String::new(),
                    payee: String::new(),
                    payee_name: String::new(),
                });
            }
        }
    }
    let to = months[&crate::budgeting::month_name(end)?]["summary"]["to_budget"]
        .as_i64()
        .unwrap_or(0);
    let previous = months[&crate::budgeting::month_name(start)?]["summary"]["from_last_month"]
        .as_i64()
        .unwrap_or(0);
    let next = months[&crate::budgeting::month_name(end + 1)?]["summary"]["from_last_month"]
        .as_i64()
        .unwrap_or(0) as i128
        - to as i128;
    Ok((
        entries,
        BudgetContext {
            to_budget: to,
            previous,
            overspent: exact(overspent)?,
            next: exact(next)?,
            start: options.start_date[..7].into(),
            end: options.end_date[..7].into(),
        },
    ))
}
pub(super) fn read(db: &Connection, b: &str, options: &Options) -> Result<Base, Error> {
    let groups = groups(db, b)?;
    let (entries, transfers, context) = if options.mode == "spent" {
        let (entries, transfers) = spent(db, b, options, &groups)?;
        (entries, transfers, None)
    } else {
        let (entries, context) = budgeted(db, b, options, &groups)?;
        (entries, vec![], Some(context))
    };
    Ok(Base {
        groups,
        entries,
        transfers,
        context,
    })
}
fn account(graph: &mut Graph, id: &str, name: &str) {
    if id == ALL_ACCOUNTS {
        graph.labeled(id, Layer::Account, "Income", None, None);
    } else {
        graph.add(id, Layer::Account, Some(name));
    }
}
pub(super) fn graph(base: &Base, group_accounts: bool) -> Result<Graph, Error> {
    let mut graph = Graph::default();
    if let Some(context) = &base.context {
        graph.add(BUDGETED, Layer::Budget, Some("Budgeted"));
        graph.add(AVAILABLE, Layer::Account, Some("Available funds"));
        for (index, entry) in base.entries.iter().enumerate() {
            graph.tag(Some((Source::Entry(index), entry.value as i128)));
            let value = entry.value as i128;
            if entry.income {
                graph.add(
                    &entry.category,
                    Layer::IncomeCategory,
                    Some(&entry.category_name),
                );
                graph.link(&entry.category, AVAILABLE, value);
            } else if value >= 0 {
                graph.add(&entry.group, Layer::Group, Some(&entry.group_name));
                graph.add(&entry.category, Layer::Category, Some(&entry.category_name));
                graph.link(&entry.group, &entry.category, value);
                graph.link(BUDGETED, &entry.group, value);
                graph.link(AVAILABLE, BUDGETED, value);
            } else {
                graph.labeled(
                    &entry.category,
                    Layer::Account,
                    "From {{category}}",
                    Some(json!({"category":entry.category_name})),
                    Some(true),
                );
                graph.link(&entry.category, BUDGETED, value.abs());
                graph.link(AVAILABLE, BUDGETED, value);
            }
        }
        graph.tag(Some((
            Source::Context("to_budget".into()),
            context.to_budget as i128,
        )));
        if context.to_budget > 0 {
            graph.labeled(TO_BUDGET, Layer::Group, "To budget", None, Some(false));
            graph.link(AVAILABLE, TO_BUDGET, context.to_budget as i128);
        } else {
            graph.labeled(TO_BUDGET, Layer::Account, "Overbudgeted", None, Some(true));
            graph.link(TO_BUDGET, BUDGETED, (context.to_budget as i128).abs());
            graph.link(AVAILABLE, BUDGETED, -(context.to_budget as i128).abs());
        }
        let previous = crate::budgeting::month(&context.start)? - 1;
        let next = crate::budgeting::month(&context.end)? + 1;
        graph.tag(Some((
            Source::Context("previous".into()),
            context.previous as i128,
        )));
        graph.labeled(
            PREVIOUS,
            Layer::IncomeCategory,
            "From {{month}}",
            Some(json!({"month":crate::budgeting::month_name(previous)?})),
            None,
        );
        graph.link(PREVIOUS, AVAILABLE, context.previous as i128);
        graph.tag(Some((Source::Context("next".into()), context.next as i128)));
        graph.labeled(
            NEXT,
            Layer::Budget,
            "For {{month}}",
            Some(json!({"month":crate::budgeting::month_name(next)?})),
            None,
        );
        graph.link(AVAILABLE, NEXT, context.next as i128);
        graph.tag(Some((
            Source::Context("overspent".into()),
            context.overspent as i128,
        )));
        graph.add(OVERSPENT, Layer::Budget, Some("Overspent"));
        graph.link(AVAILABLE, OVERSPENT, (context.overspent as i128).abs());
    } else {
        for (index, entry) in base.entries.iter().enumerate() {
            graph.tag(Some((Source::Entry(index), entry.value as i128)));
            if entry.account.is_empty()
                || entry.account_name.is_empty()
                || entry.category.is_empty()
            {
                continue;
            }
            let (id, name) = if group_accounts {
                (ALL_ACCOUNTS, ALL_ACCOUNTS)
            } else {
                (entry.account.as_str(), entry.account_name.as_str())
            };
            let value = entry.value as i128;
            if entry.income {
                if entry.negative {
                    account(&mut graph, id, name);
                    let negative = entry.category.clone() + NEGATIVE;
                    graph.labeled(&negative, Layer::Group, &entry.payee_name, None, Some(true));
                    graph.link(id, &negative, value);
                } else {
                    graph.add(
                        &entry.category,
                        Layer::IncomeCategory,
                        Some(&entry.category_name),
                    );
                    account(&mut graph, id, name);
                    graph.link(&entry.category, id, value);
                    if !entry.payee.is_empty() {
                        graph.add(&entry.payee, Layer::Payee, Some(&entry.payee_name));
                        graph.link(&entry.payee, &entry.category, value);
                    }
                }
            } else if entry.negative {
                account(&mut graph, id, name);
                graph.add(&entry.group, Layer::Group, Some(&entry.group_name));
                graph.add(&entry.category, Layer::Category, Some(&entry.category_name));
                graph.link(id, &entry.group, value);
                graph.link(&entry.group, &entry.category, value);
            } else {
                let negative = entry.category.clone() + NEGATIVE;
                graph.add(&negative, Layer::IncomeCategory, Some(&entry.payee_name));
                account(&mut graph, id, name);
                graph.link(&negative, id, value);
            }
        }
        if !group_accounts {
            for (index, transfer) in base.transfers.iter().enumerate() {
                graph.tag(Some((Source::Transfer(index), transfer.amount as i128)));
                account(&mut graph, &transfer.from, &transfer.from_name);
                account(&mut graph, &transfer.to, &transfer.to_name);
                graph.link(&transfer.from, &transfer.to, transfer.amount as i128);
            }
        }
    }
    graph.tag(None);
    graph.raw()?;
    Ok(graph)
}
