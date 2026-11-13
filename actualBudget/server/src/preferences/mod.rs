use crate::{
    api::{budget_id, idempotency_key, json_error},
    error::Error,
    store::{self, Store},
};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::HeaderMap,
    routing::{get, post},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value, json};
mod conversion;
mod currency_codes;
pub use conversion::convert_actual;
pub const FEATURES: &[&str] = &[
    "newSidebarUI",
    "actionTemplating",
    "formulaMode",
    "currency",
    "balanceForecastReport",
    "customThemes",
    "budgetAnalysisReport",
    "sankeyReport",
    "mobileCalculator",
    "monteCarloReport",
];
pub const NUMBER_FORMATS: &[&str] = &[
    "comma-dot",
    "dot-comma",
    "space-comma",
    "apostrophe-dot",
    "comma-dot-in",
];
pub const DATE_FORMATS: &[&str] = &[
    "MM/dd/yyyy",
    "dd/MM/yyyy",
    "yyyy-MM-dd",
    "MM.dd.yyyy",
    "dd.MM.yyyy",
    "dd-MM-yyyy",
];
pub const LANGUAGES: &[&str] = &[
    "ca", "de", "en-GB", "en", "es", "fr", "it", "nb-NO", "nl", "pl", "pt-BR", "ru", "uk",
    "zh-Hans", "zh-Hant",
];
pub fn router() -> Router<Store> {
    Router::new()
        .route(
            "/api/v1/budgets/{budget}/preferences",
            get(read).patch(update),
        )
        .route("/api/v1/preferences/convert-actual", post(convert))
}
pub fn defaults() -> Value {
    let flags: Map<String, Value> = FEATURES
        .iter()
        .map(|name| (name.to_string(), json!(false)))
        .collect();
    json!({"number_format":"comma-dot","date_format":"MM/dd/yyyy","hide_fraction":false,"first_day_of_week":0,"currency_code":"","currency_symbol_position":"before","currency_space":false,"experimental":flags})
}
pub fn validate_change(key: &str, value: &Value) -> Result<(), Error> {
    if key != "sync_transfer_date" && !defaults().as_object().unwrap().contains_key(key) {
        return Err(Error::invalid(format!("Unknown preference: {key}")));
    }
    if value.is_null() {
        return Ok(());
    }
    let valid = match key {
        "number_format" => value.as_str().is_some_and(|v| NUMBER_FORMATS.contains(&v)),
        "date_format" => value.as_str().is_some_and(|v| DATE_FORMATS.contains(&v)),
        "sync_transfer_date" | "hide_fraction" | "currency_space" => value.is_boolean(),
        "first_day_of_week" => value.as_u64().is_some_and(|v| v <= 6),
        "currency_code" => value
            .as_str()
            .is_some_and(|v| currency_codes::CURRENCY_CODES.contains(&v)),
        "currency_symbol_position" => value
            .as_str()
            .is_some_and(|v| ["before", "after"].contains(&v)),
        "experimental" => value.as_object().is_some_and(|flags| {
            flags
                .iter()
                .all(|(name, v)| FEATURES.contains(&name.as_str()) && v.is_boolean())
        }),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(Error::invalid(format!("Invalid preference: {key}")))
    }
}
/// Actual clears the selected currency when its experimental feature is disabled.
pub(crate) fn normalize_currency(values: &mut Value) {
    if values["experimental"]["currency"] != true {
        values["currency_code"] = json!("");
    }
}
pub(crate) fn remove_retired_features(values: &mut Value) {
    if let Some(values) = values.as_object_mut() {
        values.remove("privacy_mode");
    }
    if let Some(flags) = values
        .get_mut("experimental")
        .and_then(Value::as_object_mut)
    {
        flags.remove("enableBanking");
        flags.remove("akahuBankSync");
        flags.remove("goalTemplatesEnabled");
        flags.remove("goalTemplatesUIEnabled");
    }
}
/// Storage preserves absence for source formula locale/currency fallbacks.
/// Public settings continue to expose complete, usable interface defaults.
pub(crate) fn stored_preferences(db: &Connection, budget: &str) -> Result<Value, Error> {
    store::require_budget(db, budget)?;
    let existing: Option<(String, i64)> = db
        .query_row(
            "SELECT settings,revision FROM budget_preferences WHERE budget_id=?1",
            [budget],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (mut values, revision) = match existing {
        Some((settings, revision)) => (
            serde_json::from_str::<Value>(&settings).map_err(Error::internal)?,
            revision,
        ),
        None => {
            let mut values = defaults();
            values.as_object_mut().unwrap().remove("number_format");
            values
                .as_object_mut()
                .unwrap()
                .remove("currency_symbol_position");
            (values, 0)
        }
    };
    if !values.is_object() {
        return Err(Error::invalid("Invalid stored preferences"));
    }
    remove_retired_features(&mut values);
    normalize_currency(&mut values);
    values["revision"] = json!(revision);
    Ok(values)
}
pub(crate) fn display_defaults(mut values: Value) -> Value {
    for (key, value) in defaults().as_object().unwrap() {
        values
            .as_object_mut()
            .unwrap()
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
    values
}
pub fn get_preferences(db: &Connection, budget: &str) -> Result<Value, Error> {
    Ok(display_defaults(stored_preferences(db, budget)?))
}
pub fn apply_changes(
    db: &Connection,
    budget: &str,
    changes: &Map<String, Value>,
) -> Result<Value, Error> {
    let mut values = stored_preferences(db, budget)?;
    let revision = values["revision"]
        .as_i64()
        .ok_or_else(|| Error::internal("Invalid preference revision"))?;
    values.as_object_mut().unwrap().remove("revision");
    for (key, value) in changes {
        validate_change(key, value)?;
        if value.is_null() {
            values[key] = if key == "sync_transfer_date" {
                json!(false)
            } else {
                defaults()[key].clone()
            };
        } else if key == "experimental" {
            for (name, enabled) in value.as_object().unwrap() {
                values[key][name] = enabled.clone();
            }
        } else {
            values[key] = value.clone();
        }
    }
    normalize_currency(&mut values);
    if changes.contains_key("currency_code")
        || changes
            .get("experimental")
            .is_some_and(|flags| flags.is_null() || flags.get("currency").is_some())
    {
        let clock = crate::concurrency::clock(db, budget, None)?;
        db.execute(
            "UPDATE budget_settings SET template_currency=?2,template_currency_review=0 WHERE budget_id=?1",
            params![budget, values["currency_code"].as_str().unwrap_or("")],
        )?;
        crate::concurrency::seed_field_version(
            db,
            budget,
            "budget_settings",
            "method",
            "template_currency",
            &clock,
        )?;
        crate::concurrency::seed_field_version(
            db,
            budget,
            "budget_settings",
            "method",
            "template_currency_review",
            &clock,
        )?;
        crate::concurrency::mark(db, budget, "budget_settings", "method")?;
    }
    let revision = revision
        .checked_add(1)
        .ok_or_else(|| Error::internal("Preference revision exhausted"))?;
    db.execute("INSERT INTO budget_preferences(budget_id,settings,revision) VALUES(?1,?2,?3) ON CONFLICT(budget_id) DO UPDATE SET settings=excluded.settings,revision=excluded.revision",params![budget,values.to_string(),revision])?;
    values["revision"] = json!(revision);
    Ok(display_defaults(values))
}
async fn read(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<crate::lifecycle::SnapshotQuery>, QueryRejection>,
) -> Result<Json<Value>, Error> {
    let budget = budget_id(path)?;
    let query = crate::lifecycle::snapshot_query(query)?;
    Ok(Json(
        json!({"data":store.run(move|db|{crate::concurrency::check_snapshot(db,&budget,query.snapshot.as_deref())?;get_preferences(db,&budget)}).await?}),
    ))
}
async fn update(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    let budget = budget_id(path)?;
    let Json(input) = input.map_err(json_error)?;
    let changes = input
        .as_object()
        .ok_or_else(|| Error::invalid("Expected a preference changes object"))?
        .clone();
    for (key, value) in &changes {
        // Old committed requests must still resolve their original receipt.
        // apply_changes rejects these retired flags for every new write.
        if key == "privacy_mode" && (value.is_boolean() || value.is_null()) {
            continue;
        } else if key == "experimental" && value.is_object() {
            let mut supported = value.clone();
            supported.as_object_mut().unwrap().retain(|flag, enabled| {
                !matches!(flag.as_str(), "enableBanking" | "akahuBankSync") || !enabled.is_boolean()
            });
            validate_change(key, &supported)?;
        } else {
            validate_change(key, value)?;
        }
    }
    let key = idempotency_key(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let payload = input.to_string();
    let result = store
        .run(move |db| {
            store::write(
                db,
                &format!("PATCH /budgets/{budget}/preferences"),
                &key,
                &payload,
                |db| {
                    crate::lifecycle::require_epoch(db, &budget, epoch.as_deref())?;
                    apply_changes(db, &budget, &changes)
                },
            )
        })
        .await?;
    Ok(Json(json!({"data":result})))
}
async fn convert(input: Result<Json<Value>, JsonRejection>) -> Result<Json<Value>, Error> {
    let Json(input) = input.map_err(json_error)?;
    Ok(Json(json!({"data":convert_actual(&input)?})))
}
