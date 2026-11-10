//! Custom financial cells: source grouping and visibility, one shared membership
//! predicate for aggregate and detail reads. Browser never receives full history.
use super::*;
use std::collections::BTreeMap;
#[derive(Clone)]
struct Group {
    id: String,
    name: String,
    uncategorized: Option<String>,
}
impl Group {
    fn key(&self) -> String {
        self.uncategorized
            .as_ref()
            .map_or_else(|| self.id.clone(), |kind| format!("uncategorized:{kind}"))
    }
    fn stack_key(&self) -> &str {
        if self.id.is_empty() {
            &self.name
        } else {
            &self.id
        }
    }
}
fn groups(db: &Connection, b: &str, group_by: &str) -> Result<Vec<Group>, Error> {
    let sql = match group_by {
        "Category" | "Interval" => {
            "SELECT c.id,c.name FROM categories c LEFT JOIN category_groups g ON g.budget_id=c.budget_id AND g.id=c.group_id WHERE c.budget_id=?1 AND c.tombstone=0 ORDER BY g.is_income,g.sort_order,g.id,c.sort_order,c.id"
        }
        "Group" | "CategoryGroup" => {
            "SELECT id,name FROM category_groups WHERE budget_id=?1 AND tombstone=0 ORDER BY is_income,sort_order,id"
        }
        "Payee" => {
            "SELECT p.id,coalesce(a.name,p.name) FROM payees p LEFT JOIN accounts a ON a.budget_id=p.budget_id AND a.id=p.transfer_account_id WHERE p.budget_id=?1 AND p.tombstone=0 ORDER BY coalesce(a.name,p.name) COLLATE NOCASE,p.id"
        }
        "Account" => {
            "SELECT id,name FROM accounts WHERE budget_id=?1 AND tombstone=0 ORDER BY closed,sort_order,name,id"
        }
        _ => return Err(Error::invalid("Unsupported custom report grouping")),
    };
    let mut groups = db
        .prepare(sql)?
        .query_map([b], |r| {
            Ok(Group {
                id: r.get(0)?,
                name: r.get(1)?,
                uncategorized: None,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    match group_by {
        "Category" | "Interval" => {
            for (kind, name) in [
                ("other", "Uncategorized"),
                ("off_budget", "Off budget"),
                ("transfer", "Transfers"),
            ] {
                groups.push(Group {
                    id: "".into(),
                    name: name.into(),
                    uncategorized: Some(kind.into()),
                });
            }
        }
        "Group" | "CategoryGroup" => groups.push(Group {
            id: "uncategorized".into(),
            name: "Uncategorized & Off budget".into(),
            uncategorized: Some("all".into()),
        }),
        _ => {}
    }
    if groups.len() > 500 {
        return Err(Error::invalid("Custom report exceeds500 reference groups"));
    }
    Ok(groups)
}
fn row_key(row: &Row, by: &str) -> Option<String> {
    match by {
        "Category" | "Interval" | "Group" | "CategoryGroup" => {
            if !row.off && row.category.is_some() {
                if ["Group", "CategoryGroup"].contains(&by) {
                    Some(
                        row.group
                            .clone()
                            .unwrap_or_else(|| "uncategorized:all".into()),
                    )
                } else {
                    row.category.clone()
                }
            } else {
                Some(format!(
                    "uncategorized:{}",
                    if ["Group", "CategoryGroup"].contains(&by) {
                        "all"
                    } else if row.off {
                        "off_budget"
                    } else if row.transfer.is_some() {
                        "transfer"
                    } else {
                        "other"
                    }
                ))
            }
        }
        "Payee" => row.payee.clone(),
        "Account" => Some(row.account.clone()),
        _ => None,
    }
}
fn visible(row: &Row, o: &Value) -> bool {
    (o["showHiddenCategories"] != false || !row.hidden)
        && (o["showOffBudget"] == true || !row.off)
        && (o["showUncategorized"] != false || row.category.is_some() || row.off)
}
fn metric(o: &Value) -> Result<&str, Error> {
    let m = text(o, "balanceTypeOp", "totalDebts");
    if ![
        "totalAssets",
        "totalDebts",
        "totalTotals",
        "netAssets",
        "netDebts",
        "totalBudgeted",
    ]
    .contains(&m)
    {
        return Err(Error::invalid("Unsupported custom report balance mode"));
    }
    Ok(m)
}
fn value(a: i64, d: i64, m: &str) -> i64 {
    match m {
        "totalAssets" => a,
        "totalDebts" => d,
        "netAssets" => (a + d).max(0),
        "netDebts" => (a + d).min(0),
        _ => a + d,
    }
}
fn metrics(a: i64, d: i64) -> Value {
    json!({"totalAssets":a,"totalDebts":d,"totalTotals":a+d,"totalBudgeted":a+d,"netAssets":(a+d).max(0),"netDebts":(a+d).min(0)})
}
fn period_date(s: &str, interval: &str, end: bool) -> Result<NaiveDate, Error> {
    let d = if s.len() == 4 {
        day(&format!("{s}-01-01"))?
    } else if s.len() == 7 {
        first(s)?
    } else {
        day(s)?
    };
    Ok(match interval {
        "Yearly" => {
            if end {
                d.with_month(12).unwrap().with_day(31).unwrap()
            } else {
                d.with_month(1).unwrap().with_day(1).unwrap()
            }
        }
        "Monthly" => {
            if end {
                month_end(d)
            } else {
                d.with_day(1).unwrap()
            }
        }
        _ => d,
    })
}
fn period_id(d: NaiveDate, interval: &str) -> String {
    match interval {
        "Monthly" => d.format("%Y-%m").to_string(),
        "Yearly" => d.format("%Y").to_string(),
        _ => d.to_string(),
    }
}
fn period_label(d: NaiveDate, interval: &str) -> String {
    match interval {
        "Monthly" => d.format("%b '%y").to_string(),
        "Yearly" => d.format("%Y").to_string(),
        _ => d.to_string(),
    }
}
fn first_weekday(o: &Value) -> Result<u32, Error> {
    text(o, "firstDayOfWeekIdx", text(o, "first_day_of_week", "0"))
        .parse::<u32>()
        .ok()
        .filter(|n| *n < 7)
        .ok_or_else(|| Error::invalid("Invalid first weekday"))
}
fn budget_rows(db: &Connection, b: &str, o: &Value) -> Result<Vec<Row>, Error> {
    let interval = text(o, "interval", "Monthly");
    let by = text(o, "groupBy", "Category");
    if !["Monthly", "Yearly"].contains(&interval) || ["Payee", "Account"].contains(&by) {
        return Err(Error::invalid(
            "Budgeted reports use monthly/yearly category or category-group totals",
        ));
    }
    let current: String =
        db.query_row("SELECT budget_type FROM budgets WHERE id=?1", [b], |r| {
            r.get(0)
        })?;
    let method = text(o, "budgetType", &current);
    if !["tracking", "envelope"].contains(&method) {
        return Err(Error::invalid("Invalid budget method"));
    }
    let month = |key| {
        let raw = text(o, key, "");
        if raw.len() == 4 {
            first(&format!("{raw}-01"))
        } else {
            first(raw)
        }
    };
    let start = month("startDate")?.format("%Y-%m").to_string();
    let end = month("endDate")?.format("%Y-%m").to_string();
    let mut conditions: Vec<Value> = o["conditions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| matches!(c["field"].as_str(), Some("category" | "category_group")))
        .cloned()
        .collect();
    for c in &mut conditions {
        if matches!(
            c["op"].as_str(),
            Some("contains" | "doesNotContain" | "not_contains" | "matches")
        ) {
            c["field"] = json!(if c["field"] == "category" {
                "category_name"
            } else {
                "category_group_name"
            });
        }
    }
    let mut values = vec![
        SqlValue::Text(b.into()),
        SqlValue::Text(method.into()),
        SqlValue::Text(start),
        SqlValue::Text(end),
    ];
    let filter = predicate(
        db,
        b,
        &json!({"conditions":conditions,"conditionsOp":o["conditionsOp"]}),
        &mut values,
    )?
    .replace("t.category_id", "c.id");
    let sql = format!(
        "SELECT cell.id,cell.month,cell.amount,c.id,c.name,c.group_id,(c.hidden OR coalesce(g.hidden,0)) FROM budget_cells cell JOIN categories c ON c.budget_id=cell.budget_id AND c.id=cell.category_id LEFT JOIN category_groups g ON g.budget_id=c.budget_id AND g.id=c.group_id LEFT JOIN transactions t ON 0 LEFT JOIN accounts a ON 0 LEFT JOIN payees p ON 0 LEFT JOIN accounts y ON 0 WHERE cell.budget_id=?1 AND cell.method=?2 AND cell.month>=?3 AND cell.month<=?4 AND cell.tombstone=0 AND c.tombstone=0 AND c.is_income=0 AND cell.amount<>0 AND ({filter}) ORDER BY cell.month,cell.id LIMIT 100001"
    );
    let rows = db
        .prepare(&sql)?
        .query_map(params_from_iter(values), |r| {
            Ok(Row {
                id: r.get(0)?,
                date: NaiveDate::parse_from_str(&(r.get::<_, String>(1)? + "-01"), "%Y-%m-%d")
                    .unwrap(),
                amount: r.get(2)?,
                category: r.get(3)?,
                category_name: r.get(4)?,
                group: r.get(5)?,
                hidden: r.get(6)?,
                account: "".into(),
                account_name: format!("Budget allocation ({method})"),
                off: false,
                payee: None,
                payee_name: "".into(),
                income: false,
                transfer: None,
                peer_date: None,
                peer_account: None,
                notes: "Planned budget allocation; not a recorded transaction".into(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if rows.len() > 100000
        || rows.iter().map(|r| (r.amount as i128).abs()).sum::<i128>()
            > crate::models::MAX_MONEY as i128
    {
        return Err(Error::invalid(
            "Selected allocations exceed exact report row/money bounds",
        ));
    }
    Ok(rows)
}
fn calculate_flat(db: &Connection, b: &str, o: &Value) -> Result<(Value, Vec<Row>), Error> {
    if !["total", "time"].contains(&text(o, "mode", "total")) {
        return Err(Error::invalid("Unsupported custom report mode"));
    }
    let interval = text(o, "interval", "Monthly");
    if !["Daily", "Weekly", "Monthly", "Yearly"].contains(&interval) {
        return Err(Error::invalid("Unsupported custom interval"));
    }
    let by = text(o, "groupBy", "Category");
    let m = metric(o)?;
    let sort = text(o, "sortByOp", "desc");
    if !["asc", "desc", "name", "budget"].contains(&sort) {
        return Err(Error::invalid("Unsupported custom sort"));
    }
    for field in [
        "showEmpty",
        "showOffBudget",
        "showHiddenCategories",
        "showUncategorized",
        "trimIntervals",
    ] {
        if !o[field].is_null() && !o[field].is_boolean() {
            return Err(Error::invalid("Report visibility requires boolean values"));
        }
    }
    let groups = groups(db, b, by)?;
    let first_day = first_weekday(o)?;
    let start = period_date(text(o, "startDate", ""), interval, false)?;
    let end = period_date(text(o, "endDate", ""), interval, true)?;
    let data = if m == "totalBudgeted" {
        budget_rows(db, b, o)?
    } else {
        rows(db, b, o, false)?
    };
    interval_bound(key(start, interval, first_day), end, interval, data.len())?;
    let mut periods = vec![];
    let mut cursor = key(start, interval, first_day);
    while cursor <= end {
        periods.push(cursor);
        cursor = next(cursor, interval);
    }
    if periods.len().saturating_mul(groups.len() + 1) > 20000 {
        return Err(Error::invalid(
            "Custom report exceeds20000 group/interval cells",
        ));
    }
    work_bound(data.len(), periods.len().saturating_add(groups.len()))?;
    let keys: BTreeSet<_> = groups.iter().map(Group::key).collect();
    let mut aggregate: BTreeMap<(String, NaiveDate), (i64, i64)> = BTreeMap::new();
    for row in data
        .iter()
        .filter(|row| row.date >= start && row.date <= end && visible(row, o))
    {
        if let Some(group) = row_key(row, by).filter(|k| keys.contains(k)) {
            let totals = aggregate
                .entry((group, key(row.date, interval, first_day)))
                .or_default();
            if row.amount > 0 {
                totals.0 += row.amount;
            } else {
                totals.1 += row.amount;
            }
        }
    }
    let bounds = |index: usize| {
        (
            if index == 0 {
                text(o, "startDate", "").into()
            } else {
                period_id(periods[index], interval)
            },
            if index + 1 == periods.len() {
                text(o, "endDate", "").into()
            } else {
                (periods[index + 1] - Duration::days(1)).to_string()
            },
        )
    };
    let mut total_a = 0;
    let mut total_d = 0;
    let mut total_net_a = 0;
    let mut total_net_d = 0;
    let mut interval_data = vec![];
    for (index, period) in periods.iter().enumerate() {
        let (mut a, mut d) = (0, 0);
        let mut point = json!({"date":period_label(*period,interval)});
        for group in &groups {
            let (ga, gd) = aggregate
                .get(&(group.key(), *period))
                .copied()
                .unwrap_or_default();
            a += ga;
            d += gd;
            let amount = value(ga, gd, m);
            point[group.stack_key()] = json!(if ["totalDebts", "netDebts"].contains(&m) {
                amount.abs()
            } else {
                amount
            });
        }
        let (from, to) = bounds(index);
        point["intervalStartDate"] = json!(from);
        point["intervalEndDate"] = json!(to);
        for (k, v) in metrics(a, d).as_object().unwrap() {
            point[k] = v.clone();
        }
        total_a += a;
        total_d += d;
        total_net_a += (a + d).max(0);
        total_net_d += (a + d).min(0);
        interval_data.push(point);
    }
    let mut grouped = vec![];
    for group in &groups {
        let (mut a, mut d, mut previous) = (0, 0, 0);
        let mut values = vec![];
        for (index, period) in periods.iter().enumerate() {
            let (ga, gd) = aggregate
                .get(&(group.key(), *period))
                .copied()
                .unwrap_or_default();
            a += ga;
            d += gd;
            let (from, to) = bounds(index);
            let mut point = metrics(ga, gd);
            point["date"] = json!(period_id(*period, interval));
            point["change"] = json!(if index == 0 { 0 } else { ga + gd - previous });
            point["intervalStartDate"] = json!(from);
            point["intervalEndDate"] = json!(to);
            previous = ga + gd;
            values.push(point);
        }
        let keep = o["showEmpty"] == true
            || if ["totalTotals", "totalBudgeted"].contains(&m) {
                a != 0 || d != 0
            } else {
                value(a, d, m) != 0
            };
        if !keep {
            continue;
        }
        let mut item = metrics(a, d);
        item["id"] = json!(group.id);
        item["name"] = json!(group.name);
        if let Some(kind) = &group.uncategorized {
            item["uncategorizedId"] = json!(kind);
        }
        item["intervalData"] = json!(values);
        grouped.push(item);
    }
    if o["trimIntervals"] == true {
        let active = (0..periods.len())
            .filter(|index| {
                interval_data[*index][m].as_i64().unwrap_or(0) != 0
                    || grouped
                        .iter()
                        .any(|g| g["intervalData"][*index][m].as_i64().unwrap_or(0) != 0)
            })
            .collect::<Vec<_>>();
        let trim = |values: &mut Vec<Value>| {
            *values = if let (Some(first), Some(last)) = (active.first(), active.last()) {
                values[*first..=*last].to_vec()
            } else {
                vec![]
            };
        };
        trim(&mut interval_data);
        for group in &mut grouped {
            let mut values = group["intervalData"].take().as_array().cloned().unwrap();
            trim(&mut values);
            group["intervalData"] = json!(values);
        }
    }
    grouped.sort_by(|a, b| {
        if sort == "name" {
            return a["name"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .cmp(&b["name"].as_str().unwrap().to_lowercase());
        }
        if sort == "budget" {
            return std::cmp::Ordering::Equal;
        }
        let cmp = a[m].as_i64().unwrap().cmp(&b[m].as_i64().unwrap());
        if (sort == "desc") != ["totalDebts", "netDebts"].contains(&m) {
            cmp.reverse()
        } else {
            cmp
        }
    });
    Ok((
        json!({"data":grouped,"intervalData":interval_data,"startDate":o["startDate"],"endDate":o["endDate"],"totalAssets":total_a,"totalDebts":total_d,"totalTotals":total_a+total_d,"totalBudgeted":total_a+total_d,"netAssets":total_net_a,"netDebts":total_net_d}),
        data,
    ))
}
pub(super) fn calculate(db: &Connection, b: &str, o: &Value) -> Result<(Value, Vec<Row>), Error> {
    if !o["nested"].is_null() && !o["nested"].is_boolean() {
        return Err(Error::invalid("Nested report flag must be boolean"));
    }
    let (mut result, data) = calculate_flat(db, b, o)?;
    if o["nested"] != true {
        return Ok((result, data));
    }
    if o["groupBy"] != "CategoryGroup" {
        return Err(Error::invalid(
            "Nested reports require category-group grouping",
        ));
    }
    let mut untrimmed = o.clone();
    untrimmed["trimIntervals"] = json!(false);
    untrimmed["nested"] = json!(false);
    let (mut parents, _) = calculate_flat(db, b, &untrimmed)?;
    untrimmed["groupBy"] = json!("Category");
    let (children, _) = calculate_flat(db, b, &untrimmed)?;
    let count = parents["intervalData"].as_array().map_or(0, Vec::len);
    let group_count = groups(db, b, "Group")?.len();
    let category_count = groups(db, b, "Category")?.len();
    if count.saturating_mul(group_count * 2 + category_count + 2) > 20000 {
        return Err(Error::invalid(
            "Nested report exceeds20000 combined group/category interval cells",
        ));
    }
    work_bound(
        data.len(),
        count
            .saturating_add(group_count + category_count)
            .saturating_mul(3),
    )?;
    let membership = db
        .prepare("SELECT id,group_id FROM categories WHERE budget_id=?1 AND tombstone=0")?
        .query_map([b], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?
                    .unwrap_or_else(|| "uncategorized".into()),
            ))
        })?
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let child_rows = children["data"].as_array().unwrap();
    for parent in parents["data"].as_array_mut().unwrap() {
        let id = parent["id"].as_str().unwrap();
        let mut nested = child_rows
            .iter()
            .filter(|child| {
                if id == "uncategorized" {
                    child.get("uncategorizedId").is_some()
                        || child["id"]
                            .as_str()
                            .and_then(|id| membership.get(id))
                            .is_some_and(|group| group == "uncategorized")
                } else {
                    child["id"]
                        .as_str()
                        .and_then(|id| membership.get(id))
                        .is_some_and(|group| group == id)
                }
            })
            .cloned()
            .collect::<Vec<_>>();
        if id == "uncategorized" {
            nested.sort_by_key(|c| match c["uncategorizedId"].as_str() {
                Some("other") => 0,
                Some("transfer") => 1,
                _ => 2,
            });
        }
        let m = metric(o)?;
        let sort = text(o, "sortByOp", "desc");
        nested.sort_by(|a, b| {
            if sort == "budget" {
                std::cmp::Ordering::Equal
            } else if sort == "name" {
                a["name"]
                    .as_str()
                    .unwrap()
                    .to_lowercase()
                    .cmp(&b["name"].as_str().unwrap().to_lowercase())
            } else {
                let cmp = a[m].as_i64().unwrap().cmp(&b[m].as_i64().unwrap());
                if (sort == "desc") != ["totalDebts", "netDebts"].contains(&m) {
                    cmp.reverse()
                } else {
                    cmp
                }
            }
        });
        parent["categories"] = json!(nested);
    }
    if o["trimIntervals"] == true {
        let m = metric(o)?;
        let active = (0..count)
            .filter(|index| {
                parents["data"].as_array().unwrap().iter().any(|group| {
                    group["intervalData"][*index][m].as_i64().unwrap_or(0) != 0
                        || group["categories"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|c| c["intervalData"][*index][m].as_i64().unwrap_or(0) != 0)
                })
            })
            .collect::<Vec<_>>();
        let trim = |row: &mut Value| {
            let values = row["intervalData"].as_array().unwrap();
            row["intervalData"] = json!(if let (Some(first), Some(last)) =
                (active.first(), active.last())
            {
                values[*first..=*last].to_vec()
            } else {
                vec![]
            });
        };
        for group in parents["data"].as_array_mut().unwrap() {
            trim(group);
            for child in group["categories"].as_array_mut().unwrap() {
                trim(child);
            }
        }
    }
    result["groupedData"] = parents["data"].take();
    Ok((result, data))
}
fn cell_group(group: &Value) -> String {
    group["uncategorizedId"].as_str().map_or_else(
        || group["id"].as_str().unwrap_or("").into(),
        |kind| format!("uncategorized:{kind}"),
    )
}
pub(super) fn cells(o: &Value, result: &Value) -> Result<Vec<Cell>, Error> {
    let interval = text(o, "interval", "Monthly");
    let first = first_weekday(o)?;
    let start = period_date(text(o, "startDate", ""), interval, false)?;
    let end = period_date(text(o, "endDate", ""), interval, true)?;
    let m = metric(o)?;
    let mut cells = vec![];
    let mut add = |label: String, from: NaiveDate, to: NaiveDate, group: Option<String>| {
        let id = cells.len().to_string();
        cells.push(Cell {
            id,
            label,
            start: from.to_string(),
            end: to.to_string(),
            metric: m.into(),
            account: None,
            group,
        });
    };
    add("All report entries".into(), start, end, None);
    for point in result["intervalData"].as_array().into_iter().flatten() {
        let period = key(
            period_date(
                point["intervalStartDate"].as_str().unwrap(),
                interval,
                false,
            )?,
            interval,
            first,
        );
        let last = (next(period, interval) - Duration::days(1)).min(end);
        add(
            format!("{} total", point["date"].as_str().unwrap()),
            period.max(start),
            last,
            None,
        );
    }
    for group in result["data"].as_array().into_iter().flatten() {
        let group_id = cell_group(group);
        let name = group["name"].as_str().unwrap_or("");
        add(format!("{name} total"), start, end, Some(group_id.clone()));
        for point in group["intervalData"].as_array().into_iter().flatten() {
            let period = key(
                period_date(point["date"].as_str().unwrap(), interval, false)?,
                interval,
                first,
            );
            add(
                format!("{name} / {}", point["date"].as_str().unwrap()),
                period.max(start),
                (next(period, interval) - Duration::days(1)).min(end),
                Some(group_id.clone()),
            );
        }
    }
    if let Some(groups) = result["groupedData"].as_array() {
        for parent in groups {
            for child in parent["categories"].as_array().into_iter().flatten() {
                let id = format!("leaf:{}", cell_group(child));
                let name = format!(
                    "{} / {}",
                    parent["name"].as_str().unwrap_or(""),
                    child["name"].as_str().unwrap_or("")
                );
                add(format!("{name} total"), start, end, Some(id.clone()));
                for point in child["intervalData"].as_array().into_iter().flatten() {
                    let period = key(
                        period_date(point["date"].as_str().unwrap(), interval, false)?,
                        interval,
                        first,
                    );
                    add(
                        format!("{name} / {}", point["date"].as_str().unwrap()),
                        period.max(start),
                        (next(period, interval) - Duration::days(1)).min(end),
                        Some(id.clone()),
                    );
                }
            }
        }
    }
    if cells.len() > 20000 {
        return Err(Error::invalid(
            "Custom report exceeds20000 inspection cells",
        ));
    }
    Ok(cells)
}
pub(super) fn membership_keys(
    db: &Connection,
    b: &str,
    o: &Value,
) -> Result<BTreeSet<String>, Error> {
    Ok(groups(db, b, text(o, "groupBy", "Category"))?
        .iter()
        .map(Group::key)
        .collect())
}
pub(super) fn matching(
    o: &Value,
    row: &Row,
    cell: &Cell,
    keys: &BTreeSet<String>,
) -> Result<bool, Error> {
    if !visible(row, o) {
        return Ok(false);
    }
    let by = text(o, "groupBy", "Category");
    let m = metric(o)?;
    let Some(key) = row_key(row, by) else {
        return Ok(false);
    };
    if let Some(group) = &cell.group {
        if let Some(leaf) = group.strip_prefix("leaf:") {
            if row_key(row, "Category").as_deref() != Some(leaf) || !keys.contains(&key) {
                return Ok(false);
            }
        } else if group != &key {
            return Ok(false);
        }
    } else if !keys.contains(&key) {
        return Ok(false);
    }
    Ok(match m {
        "totalAssets" => row.amount > 0,
        "totalDebts" => row.amount < 0,
        _ => true,
    })
}
