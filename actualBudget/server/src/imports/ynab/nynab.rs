use super::*;
use chrono::{Datelike, NaiveDate, TimeDelta};
fn live(row: &Value) -> bool {
    row["deleted"] != true
}
fn mapped(s: &Source, kind: &str, value: &Value) -> Value {
    value
        .as_str()
        .and_then(|v| s.mappings.get(&format!("{kind}:{v}")))
        .map(|v| json!(v))
        .unwrap_or(Value::Null)
}
fn amount(v: &Value) -> Result<i64, Error> {
    rounded(v.as_f64().ok_or_else(|| invalid("Invalid source amount"))? / 10.0)
}
fn trim(v: &Value) -> &str {
    v.as_str()
        .unwrap_or("")
        .trim_matches(crate::payees_tags::js_whitespace)
}
fn color(v: &str) -> Option<&'static str> {
    match v {
        "red" => Some("#FF6666"),
        "orange" => Some("#F57C00"),
        "yellow" => Some("#FBC02D"),
        "green" => Some("#689F38"),
        "blue" => Some("#1976D2"),
        "purple" => Some("#512DA8"),
        _ => None,
    }
}
fn tag(row: &Value, conflicts: &BTreeSet<String>) -> String {
    let name = trim(&row["flag_name"]);
    let c = trim(&row["flag_color"]);
    if name.is_empty() {
        c.to_owned()
    } else if conflicts.contains(name) {
        format!("{name}-{c}")
    } else {
        name.to_owned()
    }
}
fn notes(row: &Value, conflicts: &BTreeSet<String>) -> Value {
    let t = tag(row, conflicts);
    let memo = trim(&row["memo"]);
    let note = if t.is_empty() {
        memo.to_owned()
    } else if memo.is_empty() {
        format!("#{t}")
    } else {
        format!("{memo} #{t}")
    };
    if note.is_empty() {
        Value::Null
    } else {
        json!(note)
    }
}
fn unique(base: &str, hidden: bool, names: &mut BTreeSet<String>) -> Result<String, Error> {
    let base = if hidden {
        format!("{base} (hidden)")
    } else {
        base.to_owned()
    };
    for n in 0..=20 {
        let name = if n == 0 {
            base.clone()
        } else {
            format!("{base} ({n})")
        };
        if names.insert(name.to_lowercase()) {
            return Ok(name);
        }
    }
    Err(invalid("Could not assign a unique imported category name"))
}
fn date(row: &Value, key: &str) -> Result<i64, Error> {
    let value = text(row, key)?;
    crate::models::validate_date(value)?;
    value
        .replace('-', "")
        .parse()
        .map_err(|_| invalid("Invalid source date"))
}
fn payee(s: &Source, row: &Value, fallback: &Value) -> (Value, Value) {
    if row["transfer_account_id"].is_string() {
        let account = mapped(s, "account", &row["transfer_account_id"]);
        return (
            account
                .as_str()
                .map(|id| json!(identity("transfer-payee", id)))
                .unwrap_or(Value::Null),
            Value::Null,
        );
    }
    if let Some(id) = row["payee_id"].as_str() {
        return (
            mapped(s, "payee", &row["payee_id"]),
            s.payee_names
                .get(id)
                .map(|v| json!(v))
                .unwrap_or(Value::Null),
        );
    }
    (fallback.clone(), Value::Null)
}
fn recurrence(row: &Value) -> Result<Value, Error> {
    let frequency = text(row, "frequency")?;
    if frequency == "never" {
        date(row, "date_next")?;
        return Ok(row["date_next"].clone());
    }
    let start = text(row, "date_first")?;
    crate::models::validate_date(start)?;
    let parsed = NaiveDate::parse_from_str(start, "%Y-%m-%d")
        .map_err(|_| invalid("Invalid schedule start"))?;
    let (frequency, interval) = match frequency {
        "daily" => ("daily", 1),
        "weekly" => ("weekly", 1),
        "monthly" => ("monthly", 1),
        "yearly" => ("yearly", 1),
        "everyOtherWeek" => ("weekly", 2),
        "every4Weeks" => ("weekly", 4),
        "everyOtherMonth" => ("monthly", 2),
        "every3Months" => ("monthly", 3),
        "every4Months" => ("monthly", 4),
        "everyOtherYear" => ("yearly", 2),
        "twiceAYear" => ("monthly", 6),
        "twiceAMonth" => ("monthly", 1),
        _ => return Err(invalid("Unsupported scheduled frequency")),
    };
    let mut result = json!({"frequency":frequency,"start":start,"skipWeekend":false,"weekendSolveMode":"after","endMode":"never"});
    if interval != 1 {
        result["interval"] = json!(interval)
    }
    if frequency == "monthly" {
        let d = |day: u32| if day == 31 { -1 } else { day as i32 };
        let mut patterns = vec![];
        if row["frequency"] == "twiceAMonth" {
            let second = parsed
                .checked_add_signed(TimeDelta::days(15))
                .ok_or_else(|| invalid("Schedule outside calendar"))?;
            patterns = vec![
                json!({"type":"day","value":d(parsed.day())}),
                json!({"type":"day","value":d(second.day())}),
            ]
        } else if parsed.day() == 31 {
            patterns.push(json!({"type":"day","value":-1}))
        }
        result["patterns"] = json!(patterns)
    }
    Ok(result)
}
pub(super) fn convert(
    _filename: &str,
    bytes: &[u8],
) -> Result<(String, Source, Vec<Value>), Error> {
    let mut data: Value =
        serde_json::from_slice(bytes).map_err(|_| invalid("Choose a valid nYNAB JSON export"))?;
    if data["data"].is_object() {
        data = data["data"].take()
    }
    if data["plan"].is_object() {
        data = data["plan"].take()
    } else if data["budget"].is_object() {
        data = data["budget"].take()
    }
    for field in [
        "accounts",
        "payees",
        "category_groups",
        "categories",
        "months",
        "transactions",
        "subtransactions",
        "scheduled_transactions",
        "scheduled_subtransactions",
    ] {
        rows(&data, field)?;
    }
    let name = data["budget_name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| data["name"].as_str())
        .ok_or_else(|| invalid("Missing nYNAB budget name"))?
        .to_owned();
    crate::models::validate_name(&name)?;
    let mut s = Source::new()?;
    s.income();
    for (i, a) in rows(&data, "accounts")?
        .iter()
        .filter(|a| live(a))
        .enumerate()
    {
        let id = s.map("account", text(a, "id")?)?;
        let title = text(a, "name")?;
        s.off_budget.insert(id.clone(), a["on_budget"] != true);
        s.push("accounts",json!({"id":id,"name":title,"offbudget":i32::from(a["on_budget"]!=true),"closed":i32::from(a["closed"]==true),"sort_order":i,"tombstone":0}));
        s.push("payees",json!({"id":identity("transfer-payee",&id),"name":title,"transfer_acct":id,"tombstone":0}));
    }
    let groups = rows(&data, "category_groups")?;
    let group_by_id = groups
        .iter()
        .map(|g| (g["id"].as_str().unwrap_or(""), g))
        .collect::<BTreeMap<_, _>>();
    let mut grouped = BTreeMap::<&str, Vec<(usize, &Value)>>::new();
    for (i, c) in rows(&data, "categories")?.iter().enumerate() {
        grouped
            .entry(text(c, "category_group_id")?)
            .or_default()
            .push((i, c));
    }
    let mut group_names = BTreeSet::from(["income".to_owned()]);
    for (i, g) in groups.iter().filter(|g| live(g)).enumerate() {
        let original = text(g, "id")?;
        let title = text(g, "name")?;
        let special = title.to_lowercase();
        let group = if special == "income" {
            Some(identity("builtin", "income-group"))
        } else if matches!(
            special.as_str(),
            "internal master category" | "credit card payments" | "hidden categories"
        ) {
            None
        } else {
            let id = s.map("group", original)?;
            let title = unique(title, g["hidden"] == true, &mut group_names)?;
            s.push("category_groups",json!({"id":id,"name":title,"is_income":0,"hidden":i32::from(g["hidden"]==true),"sort_order":i,"tombstone":0}));
            s.note(&id, &g["note"])?;
            Some(id)
        };
        let mut names = BTreeSet::new();
        for (index, c) in grouped.get(original).into_iter().flatten().rev() {
            if !live(c) {
                continue;
            }
            let original = text(c, "id")?;
            let title = text(c, "name")?;
            if special == "income"
                || (special == "internal master category"
                    && matches!(
                        title.to_lowercase().as_str(),
                        "to be budgeted" | "inflow: ready to assign"
                    ))
            {
                s.mappings.insert(
                    format!("category:{original}"),
                    identity("builtin", "income"),
                );
                continue;
            }
            let Some(group) = &group else { continue };
            let id = s.map("category", original)?;
            let title = unique(title, c["hidden"] == true, &mut names)?;
            s.push("categories",json!({"id":id,"name":title,"cat_group":group,"is_income":0,"hidden":i32::from(c["hidden"]==true),"sort_order":index,"tombstone":0}));
            s.note(&id, &c["note"])?;
        }
    }
    for p in rows(&data, "payees")?
        .iter()
        .filter(|p| live(p) && !p["transfer_account_id"].is_string())
    {
        let original = text(p, "id")?;
        let id = s.map("payee", original)?;
        let name = text(p, "name")?;
        s.payee_names.insert(original.to_owned(), name.to_owned());
        s.push(
            "payees",
            json!({"id":id,"name":name,"transfer_acct":null,"tombstone":0}),
        );
    }
    let flagged = rows(&data, "transactions")?
        .iter()
        .chain(rows(&data, "scheduled_transactions")?)
        .filter(|r| live(r))
        .collect::<Vec<_>>();
    let mut colors = BTreeMap::<String, BTreeSet<String>>::new();
    for row in &flagged {
        let n = trim(&row["flag_name"]);
        let c = trim(&row["flag_color"]);
        if !n.is_empty() && color(c).is_some() {
            colors.entry(n.to_owned()).or_default().insert(c.to_owned());
        }
    }
    let conflicts = colors
        .into_iter()
        .filter(|(_, c)| c.len() > 1)
        .map(|(n, _)| n)
        .collect::<BTreeSet<_>>();
    let mut tags = BTreeSet::new();
    for row in flagged {
        let c = trim(&row["flag_color"]);
        if let Some(color) = color(c) {
            let name = tag(row, &conflicts);
            if tags.insert(name.clone()) {
                s.push("tags",json!({"id":identity("tag",&name),"tag":name,"color":color,"description":"Imported from YNAB","tombstone":0,"hidden":0}));
            }
        }
    }
    // Index every source identity before resolving reciprocal edges.
    let roots = rows(&data, "transactions")?;
    let children = rows(&data, "subtransactions")?;
    let mut child_groups = BTreeMap::<&str, Vec<&Value>>::new();
    for c in children {
        s.map("transaction", text(c, "id")?)?;
        child_groups
            .entry(text(c, "transaction_id")?)
            .or_default()
            .push(c);
    }
    let root_by_id = roots
        .iter()
        .map(|r| (r["id"].as_str().unwrap_or(""), r))
        .collect::<BTreeMap<_, _>>();
    for r in roots {
        s.map("transaction", text(r, "id")?)?;
    }
    let mut orphans = BTreeMap::<String, Vec<&Value>>::new();
    for r in roots {
        if r["transfer_account_id"].is_string() && !r["transfer_transaction_id"].is_string() {
            orphans
                .entry(format!(
                    "{}#{}",
                    text(r, "account_id")?,
                    text(r, "transfer_account_id")?
                ))
                .or_default()
                .push(r);
        }
    }
    let mut sub_orphans = BTreeMap::<String, Vec<&Value>>::new();
    for c in children {
        if let Some(target) = c["transfer_account_id"].as_str() {
            let parent = root_by_id
                .get(text(c, "transaction_id")?)
                .ok_or_else(|| invalid("Missing split parent"))?;
            sub_orphans
                .entry(format!("{target}#{}", text(parent, "account_id")?))
                .or_default()
                .push(c);
        }
    }
    let key = |r: &Value, child: bool| -> Result<(String, i64, String), Error> {
        let parent = if child {
            *root_by_id
                .get(text(r, "transaction_id")?)
                .ok_or_else(|| invalid("Missing split parent"))?
        } else {
            r
        };
        let n = r["amount"]
            .as_i64()
            .ok_or_else(|| invalid("Invalid source amount"))?;
        Ok((
            text(parent, "date")?.to_owned(),
            if child {
                n.checked_neg()
                    .ok_or_else(|| invalid("Source amount exceeds exact range"))?
            } else {
                n
            },
            r["memo"].as_str().unwrap_or("").to_owned(),
        ))
    };
    let mut links = BTreeMap::new();
    for (group, rows) in &mut orphans {
        if let Some(subs) = sub_orphans.get_mut(group) {
            let mut rs = rows
                .iter()
                .map(|r| Ok((key(r, false)?, *r)))
                .collect::<Result<Vec<_>, Error>>()?;
            let mut cs = subs
                .iter()
                .map(|r| Ok((key(r, true)?, *r)))
                .collect::<Result<Vec<_>, Error>>()?;
            rs.sort_by(|a, b| a.0.cmp(&b.0));
            cs.sort_by(|a, b| a.0.cmp(&b.0));
            let (mut a, mut b) = (0, 0);
            while a < rs.len() && b < cs.len() {
                match rs[a].0.cmp(&cs[b].0) {
                    std::cmp::Ordering::Less => a += 1,
                    std::cmp::Ordering::Greater => b += 1,
                    std::cmp::Ordering::Equal => {
                        let x = text(rs[a].1, "id")?;
                        let y = text(cs[b].1, "id")?;
                        links.insert(x.to_owned(), mapped(&s, "transaction", &json!(y)));
                        links.insert(y.to_owned(), mapped(&s, "transaction", &json!(x)));
                        a += 1;
                        b += 1
                    }
                }
            }
        }
    }
    let starting = rows(&data, "payees")?
        .iter()
        .find(|p| {
            p["name"]
                .as_str()
                .is_some_and(|n| n.eq_ignore_ascii_case("Starting Balance"))
        })
        .and_then(|p| p["id"].as_str());
    for (index, r) in roots.iter().enumerate().filter(|(_, r)| live(r)) {
        let id = mapped(&s, "transaction", &r["id"]);
        let account = mapped(&s, "account", &r["account_id"]);
        let account_id = account
            .as_str()
            .ok_or_else(|| invalid("Missing transaction account"))?;
        let subs = child_groups
            .get(text(r, "id")?)
            .cloned()
            .unwrap_or_default();
        let rank = (100001 - index as i64) * 100001;
        let (mut p, imported) = payee(&s, r, &Value::Null);
        let mut cat = mapped(&s, "category", &r["category_id"]);
        if r["payee_id"].as_str() == starting && cat == identity("builtin", "income") {
            cat = json!(identity("builtin", "starting-balances"));
            p = Value::Null;
        }
        if !subs.is_empty() || s.off_budget.get(account_id) == Some(&true) {
            cat = Value::Null
        }
        let transferred = mapped(&s, "transaction", &r["transfer_transaction_id"]);
        let transferred = if transferred.is_null() {
            links.get(text(r, "id")?).cloned().unwrap_or(Value::Null)
        } else {
            transferred
        };
        let cleared = matches!(r["cleared"].as_str(), Some("cleared" | "reconciled"));
        let reconciled = r["cleared"] == "reconciled";
        let d = date(r, "date")?;
        s.push("transactions",json!({"id":id,"acct":account,"amount":amount(&r["amount"])?,"date":d,"description":p,"imported_description":imported,"category":cat,"notes":notes(r,&conflicts),"imported_id":r["import_id"],"transferred_id":transferred,"cleared":i32::from(cleared),"reconciled":i32::from(reconciled),"isParent":i32::from(!subs.is_empty()),"isChild":0,"parent_id":null,"sort_order":rank,"tombstone":0,"starting_balance_flag":0}));
        for (i, c) in subs.iter().enumerate() {
            let (payee, imported) = payee(&s, c, &p);
            s.push("transactions",json!({"id":mapped(&s,"transaction",&c["id"]),"acct":account,"amount":amount(&c["amount"])?,"date":d,"description":payee,"imported_description":imported,"category":mapped(&s,"category",&c["category_id"]),"notes":c["memo"],"transferred_id":links.get(text(c,"id")?).cloned().unwrap_or(Value::Null),"cleared":i32::from(cleared),"reconciled":i32::from(reconciled),"isParent":0,"isChild":1,"parent_id":id,"sort_order":rank-i as i64-1,"tombstone":0,"starting_balance_flag":0}));
        }
    }
    if !data["payee_locations"].is_null() {
        for row in rows(&data, "payee_locations")?.iter().filter(|r| live(r)) {
            let p = mapped(&s, "payee", &row["payee_id"]);
            if p.is_null() {
                continue;
            }
            let parse = |v: &Value| -> Option<f64> {
                if let Some(n) = v.as_f64() {
                    Some(n)
                } else {
                    let t = v.as_str()?.trim_start();
                    (1..=t.len())
                        .rev()
                        .filter(|n| t.is_char_boundary(*n))
                        .find_map(|n| t[..n].parse::<f64>().ok())
                }
            };
            let (Some(lat), Some(lng)) = (parse(&row["latitude"]), parse(&row["longitude"])) else {
                continue;
            };
            if !lat.is_finite()
                || !lng.is_finite()
                || !(-90.0..=90.0).contains(&lat)
                || !(-180.0..=180.0).contains(&lng)
            {
                continue;
            }
            let id = s.map("location", text(row, "id")?)?;
            s.push("payee_locations",json!({"id":id,"payee_id":p,"latitude":lat,"longitude":lng,"tombstone":0,"created_at":0}));
        }
    }
    let mut scheduled_children = BTreeMap::<&str, Vec<&Value>>::new();
    for c in rows(&data, "scheduled_subtransactions")?
        .iter()
        .filter(|c| live(c))
    {
        scheduled_children
            .entry(text(c, "scheduled_transaction_id")?)
            .or_default()
            .push(c);
    }
    let today = chrono::DateTime::from_timestamp(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(Error::internal)?
            .as_secs() as i64,
        0,
    )
    .ok_or_else(|| invalid("Server date outside calendar"))?
    .date_naive()
    .to_string();
    let mut work = 2_000_000;
    let mut schedule_names = BTreeSet::new();
    for r in rows(&data, "scheduled_transactions")?
        .iter()
        .filter(|r| live(r))
    {
        let account = mapped(&s, "account", &r["account_id"]);
        let (p, _) = payee(&s, r, &Value::Null);
        if account.is_null() || p.is_null() {
            continue;
        }
        let id = s.map("schedule", text(r, "id")?)?;
        let rule = identity("schedule-rule", &id);
        let value = recurrence(r)?;
        let next = crate::schedules::template_next(&value, &today, &mut work)?
            .ok_or_else(|| invalid("No supported next schedule date"))?;
        let number = next
            .replace('-', "")
            .parse::<i64>()
            .map_err(|_| invalid("Invalid schedule date"))?;
        let title = r["memo"].as_str().unwrap_or("");
        let title = unique(title, false, &mut schedule_names)?;
        let mut actions = vec![json!({"op":"link-schedule","value":id})];
        let note = notes(r, &conflicts);
        if !note.is_null() {
            actions.push(json!({"op":"set","field":"notes","value":note}))
        }
        let subs = scheduled_children
            .get(text(r, "id")?)
            .cloned()
            .unwrap_or_default();
        if subs.is_empty() {
            let c = mapped(&s, "category", &r["category_id"]);
            if r["transfer_account_id"].is_null() && !c.is_null() {
                actions.push(json!({"op":"set","field":"category","value":c}))
            }
        } else {
            for (i, c) in subs.iter().enumerate() {
                let options = json!({"splitIndex":i+1});
                actions.push(json!({"op":"set-split-amount","value":amount(&c["amount"])?,"options":{"splitIndex":i+1,"method":"fixed-amount"}}));
                if c["memo"].as_str().is_some_and(|n| !n.is_empty()) {
                    actions.push(
                        json!({"op":"set","field":"notes","value":c["memo"],"options":options}),
                    )
                }
                let (p, _) = payee(&s, c, &p);
                if !p.is_null() {
                    actions.push(json!({"op":"set","field":"payee","value":p,"options":options}))
                }
                let cat = mapped(&s, "category", &c["category_id"]);
                if c["transfer_account_id"].is_null() && !cat.is_null() {
                    actions
                        .push(json!({"op":"set","field":"category","value":cat,"options":options}))
                }
            }
        }
        let conditions = json!([{"field":"description","op":"is","value":p},{"field":"acct","op":"is","value":account},{"field":"date","op":"isapprox","value":value},{"field":"amount","op":"is","value":amount(&r["amount"])?}]);
        s.push("rules",json!({"id":rule,"stage":null,"conditions_op":"and","conditions":conditions.to_string(),"actions":serde_json::to_string(&actions).map_err(Error::internal)?,"tombstone":0}));
        s.push("schedules",json!({"id":id,"rule":rule,"name":title,"posts_transaction":0,"completed":0,"tombstone":0,"sort_order":0}));
        s.push("schedules_next_date",json!({"id":identity("schedule-state",&id),"schedule_id":id,"local_next_date":number,"base_next_date":number,"local_next_date_ts":0,"base_next_date_ts":0,"tombstone":0}));
    }
    let mut months = rows(&data, "months")?.iter().collect::<Vec<_>>();
    months.sort_by_key(|m| m["month"].as_str().unwrap_or(""));
    let mut count = 0;
    for m in months {
        let d = date(m, "month")? / 100;
        for c in rows(m, "categories")? {
            let cat = mapped(&s, "category", &c["id"]);
            let group = c["category_group_id"]
                .as_str()
                .and_then(|id| group_by_id.get(id))
                .and_then(|g| g["name"].as_str())
                .unwrap_or("");
            if cat.is_null()
                || matches!(
                    group.to_lowercase().as_str(),
                    "internal master category" | "credit card payments"
                )
            {
                continue;
            }
            count += 1;
            if count > 100000 {
                return Err(invalid("Allocation history exceeds100000 cells"));
            }
            s.push("zero_budgets",json!({"id":identity("budget-cell",&format!("{d}:{cat}")),"month":d,"category":cat,"amount":amount(&c["budgeted"])?,"carryover":0,"goal":null,"long_goal":null}));
        }
    }
    Ok((
        name,
        s,
        vec![
            json!({"feature":"nynab_source_model","count":1,"blocking":false,"message":"Credit-card payment categories, targets, approvals, source currency settings and edit logs are not imported by the baseline. Review available funds and card debt; scheduled payments are manual and flags become tags. Original JSON remains available."}),
        ],
    ))
}
