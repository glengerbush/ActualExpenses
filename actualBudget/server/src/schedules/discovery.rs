//! Pinned Actual26.10 find-schedules search, with bounded read-only work.
use super::*;
use chrono::{Datelike, Months, NaiveDate, TimeDelta};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    snapshot: String,
    as_of: Option<String>,
    account_id: Option<String>,
    #[serde(default)]
    offset: usize,
    #[serde(default = "super::page_limit")]
    limit: usize,
}
#[derive(Clone)]
struct Row {
    date: NaiveDate,
    amount: i64,
    payee: Option<String>,
    parent: bool,
    child: bool,
    linked: bool,
    transfer: bool,
}
#[derive(Clone)]
struct Candidate {
    rank: f64,
    account: String,
    payee: Option<String>,
    amount: i64,
    config: Value,
    exact_date: bool,
    exact_amount: bool,
}
fn spend(work: &mut crate::rules::Work) -> Result<(), Error> {
    work.remaining = work.remaining.checked_sub(1).ok_or_else(|| {
        Error::invalid("Schedule discovery exceeds its work limit. Narrow the account.")
    })?;
    Ok(())
}
fn date(s: &str) -> Result<NaiveDate, Error> {
    crate::models::validate_date(s)?;
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(Error::internal)
}
fn days(d: NaiveDate, n: i64) -> Result<NaiveDate, Error> {
    d.checked_add_signed(TimeDelta::days(n))
        .filter(|v| v.year() > 0 && v.year() <= 9999)
        .ok_or_else(|| Error::invalid("Discovery date outside supported calendar"))
}
fn months(d: NaiveDate, n: u32) -> Result<NaiveDate, Error> {
    d.checked_sub_months(Months::new(n))
        .filter(|v| v.year() > 0)
        .ok_or_else(|| Error::invalid("Discovery date outside supported calendar"))
}
fn rank(expected: NaiveDate, actual: NaiveDate) -> f64 {
    1.0 / ((expected - actual).num_days().abs() + 1) as f64
}
fn configs(latest: NaiveDate, as_of: NaiveDate) -> Result<Vec<Value>, Error> {
    let mut result = vec![];
    for (back, interval) in [(28, 1), (49, 2)] {
        for i in 0..14 {
            let mut c = json!({"frequency":"weekly","start":days(latest,-back+i)?.to_string()});
            if interval == 2 {
                c["interval"] = json!(2);
            }
            result.push(c);
        }
    }
    let first = months(latest, 4)?;
    for i in 0..62 {
        let start = days(first, i)?;
        if start.day() <= 28 {
            result.push(json!({"start":start.to_string(),"frequency":"monthly"}));
        }
    }
    for back in [3, 4] {
        result.push(json!({"frequency":"monthly","patterns":[{"type":"day","value":-1}],"start":months(latest,back)?.to_string()}));
    }
    // The source uses today's weekday, and eight *months* for 2nd/4th patterns.
    // Preserve both quirks rather than substituting a different detector.
    let weekday =
        ["MO", "TU", "WE", "TH", "FR", "SA", "SU"][as_of.weekday().num_days_from_monday() as usize];
    for (first, positions) in [(days(latest, -56)?, [1, 3]), (months(latest, 8)?, [2, 4])] {
        for i in 0..14 {
            result.push(json!({"start":days(first,i)?.to_string(),"frequency":"monthly","patterns":[{"type":weekday,"value":positions[0]},{"type":weekday,"value":positions[1]}]}));
        }
    }
    Ok(result)
}
fn window(rows: &BTreeMap<NaiveDate, Vec<Row>>, day: NaiveDate) -> Result<Vec<&Row>, Error> {
    Ok(rows
        .range(days(day, -2)?..=days(day, 2)?)
        .rev()
        .flat_map(|(_, rows)| rows.iter())
        .filter(|r| !r.child && !r.linked && !r.transfer)
        .collect())
}
fn collect(
    account: &str,
    rows: &BTreeMap<NaiveDate, Vec<Row>>,
    latest: NaiveDate,
    as_of: NaiveDate,
    winners: &mut BTreeMap<Option<String>, Candidate>,
    work: &mut crate::rules::Work,
) -> Result<(), Error> {
    for config in configs(latest, as_of)? {
        let start = config["start"].as_str().unwrap();
        let dates = calendar::dates(&config, start, 3, &mut work.remaining)?
            .iter()
            .map(|d| date(d))
            .collect::<Result<Vec<_>, _>>()?;
        if dates.len() != 3 {
            continue;
        }
        let windows = dates
            .iter()
            .map(|d| window(rows, *d))
            .collect::<Result<Vec<_>, _>>()?;
        for base in &windows[2] {
            spend(work)?;
            let threshold = ((base.amount as f64).abs() * 0.075).round();
            let low = base.amount as f64 - threshold;
            let high = base.amount as f64 + threshold;
            let mut score = rank(dates[2], base.date);
            let mut exact_amount = true;
            let mut found = true;
            for index in [1, 0] {
                let mut matched = None;
                for row in &windows[index] {
                    spend(work)?;
                    if (row.amount as f64) >= low && (row.amount as f64) <= high {
                        matched = Some(*row);
                        break;
                    }
                }
                // Source picks the first amount match, then compares payee. It
                // does not continue searching for a later same-payee candidate.
                if let Some(row) = matched.filter(|r| r.payee == base.payee) {
                    score += rank(dates[index], row.date);
                    exact_amount &= row.amount == base.amount;
                } else {
                    found = false;
                    break;
                }
            }
            if found && winners.get(&base.payee).is_none_or(|old| score > old.rank) {
                winners.insert(
                    base.payee.clone(),
                    Candidate {
                        rank: score,
                        account: account.into(),
                        payee: base.payee.clone(),
                        amount: base.amount,
                        config: config.clone(),
                        exact_date: score == 3.0,
                        exact_amount,
                    },
                );
            }
        }
    }
    Ok(())
}
fn conditions(c: &Candidate) -> Vec<Value> {
    vec![
        json!({"op":"is","field":"account","value":c.account}),
        json!({"op":"is","field":"payee","value":c.payee}),
        json!({"op":if c.exact_date{"is"}else{"isapprox"},"field":"date","value":c.config}),
        json!({"op":if c.exact_amount{"is"}else{"isapprox"},"field":"amount","value":c.amount}),
    ]
}
fn extend_start(
    db: &Connection,
    b: &str,
    c: &mut Candidate,
    rows: &BTreeMap<NaiveDate, Vec<Row>>,
    work: &mut crate::rules::Work,
) -> Result<(), Error> {
    loop {
        spend(work)?;
        let previous = c.config.clone();
        let start = date(previous["start"].as_str().unwrap())?;
        let interval = previous["interval"].as_u64().unwrap_or(1);
        let earlier = match previous["frequency"].as_str().unwrap() {
            "weekly" => days(start, -7 * interval as i64),
            "monthly" => months(start, interval as u32),
            _ => return Err(Error::internal("Unexpected discovery frequency")),
        };
        let Ok(earlier) = earlier else {
            break;
        };
        c.config["start"] = json!(earlier.to_string());
        let occurs = calendar::dates(&c.config, &earlier.to_string(), 1, &mut work.remaining)?;
        let Some(first) = occurs.first() else {
            c.config = previous;
            break;
        };
        let first = date(first)?;
        let width = if c.exact_date { 0 } else { 2 };
        let mut query = conditions(c);
        // findStartDate uses recurDateBounds=1, unlike normal live rule matching.
        query[2]["value"] = json!(first.to_string());
        let mut found = false;
        for row in rows
            .range(days(first, -width)?..=days(first, width)?)
            .flat_map(|(_, rows)| rows)
            .filter(|r| !r.parent)
        {
            let value = json!({"account_id":c.account,"payee_id":row.payee,"amount":row.amount,"date":row.date.to_string()});
            if crate::rules::match_schedule_conditions(db, b, &query, &value, None, work)? {
                found = true;
                break;
            }
        }
        if !found {
            c.config = previous;
            break;
        }
    }
    Ok(())
}
fn discover(db: &Connection, b: &str, query: &Request, as_of: &str) -> Result<Value, Error> {
    sync::check_snapshot(db, b, Some(&query.snapshot))?;
    if let Some(account) = &query.account_id {
        crate::ledger::account_exists(db, b, account)?;
    }
    let accounts=db.prepare("SELECT id FROM accounts WHERE budget_id=?1 AND tombstone=0 AND closed=0 AND (?2 IS NULL OR id=?2) ORDER BY sort_order,id")?.query_map(params![b,query.account_id],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
    let mut histories = BTreeMap::new();
    let mut total = 0usize;
    let mut winners = BTreeMap::new();
    let mut work = crate::rules::Work::default();
    let today = date(as_of)?;
    for account in accounts {
        let sql = format!(
            "SELECT t.date,t.amount,t.payee_id,t.is_parent,t.parent_id IS NOT NULL,EXISTS(SELECT 1 FROM schedule_links l WHERE l.budget_id=t.budget_id AND l.id=t.id AND l.tombstone=0),p.transfer_account_id IS NOT NULL FROM transactions t LEFT JOIN payees p ON p.budget_id=t.budget_id AND p.id=t.payee_id WHERE t.budget_id=?1 AND t.account_id=?2 AND t.tombstone=0 ORDER BY t.date DESC,t.starting_balance ASC,{} DESC,t.id LIMIT ?3",
            crate::android_upgrade::order::SQL
        );
        let values = db
            .prepare(&sql)?
            .query_map(params![b, account, (100001 - total) as i64], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, bool>(3)?,
                    r.get::<_, bool>(4)?,
                    r.get::<_, bool>(5)?,
                    r.get::<_, bool>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        total += values.len();
        if total > 100000 {
            return Err(Error::invalid(
                "Schedule discovery exceeds100000 rows. Narrow the account.",
            ));
        }
        let mut history: BTreeMap<NaiveDate, Vec<Row>> = BTreeMap::new();
        let mut latest = None;
        for (day, amount, payee, parent, child, linked, transfer) in values {
            let day = date(&day)?;
            if !child {
                latest = Some(latest.map_or(day, |old: NaiveDate| old.max(day)));
            }
            history.entry(day).or_default().push(Row {
                date: day,
                amount,
                payee,
                parent,
                child,
                linked,
                transfer,
            });
        }
        if let Some(latest) = latest {
            collect(&account, &history, latest, today, &mut winners, &mut work)?;
        }
        histories.insert(account, history);
    }
    let mut suggestions = vec![];
    for mut candidate in winners.into_values() {
        let account = candidate.account.clone();
        extend_start(db, b, &mut candidate, &histories[&account], &mut work)?;
        let conditions = conditions(&candidate);
        let id = data::identity(&[
            "schedule-suggestion",
            b,
            &serde_json::to_string(&conditions).map_err(Error::internal)?,
        ]);
        suggestions.push(json!({"id":id,"account_id":candidate.account,"payee_id":candidate.payee,"date":candidate.config,"amount":candidate.amount,"conditions":conditions}));
    }
    suggestions.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    let total = suggestions.len();
    Ok(
        json!({"data":{"suggestions":suggestions.into_iter().skip(query.offset).take(query.limit).collect::<Vec<_>>(),"total_count":total,"next_offset":(query.offset+query.limit<total).then_some(query.offset+query.limit),"snapshot":sync::cursor(db,b)?,"epoch":crate::lifecycle::epoch(db,b)?,"as_of":as_of}}),
    )
}
pub(super) async fn read(
    State(store): State<Store>,
    Path(b): Path<String>,
    query: Result<Query<Request>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    let Query(query) = query.map_err(|_| Error::invalid("Invalid discovery query"))?;
    if query.limit == 0 || query.limit > 1000 || query.offset > 100000 {
        return Err(Error::invalid("Discovery pages contain1–1000 suggestions"));
    }
    if let Some(account) = &query.account_id {
        crate::models::validate_id(account)?;
    }
    let as_of = query.as_of.clone().map_or_else(calendar::day, Ok)?;
    crate::models::validate_date(&as_of)?;
    Ok(Json(
        store
            .run(move |db| discover(db, &b, &query, &as_of))
            .await?,
    ))
}
