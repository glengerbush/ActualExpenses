use super::*;
use chrono::{Datelike, Months, NaiveDate};
pub fn horizon_days(value: &str, today: &str) -> Result<i64, Error> {
    crate::models::validate_date(today)?;
    let day = NaiveDate::parse_from_str(today, "%Y-%m-%d").map_err(Error::internal)?;
    let month = NaiveDate::from_ymd_opt(day.year(), day.month(), 1).unwrap();
    let result = match value {
        "currentMonth" => {
            (month
                .checked_add_months(Months::new(1))
                .ok_or_else(|| Error::invalid("Horizon outside calendar"))?
                - day)
                .num_days()
                - 1
        }
        "oneMonth" => (month
            .checked_add_months(Months::new(1))
            .ok_or_else(|| Error::invalid("Horizon outside calendar"))?
            - month)
            .num_days(),
        _ => {
            if let Some((n, unit)) = value.split_once('-') {
                let n = n
                    .parse::<u32>()
                    .map_err(|_| Error::invalid("Invalid upcoming length"))?
                    .max(1);
                match unit {
                    "day" => i64::from(n),
                    "week" => i64::from(n) * 7,
                    "month" | "year" => {
                        let months = n
                            .checked_mul(if unit == "year" { 12 } else { 1 })
                            .ok_or_else(|| Error::invalid("Upcoming length exceeds calendar"))?;
                        (month
                            .checked_add_months(Months::new(months))
                            .ok_or_else(|| Error::invalid("Upcoming length exceeds calendar"))?
                            - month)
                            .num_days()
                            + 1
                    }
                    _ => return Err(Error::invalid("Use days, weeks, months or years")),
                }
            } else {
                value
                    .parse::<i64>()
                    .map_err(|_| Error::invalid("Invalid upcoming length"))?
            }
        }
    };
    if !(0..=36600).contains(&result) {
        return Err(Error::invalid("Upcoming length must fit within100 years"));
    }
    Ok(result)
}
pub fn has_transactions(db: &Connection, b: &str, row: &Value) -> Result<bool, Error> {
    let Some(date) = row["next_date"].as_str() else {
        return Ok(false);
    };
    let condition = calendar::date_condition(row["conditions"].as_array().unwrap())?;
    let start = if condition["op"] == "is" || row["posts_transaction"] == true {
        date.to_owned()
    } else {
        calendar::shift(date, -2)?
    };
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM schedule_links l JOIN transactions t ON t.budget_id=l.budget_id AND t.id=l.id WHERE l.budget_id=?1 AND l.schedule_id=?2 AND l.tombstone=0 AND t.tombstone=0 AND t.date>=?3)",params![b,row["id"].as_str(),start],|r|r.get(0))?)
}
pub fn list(db: &Connection, b: &str) -> Result<Value, Error> {
    let today = calendar::day()?;
    let horizon: String = db.query_row(
        "SELECT value FROM schedule_settings WHERE budget_id=?1 AND id='horizon'",
        [b],
        |r| r.get(0),
    )?;
    let mut rows = data::list(db, b)?;
    let mut previews = vec![];
    let mut work = 2_000_000usize;
    for row in &mut rows {
        let length = row["custom_upcoming_length"].as_str().unwrap_or(&horizon);
        let end = calendar::shift(&today, horizon_days(length, &today)?)?;
        let paid = has_transactions(db, b, row)?;
        let date_owned = row["next_date"].as_str().map(str::to_owned);
        let date = date_owned.as_deref();
        let status = if row["completed"] == true {
            "completed"
        } else if paid {
            "paid"
        } else if date == Some(today.as_str()) {
            "due"
        } else if date.is_some_and(|d| d < today.as_str()) {
            "missed"
        } else if date.is_some_and(|d| d <= end.as_str()) {
            "upcoming"
        } else {
            "scheduled"
        };
        row["status"] = json!(status);
        let conditions = row["conditions"].as_array().unwrap().clone();
        for (target, field) in [
            ("account_id", "account"),
            ("payee_id", "payee"),
            ("amount", "amount"),
        ] {
            row[target] = conditions
                .iter()
                .find(|c| {
                    c["field"] == field
                        && matches!(c["op"].as_str(), Some("is" | "isapprox" | "isbetween"))
                })
                .map(|c| c["value"].clone())
                .unwrap_or(Value::Null);
        }
        if !matches!(status, "completed" | "scheduled") {
            if let Some(date) = date.map(str::to_owned) {
                let value =
                    calendar::date_condition(row["conditions"].as_array().unwrap())?["value"]
                        .clone();
                let mut dates = vec![date.clone()];
                let mut current = date;
                while value.is_object() && current <= end {
                    let Some(next) = calendar::after(&value, &current, &mut work)? else {
                        break;
                    };
                    if next > end {
                        break;
                    }
                    dates.push(next.clone());
                    current = next;
                    if dates.len() > 1000 {
                        return Err(Error::invalid(
                            "Narrow upcoming length to preview fewer than1000 occurrences",
                        ));
                    }
                }
                if paid {
                    dates.remove(0);
                }
                for date in dates {
                    previews.push(json!({"id":format!("preview/{}/{}",row["id"].as_str().unwrap(),date),"schedule_id":row["id"],"date":date,"account_id":row["account_id"],"payee_id":row["payee_id"],"amount":commands::scheduled_amount(&row["amount"])?,"status":status}));
                }
            }
        }
    }
    Ok(
        json!({"schedules":rows,"previews":previews,"upcoming_length":horizon,"today":today,"snapshot":sync::cursor(db,b)?,"epoch":crate::lifecycle::epoch(db,b)?}),
    )
}
