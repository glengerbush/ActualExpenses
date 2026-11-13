//! Forecast reads are bound to one immutable input revision. Simulation never posts.
use super::revision_cache as cache;
use super::*;
use axum::Extension;
use std::sync::{Arc, Mutex};
type SharedCache = Arc<Mutex<cache::Cache>>;
pub(crate) fn router() -> Router<Store> {
    Router::new()
        .route("/api/v1/budgets/{budget}/reports/forecast", post(read))
        .layer(Extension(SharedCache::default()))
}
use std::collections::{BTreeMap, BTreeSet};
const UNASSIGNED: &str = "__unassigned_schedule__";
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    version: u8,
    as_of: String,
    options: Options,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Options {
    start_date: String,
    end_date: String,
    #[serde(default)]
    view_start: Option<String>,
    #[serde(default)]
    view_end: Option<String>,
    #[serde(default)]
    account_ids: Option<Vec<String>>,
    #[serde(default)]
    conditions: Vec<Value>,
    #[serde(default = "and")]
    conditions_op: String,
    #[serde(default = "schedules")]
    source: String,
    #[serde(default = "monthly")]
    granularity: String,
    #[serde(default)]
    include_accountless_schedules: bool,
}
fn and() -> String {
    "and".into()
}
fn schedules() -> String {
    "schedules".into()
}
fn monthly() -> String {
    "Monthly".into()
}
#[derive(Clone)]
struct Account {
    id: String,
    name: String,
    off: bool,
}
struct Posted {
    date: String,
    account: String,
    amount: i64,
    schedule: Option<String>,
}
struct Simulated {
    row: Value,
    schedule: String,
    name: String,
}
fn money(n: i128) -> Result<i64, Error> {
    if n.abs() > crate::models::MAX_MONEY as i128 {
        Err(Error::invalid("Forecast exceeds exact monetary range"))
    } else {
        Ok(n as i64)
    }
}
async fn read(
    State(store): State<Store>,
    Path(b): Path<String>,
    Extension(cache): Extension<SharedCache>,
    input: Result<Json<Request>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    let Json(request) = input.map_err(crate::api::json_error)?;
    if request.version != 1 {
        return Err(Error::invalid("Unsupported forecast version"));
    }
    let today = day(&request.as_of)?;
    let start = day(&request.options.start_date)?;
    let end = day(&request.options.end_date)?;
    if start > end {
        return Err(Error::invalid("Forecast end precedes start"));
    }
    if !["Daily", "Monthly"].contains(&request.options.granularity.as_str()) {
        return Err(Error::invalid(
            "Forecast granularity must be Daily or Monthly",
        ));
    }
    display_bounds(&request)?;
    if request
        .options
        .account_ids
        .as_ref()
        .is_some_and(|ids| ids.len() > 500)
        || request.options.conditions.len() > 20
    {
        return Err(Error::invalid(
            "Forecast permits500 accounts and20 conditions",
        ));
    }
    if !["and", "or"].contains(&request.options.conditions_op.as_str())
        || request.options.conditions.iter().any(|c| {
            !c.is_object()
                || !c["field"].is_string()
                || !c["op"].is_string()
                || c.get("value").is_none()
        })
    {
        return Err(Error::invalid("Invalid forecast conditions"));
    }
    if let Some(ids) = &request.options.account_ids {
        for id in ids {
            crate::models::validate_id(id)?;
        }
    }
    let signature = serde_json::to_string(&request).map_err(Error::internal)?;
    if signature.len() > 65536 {
        return Err(Error::invalid(
            "Forecast options exceed64KiB; reduce the filter size",
        ));
    }
    let value = store
        .run(move |db| {
            let tx = db.transaction()?;
            let prefs = crate::preferences::get_preferences(&tx, &b)?;
            if prefs["experimental"]["balanceForecastReport"] != true {
                return Err(Error::new(
                    StatusCode::CONFLICT,
                    "feature_disabled",
                    "Enable Balance forecast in experimental settings before calculating it",
                ));
            }
            let key = format!("{b}:{}:{signature}", sync::cursor(&tx, &b)?);
            if let Some(mut value) = cache.lock().map_err(Error::internal)?.get(&key) {
                value["cache"]["hit"] = json!(true);
                return Ok(value);
            }
            let value = crate::register::with_report_predicates(&tx, || {
                calculate(&tx, &b, &request, today, start, end)
            })?;
            cache
                .lock()
                .map_err(Error::internal)?
                .put(key, value.clone());
            Ok(value)
        })
        .await?;
    Ok(Json(json!({"data":value})))
}
fn filter_options(q: &Request) -> Result<Value, Error> {
    if q.options.conditions.len() > 20 || !["and", "or"].contains(&q.options.conditions_op.as_str())
    {
        return Err(Error::invalid(
            "Forecast filters permit20 conditions with and/or",
        ));
    }
    let mut conditions = vec![];
    for original in &q.options.conditions {
        if original.get("queryFilter").is_some_and(|v| !v.is_null()) {
            return Err(Error::invalid(
                "This saved forecast query filter is unsupported; revise it before calculating",
            ));
        }
        if truthy(&original["customName"]) {
            continue;
        }
        let mut condition = original.clone();
        if let Some(object) = condition.as_object_mut() {
            object.remove("customName");
        }
        conditions.push(condition);
    }
    Ok(json!({"conditions":conditions,"conditionsOp":q.options.conditions_op}))
}
fn accounts(db: &Connection, b: &str, q: &Request) -> Result<Vec<Account>, Error> {
    let mut values = vec![SqlValue::Text(b.into())];
    let mut constraints = vec!["a.budget_id=?1 AND a.tombstone=0".to_owned()];
    if let Some(ids) = &q.options.account_ids {
        if ids.len() > 500 {
            return Err(Error::invalid("Select at most500 forecast accounts"));
        }
        let mut slots = vec![];
        for id in ids {
            crate::models::validate_id(id)?;
            values.push(SqlValue::Text(id.clone()));
            slots.push(format!("?{}", values.len()));
        }
        constraints.push(if slots.is_empty() {
            "0".into()
        } else {
            format!("a.id IN({})", slots.join(","))
        });
    }
    let options = filter_options(q)?;
    let all = options["conditions"].as_array().unwrap();
    let account_conditions = all
        .iter()
        .filter(|c| c["field"] == "account")
        .cloned()
        .collect::<Vec<_>>();
    if !account_conditions.is_empty()
        && (account_conditions.len() == all.len() || q.options.conditions_op == "and")
    {
        let mut matches = vec![];
        for condition in account_conditions {
            let op = condition["op"].as_str().unwrap_or("");
            let mut expression = predicate(
                db,
                b,
                &json!({"conditions":[condition],"conditionsOp":"and"}),
                &mut values,
            )?
            .replace("t.account_id", "a.id");
            if matches!(op, "contains" | "doesNotContain") {
                let text = condition["value"]
                    .as_str()
                    .ok_or_else(|| Error::invalid("Account name condition requires text"))?;
                *values.last_mut().unwrap() = SqlValue::Text(text.to_lowercase());
                expression = format!(
                    "instr(register_lower(a.name),?{}){}0",
                    values.len(),
                    if op == "contains" { ">" } else { "=" }
                );
            } else if op == "matches" {
                expression = expression.replace("a.name", "register_lower(a.name)");
            }
            matches.push(expression);
        }
        constraints.push(format!(
            "({})",
            matches.join(if q.options.conditions_op == "or" {
                " OR "
            } else {
                " AND "
            })
        ));
    }
    let sql = format!(
        "SELECT a.id,a.name,a.off_budget FROM accounts a WHERE {} ORDER BY a.sort_order,a.name,a.id LIMIT 501",
        constraints.join(" AND ")
    );
    let mut selected = db
        .prepare(&sql)?
        .query_map(params_from_iter(values), |r| {
            Ok(Account {
                id: r.get(0)?,
                name: r.get(1)?,
                off: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if selected.len() > 500 {
        return Err(Error::invalid("Select at most500 forecast accounts"));
    }
    if !selected.is_empty() && q.options.include_accountless_schedules {
        selected.push(Account {
            id: UNASSIGNED.into(),
            name: "".into(),
            off: false,
        });
    }
    Ok(selected)
}
fn posted(
    db: &Connection,
    b: &str,
    q: &Request,
    accounts: &[Account],
) -> Result<Vec<Posted>, Error> {
    let options = filter_options(q)?;
    let mut values = vec![SqlValue::Text(b.into())];
    let filter = predicate(db, b, &options, &mut values)?;
    let account_clause = if accounts.is_empty() {
        "0".into()
    } else {
        let mut slots = vec![];
        for account in accounts {
            values.push(SqlValue::Text(account.id.clone()));
            slots.push(format!("?{}", values.len()));
        }
        format!("t.account_id IN({})", slots.join(","))
    };
    let sql = format!(
        "SELECT t.date,t.account_id,t.amount,link.schedule_id {JOINS} LEFT JOIN schedule_links link ON link.budget_id=t.budget_id AND link.id=t.id AND link.tombstone=0 WHERE t.budget_id=?1 AND t.tombstone=0 AND t.is_parent=0 AND ({filter}) AND ({account_clause}) ORDER BY t.date,t.id LIMIT 100001"
    );
    let rows = db
        .prepare(&sql)?
        .query_map(params_from_iter(values), |r| {
            Ok(Posted {
                date: r.get(0)?,
                account: r.get(1)?,
                amount: r.get(2)?,
                schedule: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if rows.len() > 100000 {
        return Err(Error::invalid(
            "Forecast exceeds100000 selected posted entries",
        ));
    }
    money(rows.iter().map(|r| (r.amount as i128).abs()).sum())?;
    Ok(rows)
}
fn simulate(
    db: &Connection,
    b: &str,
    q: &Request,
    accounts: &[Account],
    posted: &[Posted],
) -> Result<Vec<Simulated>, Error> {
    let mut work = crate::rules::Work::default();
    let raw = crate::schedules::forecast_inputs(db, b, &q.options.end_date, &mut work.remaining)?;
    if raw.is_empty() {
        return Ok(vec![]);
    }
    let sizes=db.prepare("SELECT length(CAST(conditions AS BLOB))+length(CAST(actions AS BLOB)) FROM rules WHERE budget_id=?1 AND tombstone=0 LIMIT 10001")?.query_map([b],|r|r.get::<_,i64>(0))?.collect::<Result<Vec<_>,_>>()?;
    if sizes.len() > 10000 || sizes.iter().sum::<i64>() > 8 * 1024 * 1024 {
        return Err(Error::invalid(
            "This rule set exceeds forecast limits. Review large or obsolete rules before calculating.",
        ));
    }
    let mut runner = crate::rules::Runner::with_work(crate::rules::load(db, b)?, work);
    let mut result = vec![];
    let paid = posted
        .iter()
        .filter_map(|p| p.schedule.as_ref().map(|s| (s.clone(), p.date.clone())))
        .fold(
            BTreeMap::<String, Vec<String>>::new(),
            |mut map, (id, date)| {
                map.entry(id).or_default().push(date);
                map
            },
        );
    for input in raw {
        let schedule = input["schedule_id"].as_str().unwrap();
        let name = input["schedule_name"].as_str().unwrap();
        let date = input["date"].as_str().unwrap();
        if input["account_id"].is_null() && !q.options.include_accountless_schedules {
            continue;
        }
        if paid.get(schedule).is_some_and(|dates| {
            dates.iter().any(|date| {
                date.as_str() >= input["posted_from"].as_str().unwrap()
                    && date.as_str() <= input["date"].as_str().unwrap()
            })
        }) {
            continue;
        }
        let id = format!("forecast-{schedule}-{date}");
        let base = json!({"id":id,"account_id":input["account_id"].as_str().unwrap_or(UNASSIGNED),"amount":input["amount"],"date":date,"payee_id":input["payee_id"],"category_id":null,"schedule_id":schedule,"cleared":false,"reconciled":false,"is_parent":false,"parent_id":null,"transfer_id":null,"notes":"","imported_payee":null});
        let (mut source, _) = runner.evaluate_with_account_context(
            db,
            b,
            &base,
            (base["account_id"] == UNASSIGNED).then_some(false),
        )?;
        let destination=source["payee_id"].as_str().map(|id|db.query_row("SELECT transfer_account_id FROM payees WHERE budget_id=?1 AND id=?2 AND tombstone=0",params![b,id],|r|r.get::<_,Option<String>>(0)).optional()).transpose()?.flatten().flatten();
        if let Some(destination) = destination
            && source["account_id"] != UNASSIGNED
            && source["account_id"] != destination
        {
            let source_account = source["account_id"].as_str().unwrap();
            let reverse=db.query_row("SELECT id FROM payees WHERE budget_id=?1 AND transfer_account_id=?2 AND tombstone=0 ORDER BY id LIMIT 1",params![b,source_account],|r|r.get::<_,String>(0)).optional()?;
            let peer_id = format!("{id}-transfer");
            let peer = json!({"id":peer_id,"account_id":destination,"amount":-source["amount"].as_i64().ok_or_else(||Error::invalid("Forecast rule did not produce an exact amount"))?,"payee_id":reverse,"date":source["date"],"transfer_id":id,"notes":source["notes"],"schedule_id":source["schedule_id"],"category_id":null,"cleared":false,"reconciled":false,"is_parent":false,"parent_id":null,"imported_payee":null});
            let (mut peer, _) = runner.evaluate(db, b, &peer)?;
            if accounts
                .iter()
                .find(|a| source["account_id"] == a.id)
                .zip(accounts.iter().find(|a| peer["account_id"] == a.id))
                .is_some_and(|(a, c)| a.off == c.off)
            {
                source["category_id"] = Value::Null;
                peer["category_id"] = Value::Null;
            }
            source["transfer_id"] = json!(peer_id);
            result.push(Simulated {
                row: source,
                schedule: schedule.into(),
                name: name.into(),
            });
            result.push(Simulated {
                row: peer,
                schedule: schedule.into(),
                name: name.into(),
            });
        } else {
            result.push(Simulated {
                row: source,
                schedule: schedule.into(),
                name: name.into(),
            });
        }
    }
    if result.len() > 20000 {
        return Err(Error::invalid(
            "Forecast exceeds20000 simulated transfer legs",
        ));
    }
    money(
        result
            .iter()
            .map(|r| {
                r.row["amount"]
                    .as_i64()
                    .map(|n| (n as i128).abs())
                    .ok_or_else(|| Error::invalid("Forecast rule did not produce an exact amount"))
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .sum(),
    )?;
    Ok(result)
}
fn sql_value(value: &Value) -> Result<SqlValue, Error> {
    Ok(match value {
        Value::Null => SqlValue::Null,
        Value::Bool(v) => SqlValue::Integer(i64::from(*v)),
        Value::Number(v) => SqlValue::Integer(
            v.as_i64()
                .ok_or_else(|| Error::invalid("Forecast requires exact monetary input"))?,
        ),
        Value::String(v) => SqlValue::Text(v.clone()),
        _ => {
            return Err(Error::invalid(
                "Forecast rule produced an unsupported value",
            ));
        }
    })
}
fn filter_simulations(
    db: &Connection,
    b: &str,
    q: &Request,
    simulations: Vec<Simulated>,
) -> Result<Vec<Simulated>, Error> {
    let options = filter_options(q)?;
    if options["conditions"].as_array().unwrap().is_empty() {
        return Ok(simulations);
    }
    // Bind virtual rows into the same validated SQL predicates as recorded rows.
    // The relation is transient SELECT data; no rows or references are inserted.
    let fields = [
        "id",
        "date",
        "account_id",
        "amount",
        "payee_id",
        "category_id",
        "notes",
        "cleared",
        "reconciled",
        "is_parent",
        "parent_id",
        "transfer_id",
        "imported_payee",
    ];
    let mut values = vec![SqlValue::Text(b.into())];
    values.extend(fields.iter().map(|_| SqlValue::Null));
    let mut clauses = vec![];
    for condition in options["conditions"].as_array().unwrap() {
        let clause = predicate(
            db,
            b,
            &json!({"conditions":[condition],"conditionsOp":"and"}),
            &mut values,
        )?;
        if matches!(
            condition["op"].as_str(),
            Some("contains" | "doesNotContain")
        ) {
            let text = condition["value"]
                .as_str()
                .ok_or_else(|| Error::invalid("Forecast text condition requires text"))?;
            // Source simulated $like anchors the whole value, treats '?' literally,
            // and expands every '%' even after a literal backslash.
            *values.last_mut().unwrap() = SqlValue::Text(format!(
                "^.*{}.*$",
                crate::register_regex::escape(text).replace('%', ".*")
            ));
        }
        clauses.push(clause);
    }
    let condition = format!(
        "({})",
        clauses.join(if q.options.conditions_op == "or" {
            " OR "
        } else {
            " AND "
        })
    );
    let columns = fields
        .iter()
        .enumerate()
        .map(|(i, field)| format!("?{} AS {field}", i + 2))
        .collect::<Vec<_>>()
        .join(",");
    let joins=JOINS.replace("FROM transactions t",&format!("FROM (SELECT ?1 AS budget_id,{columns}) t")).replace("JOIN accounts a ON",&format!("JOIN (SELECT budget_id,id,name,off_budget FROM accounts UNION ALL SELECT ?1,'{UNASSIGNED}','',0) a ON"));
    let mut statement = db.prepare(&format!(
        "SELECT CASE WHEN ({condition}) THEN 1 ELSE 0 END {joins}"
    ))?;
    let mut result = vec![];
    for simulation in simulations {
        for (i, field) in fields.iter().enumerate() {
            values[i + 1] = sql_value(&simulation.row[*field])?;
        }
        let matches: bool = statement.query_row(params_from_iter(values.iter()), |r| r.get(0))?;
        if matches {
            result.push(simulation);
        }
    }
    Ok(result)
}
fn display_bounds(q: &Request) -> Result<(NaiveDate, NaiveDate), Error> {
    let convert = |value: &str, last: bool| -> Result<NaiveDate, Error> {
        let date = if value.len() == 7 {
            first(value)?
        } else {
            day(value)?
        };
        Ok(if q.options.granularity == "Monthly" || value.len() == 7 {
            if last {
                month_end(date)
            } else {
                date.with_day(1).unwrap()
            }
        } else {
            date
        })
    };
    let start = convert(
        q.options
            .view_start
            .as_deref()
            .unwrap_or(&q.options.start_date),
        false,
    )?;
    let end = convert(
        q.options.view_end.as_deref().unwrap_or(&q.options.end_date),
        true,
    )?;
    if start > end {
        return Err(Error::invalid("Forecast view end precedes its start"));
    }
    let raw_start = day(&q.options.start_date)?;
    let raw_end = day(&q.options.end_date)?;
    if q.options.granularity == "Daily" && (start < raw_start || end > raw_end)
        || q.options.granularity == "Monthly"
            && (start < raw_start.with_day(1).unwrap() || end > month_end(raw_end))
    {
        return Err(Error::invalid(
            "Forecast view must fit inside the calculated date range",
        ));
    }
    Ok((start, end))
}
fn displayed(q: &Request, result: &Value) -> Result<(Vec<Value>, usize, Option<Value>), Error> {
    let (start, end) = display_bounds(q)?;
    let points = result["dataPoints"]
        .as_array()
        .ok_or_else(|| Error::internal("Forecast points unavailable"))?;
    if points.is_empty() {
        return Ok((vec![], 0, None));
    }
    let start_text = start.to_string();
    let end_text = end.to_string();
    let mut combined = BTreeMap::<String, i128>::new();
    let mut occurrences = BTreeSet::new();
    for point in points {
        let date = point["date"].as_str().unwrap();
        *combined.entry(date.into()).or_default() += point["balance"].as_i64().unwrap() as i128;
        if date >= start_text.as_str() && date <= end_text.as_str() {
            for transaction in point["transactions"].as_array().unwrap() {
                occurrences.insert((date.to_owned(), transaction["scheduleId"].to_string()));
            }
        }
    }
    let mut chart = vec![];
    if q.options.granularity == "Daily" {
        let mut balance = combined
            .range(..start.to_string())
            .next_back()
            .map(|(_, amount)| *amount)
            .unwrap_or(0);
        let mut date = start;
        while date <= end {
            let label = date.to_string();
            if let Some(amount) = combined.get(&label) {
                balance = *amount;
            }
            chart.push(json!({"date":label,"balance":money(balance)?}));
            date += Duration::days(1);
        }
    } else {
        let mut by_month = BTreeMap::new();
        for (date, amount) in combined {
            by_month.insert(date[..7].to_owned(), amount);
        }
        let mut month = start.with_day(1).unwrap();
        let mut balance = 0;
        while month <= end {
            let label = month.format("%Y-%m").to_string();
            if let Some(amount) = by_month.get(&label) {
                balance = *amount;
            }
            chart.push(json!({"date":label,"balance":money(balance)?}));
            month = month
                .checked_add_months(chrono::Months::new(1))
                .ok_or_else(|| Error::invalid("Forecast view exceeds calendar"))?;
        }
    }
    let low = chart
        .iter()
        .min_by_key(|p| p["balance"].as_i64().unwrap())
        .cloned();
    Ok((chart, occurrences.len(), low))
}
fn response(
    db: &Connection,
    b: &str,
    q: &Request,
    result: Value,
    mut assumptions: Vec<Value>,
) -> Result<Value, Error> {
    assumptions.insert(0,json!({"code":"estimate","message":"Estimated balances are not recorded account balances. No entries are posted and no schedules are advanced."}));
    let (chart, occurrences, lowest_displayed) = displayed(q, &result)?;
    let snapshot = sync::cursor(db, b)?;
    let value = json!({"result":result,"chart":chart,"lowest_displayed":lowest_displayed,"scheduled_occurrence_count":occurrences,"assumptions":assumptions,"recorded":false,"snapshot":snapshot,"epoch":crate::lifecycle::epoch(db,b)?,"as_of":q.as_of,"cache":{"hit":false,"input_revision":snapshot}});
    if value.to_string().len() > 8 * 1024 * 1024 {
        return Err(Error::invalid(
            "Forecast exceeds8MiB; narrow accounts or dates",
        ));
    }
    Ok(value)
}
fn tracking_plans(
    db: &Connection,
    b: &str,
    start: &str,
    end: &str,
) -> Result<BTreeMap<String, (i128, i128)>, Error> {
    let first_income:Option<String>=db.query_row("SELECT id FROM category_groups WHERE budget_id=?1 AND tombstone=0 AND is_income=1 ORDER BY sort_order,id LIMIT 1",[b],|r|r.get(0)).optional()?;
    let mut statement=db.prepare("SELECT cell.month,cell.amount,c.is_income,c.group_id,g.id,coalesce(g.is_income,0),0 FROM budget_cells cell JOIN categories c ON c.budget_id=cell.budget_id AND c.id=cell.category_id AND c.tombstone=0 LEFT JOIN category_groups g ON g.budget_id=c.budget_id AND g.id=c.group_id AND g.tombstone=0 WHERE cell.budget_id=?1 AND cell.method='tracking' AND cell.month>=?2 AND cell.month<=?3 AND cell.tombstone=0 AND cell.amount<>0 ORDER BY cell.month,cell.id LIMIT 100001")?;
    let mut rows = statement.query(params![b, start, end])?;
    let mut totals = BTreeMap::<String, (i128, i128)>::new();
    let mut count = 0usize;
    let mut flow = 0i128;
    while let Some(row) = rows.next()? {
        count += 1;
        if count > 100000 {
            return Err(Error::invalid(
                "Forecast exceeds100000 planned allocations; narrow dates",
            ));
        }
        let month: String = row.get(0)?;
        let amount: i64 = row.get(1)?;
        flow += (amount as i128).abs();
        money(flow)?;
        let category_income: bool = row.get(2)?;
        let group: Option<String> = row.get(3)?;
        let found: Option<String> = row.get(4)?;
        let group_income: bool = row.get(5)?;
        if group.is_some() && found.is_none() {
            return Err(Error::invalid(
                "A planned category has an unavailable group. Review categories or repair data before forecasting.",
            ));
        }
        let total = totals.entry(month).or_default();
        if group.is_none() {
            if category_income {
                total.0 += amount as i128;
            } else {
                total.1 += amount as i128;
            }
        } else if group_income {
            if group == first_income {
                total.0 += amount as i128;
            }
        } else {
            total.1 += amount as i128;
        }
    }
    Ok(totals)
}
fn tracking(
    db: &Connection,
    b: &str,
    q: &Request,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Value, Error> {
    if q.options.granularity != "Monthly" {
        return Err(Error::invalid(
            "Tracking-budget forecasts use monthly granularity; choose Monthly",
        ));
    }
    let months =
        ((end.year() - start.year()) * 12 + end.month() as i32 - start.month() as i32 + 1) as usize;
    if months > 12 {
        return Err(Error::invalid(
            "Tracking-budget forecasts support at most12 months; shorten the date range",
        ));
    }
    let plans = tracking_plans(db, b, &q.options.start_date[..7], &q.options.end_date[..7])?;
    let recorded = rows(
        db,
        b,
        &json!({"conditions":[{"field":"account","op":"onBudget","value":null}]}),
        false,
    )?;
    let mut balance = recorded.iter().map(|r| r.amount as i128).sum::<i128>();
    money(balance)?;
    let mut points = vec![];
    let mut lowest: Option<Value> = None;
    let mut month = start.with_day(1).unwrap();
    for _ in 0..months {
        let label = month.format("%Y-%m").to_string();
        let (income, expense) = plans.get(&label).copied().unwrap_or_default();
        balance += income - expense;
        let amount = money(balance)?;
        let date = month_end(month).to_string();
        let point = json!({"date":date,"balance":amount,"accountId":"tracking-budget","accountName":"Tracking Budget","transactions":[]});
        if lowest
            .as_ref()
            .is_none_or(|p| amount < p["balance"].as_i64().unwrap())
        {
            lowest = Some(
                json!({"date":date,"balance":amount,"accountId":"tracking-budget","accountName":"Tracking Budget"}),
            );
        }
        points.push(point);
        month = month
            .checked_add_months(chrono::Months::new(1))
            .ok_or_else(|| Error::invalid("Forecast dates exceed supported calendar"))?;
    }
    response(
        db,
        b,
        q,
        json!({"dataPoints":points,"lowestBalance":lowest,"forecastStartDate":q.options.start_date,"forecastEndDate":q.options.end_date}),
        vec![
            json!({"code":"tracking_monthly","message":"Tracking budget estimates use whole months: planned income minus planned expenses. Account selection, report filters and schedules are ignored. Category-group membership determines income or expenses; ungrouped categories use their own setting."}),
            json!({"code":"tracking_start","message":"The starting amount includes all recorded on-budget balances, including future-dated entries. Planning is limited to12 months."}),
        ],
    )
}
fn calculate(
    db: &Connection,
    b: &str,
    q: &Request,
    today: NaiveDate,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Value, Error> {
    if q.options.source == "tracking-budget" {
        return tracking(db, b, q, start, end);
    }
    if q.options.source != "schedules" {
        return Err(Error::invalid(
            "Choose scheduled transactions or Tracking Budget as forecast source",
        ));
    }
    let accounts = accounts(db, b, q)?;
    if accounts.is_empty() {
        return response(
            db,
            b,
            q,
            json!({"dataPoints":[],"lowestBalance":{"date":q.options.start_date,"balance":0,"accountId":"","accountName":""},"forecastStartDate":q.options.start_date,"forecastEndDate":q.options.end_date}),
            vec![],
        );
    }
    let days = (end - start).num_days() as usize + 1;
    if days.saturating_mul(accounts.len().max(1)) > 20000 {
        return Err(Error::invalid(
            "Forecast contains more than20000 daily account balances. Select fewer accounts or a shorter date range.",
        ));
    }
    let posted = posted(db, b, q, &accounts)?;
    let simulations = simulate(db, b, q, &accounts, &posted)?;
    let simulations = filter_simulations(db, b, q, simulations)?;
    let first_forecast = if end < today { start } else { start.max(today) }.to_string();
    let mut points = vec![];
    let mut combined = BTreeMap::<String, i128>::new();
    let mut posted_by_account = BTreeMap::<String, (i128, BTreeMap<String, i128>)>::new();
    for p in posted {
        let entry = posted_by_account.entry(p.account).or_default();
        if p.date < q.options.start_date {
            entry.0 += p.amount as i128;
        } else {
            *entry.1.entry(p.date).or_default() += p.amount as i128;
        }
    }
    let mut by_account_date = BTreeMap::<(String, String), Vec<Simulated>>::new();
    for simulated in simulations {
        let date = simulated.row["date"]
            .as_str()
            .ok_or_else(|| Error::invalid("Forecast rule produced an invalid date"))?;
        day(date)?;
        if date >= first_forecast.as_str() && date <= q.options.end_date.as_str() {
            by_account_date
                .entry((
                    simulated.row["account_id"]
                        .as_str()
                        .ok_or_else(|| Error::invalid("Forecast rule produced no account"))?
                        .into(),
                    date.into(),
                ))
                .or_default()
                .push(simulated);
        }
    }
    for account in accounts {
        let (mut balance, deltas) = posted_by_account.remove(&account.id).unwrap_or_default();
        money(balance)?;
        for offset in 0..days {
            let date = (start + Duration::days(offset as i64)).to_string();
            balance += deltas.get(&date).copied().unwrap_or(0);
            let mut transactions = vec![];
            for simulated in by_account_date
                .remove(&(account.id.clone(), date.clone()))
                .unwrap_or_default()
            {
                let amount = simulated.row["amount"].as_i64().unwrap();
                balance += amount as i128;
                let payee=simulated.row["payee_id"].as_str().map(|id|db.query_row("SELECT coalesce(a.name,p.name) FROM payees p LEFT JOIN accounts a ON a.budget_id=p.budget_id AND a.id=p.transfer_account_id WHERE p.budget_id=?1 AND p.id=?2 AND p.tombstone=0",params![b,id],|r|r.get::<_,String>(0)).optional()).transpose()?.flatten().unwrap_or_else(||simulated.name.clone());
                transactions.push(json!({"amount":amount,"payee":payee,"scheduleId":simulated.schedule,"scheduleName":simulated.name}));
            }
            let value = money(balance)?;
            *combined.entry(date.clone()).or_default() += balance;
            points.push(json!({"date":date,"balance":value,"accountId":account.id,"accountName":account.name,"transactions":transactions}));
        }
    }
    points.sort_by(|a, b| a["date"].as_str().cmp(&b["date"].as_str()));
    let low = combined.iter().min_by_key(|(_, amount)| *amount);
    let lowest = json!({"date":low.map(|(date,_)|date.as_str()).unwrap_or(&q.options.start_date),"balance":money(low.map(|(_,amount)|*amount).unwrap_or(0))?,"accountId":"","accountName":""});

    response(
        db,
        b,
        q,
        json!({"dataPoints":points,"lowestBalance":lowest,"forecastStartDate":q.options.start_date,"forecastEndDate":q.options.end_date}),
        vec![
            json!({"code":"schedule_cutoff","message":format!("Recorded entries follow the selected account/transaction filters. Schedule simulation starts on {first_forecast}; already-paid occurrences are excluded.")}),
            json!({"code":"unassigned","message":if q.options.include_accountless_schedules{"Unassigned schedules are included as a separate combined-budget estimate; they are not assigned to a real account."}else{"Schedules without an account are excluded."}}),
        ],
    )
}
