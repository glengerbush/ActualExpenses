//! Actual 26.10 register date filters inspect the first 100 raw occurrences.
//! Weekend shifting is schedule posting behavior, not conditionsToAQL behavior.
use crate::{error::Error, models::validate_date};
use chrono::{Datelike, Months, NaiveDate, TimeDelta};
use serde::Deserialize;
use serde_json::Value;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Recurrence {
    frequency: String,
    start: String,
    interval: Option<u32>,
    #[serde(default)]
    patterns: Vec<Pattern>,
    end_mode: Option<String>,
    end_occurrences: Option<u32>,
    end_date: Option<String>,
    skip_weekend: Option<bool>,
    weekend_solve_mode: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pattern {
    r#type: String,
    value: i32,
}
pub fn dates(value: &Value) -> Result<Vec<String>, Error> {
    let config: Recurrence = serde_json::from_value(value.clone())
        .map_err(|_| Error::invalid("Invalid recurring date filter"))?;
    validate_date(&config.start)?;
    let start = NaiveDate::parse_from_str(&config.start, "%Y-%m-%d").map_err(Error::internal)?;
    let interval = config.interval.unwrap_or(1);
    if interval == 0 || interval > 10000 || config.patterns.len() > 31 {
        return Err(Error::invalid("Invalid recurrence interval or patterns"));
    }
    let _ = config.skip_weekend;
    if config
        .weekend_solve_mode
        .as_deref()
        .is_some_and(|v| !["before", "after"].contains(&v))
    {
        return Err(Error::invalid("Invalid weekend mode"));
    }
    let count = match config.end_mode.as_deref() {
        None | Some("never") => 100,
        Some("after_n_occurrences") => config
            .end_occurrences
            .filter(|v| *v > 0)
            .ok_or_else(|| Error::invalid("Invalid recurrence count"))?
            .min(100),
        Some("on_date") => 100,
        _ => return Err(Error::invalid("Invalid recurrence end mode")),
    } as usize;
    let end = if config.end_mode.as_deref() == Some("on_date") {
        let date = config
            .end_date
            .as_deref()
            .ok_or_else(|| Error::invalid("Missing recurrence end date"))?;
        validate_date(date)?;
        NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(Error::internal)?
    } else {
        NaiveDate::from_ymd_opt(9999, 12, 31).unwrap()
    };
    let mut groups = [Vec::new(), Vec::new()];
    for pattern in &config.patterns {
        if pattern.r#type == "day" {
            if pattern.value == 0 || pattern.value.unsigned_abs() > 31 {
                return Err(Error::invalid("Invalid monthly day"));
            }
        } else if !["SU", "MO", "TU", "WE", "TH", "FR", "SA"].contains(&pattern.r#type.as_str())
            || pattern.value == 0
            || pattern.value.unsigned_abs() > 5
        {
            return Err(Error::invalid("Invalid weekday pattern"));
        }
    }
    for n in 0u32..120000 {
        if groups.iter().all(|g| g.len() >= count) {
            break;
        }
        let next = match config.frequency.as_str() {
            "daily" | "weekly" => start.checked_add_signed(TimeDelta::days(
                i64::from(n)
                    * i64::from(interval)
                    * if config.frequency == "weekly" { 7 } else { 1 },
            )),
            "monthly" => NaiveDate::from_ymd_opt(start.year(), start.month(), 1)
                .unwrap()
                .checked_add_months(Months::new(n.saturating_mul(interval))),
            "yearly" => start
                .year()
                .checked_add((n.saturating_mul(interval)) as i32)
                .and_then(|year| NaiveDate::from_ymd_opt(year, 1, 1)),
            _ => return Err(Error::invalid("Invalid recurrence frequency")),
        };
        let Some(next) = next else {
            break;
        };
        if next > end || next.year() > 9999 {
            break;
        }
        let mut candidates: Vec<(usize, NaiveDate)> = vec![];
        match config.frequency.as_str() {
            "monthly" if !config.patterns.is_empty() => {
                let days = (28..=31)
                    .rev()
                    .find(|day| NaiveDate::from_ymd_opt(next.year(), next.month(), *day).is_some())
                    .unwrap();
                for day in 1..=days {
                    let date = NaiveDate::from_ymd_opt(next.year(), next.month(), day).unwrap();
                    for pattern in &config.patterns {
                        let group = usize::from(pattern.r#type != "day");
                        let matched = if group == 0 {
                            day as i32
                                == if pattern.value > 0 {
                                    pattern.value
                                } else {
                                    days as i32 + 1 + pattern.value
                                }
                        } else {
                            let weekday = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"]
                                [date.weekday().num_days_from_monday() as usize];
                            weekday == pattern.r#type
                                && if pattern.value > 0 {
                                    (day as i32 - 1) / 7 + 1 == pattern.value
                                } else {
                                    -((days as i32 - day as i32) / 7 + 1) == pattern.value
                                }
                        };
                        if matched {
                            candidates.push((group, date));
                        }
                    }
                }
            }
            "monthly" => {
                if let Some(date) = NaiveDate::from_ymd_opt(next.year(), next.month(), start.day())
                {
                    candidates.push((0, date));
                }
            }
            "yearly" => {
                if let Some(date) = NaiveDate::from_ymd_opt(next.year(), start.month(), start.day())
                {
                    candidates.push((0, date));
                }
            }
            _ => candidates.push((0, next)),
        }
        for (group, date) in candidates {
            if date >= start
                && date <= end
                && groups[group].len() < count
                && !groups[group].contains(&date)
            {
                groups[group].push(date);
            }
        }
        if groups[0].len() >= count
            && (config.frequency != "monthly" || !config.patterns.iter().any(|p| p.r#type != "day"))
        {
            break;
        }
        if groups[1].len() >= count && !config.patterns.iter().any(|p| p.r#type == "day") {
            break;
        }
    }
    let mut result = groups.concat();
    result.sort();
    result.dedup();
    result.truncate(100);
    Ok(result.iter().map(ToString::to_string).collect())
}
