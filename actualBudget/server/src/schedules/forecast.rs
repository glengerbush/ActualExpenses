//! Bounded read-only schedule expansion; never completion evidence or posting.
use super::*;
pub(crate) fn inputs(
    db: &Connection,
    b: &str,
    through: &str,
    work: &mut usize,
) -> Result<Vec<Value>, Error> {
    let mut output = vec![];
    for row in data::forecast_rows(db, b)? {
        if row["completed"] == true {
            continue;
        }
        let Some(from) = row["next_date"].as_str() else {
            continue;
        };
        let conditions = row["conditions"]
            .as_array()
            .ok_or_else(|| Error::invalid("Forecast schedule has invalid conditions"))?;
        let date_condition = calendar::date_condition(conditions)?;
        let amount = conditions
            .iter()
            .find(|c| c["field"] == "amount")
            .map(|c| commands::scheduled_amount(&c["value"]))
            .transpose()?;
        let Some(amount) = amount else {
            continue;
        };
        let account = conditions
            .iter()
            .find(|c| matches!(c["field"].as_str(), Some("account" | "acct")) && c["op"] == "is")
            .and_then(|c| c["value"].as_str());
        let payee = conditions
            .iter()
            .find(|c| {
                matches!(c["field"].as_str(), Some("payee" | "description")) && c["op"] == "is"
            })
            .map(|c| c["value"].clone())
            .unwrap_or(Value::Null);
        let mut dates = std::collections::BTreeSet::new();
        dates.insert(from.to_owned());
        if date_condition["value"].is_string() {
            dates.clear();
            dates.insert(date_condition["value"].as_str().unwrap().to_owned());
        } else if from <= through {
            let span = (chrono::NaiveDate::parse_from_str(through, "%Y-%m-%d")
                .map_err(Error::internal)?
                - chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d").map_err(Error::internal)?)
            .num_days() as usize;
            let count = span
                .saturating_add(3)
                .saturating_mul(3)
                .saturating_add(6)
                .min(10001);
            dates.extend(calendar::dates(
                &date_condition["value"],
                from,
                count,
                work,
            )?);
        }
        for date in dates.into_iter().filter(|date| date.as_str() <= through) {
            if output.len() >= 10000 {
                return Err(Error::invalid(
                    "Forecast would expand more than10000 schedule dates. Review overdue schedules or choose an earlier forecast end.",
                ));
            }
            output.push(json!({"date":date,"amount":amount,"account_id":account,"payee_id":payee,"schedule_id":row["id"],"schedule_name":row["name"].as_str().unwrap_or("Unknown"),"posted_from":if date_condition["op"]=="is"||row["posts_transaction"]==true{date.clone()}else{calendar::shift(&date,-2)?}}));
        }
    }
    Ok(output)
}
