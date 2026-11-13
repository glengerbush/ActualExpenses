//! Ordered fixed rule actions and exact split allocation.
use super::*;
use sha2::{Digest, Sha256};
pub fn validate(a: &Value) -> Result<(), Error> {
    if a.as_object().is_none_or(|o| {
        o.keys()
            .any(|k| !["op", "field", "value", "options", "type"].contains(&k.as_str()))
    }) {
        return Err(Error::invalid("Unknown action field"));
    }
    let op = a["op"].as_str().unwrap_or("");
    let field = a["field"].as_str().unwrap_or("");
    let value = &a["value"];
    let index = a["options"]["splitIndex"].as_u64().unwrap_or(0);
    if index > 100
        || !a["options"]["splitIndex"].is_null() && a["options"]["splitIndex"].as_u64().is_none()
    {
        return Err(Error::invalid("Split index must be0–100"));
    }
    if !a["options"].is_null()
        && a["options"].as_object().is_none_or(|o| {
            o.keys()
                .any(|k| !["splitIndex", "method"].contains(&k.as_str()))
        })
    {
        return Err(Error::invalid("Unknown action option"));
    }
    match op {
        "set" => {
            match field {
                "account" | "payee" | "category" => {
                    if !(field != "account" && value.is_null()) {
                        crate::models::validate_id(
                            value
                                .as_str()
                                .ok_or_else(|| Error::invalid("Reference action requires an ID"))?,
                        )?;
                    }
                }
                "amount" => {
                    money(
                        value.as_i64().ok_or_else(|| {
                            Error::invalid("Amount action requires exact minor units")
                        })? as i128,
                    )?;
                }
                "date" => crate::models::validate_date(
                    value
                        .as_str()
                        .ok_or_else(|| Error::invalid("Date action requires YYYY-MM-DD"))?,
                )?,
                "cleared" => {
                    if !value.is_boolean() {
                        return Err(Error::invalid("Cleared action requires true or false"));
                    }
                }
                "payee_name" => {
                    let v = value
                        .as_str()
                        .ok_or_else(|| Error::invalid("Payee name requires text"))?;
                    if !v.is_empty() {
                        crate::models::validate_name(v)?;
                    }
                }
                _ => return Err(Error::invalid("Unsupported rule action field")),
            };
        }
        "delete-transaction" => {}
        "link-schedule" => {
            crate::models::validate_id(
                value
                    .as_str()
                    .ok_or_else(|| Error::invalid("Schedule action requires an ID"))?,
            )?;
        }
        "set-split-amount" => {
            if index == 0 {
                return Err(Error::invalid(
                    "Split allocation needs a positive split index",
                ));
            }
            match a["options"]["method"].as_str() {
                Some("fixed-amount") => {
                    money(value.as_i64().ok_or_else(|| {
                        Error::invalid("Fixed allocation requires exact minor units")
                    })? as i128)?;
                }
                Some("fixed-percent") => {
                    percent(value)?;
                }
                Some("remainder") => {}
                _ => return Err(Error::invalid("Unknown split allocation method")),
            }
        }
        _ => return Err(Error::invalid("Unsupported rule action")),
    }
    Ok(())
}

