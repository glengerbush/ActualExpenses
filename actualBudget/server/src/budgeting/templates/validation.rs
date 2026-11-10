use super::*;
pub(super) fn templates(values: &[Value]) -> Result<(), Error> {
    if values.len() > 256 {
        return Err(Error::invalid("Use at most256 templates per category"));
    }
    for t in values {
        let kind = t["type"]
            .as_str()
            .ok_or_else(|| Error::invalid("Template type is required"))?;
        let fields: &[&str] = match kind {
            "simple" => &["monthly", "limit"],
            "refill" => &[],
            "limit" => &["amount", "hold", "period", "start"],
            "by" => &["amount", "month", "from", "annual", "repeat"],
            "spend" => &["amount", "month", "from", "annual", "repeat"],
            "periodic" => &["amount", "period", "starting", "limit"],
            "percentage" => &["percent", "category", "previous"],
            "schedule" => &["scheduleId", "name", "full", "adjustment", "adjustmentType"],
            "average" => &["numMonths", "adjustment", "adjustmentType"],
            "copy" => &["lookBack", "limit"],
            "remainder" => &["weight", "limit"],
            "goal" => &["amount"],
            _ => return Err(Error::invalid("Choose a supported template type")),
        };
        if t.as_object().is_none_or(|o| {
            o.keys().any(|k| {
                !["type", "directive", "priority", "description"].contains(&k.as_str())
                    && !fields.contains(&k.as_str())
            })
        }) {
            return Err(Error::invalid(format!("Unknown {kind} template field")));
        }
        if t["directive"] != if kind == "goal" { "goal" } else { "template" } {
            return Err(Error::invalid("Template directive does not match its type"));
        }
        if !t["description"].is_null()
            && t["description"]
                .as_str()
                .is_none_or(|s| s.chars().count() > 10000)
        {
            return Err(Error::invalid(
                "Template description must contain at most10000 characters",
            ));
        }
        if !matches!(kind, "goal" | "remainder" | "limit")
            && t["priority"]
                .as_i64()
                .is_none_or(|n| !(0..=i32::MAX as i64).contains(&n))
        {
            return Err(Error::invalid(
                "Template priority must be a nonnegative integer",
            ));
        }
        if matches!(kind, "goal" | "remainder" | "limit") && !t["priority"].is_null() {
            return Err(Error::invalid(
                "Goals, limits and remainders do not take a priority",
            ));
        }
        for field in ["amount", "monthly", "percent", "adjustment", "weight"] {
            if !t[field].is_null()
                && t[field]
                    .as_f64()
                    .is_none_or(|n| !n.is_finite() || n.abs() > crate::models::MAX_MONEY as f64)
            {
                return Err(Error::invalid(format!(
                    "Template {field} must be a finite amount"
                )));
            }
        }
        match kind {
            "simple" => {
                if t["monthly"].is_null() && t["limit"].is_null() {
                    return Err(Error::invalid("Enter an amount or refill limit"));
                }
            }
            "goal" | "by" | "spend" | "periodic" | "limit" => {
                if t["amount"].as_f64().is_none() {
                    return Err(Error::invalid("Template amount is required"));
                }
            }
            "remainder" => {
                if t["weight"].as_f64().is_none_or(|n| n <= 0.0) {
                    return Err(Error::invalid("Remainder weight must be positive"));
                }
            }
            "percentage" => {
                if t["percent"].as_f64().is_none_or(|n| n < 0.0)
                    || t["category"].as_str().is_none_or(str::is_empty)
                    || t["previous"].as_bool().is_none()
                {
                    return Err(Error::invalid(
                        "Percentage requires a nonnegative percentage, income source and previous-month choice",
                    ));
                }
            }
            "average" | "copy" => {
                let field = if kind == "average" {
                    "numMonths"
                } else {
                    "lookBack"
                };
                if t[field].as_i64().is_none_or(|n| !(1..=1200).contains(&n)) {
                    return Err(Error::invalid("History must be1–1200 months"));
                }
            }
            "schedule" => {
                if t["scheduleId"].as_str().is_none()
                    && t["name"].as_str().is_none_or(str::is_empty)
                {
                    return Err(Error::invalid("Choose a schedule"));
                }
                if let Some(id) = t["scheduleId"].as_str() {
                    crate::models::validate_id(id)?;
                }
                if !t["full"].is_null() && !t["full"].is_boolean() {
                    return Err(Error::invalid("Full schedule allocation must be a boolean"));
                }
            }
            _ => {}
        }
        if matches!(kind, "by" | "spend") {
            super::super::month(
                t["month"]
                    .as_str()
                    .ok_or_else(|| Error::invalid("Target month is required"))?,
            )?;
            if kind == "spend" {
                super::super::month(
                    t["from"]
                        .as_str()
                        .ok_or_else(|| Error::invalid("Spend start month is required"))?,
                )?;
            }
            if !t["annual"].is_null() && !t["annual"].is_boolean() {
                return Err(Error::invalid("Annual repeat must be a boolean"));
            }
            if !t["repeat"].is_null()
                && t["repeat"]
                    .as_i64()
                    .is_none_or(|n| !(1..=120000).contains(&n))
            {
                return Err(Error::invalid("Repeat must be a positive integer"));
            }
        }
        if kind == "periodic" {
            let p = &t["period"];
            if p.as_object().is_none_or(|o| {
                o.keys()
                    .any(|k| !["period", "amount"].contains(&k.as_str()))
            }) || !matches!(
                p["period"].as_str(),
                Some("day" | "week" | "month" | "year")
            ) || p["amount"]
                .as_i64()
                .is_none_or(|n| !(1..=120000).contains(&n))
            {
                return Err(Error::invalid(
                    "Choose a periodic unit and positive interval",
                ));
            }
            crate::models::validate_date(
                t["starting"]
                    .as_str()
                    .ok_or_else(|| Error::invalid("Periodic start date is required"))?,
            )?;
        }
        if !t["adjustment"].is_null() {
            let amount = t["adjustment"].as_f64().unwrap();
            match t["adjustmentType"].as_str() {
                Some("percent") if amount <= -100.0 || amount > 1000.0 => {
                    return Err(Error::invalid(
                        "Adjustment percentage must be greater than−100 and at most1000",
                    ));
                }
                Some("percent" | "fixed") => {}
                _ => return Err(Error::invalid("Adjustment type must be fixed or percent")),
            }
        }
        if kind == "limit" {
            limit(t)?;
        } else if !t["limit"].is_null() {
            limit(&t["limit"])?;
        }
    }
    Ok(())
}
fn limit(t: &Value) -> Result<(), Error> {
    if t["amount"].as_f64().is_none_or(|n| !n.is_finite())
        || !matches!(t["period"].as_str(), Some("daily" | "weekly" | "monthly"))
        || !t["hold"].is_null() && !t["hold"].is_boolean()
    {
        return Err(Error::invalid(
            "A limit needs a finite amount, period and hold choice",
        ));
    }
    if t["period"] == "weekly" {
        crate::models::validate_date(
            t["start"]
                .as_str()
                .ok_or_else(|| Error::invalid("Weekly limit start is required"))?,
        )?;
    }
    Ok(())
}
