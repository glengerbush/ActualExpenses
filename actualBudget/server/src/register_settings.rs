//! Online register metadata, with immutable retries and document compare-and-swap.
use crate::{
    api::{budget_id, idempotency_key, json_error},
    error::Error,
    register::Query,
    store::{self, Store},
};
use axum::{
    Json, Router,
    extract::{
        Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::HeaderMap,
    routing::get,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct View {
    pub query: Query,
    pub columns: Vec<String>,
    pub as_of: String,
    pub show_extra_balances: bool,
}
impl Default for View {
    fn default() -> Self {
        Self {
            query: Query::default(),
            columns: [
                "date", "account", "payee", "notes", "category", "amount", "status", "balance",
            ]
            .map(str::to_string)
            .to_vec(),
            as_of: String::new(),
            show_extra_balances: true,
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SavedFilter {
    pub id: String,
    pub name: String,
    pub query: Query,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub views: BTreeMap<String, View>,
    pub saved_filters: Vec<SavedFilter>,
}
impl Settings {
    pub fn validate(&self) -> Result<(), Error> {
        if self.views.len() > 1000 || self.saved_filters.len() > 100 {
            return Err(Error::invalid("Too many register views or filters"));
        }
        for (scope, view) in &self.views {
            if !["all", "onbudget", "offbudget", "uncategorized", "closed"]
                .contains(&scope.as_str())
            {
                crate::models::validate_id(scope)?;
            }
            view.query.validate()?;
            if !view.as_of.is_empty() {
                crate::models::validate_date(&view.as_of)?;
            }
            let mut seen = HashSet::new();
            if view.columns.is_empty()
                || view.columns.iter().any(|c| {
                    !seen.insert(c)
                        || ![
                            "date", "account", "payee", "notes", "group", "category", "amount",
                            "payment", "deposit", "status", "balance",
                        ]
                        .contains(&c.as_str())
                })
            {
                return Err(Error::invalid("Invalid register columns"));
            }
        }
        let mut seen = HashSet::new();
        for f in &self.saved_filters {
            crate::models::validate_id(&f.id)?;
            crate::models::validate_name(&f.name)?;
            f.query.validate()?;
            if !seen.insert(&f.id) {
                return Err(Error::invalid("Duplicate saved filter"));
            }
        }
        Ok(())
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Update {
    revision: i64,
    settings: Settings,
}
pub fn router() -> Router<Store> {
    Router::new().route(
        "/api/v1/budgets/{budget}/register-settings",
        get(read).patch(update),
    )
}
pub fn read_settings(db: &Connection, budget: &str) -> Result<Value, Error> {
    store::require_budget(db, budget)?;
    let result: Option<(String, i64)> = db
        .query_row(
            "SELECT settings,revision FROM register_settings WHERE budget_id=?1",
            [budget],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (settings, revision) = match result {
        Some((text, revision)) => (
            serde_json::from_str::<Settings>(&text).map_err(Error::internal)?,
            revision,
        ),
        None => (Settings::default(), 0),
    };
    Ok(
        json!({"settings":settings,"revision":revision,"epoch":crate::concurrency::cursor(db,budget)?[2..34]}),
    )
}
async fn read(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<Value>, Error> {
    let budget = budget_id(path)?;
    Ok(Json(
        json!({"data":store.run(move|db|{let read=db.transaction()?;read_settings(&read,&budget)}).await?}),
    ))
}
async fn update(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<Update>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    let budget = budget_id(path)?;
    let Json(input) = input.map_err(json_error)?;
    input.settings.validate()?;
    let key = idempotency_key(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?
        .ok_or_else(|| Error::invalid("X-Actual-Budget-Epoch is required"))?;
    let payload = serde_json::to_string(&(epoch.clone(), &input)).map_err(Error::internal)?;
    let value=store.run(move|db|store::write(db,&format!("PATCH /budgets/{budget}/register-settings"),&key,&payload,|db|{
        crate::lifecycle::require_epoch(db,&budget,Some(&epoch))?;
        let current=read_settings(db,&budget)?;
        if current["revision"].as_i64()!=Some(input.revision){return Err(Error::new(axum::http::StatusCode::CONFLICT,"register_settings_changed","Register settings changed in another tab. Reload settings before saving."));}
        let next=input.revision.checked_add(1).ok_or_else(||Error::invalid("Revision exhausted"))?;
        db.execute("INSERT INTO register_settings(budget_id,settings,revision) VALUES(?1,?2,?3) ON CONFLICT(budget_id) DO UPDATE SET settings=excluded.settings,revision=excluded.revision",params![budget,serde_json::to_string(&input.settings).map_err(Error::internal)?,next])?;
        crate::concurrency::mark(db,&budget,"register_settings","settings")?;
        read_settings(db,&budget)
    })).await?;
    Ok(Json(json!({"data":value})))
}
pub fn validate_archive(db: &Connection, budget: &str) -> Result<(), Error> {
    let value = read_settings(db, budget)?;
    let settings: Settings =
        serde_json::from_value(value["settings"].clone()).map_err(Error::internal)?;
    settings.validate()?;
    if value["revision"].as_i64().is_none_or(|n| n < 0) {
        return Err(Error::invalid("Invalid register settings revision"));
    }
    Ok(())
}
/// Convert equivalent 26.10 settings; unsupported semantics remain explicit blockers.
pub fn convert_actual(source: &Value) -> (Settings, Vec<String>) {
    let mut settings = Settings::default();
    let mut unsupported = vec![];
    let preferences = source["preferences"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let prefs = preferences
        .iter()
        .filter_map(|r| Some((r["id"].as_str()?, r["value"].clone())))
        .collect::<BTreeMap<_, _>>();
    let mut scopes = ["all", "onbudget", "offbudget", "uncategorized", "closed"]
        .map(str::to_string)
        .to_vec();
    scopes.extend(
        source["accounts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r["id"].as_str().map(str::to_string)),
    );
    for scope in scopes {
        let suffix = if scope == "all" {
            "all-accounts"
        } else {
            &scope
        };
        let mut view = View::default();
        view.query.account_scope = match scope.as_str() {
            "onbudget" => crate::register::AccountScope::Onbudget,
            "offbudget" => crate::register::AccountScope::Offbudget,
            "closed" => crate::register::AccountScope::Closed,
            "uncategorized" => crate::register::AccountScope::Uncategorized,
            _ => crate::register::AccountScope::All,
        };
        view.query.hide_reconciled = prefs
            .get(
                format!(
                    "hide-reconciled-{}",
                    if scope == "all" { "undefined" } else { &scope }
                )
                .as_str(),
            )
            .is_some_and(|v| v == "true" || v == true);
        view.show_extra_balances = prefs
            .get(format!("show-extra-balances-{suffix}").as_str())
            .is_some_and(|v| v == "true" || v == true);
        let raw = prefs
            .get(format!("transaction-table-columns-{suffix}").as_str())
            .filter(|value| value.as_str() != Some(""))
            .or_else(|| prefs.get("transaction-table-columns"));
        if let Some(raw) = raw {
            let columns = raw
                .as_str()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .unwrap_or_else(|| raw.clone());
            let defaults = [
                "date", "account", "payee", "notes", "group", "category", "payment", "deposit",
                "balance", "cleared",
            ];
            let mut parsed: Vec<(String, bool)> = vec![];
            for c in columns.as_array().into_iter().flatten() {
                if let Some(id) = c["id"].as_str()
                    && defaults.contains(&id)
                    && !parsed.iter().any(|(s, _)| s == id)
                {
                    parsed.push((id.into(), c["hidden"] == true && id != "date"));
                }
            }
            if ["payment", "deposit"]
                .iter()
                .all(|id| parsed.iter().any(|(s, hidden)| s == id && *hidden))
            {
                for (id, hidden) in &mut parsed {
                    if id == "payment" || id == "deposit" {
                        *hidden = false;
                    }
                }
            }
            for (position, id) in defaults.iter().enumerate() {
                if !parsed.iter().any(|(s, _)| s == id) {
                    let at = parsed
                        .iter()
                        .position(|(s, _)| defaults.iter().position(|d| d == s).unwrap() > position)
                        .unwrap_or(parsed.len());
                    parsed.insert(at, (id.to_string(), *id == "group" || *id == "balance"));
                }
            }
            view.columns = parsed
                .into_iter()
                .filter(|(_, hidden)| !*hidden)
                .map(|(id, _)| if id == "cleared" { "status".into() } else { id })
                .collect();
        } else {
            view.columns = [
                "date", "account", "payee", "notes", "category", "payment", "deposit", "status",
                "balance",
            ]
            .map(str::to_string)
            .to_vec();
            if !prefs
                .get(
                    format!(
                        "show-balances-{}",
                        if scope == "all" { "undefined" } else { &scope }
                    )
                    .as_str(),
                )
                .is_some_and(|v| v == "true" || v == true)
            {
                view.columns.retain(|c| c != "balance");
            }
            // Actual's legacy hide-cleared controls the status COLUMN, not rows.
            if prefs
                .get(
                    format!(
                        "hide-cleared-{}",
                        if scope == "all" { "undefined" } else { &scope }
                    )
                    .as_str(),
                )
                .is_some_and(|v| v == "true" || v == true)
            {
                view.columns.retain(|c| c != "status");
            }
            if prefs
                .get(format!("show-group-{suffix}").as_str())
                .is_some_and(|v| v == "true" || v == true)
            {
                let position = view
                    .columns
                    .iter()
                    .position(|c| c == "category")
                    .unwrap_or(view.columns.len());
                view.columns.insert(position, "group".into());
            }
        }
        settings.views.insert(scope, view);
    }
    for row in source["transaction_filters"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if row["tombstone"] == 1 || row["tombstone"] == true {
            continue;
        }
        let conditions = row["conditions"]
            .as_str()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .unwrap_or_else(|| row["conditions"].clone());
        if conditions.as_array().is_some_and(|conditions| {
            conditions
                .iter()
                .any(|condition| condition["field"] == "notes")
        }) {
            // Omitting the entire saved filter avoids broadening a retired predicate.
            // Its original definition remains in the retained import archive.
            unsupported.push(format!("retired-saved-filter:{}", row["id"]));
            continue;
        }
        let mut query = Query::default();
        query.filter.op = if row["conditions_op"] == "or" || row["conditionsOp"] == "or" {
            crate::register::Junction::Or
        } else {
            crate::register::Junction::And
        };
        let mut valid = true;
        for condition in conditions.as_array().into_iter().flatten() {
            let mut field = condition["field"].as_str().unwrap_or("").to_string();
            let op = match condition["op"].as_str().unwrap_or("") {
                "is" => "eq",
                "isNot" => "ne",
                "oneOf" => "in",
                "notOneOf" => "not_in",
                "doesNotContain" => "not_contains",
                s => s,
            };
            if ["contains", "not_contains", "matches"].contains(&op)
                && ["account", "payee", "category", "category_group"].contains(&field.as_str())
            {
                field += "_name";
            }
            let mut value = condition["value"].clone();
            if field == "date"
                && let Some(number) = value.as_i64()
            {
                let s = number.to_string();
                if s.len() == 8 {
                    value = json!(format!("{}-{}-{}", &s[..4], &s[4..6], &s[6..]));
                }
            }
            let options = serde_json::from_value::<crate::register::Options>(
                condition
                    .get("options")
                    .filter(|v| !v.is_null())
                    .cloned()
                    .unwrap_or(json!({})),
            );
            if options.is_err() {
                valid = false;
            }
            query.filter.conditions.push(crate::register::Condition {
                field,
                op: op.into(),
                value,
                options: options.unwrap_or_default(),
            });
        }
        if conditions.as_array().is_none() || !valid || query.validate().is_err() {
            unsupported.push(format!("saved-filter:{}", row["id"]));
            continue;
        }
        settings.saved_filters.push(SavedFilter {
            id: row["id"].as_str().unwrap_or("").into(),
            name: row["name"].as_str().unwrap_or("").into(),
            query,
        });
    }
    if settings.validate().is_err() {
        unsupported.push("invalid-register-settings".into());
    }
    (settings, unsupported)
}
pub fn imported_preference(key: &str) -> bool {
    key == "transaction-table-columns"
        || [
            "transaction-table-columns-",
            "hide-reconciled-",
            "show-balances-",
            "hide-cleared-",
            "show-group-",
            "show-extra-balances-",
        ]
        .iter()
        .any(|prefix| key.starts_with(prefix))
}
pub fn enrich_legacy_imports(db: &Connection, only_budget: Option<&str>) -> Result<(), Error> {
    let mut q=db.prepare("SELECT i.budget_id,s.original FROM imported_sources i JOIN staged_imports s ON s.id=i.import_id AND COALESCE(json_extract(s.report,'$.source_format'),'actual') NOT IN('ynab4','nynab') WHERE (?1 IS NULL OR i.budget_id=?1) AND NOT EXISTS(SELECT 1 FROM register_settings r WHERE r.budget_id=i.budget_id)")?;
    let sources = q
        .query_map([only_budget], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (budget, bytes) in sources {
        let (_, data) = crate::imports::conversion::inspect(&bytes)?;
        if let Some(settings) = data.get("register_settings") {
            db.execute("INSERT OR IGNORE INTO register_settings(budget_id,settings,revision) VALUES(?1,?2,0)",params![budget,settings.to_string()])?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod retirement_tests {
    use super::*;

    #[test]
    fn note_filters_stay_inactive_while_financial_filters_are_imported() {
        let (settings, unsupported) = convert_actual(&json!({"transaction_filters":[
            {"id":"00000000-0000-4000-8000-000000000001","name":"Notes","conditions_op":"or","conditions":[{"field":"notes","op":"contains","value":"old"},{"field":"amount","op":"is","value":100}]},
            {"id":"00000000-0000-4000-8000-000000000002","name":"Amount","conditions_op":"and","conditions":[{"field":"amount","op":"is","value":100}]}
        ]}));
        assert_eq!(settings.saved_filters.len(), 1);
        assert_eq!(settings.saved_filters[0].name, "Amount");
        assert_eq!(
            unsupported,
            vec!["retired-saved-filter:\"00000000-0000-4000-8000-000000000001\""]
        );
    }
}
