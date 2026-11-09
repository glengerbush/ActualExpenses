use super::*;
use chrono::{Datelike, NaiveDate, TimeDelta};
pub fn day() -> Result<String, Error> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(Error::internal)?
        .as_secs();
    chrono::DateTime::from_timestamp(seconds as i64, 0)
        .map(|d| d.date_naive().to_string())
        .ok_or_else(|| Error::internal("Server date outside calendar"))
}
pub fn shift(day: &str, n: i64) -> Result<String, Error> {
    crate::models::validate_date(day)?;
    NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map_err(Error::internal)?
        .checked_add_signed(TimeDelta::days(n))
        .filter(|d| d.year() <= 9999)
        .map(|d| d.to_string())
        .ok_or_else(|| Error::invalid("Date outside schedule calendar"))
}
pub fn date_condition(conditions: &[Value]) -> Result<&Value, Error> {
    conditions
        .iter()
        .find(|c| c["field"] == "date" && matches!(c["op"].as_str(), Some("is" | "isapprox")))
        .ok_or_else(|| Error::invalid("A schedule requires an exact or approximate date condition"))
}
pub fn dates(
    value: &Value,
    from: &str,
    count: usize,
    work: &mut usize,
) -> Result<Vec<String>, Error> {
    if let Some(date) = value.as_str() {
        crate::models::validate_date(date)?;
        return Ok(vec![date.to_owned()]);
    }
    crate::register_recurrence::schedule_dates(value, from, count, work)
}
pub fn next(value: &Value, from: &str, work: &mut usize) -> Result<Option<String>, Error> {
    let upcoming = dates(value, from, 1, work)?;
    if let Some(date) = upcoming.first() {
        return Ok(Some(date.clone()));
    }
    // Pinned getNextDate retains the final occurrence after an ended recurrence.
    let all = dates(
        value,
        value["start"]
            .as_str()
            .ok_or_else(|| Error::invalid("Missing recurrence start"))?,
        120000,
        work,
    )?;
    Ok(all.last().cloned())
}
pub fn after(value: &Value, previous: &str, work: &mut usize) -> Result<Option<String>, Error> {
    if value.is_string() {
        return Ok(None);
    }
    let mut raw = value.clone();
    raw["skipWeekend"] = json!(false);
    let mut start = shift(previous, -1)?;
    for _ in 0..7 {
        let Some(occurrence) = next(&raw, &start, work)? else {
            return Ok(None);
        };
        if occurrence < start {
            return Ok(None);
        }
        if let Some(date) = next(value, &start, work)?
            && date.as_str() > previous
        {
            return Ok(Some(date));
        }
        start = shift(&occurrence, 1)?;
    }
    Ok(None)
}
