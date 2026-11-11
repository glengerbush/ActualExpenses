//! Private transport for JavaScript numbers that JSON cannot represent.
//! Preserve them through the rule chain; expose JSON null only at its boundary.
use super::*;
const TAG: &str = "$actualRuleNumber";

pub(super) fn read(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| {
        let object = value.as_object().filter(|o| o.len() == 1)?;
        match object.get(TAG)?.as_str()? {
            "Infinity" => Some(f64::INFINITY),
            "-Infinity" => Some(f64::NEG_INFINITY),
            "NaN" => Some(f64::NAN),
            _ => None,
        }
    })
}
pub(super) fn value(number: f64) -> Value {
    if !number.is_finite() {
        json!({TAG: if number.is_nan() {"NaN"} else if number.is_sign_positive() {"Infinity"} else {"-Infinity"}})
    } else if number.fract() == 0.0 && number.abs() <= crate::models::MAX_MONEY as f64 {
        json!(number as i64)
    } else {
        json!(number)
    }
}
pub(super) fn project(value: &mut Value) {
    if read(value).is_some_and(|n| !n.is_finite()) {
        *value = Value::Null;
    } else {
        match value {
            Value::Array(values) => values.iter_mut().for_each(project),
            Value::Object(values) => values.values_mut().for_each(project),
            _ => {}
        }
    }
}
