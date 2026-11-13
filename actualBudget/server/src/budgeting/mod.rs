//! Monthly planning derives from live on-budget leaf transactions.
//! Tracking allocations are authoritative; cached results never replace them.
mod retirement;
pub use retirement::normalize;
mod actions;
pub(crate) mod templates;
pub use actions::category_deleted;
mod import;
use crate::{concurrency as sync, error::Error, models::MAX_MONEY, store::Store};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
pub use import::{
    clock as import_clock, convert as convert_actual, enrich as enrich_legacy_imports,
    install as install_actual, validate,
};
use rusqlite::{Connection, params, params_from_iter};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub fn router() -> Router<Store> {
    Router::new()
        .route(
            "/api/v1/budgets/{budget}/budget",
            get(read).post(actions::mutate),
        )
        .route("/api/v1/budgets/{budget}/budget/transactions", get(detail))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Range {
    month: String,
    #[serde(default = "one")]
    count: usize,
}
fn one() -> usize {
    1
}
pub fn month(value: &str) -> Result<i32, Error> {
    if value.len() != 7
        || value.as_bytes()[4] != b'-'
        || !value
            .bytes()
            .enumerate()
            .all(|(i, c)| i == 4 || c.is_ascii_digit())
    {
        return Err(Error::invalid("Month must be YYYY-MM"));
    }
    let year = value[..4]
        .parse::<i32>()
        .map_err(|_| Error::invalid("Invalid month"))?;
    let m = value[5..]
        .parse::<i32>()
        .map_err(|_| Error::invalid("Invalid month"))?;
    if !(1..=9999).contains(&year) || !(1..=12).contains(&m) {
        return Err(Error::invalid("Invalid month"));
    }
    Ok(year * 12 + m - 1)
}
pub fn month_name(value: i32) -> Result<String, Error> {
    if !(12..120000).contains(&value) {
        return Err(Error::invalid("Month is outside supported calendar"));
    }
    Ok(format!("{:04}-{:02}", value / 12, value % 12 + 1))
}
fn money(value: i128) -> Result<i64, Error> {
    if value.abs() > MAX_MONEY as i128 {
        Err(Error::invalid(
            "Budget result exceeds the exact integer range",
        ))
    } else {
        Ok(value as i64)
    }
}
#[derive(Clone)]
struct Category {
    id: String,
    name: String,
    income: bool,
    group: Option<String>,
    group_income: bool,
    group_name: String,
}
#[derive(Clone)]
struct Cell {
    amount: i64,
    carry: bool,
    goal: Option<String>,
    long_goal: Option<String>,
}
impl Default for Cell {
    fn default() -> Self {
        Self {
            amount: 0,
            carry: true,
            goal: None,
            long_goal: None,
        }
    }
}
#[derive(Default)]
struct Previous {
    left: i128,
    carry: bool,
}
async fn read(
    State(store): State<Store>,
    Path(budget): Path<String>,
    Query(range): Query<Range>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&budget)?;
    Ok(Json(
        json!({"data":store.run(move|db|{let tx=db.transaction()?;view(&tx,&budget,&range.month,range.count)}).await?}),
    ))
}
pub fn view(db: &Connection, b: &str, start: &str, count: usize) -> Result<Value, Error> {
    scoped_view(db, b, start, count, None, false)
}
fn scoped_view(
    db: &Connection,
    b: &str,
    start: &str,
    count: usize,
    scope: Option<&std::collections::BTreeSet<String>>,
    summary_report: bool,
) -> Result<Value, Error> {
    scoped_view_inner(
        db,
        b,
        start,
        count,
        scope,
        ViewOptions {
            summary_report,
            include_goals: false,
            work: None,
        },
    )
}
struct ViewOptions<'a> {
    summary_report: bool,
    include_goals: bool,
    work: Option<&'a mut usize>,
}
fn scoped_view_inner(
    db: &Connection,
    b: &str,
    start: &str,
    count: usize,
    scope: Option<&std::collections::BTreeSet<String>>,
    options: ViewOptions<'_>,
) -> Result<Value, Error> {
    let ViewOptions {
        summary_report,
        include_goals,
        work,
    } = options;
    crate::store::require_budget(db, b)?;
    let start = month(start)?;
    let reporting = scope.is_some() || summary_report;
    if !(1..=if reporting { 20000 } else { 12 }).contains(&count) {
        return Err(Error::invalid(if reporting {
            "Analysis permits up to20000 monthly points"
        } else {
            "Display 1–12 months"
        }));
    }
    let end = start + count as i32 - 1;
    month_name(end)?;
    let method: String = db.query_row("SELECT budget_type FROM budgets WHERE id=?1", [b], |r| {
        r.get(0)
    })?;

    let mut q=db.prepare("SELECT c.id,c.name,c.is_income,0,c.group_id,0,'',COALESCE(g.name,'Ungrouped'),COALESCE(g.is_income,0) FROM categories c LEFT JOIN category_groups g ON g.budget_id=c.budget_id AND g.id=c.group_id WHERE c.budget_id=?1 AND c.tombstone=0 ORDER BY COALESCE(g.sort_order,0),COALESCE(c.sort_order,0),c.id")?;
    let categories = q
        .query_map([b], |r| {
            Ok(Category {
                id: r.get(0)?,
                name: r.get(1)?,
                income: r.get(2)?,
                group: r.get(4)?,
                group_name: r.get(7)?,
                group_income: r.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|category| scope.is_none_or(|ids| ids.contains(&category.id)))
        .collect::<Vec<_>>();
    if summary_report && (categories.len() > 5000 || categories.len().saturating_mul(count) > 20000)
    {
        return Err(Error::invalid(
            "Sankey budget scope exceeds5000 categories or20000 category/month cells",
        ));
    }
    let mut earliest = start;
    let mut actuals = BTreeMap::<(i32, String), i128>::new();
    let mut actual_values = vec![b.to_owned()];
    let actual_scope = if let Some(ids) = scope {
        actual_values.push(format!("{}-31", month_name(end)?));
        let slots = ids
            .iter()
            .map(|id| {
                actual_values.push(id.clone());
                format!("?{}", actual_values.len())
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            " AND t.date<=?2 AND t.category_id IN ({})",
            if slots.is_empty() { "NULL" } else { &slots }
        )
    } else if summary_report {
        actual_values.push(format!("{}-31", month_name(end)?));
        actual_values.push(format!("{}-01", month_name(start)?));
        " AND t.date<=?2 AND t.date>=?3".into()
    } else {
        String::new()
    };
    let actual_sql = format!(
        "FROM transactions t JOIN accounts a ON a.budget_id=t.budget_id AND a.id=t.account_id WHERE t.budget_id=?1 AND t.tombstone=0 AND t.is_parent=0 AND a.off_budget=0 AND t.category_id IS NOT NULL{actual_scope}"
    );
    if reporting {
        let count: i64 = db.query_row(
            &format!("SELECT count(*) FROM (SELECT t.id {actual_sql} LIMIT 100001)"),
            params_from_iter(actual_values.iter()),
            |r| r.get(0),
        )?;
        if count > 100000 {
            return Err(Error::invalid(
                "Analysis exceeds100000 selected recorded inputs; select fewer categories or an earlier ending month",
            ));
        }
    }
    let mut q = db.prepare(&format!(
        "SELECT substr(t.date,1,7),t.category_id,t.amount {actual_sql}"
    ))?;
    for row in q.query_map(params_from_iter(actual_values.iter()), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })? {
        let (date, id, amount) = row?;
        if scope.is_some_and(|ids| !ids.contains(&id)) {
            continue;
        }
        let m = month(&date)?;
        if m <= end {
            earliest = earliest.min(m);
            *actuals.entry((m, id)).or_default() += amount as i128;
        }
    }
    let mut cells = BTreeMap::new();
    let goal_columns = if reporting && !include_goals {
        "NULL,NULL"
    } else {
        "goal,long_goal"
    };
    let mut cell_values = vec![b.to_owned(), method.clone()];
    let cell_scope = if let Some(ids) = scope {
        cell_values.push(month_name(end)?);
        let slots = ids
            .iter()
            .map(|id| {
                cell_values.push(id.clone());
                format!("?{}", cell_values.len())
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            " AND month<=?3 AND category_id IN ({})",
            if slots.is_empty() { "NULL" } else { &slots }
        )
    } else if summary_report {
        cell_values.push(month_name(end)?);
        cell_values.push(month_name(start)?);
        " AND month<=?3 AND month>=?4".into()
    } else {
        String::new()
    };
    let cell_sql =
        format!("FROM budget_cells WHERE budget_id=?1 AND method=?2 AND tombstone=0{cell_scope}");
    if reporting {
        let count: i64 = db.query_row(
            &format!("SELECT count(*) FROM (SELECT id {cell_sql} LIMIT 100001)"),
            params_from_iter(cell_values.iter()),
            |r| r.get(0),
        )?;
        if count > 100000 {
            return Err(Error::invalid(
                "Analysis exceeds100000 selected planned inputs; select fewer categories or an earlier ending month",
            ));
        }
    }
    let mut q = db.prepare(&format!(
        "SELECT month,category_id,amount,carryover,{goal_columns} {cell_sql}"
    ))?;
    for row in q.query_map(params_from_iter(cell_values.iter()), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            Cell {
                amount: r.get(2)?,
                carry: r.get(3)?,
                goal: r.get(4)?,
                long_goal: r.get(5)?,
            },
        ))
    })? {
        let (m, id, cell) = row?;
        if scope.is_some_and(|ids| !ids.contains(&id)) {
            continue;
        }
        let m = month(&m)?;
        if m <= end {
            earliest = earliest.min(m);
            cells.insert((m, id), cell);
        }
    }
    // Bound adversarial centuries × huge category sets without silently truncating.
    if (end - earliest + 1) as usize * categories.len().max(1) > 5_000_000 {
        return Err(Error::invalid(
            "Budget range exceeds calculation work limit",
        ));
    }
    if let Some(remaining) = work {
        let used = (end - earliest + 1) as usize * categories.len().max(1);
        *remaining=remaining.checked_sub(used).ok_or_else(||Error::invalid("Formula cumulative budget work limit reached; select fewer categories or shorter history"))?;
    }
    let mut previous = BTreeMap::<String, Previous>::new();
    let mut output = BTreeMap::new();
    for m in earliest..=end {
        let mut rows = vec![];
        let mut next = BTreeMap::new();
        let mut income = 0i128;
        let mut spending = 0i128;
        let mut allocated = 0i128;
        let mut planned_income = 0i128;
        let mut balance = 0i128;
        for cat in &categories {
            let cell = cells.get(&(m, cat.id.clone())).cloned().unwrap_or_default();
            let actual = *actuals.get(&(m, cat.id.clone())).unwrap_or(&0);
            let prev = previous.get(&cat.id);
            // Scoped analysis reports income-group actuals without allocations.
            let actual_only = scope.is_some() && cat.group_income;
            let carried = if actual_only || summary_report {
                0
            } else {
                prev.map_or(0, |p| if p.carry { p.left } else { 0 })
            };
            let left = if actual_only || summary_report {
                0
            } else if cat.income {
                cell.amount as i128 - actual + carried
            } else {
                cell.amount as i128 + actual + carried
            };
            money(cell.amount as i128)?;
            money(left)?;
            money(actual)?;
            if scope.is_none() && !summary_report {
                if cat.income {
                    income += actual;
                    planned_income += cell.amount as i128;
                } else {
                    spending += actual;
                    allocated += cell.amount as i128;
                    balance += left;
                }
            }
            next.insert(
                cat.id.clone(),
                Previous {
                    left,
                    carry: cell.carry,
                },
            );
            if m >= start {
                rows.push(json!({"id":cat.id,"name":cat.name,"is_income":cat.income,"group_id":cat.group,"group_name":cat.group_name,"budgeted":if actual_only{0}else{cell.amount},"actual":money(actual)?,"balance":money(left)?,"carryover":!actual_only && cell.carry,"carryover_from_previous":!actual_only && prev.is_none_or(|p|p.carry),"goal":cell.goal,"long_goal":cell.long_goal}));
            }
        }
        if m >= start && scope.is_none() && !summary_report {
            crate::categories::decorate_colors(db, b, &mut rows, "id")?;
        }
        if scope.is_some() || summary_report {
            if m >= start {
                output.insert(month_name(m)?, json!({"categories":rows}));
            }
            previous = next;
            continue;
        }
        // Reject overflow in intermediate carry months as well as displayed months.
        for n in [
            income,
            spending,
            allocated,
            planned_income,
            balance,
            income + spending,
            planned_income - allocated,
        ] {
            money(n)?;
        }
        if m >= start {
            let summary = json!({"total_budgeted":money(allocated)?,"total_budget_income":money(planned_income)?,"total_saved":money(planned_income-allocated)?,"total_income":money(income)?,"total_spent":money(spending)?,"real_saved":money(income+spending)?,"total_leftover":money(balance)?});
            output.insert(
                month_name(m)?,
                json!({"month":month_name(m)?,"summary":summary,"categories":rows}),
            );
        }
        previous = next;
    }
    Ok(
        json!({"method":method,"months":output,"snapshot":sync::cursor(db,b)?,"epoch":crate::lifecycle::epoch(db,b)?,"edit_clock":db.query_row("SELECT clock FROM budget_sync WHERE budget_id=?1",[b],|r|r.get::<_,String>(0))?}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Detail {
    month: String,
    category: String,
    #[serde(default)]
    offset: usize,
    #[serde(default = "detail_limit")]
    limit: usize,
    snapshot: Option<String>,
}
fn detail_limit() -> usize {
    100
}
async fn detail(
    State(store): State<Store>,
    Path(b): Path<String>,
    Query(query): Query<Detail>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    crate::models::validate_id(&query.category)?;
    month(&query.month)?;
    if !(1..=500).contains(&query.limit) || query.offset > 100000 {
        return Err(Error::invalid("Invalid transaction page"));
    }
    Ok(Json(store.run(move|db|{let tx=db.transaction()?;crate::store::require_budget(&tx,&b)?;sync::check_snapshot(&tx,&b,query.snapshot.as_deref())?;
 let category=crate::categories::get(&tx,&b,"categories",&query.category)?;if category["deleted"]==true{return Err(Error::missing());}
 let mut q=tx.prepare("SELECT t.id,t.amount FROM transactions t JOIN accounts a ON a.budget_id=t.budget_id AND a.id=t.account_id WHERE t.budget_id=?1 AND t.category_id=?2 AND substr(t.date,1,7)=?3 AND t.tombstone=0 AND t.is_parent=0 AND a.off_budget=0 ORDER BY t.date DESC,t.sort_order DESC,t.id")?;
 let rows=q.query_map(params![b,query.category,query.month],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?)))?.collect::<Result<Vec<_>,_>>()?;
 let total=money(rows.iter().map(|(_,n)|*n as i128).sum())?;let ids=rows.iter().skip(query.offset).take(query.limit).map(|(id,_)|id.clone()).collect::<Vec<_>>();
 Ok(json!({"data":crate::ledger::get_many(&tx,&b,&ids)?,"total_count":rows.len(),"actual_total":total,"snapshot":sync::cursor(&tx,&b)?,"category":category}))
 }).await?))
}

/// Change cursors include authoritative planning records, including retired cells.
pub fn record(db: &Connection, b: &str, entity: &str, id: &str) -> Result<Value, Error> {
    let value = match entity {
        "budget_cells" => db.query_row("SELECT method,month,category_id,amount,carryover,goal,long_goal,tombstone FROM budget_cells WHERE budget_id=?1 AND id=?2",params![b,id],|r|Ok(json!({"id":id,"method":r.get::<_,String>(0)?,"month":r.get::<_,String>(1)?,"category_id":r.get::<_,String>(2)?,"amount":r.get::<_,i64>(3)?,"carryover":r.get::<_,bool>(4)?,"goal":r.get::<_,Option<String>>(5)?,"long_goal":r.get::<_,Option<String>>(6)?,"deleted":r.get::<_,bool>(7)?})))?,
        "budget_months" => db.query_row("SELECT buffered,notes,tombstone FROM budget_months WHERE budget_id=?1 AND id=?2",params![b,id],|r|Ok(json!({"id":id,"buffered":r.get::<_,i64>(0)?,"notes":r.get::<_,String>(1)?,"deleted":r.get::<_,bool>(2)?})))?,
        "budget_settings" => db.query_row("SELECT method,template_currency,template_currency_review FROM budget_settings WHERE budget_id=?1 AND id=?2",params![b,id],|r|Ok(json!({"id":id,"method":r.get::<_,String>(0)?,"template_currency":r.get::<_,Option<String>>(1)?,"template_currency_review":r.get::<_,bool>(2)?,"deleted":false})))?,
        _ => return Err(Error::internal("Unknown planning change entity")),
    };
    Ok(value)
}
