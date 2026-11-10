use super::*;
use chrono::{Datelike, NaiveDate, TimeDelta};
use std::collections::BTreeMap;
pub(super) struct Environment<'a> {
    pub db: &'a Connection,
    pub budget: &'a str,
    pub month: String,
    pub number: i32,
    pub method: String,
    pub multiplier: f64,
    pub hide: bool,
    pub cache: BTreeMap<i32, Value>,
    pub work: usize,
}
impl<'a> Environment<'a> {
    pub fn new(db: &'a Connection, b: &'a str, m: &str, current: Value) -> Result<Self, Error> {
        let p = crate::preferences::get_preferences(db, b)?;
        let imported: Option<String> = db.query_row(
            "SELECT template_currency FROM budget_settings WHERE budget_id=?1",
            [b],
            |r| r.get(0),
        )?;
        let code = imported.as_deref().or(p["currency_code"].as_str());
        let digits = if matches!(code, Some("IRR" | "JPY" | "KRW")) {
            0
        } else {
            2
        };
        let method = current["method"].as_str().unwrap().to_owned();
        let number = super::super::month(m)?;
        let mut cache = BTreeMap::new();
        cache.insert(number, current["months"][m].clone());
        Ok(Self {
            db,
            budget: b,
            month: m.into(),
            number,
            method,
            multiplier: 10_f64.powi(digits),
            hide: p["hide_fraction"] == true,
            cache,
            work: 2_000_000,
        })
    }
    pub fn tick(&mut self) -> Result<(), Error> {
        self.work = self.work.checked_sub(1).ok_or_else(|| {
            Error::invalid("Template work limit reached; narrow the selected categories or history")
        })?;
        Ok(())
    }
    pub fn data(&mut self, n: i32) -> Result<&Value, Error> {
        self.tick()?;
        if !self.cache.contains_key(&n) {
            let name = super::super::month_name(n)?;
            let v = super::super::view(self.db, self.budget, &name, 1)?;
            self.cache.insert(n, v["months"][&name].clone());
        }
        Ok(self.cache.get(&n).unwrap())
    }
    pub fn category(&mut self, n: i32, id: &str) -> Result<Value, Error> {
        self.data(n)?["categories"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .cloned()
            .ok_or_else(|| Error::invalid("Template category is unavailable"))
    }
    pub fn money(&self, v: f64) -> Result<f64, Error> {
        Ok(engine::round(v * self.multiplier)? as f64)
    }
    pub fn fraction(&self, v: f64) -> Result<f64, Error> {
        Ok(engine::round(v / self.multiplier)? as f64 * self.multiplier)
    }
    pub fn value(t: &Value, key: &str) -> Result<f64, Error> {
        let n = t[key]
            .as_f64()
            .ok_or_else(|| Error::invalid(format!("Template {key} must be numeric")))?;
        if !n.is_finite() {
            return Err(Error::invalid("Template number must be finite"));
        }
        Ok(n)
    }
    pub fn average(&mut self, id: &str, months: i32) -> Result<f64, Error> {
        if !(1..=1200).contains(&months) {
            return Err(Error::invalid("Average history must be1–1200 months"));
        }
        let today = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now());
        let current = today.year() * 12 + today.month0() as i32;
        let start = (self.number - 1).min(current - 1);
        let method = self.method.clone();
        let earliest:Option<String>=self.db.query_row("SELECT MIN(month) FROM(SELECT month FROM budget_cells WHERE budget_id=?1 AND method=?2 AND category_id=?3 AND month<=?4 UNION ALL SELECT substr(t.date,1,7) month FROM transactions t JOIN accounts a ON a.budget_id=t.budget_id AND a.id=t.account_id WHERE t.budget_id=?1 AND t.category_id=?3 AND t.tombstone=0 AND t.is_parent=0 AND a.off_budget=0 AND substr(t.date,1,7)<=?4)",rusqlite::params![self.budget,method,id,super::super::month_name(start)?],|r|r.get(0))?;
        let earliest = earliest.as_deref().map(super::super::month).transpose()?;
        let count = months
            .min(earliest.map(|e| start - e + 1).unwrap_or(months))
            .max(0);
        if count == 0 {
            return Ok(0.0);
        }
        let mut total = 0.0;
        for n in 0..count {
            total += self.category(start - n, id)?["actual"]
                .as_f64()
                .unwrap_or(0.0);
        }
        Ok(engine::round(total / count as f64)? as f64)
    }
}
pub(super) fn target_month(t: &Value, current: i32) -> Result<(i32, Option<i32>), Error> {
    let target = super::super::month(
        t["month"]
            .as_str()
            .ok_or_else(|| Error::invalid("Target month is required"))?,
    )?;
    let interval = if t["annual"] == true {
        Some(
            (t["repeat"].as_i64().unwrap_or(1))
                .checked_mul(12)
                .ok_or_else(|| Error::invalid("Repeat interval is too large"))?,
        )
    } else {
        t["repeat"].as_i64()
    };
    let interval = interval
        .map(|n| i32::try_from(n).map_err(|_| Error::invalid("Repeat interval is too large")))
        .transpose()?;
    if interval.is_some_and(|n| n <= 0) {
        return Err(Error::invalid("Repeat interval must be positive"));
    }
    if target < current && interval.is_none() {
        return Err(Error::invalid(
            "Target month has passed, remove or update the target month",
        ));
    }
    let target = if target < current {
        let p = interval.unwrap();
        target + ((current - target + p - 1) / p) * p
    } else {
        target
    };
    Ok((target, interval))
}
pub(super) fn by(
    templates: &[Value],
    env: &Environment,
    from: f64,
) -> Result<(f64, Vec<f64>), Error> {
    let info = templates
        .iter()
        .map(|t| target_month(t, env.number).map(|(n, p)| (n - env.number, p)))
        .collect::<Result<Vec<_>, _>>()?;
    let short = info.iter().map(|r| r.0).min().unwrap_or(0);
    let mut needs = vec![];
    for (t, (months, period)) in templates.iter().zip(info) {
        let amount = env.money(Environment::value(t, "amount")?)?;
        let amount = if months > short {
            if let Some(period) = period {
                engine::round(amount / period as f64 * (period - months + short) as f64)? as f64
            } else {
                engine::round(amount / (months + 1) as f64 * (short + 1) as f64)? as f64
            }
        } else {
            amount
        };
        needs.push(amount);
    }
    Ok((
        engine::round((needs.iter().sum::<f64>() - from) / (short + 1) as f64)? as f64,
        needs,
    ))
}
pub(super) fn period(t: &Value, env: &mut Environment) -> Result<f64, Error> {
    let step = t["period"]["amount"]
        .as_i64()
        .filter(|n| *n > 0 && *n <= 120000)
        .ok_or_else(|| Error::invalid("Periodic interval must be1–120000"))?;
    let unit = t["period"]["period"]
        .as_str()
        .ok_or_else(|| Error::invalid("Periodic unit is required"))?;
    let mut date = t["starting"].as_str().unwrap_or("").to_owned();
    if date.is_empty() {
        date = format!("{}-01", env.month);
    }
    crate::models::validate_date(&date)?;
    let end = super::super::month_name(env.number + 1)?;
    let amount = env.money(Environment::value(t, "amount")?)?;
    let mut result = 0.0;
    while date < end {
        env.tick()?;
        if date >= env.month {
            result += amount;
        }
        date = match unit {
            "day" | "week" => {
                let d = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
                    .map_err(|_| Error::invalid("Invalid periodic calendar date"))?;
                d.checked_add_signed(TimeDelta::days(step * if unit == "week" { 7 } else { 1 }))
                    .ok_or_else(|| Error::invalid("Periodic date is outside supported calendar"))?
                    .to_string()
            }
            "month" | "year" => {
                let n = super::super::month(&date[..7])?
                    + i32::try_from(step * if unit == "year" { 12 } else { 1 })
                        .map_err(|_| Error::invalid("Periodic interval is too large"))?;
                super::super::month_name(n)?
            }
            _ => return Err(Error::invalid("Unrecognized periodic period")),
        };
    }
    Ok(result)
}
pub(super) fn spend(t: &Value, env: &mut Environment, id: &str, from: f64) -> Result<f64, Error> {
    let (target, period) = target_month(t, env.number)?;
    let original = super::super::month(t["month"].as_str().unwrap())?;
    let mut start = super::super::month(
        t["from"]
            .as_str()
            .ok_or_else(|| Error::invalid("Spend start month is required"))?,
    )?;
    if period.is_some() {
        start += target - original;
    }
    let mut already = from;
    for n in start..env.number {
        env.tick()?;
        let row = env.category(n, id)?;
        if n == start {
            already = row["balance"].as_f64().unwrap() - row["actual"].as_f64().unwrap();
        } else {
            already += row["budgeted"].as_f64().unwrap();
        }
    }
    let amount = env.money(Environment::value(t, "amount")?)?;
    Ok(engine::round((amount - already) / (target - env.number + 1) as f64)? as f64)
}
pub(super) fn adjust(value: f64, t: &Value, env: &Environment) -> Result<f64, Error> {
    let Some(n) = t["adjustment"].as_f64() else {
        return Ok(value);
    };
    let result = match t["adjustmentType"].as_str() {
        Some("percent") => {
            if n <= -100.0 || n > 1000.0 {
                return Err(Error::invalid(
                    "Adjustment percentage must be greater than−100 and at most1000",
                ));
            }
            value * (1.0 + n / 100.0)
        }
        Some("fixed") => value + env.money(n)?,
        _ => return Err(Error::invalid("Adjustment type must be fixed or percent")),
    };
    Ok(result)
}
