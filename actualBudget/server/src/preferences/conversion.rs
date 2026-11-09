use super::{FEATURES, LANGUAGES, defaults, validate_change};
use crate::error::Error;
use serde_json::{Map, Value, json};

/// Converts only preference records; full archive import calls this same function.
/// Credentials are deliberately neither restored nor returned in `unmapped`.
pub fn convert_actual(input: &Value) -> Result<Value, Error> {
    let input = input
        .as_object()
        .ok_or_else(|| Error::invalid("Expected an Actual preferences object"))?;
    if input.keys().any(|key| key != "synced" && key != "global") {
        return Err(Error::invalid("Expected synced and global preference maps"));
    }
    let empty = Map::new();
    let synced = match input.get("synced") {
        Some(v) => v
            .as_object()
            .ok_or_else(|| Error::invalid("synced must be an object"))?,
        None => &empty,
    };
    let global = match input.get("global") {
        Some(v) => v
            .as_object()
            .ok_or_else(|| Error::invalid("global must be an object"))?,
        None => &empty,
    };
    let mut shared = defaults();
    let mut device = json!({"theme":"system","preferred_dark_theme":"dark","language":""});
    let mut warnings = Vec::new();
    let mut unmapped_synced = Map::new();
    let mut unmapped_global = Map::new();
    for (key, value) in synced {
        let mapped = match key.as_str() {
            "sync-transfer-date" => Some("sync_transfer_date"),
            "numberFormat" => Some("number_format"),
            "dateFormat" => Some("date_format"),
            "hideFraction" => Some("hide_fraction"),
            "firstDayOfWeekIdx" => Some("first_day_of_week"),
            "defaultCurrencyCode" => Some("currency_code"),
            "currencySymbolPosition" => Some("currency_symbol_position"),
            "currencySpaceBetweenAmountAndSymbol" => Some("currency_space"),
            _ => None,
        };
        if let Some(mapped) = mapped {
            let converted = match mapped {
                "sync_transfer_date" | "hide_fraction" | "currency_space" => match value {
                    Value::Bool(_) => Some(value.clone()),
                    Value::String(v) if v == "true" || v == "false" => Some(json!(v == "true")),
                    _ => None,
                },
                "first_day_of_week" => value
                    .as_str()
                    .and_then(|s| s.parse::<u64>().ok())
                    .or_else(|| value.as_u64())
                    .map(|v| json!(v)),
                _ => Some(value.clone()),
            };
            if let Some(converted) =
                converted.filter(|v| !v.is_null() && validate_change(mapped, v).is_ok())
            {
                shared[mapped] = converted;
            } else {
                unmapped_synced.insert(key.clone(), value.clone());
                warnings.push(
                    json!({"key":key,"reason":"Unsupported preference value; default retained"}),
                );
            }
        } else if let Some(flag) = key.strip_prefix("flags.") {
            if FEATURES.contains(&flag)
                && (value.is_boolean() || matches!(value.as_str(), Some("true" | "false")))
            {
                shared["experimental"][flag] = json!(value.as_bool().unwrap_or(value == "true"));
            } else {
                unmapped_synced.insert(key.clone(), value.clone());
                warnings.push(json!({"key":key,"reason":"Unsupported experimental preference retained for review"}));
            }
        } else {
            unmapped_synced.insert(key.clone(), value.clone());
            warnings.push(json!({"key":key,"reason":"Preference belongs to another feature; retained for its importer"}));
        }
    }
    const REMOVED: &[&str] = &[
        "user-id",
        "user-key",
        "user-token",
        "encrypt-keys",
        "encrypt-key",
        "keyId",
        "readOnly",
        "did-bootstrap",
        "server-self-signed-cert",
        "document-dir",
        "syncServerConfig",
    ];
    for (key, value) in global {
        if REMOVED.contains(&key.as_str()) {
            warnings.push(json!({"key":key,"reason":"Removed identity, encryption or desktop preference was not restored"}));
            continue;
        }
        match key.as_str() {
            "theme" => {
                if let Some(v) = value
                    .as_str()
                    .filter(|v| ["light", "dark", "midnight", "auto", "system"].contains(v))
                {
                    device["theme"] = json!(if v == "auto" { "system" } else { v });
                } else {
                    unmapped_global.insert(key.clone(), value.clone());
                    warnings.push(json!({"key":key,"reason":"Custom or unknown theme retained for custom-theme migration"}));
                }
            }
            "preferred-dark-theme" => {
                if matches!(value.as_str(), Some("dark" | "midnight")) {
                    device["preferred_dark_theme"] = value.clone();
                } else {
                    unmapped_global.insert(key.clone(), value.clone());
                    warnings.push(json!({"key":key,"reason":"Unsupported preferred dark theme; default retained"}));
                }
            }
            "language" => {
                if let Some(raw) = value.as_str() {
                    if raw.is_empty() {
                        continue;
                    }
                    let normalized = LANGUAGES
                        .iter()
                        .find(|locale| locale.eq_ignore_ascii_case(raw))
                        .or_else(|| {
                            LANGUAGES.iter().find(|locale| {
                                locale.eq_ignore_ascii_case(raw.split('-').next().unwrap_or(raw))
                            })
                        });
                    if let Some(locale) = normalized {
                        device["language"] = json!(locale);
                    } else {
                        device["language"] = json!("en");
                        unmapped_global.insert(key.clone(), value.clone());
                        warnings.push(json!({"key":key,"reason":"Unavailable language; English fallback selected"}));
                    }
                } else {
                    unmapped_global.insert(key.clone(), value.clone());
                    warnings.push(json!({"key":key,"reason":"Invalid language preference; system default retained"}));
                }
            }
            _ => {
                unmapped_global.insert(key.clone(), value.clone());
                warnings.push(json!({"key":key,"reason":"Device preference belongs to another feature; retained for its importer"}));
            }
        }
    }
    super::normalize_currency(&mut shared);
    Ok(
        json!({"shared":shared,"device":device,"warnings":warnings,"unmapped":{"synced":unmapped_synced,"global":unmapped_global}}),
    )
}
