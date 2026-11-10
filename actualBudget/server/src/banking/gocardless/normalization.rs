//! Pinned bank selection and normalized core input, before the shared writer.
use super::*;
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use std::{cmp::Ordering, collections::BTreeMap, sync::LazyLock};
static BANKS: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("bank-registry.json")).expect("bundled pinned bank registry")
});
pub(super) fn family(institution: &str) -> &str {
    BANKS
        .get(institution)
        .map(String::as_str)
        .unwrap_or("default")
}
pub(super) fn supported(institution: &str) -> Result<(), Error> {
    if !matches!(family(institution), "default" | "ing_ingddeff") {
        return Err(transport::failure(
            "provider_bank_unsupported",
            "This institution's bank-specific normalization is not available yet. No financial data was applied",
        ));
    }
    Ok(())
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::String(v) => !v.is_empty(),
        Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
        _ => true,
    }
}
fn text(v: &Value) -> Result<&str, Error> {
    v.as_str().filter(|v| v.len() <= 40000).ok_or_else(|| {
        transport::failure(
            "provider_invalid_response",
            "A bank text field is invalid or exceeds40000 bytes",
        )
    })
}
fn nonnull<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.get(key).filter(|v| !v.is_null())
}
fn join(v: &Value, separator: &str) -> Result<String, Error> {
    let rows = v.as_array().filter(|r| r.len() <= 1000).ok_or_else(|| {
        transport::failure("provider_invalid_response", "A bank text array is invalid")
    })?;
    rows.iter()
        .map(|v| if v.is_null() { Ok("") } else { text(v) })
        .collect::<Result<Vec<_>, _>>()
        .map(|r| r.join(separator))
}
fn timestamp(raw: &str) -> Result<i64, Error> {
    if raw.len() > 128 {
        return Err(transport::failure(
            "provider_invalid_response",
            "A bank date is invalid",
        ));
    }
    if let Ok(date) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return Ok(date
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis());
    }
    if let Ok(date) = DateTime::parse_from_rfc3339(raw) {
        return Ok(date.timestamp_millis());
    }
    for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M"] {
        if let Ok(date) = NaiveDateTime::parse_from_str(raw, format) {
            return Ok(date.and_utc().timestamp_millis());
        }
    }
    Err(transport::failure(
        "provider_invalid_response",
        "A bank transaction date is invalid. No data was applied",
    ))
}
fn date(raw: &str) -> Result<String, Error> {
    DateTime::<Utc>::from_timestamp_millis(timestamp(raw)?)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .ok_or_else(|| {
            transport::failure(
                "provider_invalid_response",
                "A bank transaction date is outside the supported calendar",
            )
        })
}
fn payee(trans: &Value) -> Result<String, Error> {
    let amount = trans["transactionAmount"]["amount"]
        .as_str()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .ok_or_else(|| {
            transport::failure("provider_invalid_response", "A bank amount is invalid")
        })?;
    let incoming = amount > 0.0 || (amount == 0.0 && !amount.is_sign_negative());
    let mut name = trans[if incoming {
        "debtorName"
    } else {
        "creditorName"
    }]
    .clone();
    let mut account = trans[if incoming {
        "debtorAccount"
    } else {
        "creditorAccount"
    }]
    .clone();
    if !truthy(&name) {
        account = if truthy(&trans["debtorAccount"]) {
            trans["debtorAccount"].clone()
        } else {
            trans["creditorAccount"].clone()
        };
    }
    if !truthy(&name) {
        let array = if let Some(v) = nonnull(trans, "remittanceInformationUnstructuredArray") {
            join(v, ", ")?
        } else {
            String::new()
        };
        name = [
            trans["debtorName"].clone(),
            trans["creditorName"].clone(),
            trans["remittanceInformationUnstructured"].clone(),
            json!(array),
            trans["additionalInformation"].clone(),
        ]
        .into_iter()
        .find(truthy)
        .unwrap_or(Value::Null);
    }
    let mut parts = vec![];
    if truthy(&name) {
        parts.push(crate::transaction_imports::bank_title(text(&name)?));
    }
    if account.is_object() && truthy(&account["iban"]) {
        let iban = text(&account["iban"])?;
        if !iban.is_ascii() || iban.len() > 128 {
            return Err(transport::failure(
                "provider_invalid_response",
                "A provider account identifier is invalid",
            ));
        }
        parts.push(format!(
            "({} XXX {})",
            &iban[..iban.len().min(4)],
            &iban[iban.len().saturating_sub(4)..]
        ));
    }
    Ok(parts.join(" "))
}
pub(super) fn account(details: &Value, institution: &Value) -> Result<String, Error> {
    supported(institution["id"].as_str().unwrap_or(""))?;
    let name = nonnull(details, "name")
        .or_else(|| nonnull(details, "displayName"))
        .or_else(|| nonnull(details, "product"));
    let mut parts = vec![];
    if let Some(name) = name
        && truthy(name)
    {
        parts.push(text(name)?.to_owned());
    }
    if truthy(&details["iban"]) {
        let iban = text(&details["iban"])?;
        if !iban.is_ascii() || iban.len() > 128 {
            return Err(transport::failure(
                "provider_invalid_response",
                "A provider account identifier is invalid",
            ));
        }
        parts.push(format!("(XXX {})", &iban[iban.len().saturating_sub(4)..]));
    }
    if truthy(&details["currency"]) {
        parts.push(text(&details["currency"])?.to_owned());
    }
    Ok(parts.join(" "))
}
pub(super) fn transactions(raw: &Value, institution: &str) -> Result<Vec<Value>, Error> {
    supported(institution)?;
    let mut rows = vec![];
    for (kind, booked) in [("booked", true), ("pending", false)] {
        let source = raw["transactions"][kind]
            .as_array()
            .filter(|r| r.len() <= 10000)
            .ok_or_else(|| {
                transport::failure(
                    "provider_response_limit",
                    "Provider transactions must contain bounded booked/pending arrays",
                )
            })?;
        for trans in source {
            if rows.len() >= 10000 {
                return Err(transport::failure(
                    "provider_response_limit",
                    "A provider account exceeds10000 transactions. Request a narrower range",
                ));
            }
            let Some(raw_date) = [
                "date",
                "bookingDate",
                "bookingDateTime",
                "valueDate",
                "valueDateTime",
            ]
            .iter()
            .find_map(|k| trans.get(*k).filter(|v| truthy(v))) else {
                continue;
            };
            let mut edited = trans.clone();
            if family(institution) == "ing_ingddeff"
                && let Some(note) = trans["remittanceInformationUnstructured"].as_str()
            {
                static NOTE: LazyLock<regress::Regex> = LazyLock::new(|| {
                    regress::Regex::new(r"remittanceinformation:(.*)$")
                        .expect("trusted pinned note expression")
                });
                if let Some(found) = NOTE.find(note)
                    && let Some(range) = found.group(1)
                {
                    edited["remittanceInformationUnstructured"] = json!(&note[range]);
                }
            }
            let notes = if let Some(v) = nonnull(&edited, "notes")
                .or_else(|| nonnull(&edited, "remittanceInformationUnstructured"))
            {
                text(v)?.to_owned()
            } else if let Some(v) = nonnull(&edited, "remittanceInformationUnstructuredArray") {
                join(v, " ")?
            } else {
                String::new()
            };
            let name = if let Some(v) = nonnull(&edited, "payeeName") {
                text(v)?.to_owned()
            } else {
                payee(&edited)?
            };
            let mut normalized = trans.clone();
            if !normalized.is_object() {
                return Err(transport::failure(
                    "provider_invalid_response",
                    "A bank transaction is invalid",
                ));
            }
            for field in [
                "remittanceInformationUnstructuredArray",
                "remittanceInformationStructuredArray",
            ] {
                if let Some(v) = nonnull(trans, field) {
                    normalized[format!("{field}String")] = json!(join(v, ",")?);
                }
            }
            normalized["payeeName"] = json!(name);
            normalized["date"] = json!(date(text(raw_date)?)?);
            normalized["notes"] = json!(notes);
            normalized["booked"] = json!(booked);
            rows.push(normalized);
        }
    }
    fn parsed(v: &Value) -> Option<i64> {
        v.as_str().and_then(|s| timestamp(s).ok())
    }
    rows.sort_by(|a, b| {
        if family(institution) == "ing_ingddeff" {
            let a_date = parsed(
                a.get("valueDate")
                    .filter(|v| truthy(v))
                    .unwrap_or(&a["bookingDate"]),
            );
            let b_date = parsed(
                b.get("valueDate")
                    .filter(|v| truthy(v))
                    .unwrap_or(&b["bookingDate"]),
            );
            let order = b_date.cmp(&a_date);
            if order != Ordering::Equal {
                return order;
            }
            let integer = |v: &Value| {
                v.as_str().and_then(|s| {
                    let s = s.trim_start_matches(crate::payees_tags::js_whitespace);
                    let end = s
                        .char_indices()
                        .take_while(|(i, c)| {
                            c.is_ascii_digit() || (*i == 0 && matches!(c, '+' | '-'))
                        })
                        .map(|(i, c)| i + c.len_utf8())
                        .last()?;
                    s[..end].parse::<f64>().ok()
                })
            };
            return match (integer(&a["transactionId"]), integer(&b["transactionId"])) {
                (Some(a), Some(b)) => b.partial_cmp(&a).unwrap_or(Ordering::Equal),
                _ => Ordering::Equal,
            };
        }
        for field in [
            "bookingDate",
            "bookingDateTime",
            "valueDate",
            "valueDateTime",
        ] {
            let (a, b) = (parsed(&a[field]), parsed(&b[field]));
            let order = match (a, b) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Less,
                (Some(_), None) => Ordering::Greater,
                (Some(a), Some(b)) => b.cmp(&a),
            };
            if order != Ordering::Equal {
                return order;
            }
        }
        Ordering::Equal
    });
    Ok(rows)
}
pub(super) fn balance<'a>(
    balances: &'a Value,
    institution: &str,
) -> Result<Option<&'a Value>, Error> {
    let rows = balances["balances"]
        .as_array()
        .filter(|r| r.len() <= 1000)
        .ok_or_else(|| {
            transport::failure("provider_invalid_response", "Provider balances are invalid")
        })?;
    let priorities = if family(institution) == "ing_ingddeff" {
        &["interimBooked"][..]
    } else {
        &[
            "closingBooked",
            "expected",
            "forwardAvailable",
            "interimAvailable",
            "interimBooked",
            "nonInvoiced",
            "openingBooked",
        ][..]
    };
    Ok(priorities
        .iter()
        .find_map(|kind| rows.iter().find(|v| v["balanceType"] == *kind)))
}
pub(super) fn opening(rows: &[Value], balances: &Value, institution: &str) -> Result<i64, Error> {
    let current = balance(balances, institution)?
        .and_then(|v| v["balanceAmount"]["amount"].as_str())
        .filter(|v| !v.is_empty())
        .unwrap_or("0");
    let mut total = super::super::normalize::minor(current)? as i128;
    for row in rows.iter().filter(|r| r["booked"] == true) {
        total -= super::super::normalize::minor(
            row["transactionAmount"]["amount"].as_str().ok_or_else(|| {
                transport::failure("provider_invalid_response", "A bank amount is invalid")
            })?,
        )? as i128;
    }
    if total.abs() > models::MAX_MONEY as i128 {
        return Err(Error::invalid(
            "Calculated opening balance exceeds the exact money range",
        ));
    }
    Ok(total as i64)
}
