use crate::{api::json_error, error::Error, store::Store};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, rejection::JsonRejection},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
mod conversion;
mod css;
pub use conversion::convert;
const MAP: &[(&str, &str)] = &[
    ("--color-pageBackground", "--page"),
    ("--color-pageText", "--text"),
    ("--color-pageTextSubdued", "--muted"),
    ("--color-pageTextLink", "--link"),
    ("--color-tableBackground", "--table-surface"),
    ("--color-tableText", "--table-text"),
    ("--color-tableBorder", "--line"),
    ("--color-tableRowBackgroundHover", "--table-hover"),
    ("--color-cardBackground", "--surface"),
    ("--color-buttonPrimaryBackground", "--accent"),
    ("--color-buttonPrimaryText", "--primary-text"),
    ("--color-buttonPrimaryBackgroundHover", "--primary-hover"),
    ("--color-buttonNormalText", "--button-text"),
    ("--color-buttonNormalBackground", "--button-surface"),
    ("--color-buttonNormalBackgroundHover", "--hover"),
    ("--color-buttonNormalBorder", "--button-border"),
    ("--color-formInputBorder", "--control-border"),
    ("--color-formInputBorderSelected", "--focus"),
    ("--color-formInputBackground", "--input-surface"),
    ("--color-formInputText", "--input-text"),
    ("--color-formInputBackgroundSelection", "--selection"),
    ("--color-formInputTextSelection", "--selection-text"),
    ("--color-sidebarBackground", "--sidebar"),
    ("--color-sidebarItemText", "--sidebar-text"),
    ("--color-sidebarBudgetName", "--sidebar-title"),
    ("--color-sidebarItemBackgroundSelected", "--selected"),
    ("--color-sidebarItemTextSelected", "--selected-text"),
    ("--color-sidebarRedesignBackground", "--sidebar"),
    ("--color-sidebarRedesignItemText", "--sidebar-text"),
    (
        "--color-sidebarRedesignItemBackgroundSelected",
        "--selected",
    ),
    ("--color-sidebarRedesignItemTextSelected", "--selected-text"),
    ("--color-errorText", "--error"),
    ("--color-errorBackground", "--error-bg"),
    ("--font-family", "--font-family"),
];
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub css: String,
    #[serde(default = "light")]
    pub base_theme: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub repo: String,
}
fn light() -> String {
    "light".into()
}
pub fn router() -> Router<Store> {
    Router::new()
        .route("/api/v1/themes/catalog", get(catalog))
        .route("/api/v1/themes/compile", post(compile_http))
        .route("/api/v1/themes/convert-actual", post(convert_http))
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
}
async fn catalog() -> Json<Value> {
    Json(
        json!({"data":{"version":1,"entries":serde_json::from_str::<Value>(include_str!("catalog.json")).unwrap(),"token_map":MAP,"limits":{"css_bytes":css::MAX_CSS,"font_bytes":2097152,"total_font_bytes":10485760}}}),
    )
}
async fn compile_http(input: Result<Json<Input>, JsonRejection>) -> Result<Json<Value>, Error> {
    let Json(input) = input.map_err(json_error)?;
    Ok(Json(json!({"data":compile(input)?})))
}
async fn convert_http(input: Result<Json<Value>, JsonRejection>) -> Result<Json<Value>, Error> {
    let Json(input) = input.map_err(json_error)?;
    Ok(Json(json!({"data":convert(&input)?})))
}
fn warning(key: &str, reason: impl Into<String>) -> Value {
    json!({"key":key,"reason":reason.into()})
}
fn resolve(
    key: &str,
    values: &BTreeMap<String, String>,
    stack: &mut Vec<String>,
    used: &mut BTreeSet<String>,
) -> Result<String, String> {
    if stack.len() > 64 || stack.iter().any(|item| item == key) {
        return Err(format!("Cyclic or excessively deep variable: {key}"));
    }
    let value = values
        .get(key)
        .ok_or_else(|| format!("Unknown variable reference: {key}"))?;
    used.insert(key.into());
    stack.push(key.into());
    let mut result = value.clone();
    let mut offset = 0;
    while let Some(start) = result[offset..].find("var(") {
        let start = start + offset;
        let end = result[start..]
            .find(')')
            .ok_or("Unclosed variable reference")?
            + start;
        let reference = result[start + 4..end].trim().to_owned();
        if !css::name(&reference) {
            return Err("Unsupported variable reference".into());
        }
        let replacement = resolve(&reference, values, stack, used)?;
        result.replace_range(start..=end, &replacement);
        offset = start + replacement.len();
        if result.len() > 65536 {
            return Err("Resolved variable is too long".into());
        }
    }
    stack.pop();
    Ok(result)
}
pub fn compile(input: Input) -> Result<Value, Error> {
    if !["light", "dark", "midnight"].contains(&input.base_theme.as_str()) {
        return Err(Error::invalid("Unsupported base theme"));
    }
    if input.css.len() > css::MAX_CSS
        || input.id.len() > 512
        || input.name.len() > 200
        || input.repo.len() > 2048
    {
        return Err(Error::invalid("Theme exceeds supported size"));
    }
    let source_hash = format!("{:x}", Sha256::digest(input.css.as_bytes()));
    let mut result = json!({"version":2,"source_hash":source_hash,"id":input.id,"name":input.name,"repo":input.repo,"base_theme":input.base_theme,"source":input.css,"valid":false,"variables":{},"variables_by_base":{"light":{},"dark":{},"midnight":{}},"fonts":[],"warnings":[]});
    let parsed = match css::parse(&input.css) {
        Ok(parsed) => parsed,
        Err(reason) => {
            result["warnings"] = json!([warning("css", reason)]);
            return Ok(result);
        }
    };
    let mut by_base = serde_json::Map::new();
    let mut selected_warnings = Vec::new();
    for base in ["light", "dark", "midnight"] {
        let (variables, warnings) = map_variables(&parsed, base);
        if base == input.base_theme {
            selected_warnings = warnings;
        }
        by_base.insert(base.into(), json!(variables));
    }
    let applied = by_base[&input.base_theme].clone();
    let warnings = selected_warnings;
    result["variables_by_base"] = json!(by_base);
    result["valid"] = json!(true);
    result["variables"] = json!(applied);
    result["fonts"] = json!(parsed.fonts);
    result["warnings"] = json!(warnings);
    Ok(result)
}

