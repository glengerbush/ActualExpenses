//! YNAB source adapters produce validated canonical rows; raw exports stay intact.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
mod ynab4;
fn invalid(message: impl Into<String>) -> Error {
    Error::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid_ynab", message)
}
fn rows<'a>(data: &'a Value, key: &str) -> Result<&'a Vec<Value>, Error> {
    let rows = data[key]
        .as_array()
        .ok_or_else(|| invalid(format!("Missing {key} records")))?;
    if rows.len() > 100000 {
        return Err(invalid(
            "At most100000 source rows are supported per collection",
        ));
    }
    Ok(rows)
}
fn text<'a>(row: &'a Value, key: &str) -> Result<&'a str, Error> {
    row[key]
        .as_str()
        .ok_or_else(|| invalid(format!("Missing or invalid {key}")))
}
fn identity(kind: &str, value: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(kind.as_bytes());
    hash.update([0]);
    hash.update(value.as_bytes());
    let mut bytes: [u8; 16] = hash.finalize()[..16].try_into().unwrap();
    bytes[6] = (bytes[6] & 15) | 0x50;
    bytes[8] = (bytes[8] & 63) | 0x80;
    Uuid::from_bytes(bytes).to_string()
}
fn money(value: &Value, multiplier: f64) -> Result<i64, Error> {
    let number = value
        .as_f64()
        .ok_or_else(|| invalid("Invalid source amount"))?
        * multiplier;
    let lower = number.floor();
    let rounded = if number - lower >= 0.5 {
        lower + 1.0
    } else {
        lower
    };
    if !rounded.is_finite() || rounded.abs() > crate::models::MAX_MONEY as f64 {
        return Err(invalid("Source amount exceeds exact money range"));
    }
    Ok(rounded as i64)
}
struct Source {
    data: Value,
    mappings: BTreeMap<String, String>,
    payee_names: BTreeMap<String, String>,
    off_budget: BTreeMap<String, bool>,
}
impl Source {
    fn new() -> Result<Self, Error> {
        let tables: serde_json::Map<String, Value> =
            serde_json::from_str(include_str!("../source-columns-26.10.json"))
                .map_err(Error::internal)?;
        Ok(Self {
            data: Value::Object(
                tables
                    .into_iter()
                    .map(|(key, _)| (key, json!([])))
                    .collect(),
            ),
            mappings: BTreeMap::new(),
            payee_names: BTreeMap::new(),
            off_budget: BTreeMap::new(),
        })
    }
    fn push(&mut self, table: &str, row: Value) {
        self.data[table].as_array_mut().unwrap().push(row);
    }
    fn note(&mut self, id: &str, note: &Value) -> Result<(), Error> {
        if !note.is_null() && !note.is_string() {
            return Err(invalid("Invalid source note"));
        }
        if let Some(note) = note.as_str().filter(|s| !s.is_empty()) {
            self.push("notes", json!({"id":id,"note":note}));
        }
        Ok(())
    }
    fn map(&mut self, kind: &str, source: &str) -> Result<String, Error> {
        if source.is_empty() || source.len() > 1024 {
            return Err(invalid("Invalid source identity"));
        }
        let key = format!("{kind}:{source}");
        if self.mappings.contains_key(&key) {
            return Err(invalid(format!("Duplicate {kind} identity")));
        }
        let id = identity(kind, source);
        self.mappings.insert(key, id.clone());
        Ok(id)
    }
    fn find(&self, kind: &str, source: &Value) -> Result<Value, Error> {
        if source.is_null() {
            return Ok(Value::Null);
        }
        let raw = source
            .as_str()
            .ok_or_else(|| invalid("Invalid source reference"))?;
        self.mappings
            .get(&format!("{kind}:{raw}"))
            .map(|id| json!(id))
            .ok_or_else(|| invalid(format!("Missing {kind} reference")))
    }
    fn income(&mut self) {
        let (group, income, starting) = (
            identity("builtin", "income-group"),
            identity("builtin", "income"),
            identity("builtin", "starting-balances"),
        );
        self.push("category_groups",json!({"id":group,"name":"Income","is_income":1,"sort_order":1000000,"tombstone":0,"hidden":0}));
        for (index, id, name) in [(0, income, "Income"), (1, starting, "Starting Balances")] {
            self.push("categories",json!({"id":id,"name":name,"is_income":1,"cat_group":group,"sort_order":index,"tombstone":0,"hidden":0}));
        }
    }
}
pub(super) fn inspect(kind: &str, filename: &str, bytes: &[u8]) -> Result<(Value, Value), Error> {
    if bytes.is_empty() {
        return Err(invalid("The export is empty"));
    }
    let digest = format!("{:x}", Sha256::digest(bytes));
    let (name, source, requirements) = match kind {
        "ynab4" => ynab4::convert(filename, bytes)?,
        _ => return Err(invalid("Unsupported YNAB source format")),
    };
    let counts = source
        .data
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| (key.clone(), json!(value.as_array().unwrap().len())))
        .collect();
    let metadata = json!({"id":format!("{kind}:{digest}"),"budgetName":name,"format":kind,"filename":filename,"source_id_map":source.mappings});
    let (mut report, plan) =
        conversion::convert_loaded(source.data, metadata, counts, requirements, vec![])?;
    report["source_format"] = json!(kind);
    report["checks"]["sqlite_integrity"] = Value::Null;
    report["checks"]["source_format"] = json!(kind);
    Ok((report, plan))
}
