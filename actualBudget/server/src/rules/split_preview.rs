//! Source number arithmetic for non-integral/nonfinite projections. The write plan
//! independently requires exact minor units before any financial mutation.
use super::*;

fn number(value: &Value) -> Result<f64, Error> {
    super::numbers::read(value)
        .ok_or_else(|| Error::invalid("Calculated split preview requires numeric amounts"))
}
pub(super) fn total(children: &[Value]) -> Result<f64, Error> {
    children
        .iter()
        .try_fold(0.0, |sum, child| Ok(sum + number(&child["amount"])?))
}
fn round(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let floor = value.floor();
    if value - floor < 0.5 {
        floor
    } else {
        floor + 1.0
    }
}
fn remainder(parent: f64, children: &[Value]) -> Result<f64, Error> {
    let total = total(children)?;
    // Source getSplitRemainder returns zero for a balanced split, including
    // Infinity === Infinity; subtraction alone would produce NaN.
    Ok(if total == parent { 0.0 } else { parent - total })
}
pub(super) fn allocate(
    parent: &Value,
    children: &mut [Value],
    actions: &[&Value],
) -> Result<(), Error> {
    let parent = number(&parent["amount"])?;
    let remaining = remainder(parent, children)?;
    for action in actions
        .iter()
        .filter(|a| a["op"] == "set-split-amount" && a["options"]["method"] == "fixed-percent")
    {
        let index = action["options"]["splitIndex"].as_u64().unwrap() as usize;
        children[index]["amount"] =
            super::numbers::value(round(remaining * (number(&action["value"])? / 100.0)));
    }
    let rest = actions
        .iter()
        .filter(|a| a["op"] == "set-split-amount" && a["options"]["method"] == "remainder")
        .collect::<Vec<_>>();
    if !rest.is_empty() {
        let amount = round(remainder(parent, children)? / rest.len() as f64);
        let mut last = 0;
        for action in rest {
            let index = action["options"]["splitIndex"].as_u64().unwrap() as usize;
            children[index]["amount"] = super::numbers::value(amount);
            last = last.max(index);
        }
        children[last]["amount"] = super::numbers::value(
            number(&children[last]["amount"])? + remainder(parent, children)?,
        );
    }
    Ok(())
}