fn map_variables(parsed: &css::Parsed, base_theme: &str) -> (BTreeMap<String, String>, Vec<Value>) {
    let baseline: Value = serde_json::from_str(include_str!("baseline.json")).unwrap();
    let mut values = BTreeMap::new();
    for group in ["palette", base_theme] {
        for (key, value) in baseline[group].as_object().unwrap() {
            values.insert(key.clone(), value.as_str().unwrap().to_owned());
        }
    }
    let mut formula_values = values.clone();
    let redesign = parsed
        .variables
        .keys()
        .any(|key| key.starts_with("--color-sidebarRedesign"))
        || !parsed
            .variables
            .keys()
            .any(|key| key.starts_with("--color-sidebar"));
    if base_theme == "light" && redesign {
        for (key, value) in baseline["sidebar-redesign-root-light"].as_object().unwrap() {
            formula_values.insert(key.clone(), value.as_str().unwrap().to_owned());
        }
        for (key, value) in baseline["sidebar-redesign-light"].as_object().unwrap() {
            values.insert(key.clone(), value.as_str().unwrap().to_owned());
        }
    }
    values.extend(parsed.variables.clone());
    formula_values.extend(parsed.variables.clone());
    let mut applied = BTreeMap::new();
    let mut consumed = BTreeSet::new();
    let mut warnings = Vec::new();
    for (source, target) in MAP {
        if !values.contains_key(*source) {
            continue;
        }
        let mut dependencies = BTreeSet::new();
        match resolve(source, &values, &mut Vec::new(), &mut dependencies) {
            Ok(value) => {
                if dependencies
                    .iter()
                    .any(|key| parsed.variables.contains_key(key))
                {
                    applied.insert(target.to_string(), value);
                    consumed.extend(dependencies);
                }
            }
            Err(reason) => {
                if parsed.variables.contains_key(*source) {
                    warnings.push(warning(source, reason));
                }
            }
        }
    }
    // Preserve every known source theme role for formula named expressions,
    // including roles without an equivalent native interface token.
    for source in baseline[base_theme].as_object().unwrap().keys() {
        if !source.starts_with("--color-") {
            continue;
        }
        let mut dependencies = BTreeSet::new();
        if let Ok(value) = resolve(source, &formula_values, &mut Vec::new(), &mut dependencies)
            && dependencies
                .iter()
                .any(|key| parsed.variables.contains_key(key))
        {
            applied.insert(source.clone(), value);
            consumed.extend(dependencies);
        }
    }
    for key in parsed.variables.keys() {
        if MAP.iter().any(|(_, target)| *target == key) {
            match resolve(key, &values, &mut Vec::new(), &mut consumed) {
                Ok(value) => {
                    applied.insert(key.clone(), value);
                }
                Err(reason) => warnings.push(warning(key, reason)),
            }
        } else if !consumed.contains(key) {
            warnings.push(warning(
                key,
                "Preserved, but no supported interface token currently uses this variable",
            ));
        }
    }
    (applied, warnings)
}