pub fn money(n: i128) -> Result<i64, Error> {
    if n.abs() > crate::models::MAX_MONEY as i128 {
        Err(Error::invalid("Rule amount exceeds the exact-money range"))
    } else {
        Ok(n as i64)
    }
}
fn percent(value: &Value) -> Result<(i128, i128), Error> {
    let n = value
        .as_f64()
        .filter(|n| n.is_finite() && n.abs() <= 1000000.0)
        .ok_or_else(|| Error::invalid("Invalid split percentage"))?;
    // At the100-child exact-money ceiling a smaller percentage rounds to zero.
    if n.abs() < 1e-18 {
        return Ok((0, 1));
    }
    let raw = value.to_string();
    let (mantissa, exponent) = raw
        .split_once(['e', 'E'])
        .map_or((raw.as_str(), 0), |(n, e)| {
            (n, e.parse::<i32>().unwrap_or(0))
        });
    let fractional = mantissa.split_once('.').map_or(0, |(_, v)| v.len() as i32);
    let digits = mantissa.replace('.', "");
    let numerator = digits
        .parse::<i128>()
        .map_err(|_| Error::invalid("Invalid percentage precision"))?;
    let power = exponent - fractional;
    if power >= 0 {
        Ok((
            numerator
                .checked_mul(
                    10i128
                        .checked_pow(power as u32)
                        .ok_or_else(|| Error::invalid("Percentage precision exceeds limit"))?,
                )
                .ok_or_else(|| Error::invalid("Percentage exceeds limit"))?,
            1,
        ))
    } else {
        Ok((
            numerator,
            10i128
                .checked_pow((-power) as u32)
                .ok_or_else(|| Error::invalid("Percentage precision exceeds limit"))?,
        ))
    }
}
// JavaScript rounds ties toward positive infinity, including negative cents.
fn rounded(n: i128, d: i128) -> Result<i64, Error> {
    money((n * 2 + d).div_euclid(d * 2))
}
pub fn identity(parts: &[&str]) -> String {
    let digest = Sha256::digest(json!(parts).to_string().as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    uuid::Uuid::from_bytes(bytes).to_string()
}
fn key(field: &str) -> &str {
    match field {
        "account" => "account_id",
        "payee" => "payee_id",
        "category" => "category_id",
        v => v,
    }
}
fn exec(a: &Value, row: &mut Value) -> Result<(), Error> {
    if matches!(a["field"].as_str(), Some("cleared" | "reconciled")) {
        return Ok(());
    }
    match a["op"].as_str().unwrap() {
        "set" => {
            if a["field"] == "payee_name" {
                row["_rule_payee_name"] = a["value"].clone();
            } else {
                row[key(a["field"].as_str().unwrap())] = a["value"].clone();
                if a["field"] == "payee" {
                    row.as_object_mut().unwrap().remove("new_payee");
                    row.as_object_mut().unwrap().remove("_rule_payee_name");
                }
            }
        }
        "delete-transaction" => row["deleted"] = json!(true),
        "link-schedule" => row["schedule_id"] = a["value"].clone(),
        "set-split-amount" if a["options"]["method"] == "fixed-amount" => {
            row["amount"] = a["value"].clone()
        }
        _ => {}
    };
    Ok(())
}
fn resolve_name(db: &Connection, b: &str, row: &mut Value) -> Result<(), Error> {
    let Some(name) = row.as_object_mut().unwrap().remove("_rule_payee_name") else {
        return Ok(());
    };
    let name = name.as_str().unwrap();
    row["payee_name"] = json!(name);
    if name.is_empty() {
        row["payee_id"] = Value::Null;
        row.as_object_mut().unwrap().remove("new_payee");
        return Ok(());
    }
    let existing:Option<String>=db.query_row("SELECT id FROM payees WHERE budget_id=?1 AND tombstone=0 AND transfer_account_id IS NULL AND lower(name)=lower(?2) ORDER BY id LIMIT 1",params![b,name],|r|r.get(0)).optional()?;
    if let Some(id) = existing {
        row["payee_id"] = json!(id);
        row.as_object_mut().unwrap().remove("new_payee");
    } else {
        let id = identity(&["actualBudget.rule-payee.v1", b, &sync::cursor(db, b)?, name]);
        row["payee_id"] = json!(id);
        row["new_payee"] = json!({"id":id,"name":name});
    }
    Ok(())
}
fn child(parent: &Value, payee: &Value, id: String, order: f64) -> Value {
    json!({"id":id,"amount":0,"category_id":parent["category_id"],"payee_id":payee,"account_id":parent["account_id"],"date":parent["date"],"cleared":parent["cleared"],"parent_id":parent["id"],"is_child":true,"sort_order":order,"reconciled":parent["reconciled"],"starting_balance_flag":parent.get("starting_balance_flag").unwrap_or(&parent["starting_balance"]),"error":null})
}
pub fn apply(
    db: &Connection,
    b: &str,
    input: &Value,
    rule: &Rule,
    retain_first: bool,
) -> Result<Value, Error> {
    let mut row = input.clone();
    for a in rule
        .actions
        .iter()
        .filter(|a| a["options"]["splitIndex"].as_u64().unwrap_or(0) == 0)
    {
        exec(a, &mut row)?;
    }
    resolve_name(db, b, &mut row)?;
    let child_actions = rule
        .actions
        .iter()
        .filter(|a| a["options"]["splitIndex"].as_u64().unwrap_or(0) > 0)
        .collect::<Vec<_>>();
    if child_actions.is_empty() || row["is_child"] == true || !row["parent_id"].is_null() {
        return Ok(row);
    }
    let retained_first = row["children"]
        .as_array()
        .and_then(|children| children.first())
        .filter(|_| retain_first)
        .cloned();
    let mut children = row["children"].as_array().cloned().unwrap_or_else(|| {
        let order = row["sort_order"]
            .as_f64()
            .filter(|value| *value != 0.0)
            .unwrap_or(-1.0);
        vec![child(
            &row,
            &row["payee_id"],
            identity(&[b, row["id"].as_str().unwrap_or("preview"), "reserved"]),
            order,
        )]
    });
    let original_payee = row["payee_id"].clone();
    if row["is_parent"] != true {
        row["payee_id"] = Value::Null;
    }
    for a in &child_actions {
        let index = a["options"]["splitIndex"].as_u64().unwrap() as usize;
        while children.len() <= index {
            let payee = children
                .last()
                .map(|r| &r["payee_id"])
                .unwrap_or(&original_payee);
            let id = identity(&[
                "actualBudget.rule-split.v1",
                b,
                &sync::cursor(db, b)?,
                row["id"].as_str().unwrap_or("preview"),
                &rule.id,
                &children.len().to_string(),
            ]);
            let order = children
                .last()
                .and_then(|c| c["sort_order"].as_f64())
                .unwrap_or(0.0)
                - 1.0;
            children.push(child(&row, payee, id, order));
        }
        exec(a, &mut children[index])?;
        resolve_name(db, b, &mut children[index])?;
    }
    let fractional =
        row["amount"].as_i64().is_none() || children.iter().any(|c| c["amount"].as_i64().is_none());
    if fractional {
        super::split_preview::allocate(&row, &mut children, &child_actions)?;
    } else {
        let remainder = |children: &[Value]| -> Result<i128, Error> {
            Ok(row["amount"]
                .as_i64()
                .ok_or_else(|| Error::invalid("Split parent requires an exact amount"))?
                as i128
                - children
                    .iter()
                    .map(|c| {
                        c["amount"].as_i64().map(i128::from).ok_or_else(|| {
                            Error::invalid("Split allocation requires exact minor units")
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .iter()
                    .sum::<i128>())
        };
        let remaining = remainder(&children)?;
        for a in child_actions
            .iter()
            .filter(|a| a["op"] == "set-split-amount" && a["options"]["method"] == "fixed-percent")
        {
            let index = a["options"]["splitIndex"].as_u64().unwrap() as usize;
            let (n, d) = percent(&a["value"])?;
            children[index]["amount"] = json!(rounded(remaining * n, d * 100)?);
        }
        let rest = child_actions
            .iter()
            .filter(|a| a["op"] == "set-split-amount" && a["options"]["method"] == "remainder")
            .collect::<Vec<_>>();
        if !rest.is_empty() {
            let amount = rounded(remainder(&children)?, rest.len() as i128)?;
            let mut last = 0;
            for a in rest {
                let index = a["options"]["splitIndex"].as_u64().unwrap() as usize;
                children[index]["amount"] = json!(amount);
                last = last.max(index);
            }
            children[last]["amount"] = json!(money(
                children[last]["amount"].as_i64().unwrap() as i128 + remainder(&children)?
            )?);
        }
    }
    children.remove(0);
    // Actual's batch updater writes the returned children without deleting the
    // old reserved first child. Retain it when composing the persisted graph.
    if let Some(first) = retained_first {
        children.insert(0, first);
    }
    for child in &children {
        for field in ["account_id", "date", "cleared"] {
            if !child[field].is_null() && child[field] != row[field] {
                return Err(Error::conflict(
                    "Rule allocations must retain their parent's account, date and cleared state. No actions were saved.",
                ));
            }
        }
    }
    let balanced = if fractional {
        Some(super::split_preview::total(&children)?) == super::numbers::read(&row["amount"])
    } else {
        let total = children
            .iter()
            .map(|c| c["amount"].as_i64().unwrap_or(0) as i128)
            .sum::<i128>();
        total == row["amount"].as_i64().unwrap() as i128
    };
    row["error"] = if balanced {
        Value::Null
    } else {
        json!({"type":"SplitTransactionError","version":1,
            "difference":super::numbers::value(super::numbers::read(&row["amount"]).unwrap() - super::split_preview::total(&children)?)})
    };
    row["children"] = json!(children);
    row["is_parent"] = json!(true);
    Ok(row)
}
