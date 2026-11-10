//! Notes retain their original text; parsed directives are a separate projection.
//! The grammar mirrors pinned goal-template.pegjs, without evaluating note text.
use crate::error::Error;
use serde_json::{Value, json};
use std::sync::OnceLock;
const AMOUNT: &str = r"\p{Sc}?[ \t]*(-?[0-9]+(?:\.[0-9]{1,2})?)";
fn captures(pattern: &str, text: &str) -> Option<Vec<Option<String>>> {
    // Only application-owned, anchored patterns reach the regex compiler.
    static CACHE: OnceLock<std::sync::Mutex<std::collections::HashMap<String, regress::Regex>>> =
        OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    if !cache.contains_key(pattern) {
        cache.insert(
            pattern.into(),
            regress::Regex::with_flags(pattern, "iu").ok()?,
        );
    }
    let found = cache.get(pattern)?.find(text)?;
    Some(
        found
            .captures
            .iter()
            .map(|range| range.clone().map(|r| text[r].to_owned()))
            .collect(),
    )
}
fn number(value: &str) -> Value {
    let n = value.parse::<f64>().unwrap_or(f64::NAN);
    if n.is_finite() && n.fract() == 0.0 && n.abs() <= crate::models::MAX_MONEY as f64 {
        json!(n as i64)
    } else {
        json!(n)
    }
}
fn num(c: &[Option<String>], i: usize) -> Value {
    c.get(i)
        .and_then(|s| s.as_deref())
        .map(number)
        .unwrap_or(Value::Null)
}
fn text(c: &[Option<String>], i: usize) -> Value {
    c.get(i)
        .and_then(|s| s.as_deref())
        .map(|s| json!(s))
        .unwrap_or(Value::Null)
}
fn limit(input: &str) -> Option<Value> {
    let c = captures(
        &format!(
            r"^up[ \t]*to[ \t]*{AMOUNT}(?:[ \t]*per[ \t]*week[ \t]*starting[ \t]*([0-9]{{4}}-[0-9]{{2}}-[0-9]{{2}})|[ \t]*per[ \t]*day)?[ \t]*(hold)?$"
        ),
        input,
    )?;
    let weekly = c[1].is_some();
    let daily = input.to_ascii_lowercase().contains("per day");
    Some(
        json!({"amount":num(&c,0),"hold":if c[2].is_some(){json!(true)}else{Value::Null},"period":if weekly{"weekly"}else if daily{"daily"}else{"monthly"},"start":text(&c,1)}),
    )
}
fn split_limit(input: &str) -> (&str, Option<Value>) {
    if let Some(c) = captures(r"^(.*?)(up[ \t]*to.*)$", input)
        && let Some(l) = limit(c[1].as_deref().unwrap().trim()) {
            return (input[..c[0].as_ref().unwrap().len()].trim_end(), Some(l));
        }
    (input, None)
}
pub(super) fn line(input: &str) -> Result<Value, String> {
    let (directive, priority, rest) = if let Some(rest) = input.strip_prefix("#template") {
        if let Some(rest) = rest.strip_prefix('-') {
            let end = rest
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(rest.len());
            if end == 0 {
                return Err("Expected a priority number after #template-".into());
            }
            let p = number(&rest[..end]);
            if p.as_i64().is_none_or(|n| n > i32::MAX as i64) {
                return Err("Priority exceeds supported integer range".into());
            }
            ("template", p, rest[end..].trim_matches([' ', '\t']))
        } else {
            ("template", json!(0), rest.trim_matches([' ', '\t']))
        }
    } else if let Some(rest) = input.strip_prefix("#goal") {
        ("goal", Value::Null, rest.trim_matches([' ', '\t']))
    } else {
        return Err("Expected #template or #goal".into());
    };
    let mut out = if directive == "goal" {
        let c = captures(&format!("^{AMOUNT}$"), rest).ok_or("Expected a goal amount")?;
        json!({"type":"goal","amount":num(&c,0)})
    } else if let Some(c) = captures(
        r"^([0-9]+(?:\.[0-9]+)?)[ \t]*%[ \t]*of[ \t]*(previous[ \t]*)?([^\r\n\t]+)$",
        rest,
    ) {
        json!({"type":"percentage","percent":num(&c,0),"previous":c[1].is_some(),"category":text(&c,2)})
    } else {
        let (main, cap) = split_limit(rest);
        if let Some(c) = captures(
            &format!(
                r"^{AMOUNT}[ \t]*repeat[ \t]*every[ \t]*(?:([0-9]+)[ \t]*)?(day|days|week|weeks|months|year|years)[ \t]*starting[ \t]*([0-9]{{4}}-[0-9]{{2}}-[0-9]{{2}})$"
            ),
            main,
        ) {
            let unit = c[2].as_deref().unwrap().to_ascii_lowercase();
            let period = unit.trim_end_matches('s');
            let amount = c[1].as_deref().unwrap_or("1");
            if (unit.ends_with('s') && c[1].is_none()) || unit == "month" {
                return Err("Expected a periodic interval".into());
            }
            json!({"type":"periodic","amount":num(&c,0),"period":{"period":period,"amount":number(amount)},"starting":text(&c,3),"limit":cap})
        } else if let Some(c) = captures(
            &format!(
                r"^{AMOUNT}[ \t]*by[ \t]*([0-9]{{4}}-[0-9]{{2}})(?:[ \t]*spend[ \t]*from[ \t]*([0-9]{{4}}-[0-9]{{2}}))?(?:[ \t]*repeat[ \t]*every[ \t]*(?:([1-9][0-9]*)[ \t]*)?(month|months|year|years))?$"
            ),
            rest,
        ) {
            let mut v = json!({"type":if c[2].is_some(){"spend"}else{"by"},"amount":num(&c,0),"month":text(&c,1),"from":text(&c,2)});
            if let Some(unit) = c[4].as_deref() {
                v["annual"] =
                    json!(unit.eq_ignore_ascii_case("year") || unit.eq_ignore_ascii_case("years"));
                if c[3].is_some() {
                    v["repeat"] = num(&c, 3);
                }
            }
            v
        } else if let Some(c) = captures(&format!("^{AMOUNT}$"), main) {
            json!({"type":"simple","monthly":num(&c,0),"limit":cap})
        } else if main.is_empty() && cap.is_some() {
            json!({"type":"simple","monthly":null,"limit":cap})
        } else if let Some(c) = captures(r"^remainder[ \t]*([1-9][0-9]*)?$", main) {
            json!({"type":"remainder","weight":c[0].as_deref().map(number).unwrap_or(json!(1)),"limit":cap})
        } else {
            let mut main = rest;
            let mut adjustment = None;
            if let Some(c) = captures(
                r"^(.*?)[ \t]*\[[ \t]*(increase|decrease)[ \t]*([0-9]+(?:\.[0-9]*)?)[ \t]*(%)?\]$",
                rest,
            ) {
                main = &rest[..c[0].as_ref().unwrap().len()];
                let n = c[2]
                    .as_deref()
                    .unwrap()
                    .parse::<f64>()
                    .map_err(|_| "Invalid adjustment")?
                    * if c[1].as_deref().unwrap().eq_ignore_ascii_case("decrease") {
                        -1.0
                    } else {
                        1.0
                    };
                if c[3].is_some() && (n <= -100.0 || n > 1000.0) {
                    return Err(format!(
                        "Invalid adjustment percentage ({n}%). Must be between -100% and 1000%"
                    ));
                }
                adjustment = Some((
                    number(&n.to_string()),
                    if c[3].is_some() { "percent" } else { "fixed" },
                ));
            }
            let mut v = if let Some(c) = captures(
                r"^schedule[ \t]*(full[ \t]*)?([^ \t\r\n][^\r\n]*)$",
                main.trim_end(),
            ) {
                json!({"type":"schedule","name":c[1].as_deref().unwrap().trim(),"full":if c[0].is_some(){json!(true)}else{Value::Null}})
            } else if let Some(c) = captures(
                r"^average[ \t]*([1-9][0-9]*)[ \t]*(?:months)?$",
                main.trim_end(),
            ) {
                json!({"type":"average","numMonths":num(&c,0)})
            } else if let Some(c) = captures(
                r"^copy from[ \t]*([1-9][0-9]*)[ \t]*months ago$",
                split_limit(rest).0.trim_end(),
            ) {
                json!({"type":"copy","lookBack":num(&c,0),"limit":cap})
            } else {
                return Err(
                    "Invalid template directive. Review the amount, dates and directive syntax."
                        .into(),
                );
            };
            if let Some((n, kind)) = adjustment {
                if !matches!(v["type"].as_str(), Some("average" | "schedule")) {
                    return Err("Adjustments apply to schedule or average templates".into());
                }
                v["adjustment"] = n;
                v["adjustmentType"] = json!(kind);
            }
            v
        }
    };
    out["priority"] = if out["type"] == "remainder" {
        Value::Null
    } else {
        priority
    };
    out["directive"] = json!(directive);
    Ok(out)
}
pub(super) fn notes(notes: &str) -> Result<Value, Error> {
    if notes.chars().count() > 10000 {
        return Err(Error::invalid("Category notes exceed 10000 characters"));
    }
    let mut templates = vec![];
    let mut errors = vec![];
    let mut description = Vec::<String>::new();
    for original in notes.split('\n') {
        let directive = original
            .find('#')
            .map(|i| &original[i..])
            .unwrap_or(original)
            .trim();
        if !directive.starts_with("#template") && !directive.starts_with("#goal") {
            if original.trim().is_empty() || directive.starts_with("#cleanup") {
                description.clear();
            } else {
                description.push(original.trim_end().into());
            }
            continue;
        }
        let mut parsed = match line(directive) {
            Ok(value) => value,
            Err(error) => {
                errors.push(json!({"line":original,"error":error}));
                json!({"type":"error","directive":"error","line":original,"error":error})
            }
        };
        if !description.is_empty() {
            parsed["description"] = json!(description.join("\n"));
        }
        description.clear();
        templates.push(parsed);
        if templates.len() > 256 {
            return Err(Error::invalid(
                "Template work limit reached; split the category's directives",
            ));
        }
    }
    Ok(json!({"templates":templates,"errors":errors}))
}
