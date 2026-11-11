use super::calculators::{Environment, adjust};
use super::*;
use rusqlite::{OptionalExtension, params};
struct Target {
    amount: f64,
    date: String,
    months: i32,
    interval: f64,
    frequency: String,
    full: bool,
    index: usize,
}
fn find(env: &Environment, t: &Value) -> Result<(String, String, bool), Error> {
    let id = t["scheduleId"].as_str();
    let name = t["name"].as_str().map(str::trim);
    env.db.query_row("SELECT id,rule_id,completed FROM schedules WHERE budget_id=?1 AND tombstone=0 AND ((?2 IS NOT NULL AND id=?2) OR (?2 IS NULL AND trim(name)=?3)) ORDER BY rowid LIMIT 1",params![env.budget,id,name],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?.ok_or_else(||Error::invalid(format!("Schedule {} does not exist",name.or(id).unwrap_or("without a name"))))
}
pub(super) fn validate(env: &Environment, t: &Value) -> Result<(), Error> {
    find(env, t).map(|_| ())
}
pub(super) fn run(
    env: &mut Environment,
    templates: &[Value],
    category: &Value,
    from: f64,
    already: f64,
) -> Result<(f64, Vec<f64>), Error> {
    let mut targets = vec![];
    let mut weights = vec![0.0; templates.len()];
    let month_first = format!("{}-01", env.month);
    let rules = crate::rules::load(env.db, env.budget)?;
    for (index, t) in templates.iter().enumerate() {
        env.tick()?;
        let (_, rule_id, completed) = find(env, t)?;
        let rule = rules
            .iter()
            .find(|r| r.id == rule_id)
            .ok_or_else(|| Error::invalid("Schedule rule is unavailable"))?;
        let date = rule
            .conditions
            .iter()
            .find(|c| c["field"] == "date")
            .ok_or_else(|| Error::invalid("Schedule date condition is unavailable"))?;
        let raw = rule
            .conditions
            .iter()
            .find(|c| c["field"] == "amount")
            .ok_or_else(|| Error::invalid("Schedule amount condition is unavailable"))?;
        let amount = if raw["op"] == "isbetween" {
            engine::round(
                raw["value"]["num1"]
                    .as_f64()
                    .ok_or_else(|| Error::invalid("Invalid schedule amount"))?
                    + raw["value"]["num2"]
                        .as_f64()
                        .ok_or_else(|| Error::invalid("Invalid schedule amount"))?,
            )? as f64
                / 2.0
        } else {
            raw["value"]
                .as_f64()
                .ok_or_else(|| Error::invalid("Invalid schedule amount"))?
        };
        let adjusted = if t["adjustmentType"] == "fixed" {
            amount + amount.signum() * env.money(t["adjustment"].as_f64().unwrap_or(0.0))?
        } else {
            adjust(amount, t, env)?
        };
        let amount = engine::round(adjusted)?;
        let next = crate::schedules::template_next(&date["value"], &month_first, &mut env.work)?
            .ok_or_else(|| Error::invalid("Schedule has no date"))?;
        let months = super::super::month(&next[..7])? - env.number;
        if months < 0 {
            return Err(Error::invalid(format!(
                "Schedule {} is in the Past",
                t["name"]
            )));
        }
        if completed {
            return Err(Error::invalid(format!(
                "Schedule {} is not active during the month in question",
                t["name"]
            )));
        }
        let draft = json!({"id":null,"sort_order":null,"amount":amount,"category_id":category["id"],"account_id":null,"date":next,"notes":"","cleared":false,"children":[]});
        let after =
            crate::rules::template_actions(env.db, env.budget, &draft, rule, &mut env.expressions)?;
        let sign = if category["is_income"] == true {
            1.0
        } else {
            -1.0
        };
        let allocated = after["children"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| c["category_id"] == category["id"])
            .collect::<Vec<_>>();
        let mut target = sign
            * if allocated.is_empty() {
                after["amount"]
                    .as_f64()
                    .ok_or_else(|| Error::invalid("Calculated schedule amount must be finite"))?
            } else {
                allocated
                    .iter()
                    .map(|c| {
                        c["amount"].as_f64().ok_or_else(|| {
                            Error::invalid("Calculated schedule allocation amount must be finite")
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .sum()
            };
        let interval = date["value"]["interval"].as_f64().unwrap_or(1.0);
        if interval <= 0.0 {
            return Err(Error::invalid("Schedule interval must be positive"));
        }
        let frequency = date["value"]["frequency"].as_str().unwrap_or("").to_owned();
        if date["value"].is_object() {
            let through = super::super::month_name(env.number + months + 1)?;
            let mut occurrence = Some(next.clone());
            let mut count = 0;
            while let Some(day) = occurrence {
                if day >= through {
                    break;
                }
                env.tick()?;
                count += 1;
                occurrence = crate::schedules::template_after(&date["value"], &day, &mut env.work)?;
            }
            target *= count as f64;
        }
        targets.push(Target {
            amount: target,
            date: next,
            months,
            interval,
            frequency,
            full: t["full"] == true,
            index,
        });
    }
    let pay = |t: &Target| {
        t.full
            || (t.frequency == "monthly" || t.frequency.is_empty())
                && t.interval == 1.0
                && t.months == 0
            || t.frequency == "weekly" && t.interval <= 4.0
            || t.frequency == "daily" && t.interval <= 31.0
            || env.method == "tracking"
    };
    let direct = targets.iter().filter(|t| pay(t)).collect::<Vec<_>>();
    let mut sinking = targets.iter().filter(|t| !pay(t)).collect::<Vec<_>>();
    sinking.sort_by(|a, b| a.date.cmp(&b.date));
    let pay_total = direct
        .iter()
        .filter(|t| t.months == 0)
        .map(|t| t.amount)
        .sum::<f64>();
    let sinking_total = sinking.iter().map(|t| t.amount).sum::<f64>();
    let base = |t: &Target| -> Result<f64, Error> {
        Ok(match t.frequency.as_str() {
            "yearly" => t.amount / t.interval / 12.0,
            "monthly" => t.amount / t.interval,
            "weekly" | "daily" => {
                let days = (t.interval * if t.frequency == "weekly" { 7.0 } else { 1.0 }) as i64;
                let next = chrono::NaiveDate::parse_from_str(&t.date, "%Y-%m-%d")
                    .map_err(Error::internal)?;
                let previous = next
                    .checked_sub_signed(chrono::TimeDelta::days(days))
                    .ok_or_else(|| Error::invalid("Schedule interval is outside calendar"))?;
                let months = (super::super::month(&t.date[..7])?
                    - super::super::month(&previous.to_string()[..7])?)
                .max(1);
                t.amount / months as f64
            }
            _ => t.amount / t.interval,
        })
    };
    let previous = env.category(env.number - 1, category["id"].as_str().unwrap())?;
    let last_goal = previous["goal"]
        .as_str()
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let submonthly = targets
        .iter()
        .any(|t| matches!(t.frequency.as_str(), "weekly" | "daily"));
    let balance = from + already;
    let amount = if balance >= sinking_total + pay_total
        || last_goal < sinking_total + pay_total
            && last_goal != 0.0
            && balance >= last_goal
            && submonthly
    {
        let mut total = pay_total;
        for t in &sinking {
            let contribution = base(t)?;
            total += contribution;
            weights[t.index] += contribution;
        }
        for t in &direct {
            if t.months == 0 {
                weights[t.index] += t.amount;
            }
        }
        already + engine::round(total)? as f64
    } else {
        let mut remaining = 0.0;
        let mut total = 0.0;
        for (index, t) in sinking.iter().enumerate() {
            remaining = if index == 0 {
                t.amount - from
            } else {
                t.amount - remaining
            };
            let need = if remaining >= 0.0 {
                let need = remaining;
                remaining = 0.0;
                need
            } else {
                remaining = remaining.abs();
                0.0
            };
            let contribution = need / (t.months + 1) as f64;
            total += contribution;
            weights[t.index] += contribution;
        }
        for t in &direct {
            if t.months == 0 {
                weights[t.index] += t.amount;
            }
        }
        already + engine::round(pay_total + total)? as f64
            - if sinking.is_empty() { from } else { 0.0 }
    };
    Ok((amount, weights))
}
