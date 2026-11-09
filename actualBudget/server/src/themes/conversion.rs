use super::{Input, compile, warning};
use crate::error::Error;
use serde_json::{Value, json};
fn field<'a>(global: &'a Value, raw: &str, alias: &str) -> Option<&'a Value> {
    global.get(raw).or_else(|| global.get(alias))
}
pub fn convert(input: &Value) -> Result<Value, Error> {
    let global = input
        .get("global")
        .filter(|v| v.is_object())
        .ok_or_else(|| Error::invalid("Expected global preference object"))?;
    let mut warnings = Vec::new();
    for (raw, alias) in [
        ("installed-custom-theme", "installedCustomLightTheme"),
        ("installed-custom-dark-theme", "installedCustomDarkTheme"),
        ("custom-css-override", "customCssOverride"),
        ("preferred-dark-theme", "preferredDarkTheme"),
    ] {
        if global.get(raw).is_some() && global.get(alias).is_some() && global[raw] != global[alias]
        {
            warnings.push(warning(
                raw,
                "Raw and parsed aliases differ; raw value wins and both originals are preserved",
            ));
        }
    }
    if field(global, "custom-css-override", "customCssOverride")
        .is_some_and(|v| !v.is_string() && !v.is_null())
    {
        warnings.push(warning(
            "custom-css-override",
            "Non-text override preserved without applying",
        ));
    }
    let mut legacy = Vec::new();
    let mut converted = Vec::new();
    for (raw, alias, base) in [
        (
            "installed-custom-theme",
            "installedCustomLightTheme",
            global["theme"]
                .as_str()
                .filter(|v| ["light", "dark", "midnight"].contains(v))
                .unwrap_or("light"),
        ),
        (
            "installed-custom-dark-theme",
            "installedCustomDarkTheme",
            field(global, "preferred-dark-theme", "preferredDarkTheme")
                .and_then(Value::as_str)
                .filter(|v| ["dark", "midnight"].contains(v))
                .unwrap_or("dark"),
        ),
    ] {
        let Some(original) = field(global, raw, alias) else {
            converted.push(Value::Null);
            continue;
        };
        let parsed = original
            .as_str()
            .and_then(|v| serde_json::from_str::<Value>(v).ok());
        if let Some(css) = parsed
            .as_ref()
            .and_then(|v| v.get("overrideCss"))
            .and_then(Value::as_str)
            .filter(|v| !v.trim().is_empty())
        {
            legacy.push(css.trim().to_owned());
        }
        let Some(parsed) = parsed.filter(|v| {
            v.is_object()
                && ["id", "name", "repo", "cssContent"]
                    .iter()
                    .all(|key| v[*key].is_string())
        }) else {
            if !original.is_null() && original != "null" {
                warnings.push(warning(
                    raw,
                    "Malformed installed theme preserved; built-in appearance retained",
                ));
            }
            converted.push(Value::Null);
            continue;
        };
        if parsed.get("baseTheme").is_some()
            && !parsed["baseTheme"]
                .as_str()
                .is_some_and(|v| ["light", "dark", "midnight"].contains(&v))
        {
            warnings.push(warning(
                raw,
                "Unsupported base theme preserved; contextual built-in base selected",
            ));
        }
        let base = parsed["baseTheme"]
            .as_str()
            .filter(|v| ["light", "dark", "midnight"].contains(v))
            .unwrap_or(base);
        let theme = compile(Input {
            css: parsed["cssContent"].as_str().unwrap().into(),
            base_theme: base.into(),
            id: parsed["id"].as_str().unwrap().into(),
            name: parsed["name"].as_str().unwrap().into(),
            repo: parsed["repo"].as_str().unwrap().into(),
        })?;
        converted.push(theme);
    }
    let existing = field(global, "custom-css-override", "customCssOverride")
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty());
    let selected = existing
        .map(str::to_owned)
        .or_else(|| legacy.first().cloned());
    if legacy.len() > 1 || existing.is_some() && !legacy.is_empty() {
        warnings.push(warning("overrideCss","Multiple override sources preserved. Standalone override wins, otherwise light precedes dark."));
    }
    let override_theme = match selected {
        Some(css) => compile(Input {
            css,
            base_theme: "light".into(),
            id: "override".into(),
            name: "Additional CSS".into(),
            repo: String::new(),
        })?,
        None => Value::Null,
    };
    let mut original = serde_json::Map::new();
    for key in [
        "installed-custom-theme",
        "installedCustomLightTheme",
        "installed-custom-dark-theme",
        "installedCustomDarkTheme",
        "custom-css-override",
        "customCssOverride",
        "theme",
        "preferred-dark-theme",
        "preferredDarkTheme",
    ] {
        if let Some(value) = global.get(key) {
            original.insert(key.into(), value.clone());
        }
    }
    Ok(
        json!({"version":1,"light":converted[0],"dark":converted[1],"override":override_theme,"original":original,"warnings":warnings}),
    )
}
