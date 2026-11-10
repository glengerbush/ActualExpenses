mod analysis;
mod forecast;
mod revision_cache;
use super::*;
mod custom;
pub(super) use analysis::router as analysis_router;
use chrono::{Datelike, Duration, NaiveDate};
pub(super) use forecast::router as forecast_router;
use rusqlite::{params_from_iter, types::Value as SqlValue};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    version: u8,
    kind: String,
    as_of: String,
    options: Value,
}
fn day(text: &str) -> Result<NaiveDate, Error> {
    {
        crate::models::validate_date(text)?;
        NaiveDate::parse_from_str(text, "%Y-%m-%d")
    }
    .map_err(|_| Error::invalid("Expected ISO date"))
}
fn first(text: &str) -> Result<NaiveDate, Error> {
    if ![7, 10].contains(&text.len()) {
        return Err(Error::invalid("Expected year-month or full ISO date"));
    }
    if text.len() == 10 {
        day(text)?;
    }
    let month = text
        .get(..7)
        .ok_or_else(|| Error::invalid("Expected year-month"))?;
    day(&format!("{month}-01"))
}
fn month_end(d: NaiveDate) -> NaiveDate {
    d.with_day(1)
        .unwrap()
        .checked_add_months(chrono::Months::new(1))
        .unwrap()
        - Duration::days(1)
}
fn text<'a>(v: &'a Value, key: &str, default: &'a str) -> &'a str {
    v[key].as_str().unwrap_or(default)
}
fn predicate(
    db: &Connection,
    b: &str,
    o: &Value,
    values: &mut Vec<SqlValue>,
) -> Result<String, Error> {
    if !o["conditions"].is_null() && !o["conditions"].is_array() {
        return Err(Error::invalid("Report conditions must be an array"));
    }
    if !o["conditionsOp"].is_null() && !matches!(o["conditionsOp"].as_str(), Some("and" | "or")) {
        return Err(Error::invalid("Report conditions require and/or"));
    }
    let mut clauses = vec![];
    for c in o["conditions"].as_array().into_iter().flatten() {
        if !c.is_object()
            || !c["field"].is_string()
            || !c["op"].is_string()
            || c.get("value").is_none()
        {
            return Err(Error::invalid("Invalid report condition"));
        }
        if c.get("customName").is_some() {
            continue;
        }
        let field = text(c, "field", "");
        let op = text(c, "op", "");
        if field == "account" && ["onBudget", "offBudget"].contains(&op) {
            clauses.push(format!(
                "a.off_budget={}",
                if op == "onBudget" { 0 } else { 1 }
            ));
            continue;
        }
        let op = match op {
            "is" => "eq",
            "isNot" => "ne",
            "oneOf" => "in",
            "notOneOf" => "not_in",
            "contains" => "contains",
            "doesNotContain" => "not_contains",
            s => s,
        };
        let mut field = if field == "categoryGroup" {
            "category_group".to_string()
        } else {
            field.to_string()
        };
        if ["contains", "not_contains", "matches"].contains(&op)
            && ["account", "payee", "category", "category_group"].contains(&field.as_str())
        {
            field += "_name";
        }
        let mut value = c["value"].clone();
        if field == "date"
            && let Some(number) = value.as_i64()
        {
            let text = number.to_string();
            if text.len() == 8 {
                value = json!(format!("{}-{}-{}", &text[..4], &text[4..6], &text[6..]));
            }
        }
        let condition=serde_json::from_value(json!({"field":field,"op":op,"value":value,"options":c.get("options").filter(|v|!v.is_null()).cloned().unwrap_or(json!({}))})).map_err(|_|Error::invalid("Invalid reporting condition"))?;
        clauses.push(crate::register::report_predicate(
            db,
            b,
            crate::register::Filter {
                conditions: vec![condition],
                ..Default::default()
            },
            values,
        )?);
    }
    if clauses.len() > 20 {
        return Err(Error::invalid("Maximum20 report conditions"));
    }
    Ok(if clauses.is_empty() {
        "1".into()
    } else {
        format!(
            "({})",
            clauses.join(if o["conditionsOp"] == "or" {
                " OR "
            } else {
                " AND "
            })
        )
    })
}
#[derive(Clone)]
struct Row {
    id: String,
    date: NaiveDate,
    amount: i64,
    account: String,
    account_name: String,
    off: bool,
    payee: Option<String>,
    payee_name: String,
    category: Option<String>,
    group: Option<String>,
    hidden: bool,
    category_name: String,
    income: bool,
    transfer: Option<String>,
    peer_date: Option<NaiveDate>,
    peer_account: Option<String>,
    notes: String,
}
const JOINS: &str = "FROM transactions t JOIN accounts a ON a.budget_id=t.budget_id AND a.id=t.account_id LEFT JOIN payees p ON p.budget_id=t.budget_id AND p.id=t.payee_id AND p.tombstone=0 LEFT JOIN accounts y ON y.budget_id=t.budget_id AND y.id=p.transfer_account_id LEFT JOIN categories c ON c.budget_id=t.budget_id AND c.id=t.category_id LEFT JOIN category_groups g ON g.budget_id=c.budget_id AND g.id=c.group_id LEFT JOIN transactions peer ON peer.budget_id=t.budget_id AND peer.id=t.transfer_id AND peer.tombstone=0";
fn rows(db: &Connection, b: &str, o: &Value, parent: bool) -> Result<Vec<Row>, Error> {
    let mut values = vec![SqlValue::Text(b.into())];
    let filter = predicate(db, b, o, &mut values)?;
    let sql = format!(
        "SELECT t.id,t.date,t.amount,t.account_id,a.name,a.off_budget,t.payee_id,coalesce(y.name,p.name,''),t.category_id,coalesce(c.name,''),c.group_id,coalesce(g.name,''),coalesce(c.hidden,0) OR coalesce(g.hidden,0),p.transfer_account_id,peer.date,peer.account_id,t.notes,coalesce(c.is_income,0) {JOINS} WHERE t.budget_id=?1 AND t.tombstone=0 AND {} AND ({filter}) ORDER BY t.date,t.id LIMIT 100001",
        if parent {
            "t.parent_id IS NULL"
        } else {
            "t.is_parent=0"
        }
    );
    let data = db
        .prepare(&sql)?
        .query_map(params_from_iter(values), |r| {
            Ok(Row {
                id: r.get(0)?,
                date: NaiveDate::parse_from_str(&r.get::<_, String>(1)?, "%Y-%m-%d").unwrap(),
                amount: r.get(2)?,
                account: r.get(3)?,
                account_name: r.get(4)?,
                off: r.get(5)?,
                payee_name: r.get(7)?,
                payee: r.get(6)?,
                category: r.get(8)?,
                group: r.get(10)?,
                hidden: r.get(12)?,
                category_name: r.get(9)?,
                transfer: r.get(13)?,
                peer_date: r
                    .get::<_, Option<String>>(14)?
                    .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()),
                peer_account: r.get(15)?,
                notes: r.get(16)?,
                income: r.get(17)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if data.len() > 100000 {
        return Err(Error::invalid(
            "Reporting scan exceeds100000 selected entries; narrow conditions",
        ));
    }
    if data.iter().map(|r| (r.amount as i128).abs()).sum::<i128>()
        > crate::models::MAX_MONEY as i128
    {
        return Err(Error::invalid(
            "Selected report flow exceeds exact money range; narrow conditions",
        ));
    }
    Ok(data)
}
fn work_bound(rows: usize, buckets: usize) -> Result<(), Error> {
    if rows.saturating_mul(buckets) > 50_000_000 {
        return Err(Error::invalid(
            "Report row/bucket work exceeds50000000; narrow conditions or reduce output intervals",
        ));
    }
    Ok(())
}
fn interval_bound(
    start: NaiveDate,
    end: NaiveDate,
    interval: &str,
    rows: usize,
) -> Result<(), Error> {
    if start > end {
        return Err(Error::invalid("Report start must not follow its end"));
    }
    let count = match interval {
        "Daily" => (end - start).num_days() + 1,
        "Weekly" => (end - start).num_days() / 7 + 1,
        "Yearly" => (end.year() - start.year() + 1) as i64,
        _ => {
            ((end.year() - start.year()) * 12 + end.month() as i32 - start.month() as i32 + 1)
                as i64
        }
    };
    let maximum = if interval == "Daily" { 3660 } else { 1200 };
    if count > maximum {
        return Err(Error::invalid(format!(
            "Report exceeds{maximum} output intervals; choose a coarser interval or shorten dates"
        )));
    }
    work_bound(rows, count as usize)
}
fn cash_flow(data: &[Row], o: &Value, today: NaiveDate) -> Result<Value, Error> {
    let start = first(text(o, "start", ""))?;
    let end = month_end(first(text(o, "end", ""))?).min(today);
    let concise = o["isConcise"] == true;
    interval_bound(
        start,
        end,
        if concise { "Monthly" } else { "Daily" },
        data.len(),
    )?;
    // Actual26.10 filters the query by transformed month, then plots only through
    // as-of. Consequently concise monthly buckets include later recorded entries.
    let selected: Vec<_> = data.iter().filter(|r| !r.off).collect();
    let mut balance: i64 = selected
        .iter()
        .filter(|r| r.date < start)
        .map(|r| r.amount)
        .sum();
    let (mut income, mut expense, mut transfers) = (0, 0, 0);
    let mut points = [vec![], vec![], vec![], vec![]];
    let mut cursor = start;
    while cursor <= end {
        let next = if concise {
            cursor.checked_add_months(chrono::Months::new(1)).unwrap()
        } else {
            cursor + Duration::days(1)
        };
        let (mut i, mut e, mut t) = (0, 0, 0);
        for r in selected
            .iter()
            .filter(|r| r.date >= cursor && r.date < next)
        {
            if r.transfer.is_some() {
                t += r.amount;
            } else if r.amount > 0 {
                i += r.amount;
            } else {
                e += r.amount;
            }
        }
        income += i;
        expense += e;
        transfers += t;
        balance += i + e + t;
        let x = format!("{cursor}T00:00:00.000Z");
        for (index, value) in [i, e, t, balance].iter().enumerate() {
            points[index].push(if index == 3 {
                json!({"x":x,"y":value,"amount":value,"premadeLabel":null})
            } else {
                json!({"x":x,"y":value})
            });
        }
        cursor = next;
    }
    let change = balance - points[3][0]["amount"].as_i64().unwrap();
    Ok(
        json!({"graphData":{"income":points[0],"expenses":points[1],"transfers":points[2],"balances":points[3]},"balance":balance,"totalIncome":income,"totalExpenses":expense,"totalTransfers":transfers,"totalChange":change}),
    )
}
pub async fn read(
    State(store): State<Store>,
    Path(b): Path<String>,
    input: Result<Json<Query>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    let Json(q) = input.map_err(crate::api::json_error)?;
    if q.version != 1 || !q.options.is_object() {
        return Err(Error::invalid("Unsupported report query version/config"));
    }
    let today = day(&q.as_of)?;
    let value=store.run(move|db|{let tx=db.transaction()?;crate::register::with_report_predicates(&tx, || {store::require_budget(&tx,&b)?;let(result,cells,_)=evaluate(&tx,&b,&q,today)?;let basis=if matches!(q.kind.as_str(),"range"|"widget-range"){"date_resolution"}else if q.kind=="custom"&&q.options["balanceTypeOp"]=="totalBudgeted"{"budget_allocations"}else{"recorded_transactions"};Ok(json!({"result":result,"cells":cells,"snapshot":sync::cursor(&tx,&b)?,"recorded":basis=="recorded_transactions","basis":basis,"as_of":q.as_of}))})}).await?;
    Ok(Json(json!({"data":value})))
}

fn week(d: NaiveDate, first: u32) -> NaiveDate {
    d - Duration::days(((d.weekday().num_days_from_sunday() + 7 - first) % 7) as i64)
}
fn key(d: NaiveDate, interval: &str, first: u32) -> NaiveDate {
    match interval {
        "Daily" => d,
        "Weekly" => week(d, first),
        "Yearly" => d.with_month(1).unwrap().with_day(1).unwrap(),
        _ => d.with_day(1).unwrap(),
    }
}
fn next(d: NaiveDate, interval: &str) -> NaiveDate {
    match interval {
        "Daily" => d + Duration::days(1),
        "Weekly" => d + Duration::days(7),
        "Yearly" => d.checked_add_months(chrono::Months::new(12)).unwrap(),
        _ => d.checked_add_months(chrono::Months::new(1)).unwrap(),
    }
}
fn net_worth(
    db: &Connection,
    b: &str,
    data: &[Row],
    o: &Value,
    today: NaiveDate,
) -> Result<Value, Error> {
    let interval = text(o, "interval", "Monthly");
    if !["Daily", "Weekly", "Monthly", "Yearly"].contains(&interval) {
        return Err(Error::invalid("Invalid report interval"));
    }
    let first_day = text(o, "first_day_of_week", "1")
        .parse::<u32>()
        .ok()
        .filter(|n| *n < 7)
        .ok_or_else(|| Error::invalid("Invalid first weekday"))?;
    let range = first(text(o, "start", ""))?;
    let end = month_end(first(text(o, "end", ""))?);
    let end = if ["Daily", "Weekly"].contains(&interval) {
        end.min(today)
    } else {
        end
    };
    let mut start = if interval == "Daily" {
        range - Duration::days(1)
    } else if interval == "Weekly" {
        week(range - Duration::days(1), first_day)
    } else {
        range.checked_sub_months(chrono::Months::new(1)).unwrap()
    };
    let earliest: Option<String> = db.query_row(
        "SELECT min(date) FROM transactions WHERE budget_id=?1 AND tombstone=0",
        [b],
        |r| r.get(0),
    )?;
    if let Some(earliest) = earliest
        && day(&earliest)? >= range
    {
        start = if ["Daily", "Weekly"].contains(&interval) {
            key(day(&earliest)?, interval, first_day)
        } else {
            range
        };
    }
    interval_bound(start, end, interval, data.len())?;
    let ids: Vec<String> = if let Some(ids) = o["accounts"].as_array() {
        if ids.len() > 500 {
            return Err(Error::invalid("Maximum500 accounts"));
        }
        ids.iter()
            .map(|v| {
                let id = v
                    .as_str()
                    .ok_or_else(|| Error::invalid("Invalid account ID"))?;
                crate::ledger::account_exists(db, b, id)?;
                Ok(id.into())
            })
            .collect::<Result<_, Error>>()?
    } else {
        db.prepare("SELECT id FROM accounts WHERE budget_id=?1 AND tombstone=0 ORDER BY name,id")?
            .query_map([b], |r| r.get(0))?
            .collect::<Result<_, _>>()?
    };
    let mut selected = data
        .iter()
        .filter(|r| ids.contains(&r.account))
        .cloned()
        .collect::<Vec<_>>();
    for r in &mut selected {
        if r.peer_account.as_ref().is_some_and(|id| ids.contains(id))
            && let Some(peer) = r.peer_date
            && peer > r.date
        {
            r.date = peer;
        }
    }
    let mut balances = std::collections::BTreeMap::new();
    let mut nonzero = BTreeSet::new();
    for id in &ids {
        balances.insert(
            id.clone(),
            selected
                .iter()
                .filter(|r| &r.account == id && r.date < start)
                .map(|r| r.amount)
                .sum::<i64>(),
        );
    }
    let prior: i64 = balances.values().sum();
    let mut points = vec![];
    let mut cursor = key(start, interval, first_day);
    let (mut low, mut high) = (i64::MAX, i64::MIN);
    let mut previous = prior;
    while cursor <= end {
        let n = next(cursor, interval);
        if (points.len() + 1) * (ids.len() + 4) > 20000 {
            return Err(Error::invalid(
                "Net worth exceeds20000 aggregate cells; shorten range or select fewer accounts",
            ));
        }
        for r in selected.iter().filter(|r| {
            r.date >= start && r.date <= end && key(r.date, interval, first_day) == cursor
        }) {
            *balances.get_mut(&r.account).unwrap() += r.amount;
        }
        let total: i64 = balances.values().sum();
        let assets: i64 = balances.values().filter(|n| **n >= 0).sum();
        let debt: i64 = balances.values().filter(|n| **n < 0).sum();
        let x = match interval {
            "Monthly" => cursor.format("%b '%y").to_string(),
            "Yearly" => cursor.format("%Y").to_string(),
            "Weekly" => cursor.to_string(),
            _ => cursor.to_string(),
        };
        let date = match interval {
            "Monthly" => cursor.format("%B %Y").to_string(),
            "Yearly" => cursor.format("%Y").to_string(),
            "Weekly" => cursor.format("%b %-d, %Y").to_string(),
            _ => cursor.format("%B %-d, %Y").to_string(),
        };
        let mut point = json!({"x":x,"y":total,"assets":assets.to_string(),"debt":if debt==0{"-0".into()}else{debt.to_string()},"change":(total-previous).to_string(),"networth":total.to_string(),"date":date});
        for (id, amount) in &balances {
            point[id] = json!(amount);
            if *amount != 0 {
                nonzero.insert(id.clone());
            }
        }
        points.push(point);
        low = low.min(total);
        high = high.max(total);
        previous = total;
        cursor = n;
    }
    let accounts = ids
        .iter()
        .filter(|id| nonzero.contains(id.as_str()))
        .map(|id| {
            let name: String = db.query_row(
                "SELECT name FROM accounts WHERE budget_id=?1 AND id=?2",
                params![b, id],
                |r| r.get(0),
            )?;
            Ok(json!({"id":id,"name":name}))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(
        json!({"graphData":{"data":points,"hasNegative":low<0,"start":start.to_string(),"end":end.to_string()},"netWorth":previous,"totalChange":previous-points[0]["y"].as_i64().unwrap(),"lowestNetWorth":low,"highestNetWorth":high,"accounts":accounts}),
    )
}
fn summary(
    db: &Connection,
    b: &str,
    data: &[Row],
    o: &Value,
    today: NaiveDate,
) -> Result<Value, Error> {
    let start = first(text(o, "start", ""))?;
    let mut end = month_end(first(text(o, "end", ""))?);
    if end.month() == today.month() {
        end = today;
    }
    if start > end {
        return Err(Error::invalid("Summary start must not follow its end"));
    }
    let selected: Vec<_> = data
        .iter()
        .filter(|r| r.date >= start && r.date <= end)
        .collect();
    let dividend: i64 = selected.iter().map(|r| r.amount).sum();
    let s = &o["summary"];
    let divisor = match text(s, "type", "sum") {
        "sum" => 0.0,
        "avgPerTransact" => selected.len() as f64,
        "avgPerMonth" => {
            if selected.is_empty() {
                0.0
            } else {
                ((end.year() - start.year()) * 12 + end.month() as i32 - start.month() as i32)
                    as f64
                    + end.day() as f64 / month_end(end).day() as f64
            }
        }
        "avgPerYear" => {
            if selected.is_empty() {
                0.0
            } else {
                ((end - start).num_days() + 1) as f64 / 365.25
            }
        }
        "percentage" => rows(
            db,
            b,
            &json!({"conditions":s["divisorConditions"],"conditionsOp":s["divisorConditionsOp"]}),
            false,
        )?
        .iter()
        .filter(|r| s["divisorAllTimeDateRange"] == true || (r.date >= start && r.date <= end))
        .map(|r| r.amount)
        .sum::<i64>() as f64,
        _ => return Err(Error::invalid("Unsupported summary type")),
    };
    let total = if s["type"] == "percentage" {
        if divisor == 0.0 {
            Value::Null
        } else {
            json!((dividend as f64 / divisor * 10000.0).round() / 100.0)
        }
    } else if s["type"] == "sum" {
        json!(dividend)
    } else if divisor == 0.0 {
        json!(0)
    } else {
        json!(dividend as f64 / divisor)
    };
    Ok(
        json!({"fromRange":start.format("%b %y").to_string(),"toRange":end.format("%b %y").to_string(),"total":total,"dividend":dividend,"divisor":divisor}),
    )
}
fn calendar(data: &[Row], o: &Value) -> Result<Value, Error> {
    let start = first(text(o, "start", ""))?;
    let end = month_end(first(text(o, "end", ""))?);
    interval_bound(start, end, "Daily", data.len())?;
    let first_day = text(o, "first_day_of_week", "0")
        .parse::<u32>()
        .ok()
        .filter(|n| *n < 7)
        .ok_or_else(|| Error::invalid("Invalid first weekday"))?;
    let mut months = vec![];
    let mut month = start;
    while month <= end {
        let last = month_end(month);
        let selected: Vec<_> = data
            .iter()
            .filter(|r| r.date >= month && r.date <= last)
            .collect();
        let income: i64 = selected
            .iter()
            .filter(|r| r.amount > 0)
            .map(|r| r.amount)
            .sum();
        let expense: i64 = selected
            .iter()
            .filter(|r| r.amount < 0)
            .map(|r| -r.amount)
            .sum();
        let mut dates = vec![];
        let mut cursor = week(month, first_day);
        let finish = week(last, first_day) + Duration::days(6);
        while cursor <= finish {
            let same = cursor.month() == month.month();
            let i: i64 = selected
                .iter()
                .filter(|r| same && r.date == cursor && r.amount > 0)
                .map(|r| r.amount)
                .sum();
            let e: i64 = selected
                .iter()
                .filter(|r| same && r.date == cursor && r.amount < 0)
                .map(|r| -r.amount)
                .sum();
            dates.push(json!({"date":format!("{cursor}T00:00:00.000Z"),"incomeValue":i,"expenseValue":e,"incomeSize":if income==0{0.0}else{i as f64/income as f64*100.0},"expenseSize":if expense==0{0.0}else{e as f64/expense as f64*100.0}}));
            cursor += Duration::days(1);
        }
        months.push(json!({"data":dates,"totalIncome":income,"totalExpense":expense,"start":format!("{month}T00:00:00.000Z"),"end":format!("{last}T23:59:59.999Z")}));
        month = next(month, "Monthly");
    }
    Ok(json!({"calendarData":months}))
}
fn spending(
    db: &Connection,
    b: &str,
    data: &[Row],
    o: &Value,
    today: NaiveDate,
) -> Result<Value, Error> {
    let compare = first(text(o, "compare", &today.to_string()))?;
    let to = if let Some(s) = o["compareTo"].as_str() {
        first(s)?
    } else {
        compare.checked_sub_months(chrono::Months::new(1)).unwrap()
    };
    let range = &o["averageRange"];
    let last = compare.checked_sub_months(chrono::Months::new(1)).unwrap();
    let start = match text(range, "mode", "last-n-months") {
        "last-n-months" => {
            let n = range["months"].as_u64().unwrap_or(3);
            if ![3, 6, 12].contains(&n) {
                return Err(Error::invalid("Average range supports3,6,12 months"));
            }
            Some(
                compare
                    .checked_sub_months(chrono::Months::new(n as u32))
                    .unwrap(),
            )
        }
        "year-to-date" => Some(compare.with_month(1).unwrap()),
        "all-time" => {
            let earliest: Option<String> = db.query_row(
                "SELECT min(date) FROM transactions WHERE budget_id=?1 AND tombstone=0",
                [b],
                |r| r.get(0),
            )?;
            earliest.map(|s| first(&s)).transpose()?
        }
        _ => return Err(Error::invalid("Invalid spending average range")),
    };
    let start = start.filter(|s| *s <= last);
    let mut averages = vec![];
    let mut cursor = start.unwrap_or(compare);
    let months_count = ((compare.year() - cursor.year()) * 12 + compare.month() as i32
        - cursor.month() as i32
        + 1)
    .max(1) as usize
        + usize::from(to < cursor || to > compare);
    if months_count.saturating_mul(28 * 2) > 20000 {
        return Err(Error::invalid(
            "Spending average exceeds20000 financial month/day cells; narrow average period",
        ));
    }
    work_bound(data.len(), months_count * 31)?;
    while cursor < compare {
        averages.push(cursor);
        cursor = next(cursor, "Monthly");
    }
    let mut months = averages.clone();
    months.push(compare);
    if !months.contains(&to) {
        months.insert(0, to);
    }
    let current_method: String =
        db.query_row("SELECT budget_type FROM budgets WHERE id=?1", [b], |r| {
            r.get(0)
        })?;
    let method = text(o, "budgetType", &current_method);
    if !["envelope", "tracking"].contains(&method) {
        return Err(Error::invalid("Invalid spending budget type"));
    }
    let mut budget: i64 = 0;
    let mut budget_rows=db.prepare("SELECT category_id,amount FROM budget_cells WHERE budget_id=?1 AND method=?2 AND month=?3 AND tombstone=0")?;
    // Category predicates are meaningful for allocations; source deliberately ignores
    // transaction-only predicates when comparing recorded spending with the budget.
    let category_conditions: Vec<Value> = o["conditions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| matches!(c["field"].as_str(), Some("category" | "category_group")))
        .cloned()
        .collect();
    let mut values = vec![SqlValue::Text(b.into())];
    let filter = predicate(
        db,
        b,
        &json!({"conditions":category_conditions,"conditionsOp":o["conditionsOp"]}),
        &mut values,
    )?;
    let category_sql = format!(
        "SELECT c.id FROM categories c LEFT JOIN category_groups g ON g.budget_id=c.budget_id AND g.id=c.group_id LEFT JOIN transactions t ON 0 LEFT JOIN accounts a ON 0 LEFT JOIN payees p ON 0 LEFT JOIN accounts y ON 0 WHERE c.budget_id=?1 AND c.tombstone=0 AND ({})",
        filter.replace("t.category_id", "c.id")
    );
    let allowed = if category_conditions.is_empty() {
        None
    } else {
        Some(
            db.prepare(&category_sql)?
                .query_map(params_from_iter(values), |r| r.get::<_, String>(0))?
                .collect::<Result<BTreeSet<_>, _>>()?,
        )
    };
    for row in budget_rows.query_map(
        params![b, method, compare.format("%Y-%m").to_string()],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
    )? {
        let (id, amount) = row?;
        if allowed.as_ref().is_none_or(|ids| ids.contains(&id)) {
            budget += amount;
        }
    }
    let daily_budget = budget as f64 / month_end(compare).day() as f64;
    let mut running = std::collections::BTreeMap::new();
    for m in &months {
        running.insert(*m, 0i64);
    }
    let (mut total_assets, mut total_debts) = (0i64, 0i64);
    let mut total_budget = 0.0;
    let mut intervals = vec![];
    for d in 1..=28u32 {
        let mut indexed = serde_json::Map::new();
        let (mut avg_sum, mut avg_count) = (0i64, 0u32);
        for m in &months {
            let stop = if d == 28 { month_end(*m).day() } else { d };
            let mut daily = 0i64;
            let mut cumulative = Value::Null;
            let mut local = 0;
            for day_number in d..=stop {
                let date = m.with_day(day_number).unwrap();
                let amount: i64 = data
                    .iter()
                    .filter(|r| !r.off && !r.income && r.date == date)
                    .map(|r| r.amount)
                    .sum();
                for row in data
                    .iter()
                    .filter(|r| !r.off && !r.income && r.date == date)
                {
                    if row.amount > 0 {
                        total_assets += row.amount;
                    } else {
                        total_debts += row.amount;
                    }
                }
                local = *running.get(m).unwrap() + amount;
                running.insert(*m, local);
                daily += amount;
                if date <= today {
                    cumulative = json!(local);
                }
                if *m == compare {
                    total_budget -= daily_budget;
                }
            }
            if averages.contains(m) {
                avg_sum += local;
                avg_count += 1;
            }
            indexed.insert(m.format("%Y-%m").to_string(),json!({"date":m.with_day(d).unwrap().to_string(),"cumulative":cumulative,"daily":daily,"month":m.format("%Y-%m").to_string()}));
        }
        let c = indexed[&compare.format("%Y-%m").to_string()]["cumulative"].clone();
        let t = indexed[&to.format("%Y-%m").to_string()]["cumulative"].clone();
        intervals.push(json!({"months":indexed,"day":format!("{d:02}"),"average":if avg_count==0{0.0}else{(avg_sum as f64/avg_count as f64+0.5).floor()},"compare":c,"compareTo":t,"budget":total_budget}));
    }
    Ok(
        json!({"intervalData":intervals,"averageRange":{"startMonth":start.map(|s|s.format("%Y-%m").to_string()),"endMonth":start.map(|_|last.format("%Y-%m").to_string()),"months":averages.iter().map(|m|m.format("%Y-%m").to_string()).collect::<Vec<_>>()},"startDate":start.unwrap_or(compare).to_string(),"endDate":month_end(compare).to_string(),"totalDebts":total_debts,"totalAssets":total_assets,"totalTotals":total_assets+total_debts}),
    )
}
fn live_range(db: &Connection, b: &str, o: &Value, today: NaiveDate) -> Result<Value, Error> {
    let (earliest, latest): (Option<String>, Option<String>) = db.query_row(
        "SELECT min(date),max(date) FROM transactions WHERE budget_id=?1 AND tombstone=0",
        [b],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let earliest = earliest.map(|s| day(&s)).transpose()?.unwrap_or(today);
    let latest = latest.map(|s| day(&s)).transpose()?.unwrap_or(today);
    let current = today.with_day(1).unwrap();
    let include = o["includeCurrent"] == true;
    let first_day = text(o, "first_day_of_week", "0")
        .parse::<u32>()
        .ok()
        .filter(|n| *n < 7)
        .ok_or_else(|| Error::invalid("Invalid first weekday"))?;
    let range = text(o, "range", "");
    let mut clip = true;
    let (mut start, end) = match range {
        "This week" => {
            clip = false;
            let s = week(today, first_day);
            (s, s + Duration::days(6))
        }
        "Last week" => {
            clip = false;
            let s = week(today, first_day) - Duration::days(7);
            (s, s + Duration::days(if include { 13 } else { 6 }))
        }
        "This month" => {
            clip = false;
            (current, month_end(current))
        }
        "Last month" => {
            clip = false;
            let s = current.checked_sub_months(chrono::Months::new(1)).unwrap();
            (
                s,
                if include {
                    month_end(current)
                } else {
                    month_end(s)
                },
            )
        }
        "Last 3 months" | "Last 6 months" | "Last 12 months" => {
            clip = false;
            let count = range.split(' ').nth(1).unwrap().parse::<u32>().unwrap();
            let s = current
                .checked_sub_months(chrono::Months::new(count))
                .unwrap();
            (
                s,
                if include {
                    month_end(current)
                } else {
                    current - Duration::days(1)
                },
            )
        }
        "Current quarter" => {
            let s = current.with_month((current.month0() / 3) * 3 + 1).unwrap();
            (
                s,
                s.checked_add_months(chrono::Months::new(3)).unwrap() - Duration::days(1),
            )
        }
        "Previous quarter" => {
            let s = current
                .with_month((current.month0() / 3) * 3 + 1)
                .unwrap()
                .checked_sub_months(chrono::Months::new(3))
                .unwrap();
            (
                s,
                s.checked_add_months(chrono::Months::new(3)).unwrap() - Duration::days(1),
            )
        }
        "Last 30 days" => (today - Duration::days(29), today),
        "Year to date" => (current.with_month(1).unwrap(), today),
        "Last year" => {
            let s = current
                .with_month(1)
                .unwrap()
                .checked_sub_months(chrono::Months::new(12))
                .unwrap();
            (
                s,
                s.checked_add_months(chrono::Months::new(12)).unwrap() - Duration::days(1),
            )
        }
        "Prior year to date" => {
            let s = current
                .with_month(1)
                .unwrap()
                .checked_sub_months(chrono::Months::new(12))
                .unwrap();
            (
                s,
                today.checked_sub_months(chrono::Months::new(12)).unwrap(),
            )
        }
        "All time" => (earliest, latest),
        _ => return Err(Error::invalid("Unsupported live date range")),
    };
    if clip {
        start = start.max(earliest);
    }
    Ok(json!([
        start.to_string(),
        end.to_string(),
        "sliding-window"
    ]))
}
#[derive(Serialize)]
struct Cell {
    id: String,
    label: String,
    start: String,
    end: String,
    metric: String,
    account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<String>,
}
fn cells(q: &Query, result: &Value, today: NaiveDate) -> Result<Vec<Cell>, Error> {
    let mut cells = vec![];
    let mut add =
        |label: String, start: NaiveDate, end: NaiveDate, metric: &str, account: Option<String>| {
            let id = cells.len().to_string();
            cells.push(Cell {
                id,
                label,
                start: start.to_string(),
                end: end.to_string(),
                metric: metric.into(),
                account,
                group: None,
            });
        };
    let o = &q.options;
    match q.kind.as_str() {
        "cash-flow" => {
            let start = first(text(o, "start", ""))?;
            let end = month_end(first(text(o, "end", ""))?).min(today);
            let concise = o["isConcise"] == true;
            let mut d = start;
            while d <= end {
                let last = if concise { month_end(d) } else { d };
                for metric in ["income", "expense", "transfer"] {
                    add(format!("{d} {metric}"), d, last, metric, None);
                }
                add(
                    format!("{d} balance"),
                    day("0001-01-01")?,
                    last,
                    "balance",
                    None,
                );
                d = if concise {
                    next(d, "Monthly")
                } else {
                    d + Duration::days(1)
                };
            }
        }
        "net-worth" => {
            let start = day(result["graphData"]["start"].as_str().unwrap())?;
            let end = day(result["graphData"]["end"].as_str().unwrap())?;
            let interval = text(o, "interval", "Monthly");
            let first = text(o, "first_day_of_week", "1")
                .parse::<u32>()
                .unwrap_or(1);
            let mut d = key(start, interval, first);
            while d <= end {
                let last = (next(d, interval) - Duration::days(1)).min(end);
                add(
                    format!("{d} net worth"),
                    day("0001-01-01")?,
                    last,
                    "balance",
                    None,
                );
                for account in result["accounts"].as_array().into_iter().flatten() {
                    add(
                        format!("{d} {}", account["name"].as_str().unwrap_or("Account")),
                        day("0001-01-01")?,
                        last,
                        "balance",
                        account["id"].as_str().map(str::to_owned),
                    );
                }
                d = next(d, interval);
            }
        }
        "summary" => {
            let start = first(text(o, "start", ""))?;
            let mut end = month_end(first(text(o, "end", ""))?);
            if end.month() == today.month() {
                end = today;
            }
            add("Summary recorded entries".into(), start, end, "net", None);
        }
        "calendar" => {
            for month in result["calendarData"].as_array().into_iter().flatten() {
                for point in month["data"].as_array().into_iter().flatten() {
                    let date = day(&point["date"].as_str().unwrap()[..10])?;
                    for metric in ["income", "expense"] {
                        if point[format!("{metric}Value")].as_i64().unwrap_or(0) > 0 {
                            add(format!("{date} {metric}"), date, date, metric, None);
                        }
                    }
                }
            }
        }
        "spending" => {
            for point in result["intervalData"].as_array().into_iter().flatten() {
                for (month, values) in point["months"].as_object().into_iter().flatten() {
                    let start = first(month)?;
                    let end = if point["day"] == "28" {
                        month_end(start)
                    } else {
                        day(values["date"].as_str().unwrap())?
                    };
                    add(
                        format!("{month} through day {}", point["day"].as_str().unwrap()),
                        start,
                        end.min(today),
                        "spending",
                        None,
                    );
                }
            }
        }
        _ => {}
    }
    if cells.len() > 20000 {
        return Err(Error::invalid("Report exceeds20000 inspectable cells"));
    }
    Ok(cells)
}
fn evaluate(
    db: &Connection,
    b: &str,
    q: &Query,
    today: NaiveDate,
) -> Result<(Value, Vec<Cell>, Vec<Row>), Error> {
    if q.version != 1 || !q.options.is_object() {
        return Err(Error::invalid("Unsupported report query version/config"));
    }
    if q.kind == "widget-range" {
        return Ok((widget_range(db, b, &q.options, today)?, vec![], vec![]));
    }
    if q.kind == "range" {
        return Ok((live_range(db, b, &q.options, today)?, vec![], vec![]));
    }
    if q.kind == "custom" {
        let (result, data) = custom::calculate(db, b, &q.options)?;
        let cells = custom::cells(&q.options, &result)?;
        return Ok((result, cells, data));
    }
    let data = rows(db, b, &q.options, false)?;
    let result = match q.kind.as_str() {
        "cash-flow" => cash_flow(&data, &q.options, today)?,
        "net-worth" => net_worth(db, b, &data, &q.options, today)?,
        "summary" => summary(db, b, &data, &q.options, today)?,
        "calendar" => calendar(&data, &q.options)?,
        "spending" => spending(db, b, &data, &q.options, today)?,
        _ => return Err(Error::invalid("Unsupported report kind")),
    };
    let cells = cells(q, &result, today)?;
    Ok((result, cells, data))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inspect {
    query: Query,
    snapshot: String,
    cell_id: String,
    offset: usize,
    limit: usize,
}
pub async fn drilldown(
    State(store): State<Store>,
    Path(b): Path<String>,
    input: Result<Json<Inspect>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    let Json(input) = input.map_err(crate::api::json_error)?;
    if input.limit == 0 || input.limit > 200 || input.offset > 100000 {
        return Err(Error::invalid(
            "Inspection limit1–200, offset maximum100000",
        ));
    }
    let today = day(&input.query.as_of)?;
    let value=store.run(move|db|{let tx=db.transaction()?;crate::register::with_report_predicates(&tx, || {sync::check_snapshot(&tx,&b,Some(&input.snapshot))?;let(result,cells,mut data)=evaluate(&tx,&b,&input.query,today)?;let cell=cells.iter().find(|c|c.id==input.cell_id).ok_or_else(||Error::invalid("Unknown report cell"))?;let start=day(&cell.start)?;let end=day(&cell.end)?;let kind=input.query.kind.as_str();let ids=input.query.options["accounts"].as_array();
 if kind=="net-worth"{for row in &mut data{if row.peer_account.as_ref().is_some_and(|id|ids.is_none_or(|ids|ids.contains(&json!(id))))&&let Some(peer)=row.peer_date&&peer>row.date{row.date=peer;}}}
 let custom_matches=if kind=="custom"{let keys=custom::membership_keys(&tx,&b,&input.query.options)?;Some(data.iter().map(|row|custom::matching(&input.query.options,row,cell,&keys)).collect::<Result<Vec<_>,_>>()?)}else{None};
 let selected=data.iter().enumerate().filter(|(index,r)|{
  if custom_matches.as_ref().is_some_and(|matches|!matches[*index]){return false;}
  if r.date<start||r.date>end||cell.account.as_ref().is_some_and(|id|id!=&r.account){return false;}
  if kind=="net-worth"&&ids.is_some_and(|ids|!ids.contains(&json!(r.account))){return false;}
  if kind=="cash-flow"&&(r.off||(cell.metric!="transfer"&&cell.metric!="balance"&&r.transfer.is_some())){return false;}
  if cell.metric=="transfer"&&r.transfer.is_none(){return false;}
  if cell.metric=="income"&&r.amount<=0||cell.metric=="expense"&&r.amount>=0{return false;}
  if cell.metric=="spending"&&(r.off||r.income){return false;}true
 }).map(|(_,row)|row).collect::<Vec<_>>();
 let amount:i64=selected.iter().map(|r|r.amount).sum();let count=selected.len();let rows=selected.into_iter().skip(input.offset).take(input.limit).map(|r|json!({"id":r.id,"date":r.date.to_string(),"amount":r.amount,"account_id":if r.account.is_empty(){None}else{Some(&r.account)},"account":r.account_name,"payee":r.payee_name,"category":r.category_name,"notes":r.notes})).collect::<Vec<_>>();
 let _=result;Ok(json!({"rows":rows,"total_count":count,"total_amount":amount,"offset":input.offset,"limit":input.limit,"has_more":input.offset+rows.len()<count,"snapshot":input.snapshot,"cell":cell,"record_kind":if input.query.options["balanceTypeOp"]=="totalBudgeted"{"budget_allocations"}else{"transactions"}}))})}).await?;
    Ok(Json(json!({"data":value})))
}
fn widget_range(db: &Connection, b: &str, o: &Value, today: NaiveDate) -> Result<Value, Error> {
    let current = today.with_day(1).unwrap();
    let frame = &o["timeFrame"];
    let default_start = current
        .checked_sub_months(chrono::Months::new(5))
        .unwrap()
        .format("%Y-%m")
        .to_string();
    let default_end = current.format("%Y-%m").to_string();
    let start = text(frame, "start", &default_start);
    let end = text(frame, "end", &default_end);
    let mode = text(frame, "mode", "sliding-window");
    first(start)?;
    first(end)?;
    let result = match mode {
        "static" => json!([start, end, mode]),
        "full" => {
            let latest: Option<String> = db.query_row(
                "SELECT max(date) FROM transactions WHERE budget_id=?1 AND tombstone=0",
                [b],
                |r| r.get(0),
            )?;
            let end = latest
                .as_deref()
                .map(first)
                .transpose()?
                .unwrap_or(current)
                .max(current);
            json!([start, end.format("%Y-%m").to_string(), mode])
        }
        "sliding-window" => {
            if start.len() == 10 && end.len() == 10 {
                let offset = (day(end)? - day(start)?).num_days().max(0);
                json!([
                    (today - Duration::days(offset)).to_string(),
                    today.to_string(),
                    mode
                ])
            } else {
                let s = first(start)?;
                let e = first(end)?;
                let offset = (e.year() - s.year()) * 12 + e.month() as i32 - s.month() as i32;
                let (a, z) = if offset >= 0 {
                    (
                        current
                            .checked_sub_months(chrono::Months::new(offset as u32))
                            .unwrap(),
                        current,
                    )
                } else {
                    (
                        current,
                        current
                            .checked_sub_months(chrono::Months::new((-offset) as u32))
                            .unwrap(),
                    )
                };
                json!([
                    a.format("%Y-%m").to_string(),
                    z.format("%Y-%m").to_string(),
                    mode
                ])
            }
        }
        "lastMonth" => {
            let d = current
                .checked_sub_months(chrono::Months::new(1))
                .unwrap()
                .format("%Y-%m")
                .to_string();
            json!([d, d, mode])
        }
        "lastYear" => json!([
            format!("{}-01", today.year() - 1),
            format!("{}-12", today.year() - 1),
            mode
        ]),
        "yearToDate" => json!([format!("{}-01", today.year()), default_end, mode]),
        "priorYearToDate" => json!([
            format!("{}-01", today.year() - 1),
            today
                .checked_sub_months(chrono::Months::new(12))
                .unwrap()
                .to_string(),
            mode
        ]),
        "currentQuarter" | "previousQuarter" => {
            let mut d = current.with_month(current.month0() / 3 * 3 + 1).unwrap();
            if mode == "previousQuarter" {
                d = d.checked_sub_months(chrono::Months::new(3)).unwrap();
            }
            json!([
                d.format("%Y-%m").to_string(),
                d.checked_add_months(chrono::Months::new(2))
                    .unwrap()
                    .format("%Y-%m")
                    .to_string(),
                mode
            ])
        }
        _ => return Err(Error::invalid("Unsupported widget date mode")),
    };
    Ok(result)
}
