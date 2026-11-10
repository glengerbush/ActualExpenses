//! Pinned core bank field mapping over the SimpleFIN transport shape. Monetary
//! input uses the source's fixed two-digit Math.round boundary; ledger math is i64.
use super::*;
use chrono::{DateTime, Utc};
pub(super) fn minor(raw: &str) -> Result<i64, Error> {
    let trimmed = raw.trim_matches(crate::payees_tags::js_whitespace);
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err(Error::invalid(
            "Invalid provider amount. No transactions were saved",
        ));
    }
    let value = trimmed
        .parse::<f64>()
        .map_err(|_| Error::invalid("Provider amounts must be numeric strings"))?
        * 100.0;
    if !value.is_finite() {
        return Err(Error::invalid("Provider amounts must be finite"));
    }
    let floor = value.floor();
    let rounded = if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    };
    if rounded.abs() > models::MAX_MONEY as f64 {
        return Err(Error::invalid(
            "A provider amount exceeds the exact money range",
        ));
    }
    Ok(rounded as i64)
}
fn day(value: &Value) -> Result<String, Error> {
    let time = value
        .as_i64()
        .and_then(|v| DateTime::<Utc>::from_timestamp(v, 0))
        .ok_or_else(|| Error::invalid("A provider transaction has an invalid date"))?;
    let date = time.format("%Y-%m-%d").to_string();
    models::validate_date(&date)?;
    Ok(date)
}
pub(super) fn core(
    account: &Value,
    start: &str,
    end: &str,
    settings: &settings::Settings,
) -> Result<Vec<Value>, Error> {
    let Some(rows) = account.get("transactions") else {
        return Ok(vec![]);
    };
    let rows = rows
        .as_array()
        .filter(|r| r.len() <= 10000)
        .ok_or_else(|| {
            Error::invalid(
                "A provider account exceeds 10000 transactions. Request a narrower range",
            )
        })?;
    let mut output = vec![];
    for row in rows {
        let id = row["id"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 2048)
            .ok_or_else(|| {
                Error::invalid("A provider transaction is missing its stable identity")
            })?;
        let pending = match row.get("pending") {
            None | Some(Value::Null) => row["posted"] == 0,
            Some(Value::Bool(v)) => *v,
            _ => return Err(Error::invalid("Provider pending status must be a boolean")),
        };
        let date = day(if pending {
            &row["transacted_at"]
        } else {
            &row["posted"]
        })?;
        if date.as_str() < start || date.as_str() >= end {
            continue;
        }
        let amount = row["amount"]
            .as_str()
            .filter(|v| v.len() <= 64)
            .ok_or_else(|| Error::invalid("Provider amounts must be numeric strings"))?;
        let payee = row
            .get("payee")
            .filter(|v| !v.is_null())
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| Error::invalid("Invalid provider payee"))
            })
            .transpose()?
            .unwrap_or("");
        let notes = row
            .get("description")
            .filter(|v| !v.is_null())
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| Error::invalid("Invalid provider description"))
            })
            .transpose()?
            .unwrap_or("");
        if payee.len() > 10000 || notes.len() > 40000 {
            return Err(Error::invalid(
                "Provider text exceeds the supported field limits",
            ));
        }
        let mut trans = json!({"transactionId":id,"date":date,"booked":!pending,"payeeName":payee,"notes":notes,"transactionAmount":{"amount":amount,"currency":account["currency"]},"sortOrder":if pending{row["transacted_at"].clone()}else{row["posted"].clone()}});
        if row
            .get("transacted_at")
            .is_some_and(|v| v != 0 && !v.is_null())
        {
            trans["transactedDate"] = json!(day(&row["transacted_at"])?);
        }
        if row.get("posted").is_some_and(|v| v != 0 && !v.is_null()) {
            trans["postedDate"] = json!(day(&row["posted"])?);
        }
        // Only explicitly configured extra mapping fields cross into core input;
        // arbitrary provider URLs/diagnostics/unknown data are not published.
        for mapping in [&settings.mappings.payment, &settings.mappings.deposit] {
            for key in [&mapping.date, &mapping.payee, &mapping.notes] {
                if trans.get(key).is_none()
                    && let Some(v) = row.get(key)
                {
                    trans[key] = v.clone();
                }
            }
        }
        output.push(trans);
    }
    output.sort_by(|a, b| b["sortOrder"].as_i64().cmp(&a["sortOrder"].as_i64()));
    Ok(output)
}
pub(super) fn transactions(
    core: &[Value],
    settings: &settings::Settings,
) -> Result<Vec<Value>, Error> {
    let mut rows = vec![];
    for raw in core {
        if !settings.import_pending && raw["booked"] != true {
            continue;
        }
        let mut source = raw.clone();
        let amount = source["transactionAmount"]["amount"]
            .as_str()
            .ok_or_else(|| Error::invalid("Invalid normalized bank amount"))?;
        let minor = minor(amount)?;
        let mapping = if amount
            .parse::<f64>()
            .map_err(|_| Error::invalid("Invalid bank amount"))?
            <= 0.0
        {
            &settings.mappings.payment
        } else {
            &settings.mappings.deposit
        };
        let date = source
            .get(&mapping.date)
            .filter(|v| !v.is_null())
            .unwrap_or(&source["date"])
            .as_str()
            .ok_or_else(|| Error::invalid("Mapped bank dates must be calendar dates"))?
            .to_owned();
        models::validate_date(&date)?;
        let payee = source
            .get(&mapping.payee)
            .filter(|v| !v.is_null())
            .unwrap_or(&source["payeeName"])
            .as_str()
            .ok_or_else(|| Error::invalid("Mapped bank payees must be text"))?
            .to_owned();
        let notes = if settings.import_notes {
            source
                .get(&mapping.notes)
                .filter(|v| !v.is_null())
                .map(|v| {
                    v.as_str()
                        .ok_or_else(|| Error::invalid("Mapped bank notes must be text"))
                })
                .transpose()?
                .map(|s| {
                    s.trim_matches(crate::payees_tags::js_whitespace)
                        .replace('#', "##")
                })
        } else {
            None
        };
        source["amount"] = source["transactionAmount"]["amount"].clone();
        source["cleared"] = source["booked"].clone();
        source["imported_payee"] = json!(payee.trim_matches(crate::payees_tags::js_whitespace));
        rows.push(json!({"amount":minor,"date":date,"payee_name":payee,"notes":notes.unwrap_or_default(),"cleared":source["booked"],"category_id":null,"imported_id":source["transactionId"],"imported_payee":source["imported_payee"],"raw_bank_transaction":source}));
    }
    Ok(rows)
}
