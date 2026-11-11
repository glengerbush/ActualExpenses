//! Validate optional saved source fields without replacing source default/clamp semantics.
use crate::error::Error;
use serde_json::Value;
fn object(value: &Value, fields: &[(&str, &str)]) -> Result<(), Error> {
    let values = value
        .as_object()
        .ok_or_else(|| Error::invalid("Plan entries must be objects"))?;
    for (key, value) in values {
        let kind = fields
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, kind)| *kind)
            .ok_or_else(|| Error::invalid(format!("Unknown Monte Carlo field: {key}")))?;
        if value.is_null() {
            continue;
        }
        let valid = match kind {
            "number" => value
                .as_f64()
                .is_some_and(|number| number.is_finite() && number.abs() <= 9007199254740991.0),
            "text" => value.as_str().is_some_and(|text| text.len() <= 4096),
            "bool" => value.is_boolean(),
            "pots" | "income" | "contributions" | "phases" | "bands" => {
                let entries = value
                    .as_array()
                    .filter(|entries| entries.len() <= 256)
                    .ok_or_else(|| Error::invalid(format!("{key} requires at most256 entries")))?;
                for entry in entries {
                    object(entry, fields_for(kind))?;
                }
                true
            }
            "rule" => {
                object(value, fields_for(kind))?;
                true
            }
            choices => value
                .as_str()
                .is_some_and(|text| choices.split('|').any(|choice| choice == text)),
        };
        if !valid {
            return Err(Error::invalid(format!("Invalid Monte Carlo {key}")));
        }
    }
    Ok(())
}
fn fields_for(kind: &str) -> &'static [(&'static str, &'static str)] {
    match kind {
        "pots" => &[
            ("id", "text"),
            ("name", "text"),
            ("startingBalance", "number"),
            (
                "allocationPreset",
                "equity-100|equity-80|equity-60|equity-40|cash|custom-mix|custom",
            ),
            ("allocationStocks", "number"),
            ("allocationBonds", "number"),
            ("allocationCash", "number"),
            ("expectedReturnMean", "number"),
            ("returnStdDev", "number"),
            ("accessAge", "number"),
            ("accountId", "text"),
            ("withdrawalTaxRate", "number"),
            ("taxableFraction", "number"),
            ("annualFeeFixed", "number"),
            ("feeAdjustsWithInflation", "bool"),
            ("annualFeeRate", "number"),
            ("isSurplus", "bool"),
        ],
        "income" => &[
            ("id", "text"),
            ("name", "text"),
            ("fromAge", "number"),
            ("toAge", "number"),
            ("annualAmount", "number"),
            ("adjustsWithInflation", "bool"),
            ("taxRate", "number"),
            ("taxableFraction", "number"),
        ],
        "contributions" => &[
            ("id", "text"),
            ("name", "text"),
            ("potId", "text"),
            ("fromAge", "number"),
            ("toAge", "number"),
            ("annualAmount", "number"),
            ("adjustsWithInflation", "bool"),
            ("sourceIncomeStreamId", "text"),
            ("beforeTax", "bool"),
        ],
        "phases" => &[
            ("id", "text"),
            ("name", "text"),
            ("fromAge", "number"),
            ("annualWithdrawal", "number"),
        ],
        "bands" => &[("id", "text"), ("from", "number"), ("rate", "number")],
        "rule" => &[
            (
                "type",
                "none|guardrails|ratcheting|floor-ceiling|boundaries",
            ),
            ("prosperityTriggerPct", "number"),
            ("prosperityIncreasePct", "number"),
            ("preservationTriggerPct", "number"),
            ("preservationCutPct", "number"),
            ("balanceThresholdMultiple", "number"),
            ("consecutiveYears", "number"),
            ("ratchetIncreasePct", "number"),
            ("floorPct", "number"),
            ("ceilingPct", "number"),
            ("upperRateThreshold", "number"),
            ("upperCutPct", "number"),
            ("lowerRateThreshold", "number"),
            ("lowerIncreasePct", "number"),
        ],
        _ => unreachable!(),
    }
}
pub(super) fn validate(config: &Value) -> Result<(), Error> {
    if config.is_null() {
        return Ok(());
    }
    if config.to_string().len() > 65536 {
        return Err(Error::invalid("Monte Carlo plan exceeds64KiB"));
    }
    object(
        config,
        &[
            ("name", "text"),
            ("pots", "pots"),
            (
                "withdrawalStrategy",
                "proportional|sequential|best-performer|target-mix",
            ),
            (
                "returnModel",
                "normal|historical-bootstrap|historical-sequence",
            ),
            ("withdrawalRule", "rule"),
            ("minimumSpending", "number"),
            ("spendingPhases", "phases"),
            ("contributions", "contributions"),
            ("incomeStreams", "income"),
            ("inflationMean", "number"),
            ("inflationStdDev", "number"),
            ("taxModel", "flat|bands"),
            ("taxBands", "bands"),
            ("currentAge", "number"),
            ("targetAge", "number"),
            ("simulationCount", "number"),
        ],
    )
}
