//! Source number arithmetic for non-integral projections only. The write plan
//! independently requires exact minor units before any financial mutation.
use super::*;

fn number(value: &Value) -> Result<f64, Error> {
    value
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(|| Error::invalid("Calculated split preview requires finite amounts"))
}
pub(super) fn total(children: &[Value]) -> Result<f64, Error> {
    children
        .iter()
        .try_fold(0.0, |sum, child| Ok(sum + number(&child["amount"])?))
}
fn round(value: f64) -> f64 {
    let floor = value.floor();
    if value - floor < 0.5 {
        floor
    } else {
        floor + 1.0
    }
}
fn value(number: f64) -> Value {
    if number.is_finite()
        && number.fract() == 0.0
        && number.abs() <= crate::models::MAX_MONEY as f64
    {
        json!(number as i64)
    } else {
        json!(number)
    }
}
pub(super) fn allocate(
    parent: &Value,
    children: &mut [Value],
    actions: &[&Value],
) -> Result<(), Error> {
    let parent = number(&parent["amount"])?;
    let remaining = parent - total(children)?;
    for action in actions
        .iter()
        .filter(|a| a["op"] == "set-split-amount" && a["options"]["method"] == "fixed-percent")
    {
        let index = action["options"]["splitIndex"].as_u64().unwrap() as usize;
        children[index]["amount"] = value(round(remaining * (number(&action["value"])? / 100.0)));
    }
    let rest = actions
        .iter()
        .filter(|a| a["op"] == "set-split-amount" && a["options"]["method"] == "remainder")
        .collect::<Vec<_>>();
    if !rest.is_empty() {
        let amount = round((parent - total(children)?) / rest.len() as f64);
        let mut last = 0;
        for action in rest {
            let index = action["options"]["splitIndex"].as_u64().unwrap() as usize;
            children[index]["amount"] = value(amount);
            last = last.max(index);
        }
        children[last]["amount"] =
            value(number(&children[last]["amount"])? + (parent - total(children)?));
    }
    Ok(())
}
