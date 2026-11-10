//! Exact monthly source analysis over scoped budget cells, never financial writes.
use super::*;
use axum::Extension;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u8,
    as_of: String,
    options: Options,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Options {
    start_date: String,
    end_date: String,
    #[serde(default)]
    conditions: Vec<Value>,
    #[serde(default = "and")]
    conditions_op: String,
    #[serde(default)]
    show_hidden_categories: bool,
}
fn and() -> String {
    "and".into()
}
type Cache = Arc<Mutex<super::revision_cache::Cache>>;
pub(crate) fn router() -> Router<Store> {
    Router::new()
        .route(
            "/api/v1/budgets/{budget}/reports/budget-analysis",
            post(read),
        )
        .route(
            "/api/v1/budgets/{budget}/reports/budget-analysis/drilldown",
            post(drilldown),
        )
        .layer(Extension(Cache::default()))
}
#[derive(Clone)]
struct Category {
    id: String,
    name: String,
    group: Option<String>,
    group_name: Option<String>,
    income_group: bool,
}
fn exact(value: i128) -> Result<i64, Error> {
    if value.abs() > crate::models::MAX_MONEY as i128 {
        Err(Error::invalid("Analysis exceeds exact monetary range"))
    } else {
        Ok(value as i64)
    }
}

fn selection(db: &Connection, b: &str, options: &Options) -> Result<Vec<Category>, Error> {
    let count:i64=db.query_row("SELECT count(*) FROM (SELECT id FROM categories WHERE budget_id=?1 AND tombstone=0 LIMIT 5001)",[b],|r|r.get(0))?;
    if count > 5000 {
        return Err(Error::invalid("Analysis permits5000 active categories"));
    }
    let mut rows=db.prepare("SELECT c.id,c.name,c.group_id,g.name,COALESCE(g.is_income,0) FROM categories c LEFT JOIN category_groups g ON g.budget_id=c.budget_id AND g.id=c.group_id WHERE c.budget_id=?1 AND c.tombstone=0 AND c.is_income=0 AND (?2 OR c.hidden=0) ORDER BY c.id LIMIT 5001")?.query_map(params![b,options.show_hidden_categories],|r|Ok(Category{id:r.get(0)?,name:r.get(1)?,group:r.get(2)?,group_name:r.get(3)?,income_group:r.get(4)?}))?.collect::<Result<Vec<_>,_>>()?;
    if rows.len() > 5000 {
        return Err(Error::invalid(
            "Analysis permits5000 expense categories; narrow the dataset",
        ));
    }
    let mut conditions = vec![];
    for value in &options.conditions {
        if !["category", "category_group"].contains(&value["field"].as_str().unwrap_or(""))
            || truthy(&value["customName"])
        {
            continue;
        }
        let regex = if value["op"] == "matches" {
            match value["value"]
                .as_str()
                .filter(|value| value.encode_utf16().count() <= 256)
            {
                Some(pattern) => match crate::register_regex::Pattern::compile(pattern, true) {
                    Ok(pattern) => Some(pattern),
                    Err(crate::register_regex::Failure::Invalid) => None,
                    Err(error) => return Err(error.api_error()),
                },
                None => None,
            }
        } else {
            None
        };
        conditions.push((value, regex));
    }
    if conditions.is_empty() {
        return Ok(rows);
    }
    let budget = crate::register_regex::replacement_budget();
    let mut selected = vec![];
    for category in rows.drain(..) {
        let mut matches = vec![];
        for (condition, regex) in &conditions {
            let key = if condition["field"] == "category" {
                json!(category.id)
            } else {
                json!(category.group)
            };
            let text = if condition["field"] == "category" {
                category.name.as_str()
            } else {
                category
                    .group_name
                    .as_deref()
                    .or(category.group.as_deref())
                    .unwrap_or("Ungrouped")
            };
            let value = &condition["value"];
            matches.push(match condition["op"].as_str().unwrap_or("") {
                "is" => *value == key,
                "isNot" => *value != key,
                "oneOf" => value.as_array().is_some_and(|values| values.contains(&key)),
                "notOneOf" => value
                    .as_array()
                    .is_some_and(|values| !values.contains(&key)),
                "contains" => value
                    .as_str()
                    .is_some_and(|v| text.to_lowercase().contains(&v.to_lowercase())),
                "doesNotContain" => value
                    .as_str()
                    .is_some_and(|v| !text.to_lowercase().contains(&v.to_lowercase())),
                "matches" => regex
                    .as_ref()
                    .map(|regex| {
                        regex
                            .matches(text, &budget)
                            .map_err(|error| error.api_error())
                    })
                    .transpose()?
                    .unwrap_or(false),
                _ => false,
            });
        }
        if if options.conditions_op == "or" {
            matches.iter().any(|value| *value)
        } else {
            matches.iter().all(|value| *value)
        } {
            selected.push(category);
        }
    }
    Ok(selected)
}
fn validate(q: &Request) -> Result<(i32, i32), Error> {
    if q.version != 1 {
        return Err(Error::invalid("Unsupported budget analysis version"));
    }
    day(&q.as_of)?;
    let start = first(&q.options.start_date)?;
    let end = first(&q.options.end_date)?;
    if start > end {
        return Err(Error::invalid("Analysis end precedes start"));
    }
    if !["and", "or"].contains(&q.options.conditions_op.as_str())
        || q.options.conditions.len() > 20
        || q.options.conditions.iter().any(|value| {
            !value.is_object()
                || !value["field"].is_string()
                || !value["op"].is_string()
                || value.get("value").is_none()
        })
    {
        return Err(Error::invalid(
            "Analysis permits20 conditions with field/operator/value and and/or",
        ));
    }
    let a = crate::budgeting::month(&start.to_string()[..7])?;
    let z = crate::budgeting::month(&end.to_string()[..7])?;
    if z - a + 1 > 20000 {
        return Err(Error::invalid(
            "Analysis contains more than20000 monthly points; shorten the date range",
        ));
    }
    if serde_json::to_vec(q).map_err(Error::internal)?.len() > 65536 {
        return Err(Error::invalid("Analysis query exceeds64KiB"));
    }
    Ok((a, z))
}
fn calculate(db: &Connection, b: &str, q: &Request, start: i32, end: i32) -> Result<Value, Error> {
    let categories = selection(db, b, &q.options)?;
    let ids = categories
        .iter()
        .map(|category| category.id.clone())
        .collect::<BTreeSet<_>>();
    let category_map = categories
        .iter()
        .map(|category| (category.id.as_str(), category))
        .collect::<BTreeMap<_, _>>();
    let mut months = BTreeMap::new();
    if !ids.is_empty() {
        let previous = (start - 1).max(12);
        months = crate::budgeting::report_cells(
            db,
            b,
            &crate::budgeting::month_name(previous)?,
            (end - previous + 1) as usize,
            &ids,
        )?;
    }
    let summarize = |month: i32| -> Result<(i128, i128, i128, i128, bool), Error> {
        let name = crate::budgeting::month_name(month)?;
        let mut budgeted = 0i128;
        let mut spent = 0i128;
        let mut carry = 0i128;
        let mut overspent = 0i128;
        let mut data = false;
        for row in months.get(&name).into_iter().flatten() {
            let category = category_map
                .get(row.id.as_str())
                .ok_or_else(|| Error::internal("Unexpected scoped category"))?;
            let allocated = if category.income_group {
                0
            } else {
                row.budgeted
            };
            let actual = row.actual;
            let balance = if category.income_group {
                0
            } else {
                row.balance
            };
            let rollover = !category.income_group && row.carryover;
            data |= allocated != 0 || actual != 0 || balance != 0;
            budgeted += allocated as i128;
            spent += actual as i128;
            if balance > 0 || balance < 0 && rollover {
                carry += balance as i128;
            } else if balance < 0 {
                overspent += balance as i128;
            }
        }
        Ok((budgeted, spent, carry, overspent, data))
    };
    let mut running = if start > 12 {
        exact(summarize(start - 1)?.2)?
    } else {
        0
    };
    let mut previous_overspent = 0i64;
    let mut total_budget = 0i128;
    let mut total_spent = 0i128;
    let mut total_adjust = 0i128;
    let mut rows = vec![];
    for month in start..=end {
        let (budgeted, spent, carry, overspent, has_data) = summarize(month)?;
        let balance = exact(budgeted + spent + running as i128)?;
        total_budget += budgeted;
        total_spent += spent;
        total_adjust += (previous_overspent as i128).abs();
        rows.push(json!({"date":crate::budgeting::month_name(month)?,"budgeted":exact(budgeted)?,"spent":exact(spent)?,"balance":balance,"overspendingAdjustment":previous_overspent.abs()}));
        if has_data {
            running = exact(carry)?;
        }
        previous_overspent = exact(overspent)?;
    }
    let mut assumptions = vec![
        json!({"code":"monthly","message":"Monthly planned allocations and signed recorded entries, including future-dated records. This analysis does not change the budget."}),
        json!({"code":"carry","message":"Running balances start from prior category cells. Positive cell balances carry; negative cells carry only when rollover is enabled. Resets appear as the next month’s positive overspending adjustment; the first adjustment is zero."}),
        json!({"code":"filters","message":"Only category and category-group conditions affect this analysis. Other saved conditions are retained and ignored. Income categories are excluded; hidden categories follow the selected option. A hidden group alone does not hide its visible categories."}),
    ];
    if categories.iter().any(|category| category.income_group) {
        assumptions.push(json!({"code":"income_group","message":"Expense categories stored in an income group contribute recorded amounts, but no planned allocation or rollover values, matching the source report."}));
    }
    if q.options
        .conditions
        .iter()
        .filter(|condition| {
            ["category", "category_group"].contains(&condition["field"].as_str().unwrap_or(""))
                && !truthy(&condition["customName"])
                && condition["op"] == "matches"
        })
        .any(|condition| {
            condition["value"].as_str().is_none_or(|pattern| {
                pattern.encode_utf16().count() > 256
                    || crate::register_regex::Pattern::compile(pattern, true).is_err()
            })
        })
    {
        assumptions.push(json!({"code":"category_pattern","message":"An invalid category-name expression or one longer than256 characters selects no categories for that condition. Revise the expression to change the selection."}));
    }
    Ok(
        json!({"result":{"intervalData":rows,"startDate":q.options.start_date,"endDate":q.options.end_date,"totalBudgeted":exact(total_budget)?,"totalSpent":exact(total_spent)?,"totalOverspendingAdjustment":exact(total_adjust)?,"finalOverspendingAdjustment":previous_overspent},"categories":categories.iter().map(|category|json!({"id":category.id,"name":category.name,"group_id":category.group,"income_group":category.income_group})).collect::<Vec<_>>(),"snapshot":sync::cursor(db,b)?,"epoch":crate::lifecycle::epoch(db,b)?,"as_of":q.as_of,"recorded":false,"basis":"budget_allocations_and_recorded_entries","assumptions":assumptions}),
    )
}
async fn read(
    State(store): State<Store>,
    Path(b): Path<String>,
    Extension(cache): Extension<Cache>,
    input: Result<Json<Request>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    let Json(q) = input.map_err(crate::api::json_error)?;
    let (start, end) = validate(&q)?;
    let signature = serde_json::to_string(&q).map_err(Error::internal)?;
    let value=store.run(move|db|{let tx=db.transaction()?;super::sql_work::run(&tx, || {let prefs=crate::preferences::get_preferences(&tx,&b)?;if prefs["experimental"]["budgetAnalysisReport"]!=true{return Err(Error::new(StatusCode::CONFLICT,"feature_disabled","Enable Budget analysis in experimental settings before calculating it"));}
  let method:String=tx.query_row("SELECT budget_type FROM budgets WHERE id=?1",[&b],|r|r.get(0))?;let revision:i64=tx.query_row("SELECT COALESCE(max(sequence),0) FROM budget_changes WHERE budget_id=?1 AND (entity IN ('transactions','accounts','categories','category_groups','budget_cells') OR (entity='budget_settings' AND entity_id='method'))",[&b],|r|r.get(0))?;let epoch=crate::lifecycle::epoch(&tx,&b)?;let revision=format!("a1{epoch}{revision:016x}:{method}");let key=format!("{b}:{revision}:{signature}");
  if let Some(mut value)=cache.lock().map_err(Error::internal)?.get(&key){value["cache"]["hit"]=json!(true);value["snapshot"]=json!(sync::cursor(&tx,&b)?);return Ok(value);}
  let mut value=calculate(&tx,&b,&q,start,end)?;value["method"]=json!(method);value["assumptions"].as_array_mut().unwrap().push(json!({"code":"method","message":if method=="tracking"{"Tracking budgets use tracking category plans and rollover flags. This report then applies its own source running-balance rules, so totals can differ from the budget summary."}else{"Envelope budgets use envelope category plans and rollover flags. The report excludes income categories and follows each category’s carryover."}}));value["cache"]=json!({"hit":false,"input_revision":revision});if value.to_string().len()>8*1024*1024{return Err(Error::invalid("Analysis exceeds8MiB; shorten the range or reduce categories"));}cache.lock().map_err(Error::internal)?.put(key,value.clone());Ok(value)
 })}).await?;
    Ok(Json(json!({"data":value})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Detail {
    query: Request,
    snapshot: String,
    month: String,
    metric: String,
    #[serde(default)]
    offset: usize,
    #[serde(default = "detail_limit")]
    limit: usize,
}
fn detail_limit() -> usize {
    50
}
async fn drilldown(
    State(store): State<Store>,
    Path(b): Path<String>,
    input: Result<Json<Detail>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    let Json(input) = input.map_err(crate::api::json_error)?;
    let (start, end) = validate(&input.query)?;
    let month = crate::budgeting::month(&input.month)?;
    if month < start
        || month > end
        || !matches!(input.metric.as_str(), "spent" | "budgeted")
        || input.limit == 0
        || input.limit > 200
        || input.offset > 100000
    {
        return Err(Error::invalid(
            "Choose an analysis month and spent/budgeted metric, limit1–200 and offset up to100000",
        ));
    }
    let value=store.run(move|db|{
        let tx=db.transaction()?;
        super::sql_work::run(&tx, || {
        let prefs=crate::preferences::get_preferences(&tx,&b)?;
        if prefs["experimental"]["budgetAnalysisReport"]!=true{return Err(Error::new(StatusCode::CONFLICT,"feature_disabled","Enable Budget analysis in experimental settings before inspecting it"));}
        sync::check_snapshot(&tx,&b,Some(&input.snapshot))?;
        let categories=selection(&tx,&b,&input.query.options)?;
        let planned=input.metric=="budgeted";
        let mut values=vec![SqlValue::Text(b.clone()),SqlValue::Text(if planned{input.month.clone()}else{format!("{}-01",input.month)})];
        if !planned{values.push(SqlValue::Text(format!("{}-31",input.month)));}
        if planned {let method:String=tx.query_row("SELECT budget_type FROM budgets WHERE id=?1",[&b],|r|r.get(0))?;values.push(SqlValue::Text(method));}
        let slots=categories.iter().filter(|category|!planned||!category.income_group).map(|category|{values.push(SqlValue::Text(category.id.clone()));format!("?{}",values.len())}).collect::<Vec<_>>();
        let selected=if slots.is_empty(){"NULL".into()}else{slots.join(",")};
        let (rows,count,total)=if planned {
            let sql=format!("SELECT cell.id,cell.amount,cell.category_id,c.name FROM budget_cells cell JOIN categories c ON c.budget_id=cell.budget_id AND c.id=cell.category_id WHERE cell.budget_id=?1 AND cell.month=?2 AND cell.method=?3 AND cell.tombstone=0 AND cell.amount<>0 AND cell.category_id IN ({selected}) ORDER BY cell.category_id,cell.id LIMIT 100001");
            let all=tx.prepare(&sql)?.query_map(params_from_iter(values.iter()),|r|Ok(json!({"id":r.get::<_,String>(0)?,"amount":r.get::<_,i64>(1)?,"category_id":r.get::<_,String>(2)?,"category_name":r.get::<_,String>(3)?,"month":input.month})))?.collect::<Result<Vec<_>,_>>()?;
            if all.len()>100000{return Err(Error::invalid("Analysis detail exceeds100000 planned inputs"));}
            let total=exact(all.iter().map(|row|row["amount"].as_i64().unwrap() as i128).sum())?;
            let count=all.len();
            (all.into_iter().skip(input.offset).take(input.limit).collect::<Vec<_>>(),count,total)
        } else {
            let sql=format!("SELECT t.id,t.amount FROM transactions t JOIN accounts a ON a.budget_id=t.budget_id AND a.id=t.account_id WHERE t.budget_id=?1 AND t.date>=?2 AND t.date<=?3 AND t.tombstone=0 AND t.is_parent=0 AND a.off_budget=0 AND t.category_id IN ({selected}) ORDER BY t.date DESC,t.sort_order DESC,t.id LIMIT 100001");
            let all=tx.prepare(&sql)?.query_map(params_from_iter(values.iter()),|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?)))?.collect::<Result<Vec<_>,_>>()?;
            if all.len()>100000{return Err(Error::invalid("Analysis detail exceeds100000 recorded inputs"));}
            let total=exact(all.iter().map(|(_,amount)|*amount as i128).sum())?;
            let count=all.len();
            let ids=all.into_iter().skip(input.offset).take(input.limit).map(|(id,_)|id).collect::<Vec<_>>();
            (crate::ledger::get_many(&tx,&b,&ids)?,count,total)
        };
        let response=json!({"rows":rows,"total_count":count,"total_amount":total,"offset":input.offset,"limit":input.limit,"has_more":input.offset+rows.len()<count,"snapshot":input.snapshot,"month":input.month,"metric":input.metric,"basis":if planned{"budget_allocations"}else{"recorded_entries"}});
        if response.to_string().len()>8*1024*1024{return Err(Error::invalid("Analysis detail exceeds8MiB; use a smaller page"));}
        Ok(response)
    })}).await?;
    Ok(Json(json!({"data":value})))
}
