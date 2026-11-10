//! Bank preferences use the original source preference identities and clocks.
use super::*;
use serde::Serialize;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Mapping {
    pub date: String,
    pub payee: String,
    pub notes: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Mappings {
    pub payment: Mapping,
    pub deposit: Mapping,
}
#[derive(Clone, Serialize)]
pub(super) struct Settings {
    pub import_transactions: bool,
    pub import_pending: bool,
    pub import_notes: bool,
    pub update_dates: bool,
    pub mappings: Mappings,
}
const KEYS: [(&str, &str); 5] = [
    ("import_transactions", "sync-import-transactions"),
    ("import_pending", "sync-import-pending"),
    ("import_notes", "sync-import-notes"),
    ("update_dates", "sync-update-dates"),
    ("mappings", "custom-sync-mappings"),
];
fn raw(db: &Connection, b: &str, a: &str) -> Result<serde_json::Map<String, Value>, Error> {
    let mut result = serde_json::Map::new();
    for (key, prefix) in KEYS {
        let id = format!("{prefix}-{a}");
        if let Some(value)=db.query_row("SELECT value FROM transaction_import_settings WHERE budget_id=?1 AND id=?2 AND tombstone=0",params![b,id],|r|r.get::<_,String>(0)).optional()?{result.insert(key.into(),serde_json::from_str(&value).map_err(Error::internal)?);}
    }
    Ok(result)
}
fn flag(raw: &serde_json::Map<String, Value>, key: &str, default: bool) -> Result<bool, Error> {
    match raw.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Bool(v)) => Ok(*v),
        Some(Value::String(v)) if v == "true" || v == "false" => Ok(v == "true"),
        _ => Err(Error::invalid(
            "A retained bank preference is unsupported. Review its settings before refreshing",
        )),
    }
}
fn mappings(value: Option<&Value>) -> Result<Mappings, Error> {
    let defaults = || Mapping {
        date: "date".into(),
        payee: "payeeName".into(),
        notes: "notes".into(),
    };
    let result = match value {
        None | Some(Value::Null) => Mappings {
            payment: defaults(),
            deposit: defaults(),
        },
        Some(Value::String(v)) => serde_json::from_str(v).map_err(|_| {
            Error::invalid("Repair the retained custom bank mappings before refreshing")
        })?,
        Some(v) => serde_json::from_value(v.clone()).map_err(|_| {
            Error::invalid("Repair the retained custom bank mappings before refreshing")
        })?,
    };
    for m in [&result.payment, &result.deposit] {
        for field in [&m.date, &m.payee, &m.notes] {
            if field.is_empty() || field.len() > 120 || field.chars().any(char::is_control) {
                return Err(Error::invalid(
                    "Bank mapping field names must contain 1–120 characters",
                ));
            }
        }
    }
    Ok(result)
}
pub(super) fn get(db: &Connection, b: &str, a: &str) -> Result<Settings, Error> {
    let values = raw(db, b, a)?;
    Ok(Settings {
        import_transactions: flag(&values, "import_transactions", true)?,
        import_pending: flag(&values, "import_pending", true)?,
        import_notes: flag(&values, "import_notes", true)?,
        update_dates: flag(&values, "update_dates", false)?,
        mappings: mappings(values.get("mappings"))?,
    })
}
pub(super) fn read(db: &Connection, b: &str, a: &str) -> Result<Value, Error> {
    crate::ledger::account_exists(db, b, a)?;
    read_retained(db, b, a)
}
// Explicit unlink can release metadata for a soft-deleted local account.
// All ordinary reads/settings/links retain the live-account guard.
pub(super) fn read_retained(db: &Connection, b: &str, a: &str) -> Result<Value, Error> {
    let raw = raw(db, b, a)?;
    let mut versions = serde_json::Map::new();
    for (key, prefix) in KEYS {
        let id = format!("{prefix}-{a}");
        let version=db.query_row("SELECT clock FROM field_versions WHERE budget_id=?1 AND entity='transaction_import_settings' AND entity_id=?2 AND field='value'",params![b,id],|r|r.get::<_,String>(0)).optional()?.unwrap_or_else(||sync::ZERO.into());
        versions.insert(key.into(), json!(version));
    }
    let (settings, error) = match get(db, b, a) {
        Ok(s) => (serde_json::to_value(s).map_err(Error::internal)?, None),
        Err(e) => (Value::Null, Some(e.message)),
    };
    Ok(json!({"settings":settings,"raw_preferences":raw,"field_versions":versions,"error":error}))
}
pub(super) fn update(
    db: &Connection,
    b: &str,
    a: &str,
    changes: &Value,
    clock: &str,
) -> Result<bool, Error> {
    let changes = changes
        .as_object()
        .filter(|o| !o.is_empty() && o.len() <= 5)
        .ok_or_else(|| Error::invalid("Choose 1–5 bank preference fields"))?;
    let mut prepared = vec![];
    for (key, value) in changes {
        let prefix = KEYS
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, prefix)| *prefix)
            .ok_or_else(|| Error::invalid("Unknown bank preference"))?;
        let value = if value.is_null() {
            Value::Null
        } else if key == "mappings" {
            json!(serde_json::to_string(&mappings(Some(value))?).map_err(Error::internal)?)
        } else {
            json!(
                value
                    .as_bool()
                    .ok_or_else(|| Error::invalid("Bank import options must be booleans or null"))?
                    .to_string()
            )
        };
        prepared.push((format!("{prefix}-{a}"), prefix, value));
    }
    let mut changed = false;
    for (id, prefix, value) in prepared {
        let before=db.query_row("SELECT value,tombstone FROM transaction_import_settings WHERE budget_id=?1 AND id=?2",params![b,id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,bool>(1)?))).optional()?;
        if before.as_ref().is_none_or(|(_, deleted)| *deleted) && value.is_null() {
            continue;
        }
        if before
            .as_ref()
            .is_some_and(|(v, deleted)| !*deleted && v == &value.to_string())
        {
            continue;
        }
        for field in ["value", "tombstone"] {
            crate::transaction_imports::guard(
                db,
                b,
                "transaction_import_settings",
                &id,
                field,
                clock,
            )?;
        }
        db.execute("INSERT INTO transaction_import_settings(budget_id,id,account_id,option_key,value,tombstone) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(budget_id,id) DO UPDATE SET value=excluded.value,tombstone=excluded.tombstone",params![b,id,a,prefix,value.to_string(),value.is_null()])?;
        for field in ["value", "tombstone"] {
            sync::seed_field_version(db, b, "transaction_import_settings", &id, field, clock)?;
        }
        sync::mark(db, b, "transaction_import_settings", &id)?;
        changed = true;
    }
    Ok(changed)
}
