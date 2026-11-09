//! Versioned, budget-scoped portable state. SQL identifiers come only from this registry.
use super::{metadata, receipts};
use crate::{error::Error, ledger, models::validate_name};
use axum::http::StatusCode;
use rusqlite::{
    Connection, params,
    types::{Value as SqlValue, ValueRef},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read, Write},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};
pub const MAX_ARCHIVE: usize = 64 * 1024 * 1024;
const MAX_EXPANDED: usize = 256 * 1024 * 1024;
pub const TABLES: &[&str] = &[
    "account_groups",
    "accounts",
    "payees",
    "category_groups",
    "tags",
    "categories",
    "transactions",
    "budget_preferences",
    "register_settings",
    "field_versions",
    "staged_imports",
    "imported_sources",
    "imported_field_clocks",
    "budget_provenance",
];
const AUXILIARY: &[&str] = &[
    "budgets",
    "operations",
    "ledger_totals",
    "budget_sync",
    "budget_changes",
    "budget_lifecycle",
    "backup_policy",
    "budget_backups",
    "native_restore_stages",
    "operation_budget_links",
];
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub scope: String,
    pub operation_id: String,
    pub payload: String,
    pub response: String,
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct BudgetArchive {
    pub budget: Value,
    pub tables: BTreeMap<String, Table>,
    pub receipts: Vec<Receipt>,
    pub clock: String,
}
pub struct Validated {
    pub legacy_category_metadata: bool,
    pub state: BudgetArchive,
    pub blobs: BTreeMap<String, Vec<u8>>,
}
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn invalid(message: impl Into<String>) -> Error {
    Error::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_budget_archive",
        message,
    )
}
pub fn capture(db: &Connection, budget: &str) -> Result<Vec<u8>, Error> {
    let info = metadata(db, budget, false)?;
    // New feature tables must explicitly join the round-trip contract, never disappear silently.
    let mut names = db.prepare(
        "SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )?;
    for name in names.query_map([], |r| r.get::<_, String>(0))? {
        let name = name?;
        if !TABLES.contains(&name.as_str()) && !AUXILIARY.contains(&name.as_str()) {
            return Err(invalid(format!(
                "No budget archive policy is registered for {name}"
            )));
        }
    }
    let mut blobs = BTreeMap::new();
    let mut tables = BTreeMap::new();
    for table in TABLES {
        let mut query = db.prepare(&format!(
            "SELECT * FROM {table} WHERE budget_id=?1 ORDER BY rowid"
        ))?;
        let columns = query
            .column_names()
            .iter()
            .map(|v| (*v).to_owned())
            .collect::<Vec<_>>();
        let rows = query
            .query_map([budget], |row| {
                let mut values = vec![];
                for index in 0..columns.len() {
                    values.push(match row.get_ref(index)? {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(v) => json!(v),
                        ValueRef::Real(v) => json!(v),
                        ValueRef::Text(v) => json!(String::from_utf8_lossy(v)),
                        ValueRef::Blob(v) => {
                            let name = format!("blobs/{}", hash(v));
                            blobs.insert(name.clone(), v.to_vec());
                            json!({"$blob":name})
                        }
                    });
                }
                Ok(values)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        tables.insert((*table).to_owned(), Table { columns, rows });
    }
    let state = BudgetArchive {
        budget: info,
        tables,
        receipts: receipts(db, budget)?,
        clock: db.query_row(
            "SELECT clock FROM budget_sync WHERE budget_id=?1",
            [budget],
            |r| r.get(0),
        )?,
    };
    let body = serde_json::to_vec(&state).map_err(Error::internal)?;
    let mut entries = BTreeMap::from([("budget.json".to_owned(), body)]);
    entries.extend(blobs);
    let manifest = json!({"format":"actualBudget","version":1,"schema_version":13,"budget_id":budget,"entries":entries.iter().map(|(name,bytes)|(name.clone(),json!({"sha256":hash(bytes),"bytes":bytes.len()}))).collect::<serde_json::Map<_,_>>()});
    if entries.len() + 1 > 1000
        || entries.values().map(Vec::len).sum::<usize>() + manifest.to_string().len() > MAX_EXPANDED
    {
        return Err(Error::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "archive_too_large",
            "The budget exceeds the expanded portable archive limit",
        ));
    }
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("manifest.json", options)
        .map_err(Error::internal)?;
    zip.write_all(&serde_json::to_vec(&manifest).map_err(Error::internal)?)
        .map_err(Error::internal)?;
    for (name, bytes) in entries {
        zip.start_file(name, options).map_err(Error::internal)?;
        zip.write_all(&bytes).map_err(Error::internal)?;
    }
    let result = zip.finish().map_err(Error::internal)?.into_inner();
    if result.len() > MAX_ARCHIVE {
        return Err(Error::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "archive_too_large",
            "The budget exceeds the configured portable archive size limit",
        ));
    }
    Ok(result)
}
pub fn validate(bytes: &[u8]) -> Result<Validated, Error> {
    if bytes.len() > MAX_ARCHIVE {
        return Err(invalid("Archive is larger than 64 MiB"));
    }
    let mut zip = ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| invalid("Upload a valid unencrypted actualBudget archive"))?;
    if zip.len() > 1000 {
        return Err(invalid("Archive has too many entries"));
    }
    let mut entries = BTreeMap::new();
    let mut total = 0usize;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|_| invalid("Encrypted or corrupt archive entry"))?;
        let name = entry.name().to_owned();
        if name != "manifest.json"
            && name != "budget.json"
            && !(name.starts_with("blobs/")
                && name.len() == 70
                && name[6..].bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(invalid("Unexpected archive entry"));
        }
        if entries.contains_key(&name) {
            return Err(invalid("Duplicate archive entry"));
        }
        let mut data = vec![];
        (&mut entry)
            .take((MAX_EXPANDED - total + 1) as u64)
            .read_to_end(&mut data)
            .map_err(|_| invalid("Corrupt archive entry"))?;
        total += data.len();
        if total > MAX_EXPANDED {
            return Err(invalid("Expanded archive exceeds 256 MiB"));
        }
        entries.insert(name, data);
    }
    let manifest: Value = serde_json::from_slice(
        entries
            .get("manifest.json")
            .ok_or_else(|| invalid("Missing manifest"))?,
    )
    .map_err(|_| invalid("Invalid manifest"))?;
    if manifest["format"] != "actualBudget"
        || manifest["version"] != 1
        || !matches!(manifest["schema_version"].as_u64(), Some(6..=13))
    {
        return Err(invalid("Unsupported actualBudget archive version"));
    }
    let declared = manifest["entries"]
        .as_object()
        .ok_or_else(|| invalid("Missing archive entry checksums"))?;
    if declared.len() + 1 != entries.len() {
        return Err(invalid("Archive entry set differs from manifest"));
    }
    for (name, expected) in declared {
        let data = entries
            .get(name)
            .ok_or_else(|| invalid("Missing declared archive entry"))?;
        if expected["sha256"] != hash(data) || expected["bytes"] != data.len() {
            return Err(invalid("Archive checksum mismatch"));
        }
    }
    let mut state: BudgetArchive = serde_json::from_slice(
        entries
            .get("budget.json")
            .ok_or_else(|| invalid("Missing budget data"))?,
    )
    .map_err(|_| invalid("Invalid budget state"))?;
    if manifest["schema_version"].as_u64().is_some_and(|v| v < 11) {
        if state.tables.contains_key("register_settings") {
            return Err(invalid("Unexpected legacy register settings"));
        }
        state.tables.insert(
            "register_settings".into(),
            Table {
                columns: vec!["budget_id".into(), "settings".into(), "revision".into()],
                rows: vec![],
            },
        );
    }
    // Schema 6 remains restorable. Only its exact old shape gains the new defaults.
    if manifest["schema_version"] == 6 {
        if state.tables.contains_key("account_groups") {
            return Err(invalid("Unexpected schema 6 account groups"));
        }
        state.tables.insert(
            "account_groups".into(),
            Table {
                columns: ["budget_id", "id", "name", "sort_order", "tombstone"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                rows: vec![],
            },
        );
        let accounts = state
            .tables
            .get_mut("accounts")
            .ok_or_else(|| invalid("Missing accounts"))?;
        let expected = ["budget_id", "id", "name", "notes", "off_budget", "closed"];
        if accounts
            .columns
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            != expected
        {
            return Err(invalid("Unsupported schema 6 accounts"));
        }
        if accounts.rows.iter().any(|row| row.len() != 6) {
            return Err(invalid("Invalid schema 6 account row"));
        }
        accounts.columns.extend(
            ["group_id", "sort_order", "tombstone"]
                .iter()
                .map(|s| s.to_string()),
        );
        let mut order: Vec<_> = accounts
            .rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                (
                    i,
                    row[2].as_str().unwrap_or("").to_owned(),
                    row[1].as_str().unwrap_or("").to_owned(),
                )
            })
            .collect();
        order.sort_by(|a, b| (&a.1, &a.2).cmp(&(&b.1, &b.2)));
        for (rank, (i, _, _)) in order.into_iter().enumerate() {
            accounts.rows[i].extend([Value::Null, json!(rank), json!(0)]);
        }
    }
    if matches!(manifest["schema_version"].as_u64(), Some(6 | 7)) {
        if state.tables.contains_key("category_groups") {
            return Err(invalid("Unexpected category domain in pre-category schema"));
        }
        state.tables.insert(
            "category_groups".into(),
            Table {
                columns: [
                    "budget_id",
                    "id",
                    "name",
                    "is_income",
                    "hidden",
                    "notes",
                    "sort_order",
                    "tombstone",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect(),
                rows: vec![],
            },
        );
        let categories = state
            .tables
            .get_mut("categories")
            .ok_or_else(|| invalid("Missing categories"))?;
        if categories.columns != ["budget_id", "id", "name", "is_income"] {
            return Err(invalid("Unsupported pre-category schema categories"));
        }
        categories.columns.extend(
            [
                "group_id",
                "hidden",
                "notes",
                "sort_order",
                "tombstone",
                "replacement_id",
            ]
            .iter()
            .map(|s| s.to_string()),
        );
        for row in &mut categories.rows {
            if row.len() != 4 {
                return Err(invalid("Invalid pre-category schema category"));
            }
            row.extend([
                Value::Null,
                json!(0),
                json!(""),
                json!(0),
                json!(0),
                Value::Null,
            ]);
        }
    }
    if manifest["schema_version"].as_u64().unwrap() < 12 {
        let accounts = state
            .tables
            .get_mut("accounts")
            .ok_or_else(|| invalid("Missing accounts"))?;
        if accounts.columns.iter().any(|c| c == "last_reconciled") {
            return Err(invalid("Unexpected legacy reconciliation column"));
        }
        accounts.columns.push("last_reconciled".into());
        for row in &mut accounts.rows {
            row.push(Value::Null);
        }
    }
    if manifest["schema_version"].as_u64().unwrap() < 9 {
        if state.tables.contains_key("tags") {
            return Err(invalid("Unexpected legacy tags"));
        }
        state.tables.insert(
            "tags".into(),
            Table {
                columns: [
                    "budget_id",
                    "id",
                    "tag",
                    "color",
                    "description",
                    "hidden",
                    "tombstone",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect(),
                rows: vec![],
            },
        );
        for (name, expected, additions, defaults) in [
            (
                "payees",
                vec!["budget_id", "id", "name"],
                vec![
                    "favorite",
                    "tombstone",
                    "transfer_account_id",
                    "category_id",
                    "merged_into",
                ],
                vec![json!(0), json!(0), Value::Null, Value::Null, Value::Null],
            ),
            (
                "transactions",
                vec![
                    "budget_id",
                    "id",
                    "account_id",
                    "amount",
                    "date",
                    "cleared",
                    "reconciled",
                    "starting_balance",
                    "payee_id",
                    "category_id",
                    "notes",
                    "tombstone",
                    "sort_order",
                ],
                vec!["imported_payee"],
                vec![Value::Null],
            ),
        ] {
            let table = state
                .tables
                .get_mut(name)
                .ok_or_else(|| invalid("Missing legacy domain"))?;
            if table.columns.iter().map(String::as_str).collect::<Vec<_>>() != expected
                || table.rows.iter().any(|r| r.len() != expected.len())
            {
                return Err(invalid("Invalid legacy domain shape"));
            }
            table
                .columns
                .extend(additions.into_iter().map(str::to_owned));
            for row in &mut table.rows {
                row.extend(defaults.clone());
            }
        }
    }
    if manifest["schema_version"].as_u64().is_some_and(|v| v < 10) {
        let transactions = state
            .tables
            .get_mut("transactions")
            .ok_or_else(|| invalid("Missing transactions"))?;
        transactions
            .columns
            .extend(["is_parent".to_owned(), "parent_id".to_owned()]);
        for row in &mut transactions.rows {
            row.extend([json!(0), Value::Null]);
        }
    }
    if manifest["schema_version"].as_u64().unwrap() < 13 {
        let table = state
            .tables
            .get_mut("transactions")
            .ok_or_else(|| invalid("Missing transactions"))?;
        if table.columns.last().map(String::as_str) != Some("parent_id")
            || table.rows.iter().any(|r| r.len() != table.columns.len())
        {
            return Err(invalid("Invalid legacy transactions"));
        }
        table.columns.push("transfer_id".into());
        for row in &mut table.rows {
            row.push(Value::Null);
        }
    }
    if state.budget["id"] != manifest["budget_id"] {
        return Err(invalid("Budget identity differs from manifest"));
    }
    let budget = state.budget["id"]
        .as_str()
        .ok_or_else(|| invalid("Missing budget identity"))?
        .to_owned();
    crate::models::validate_id(&budget).map_err(|_| invalid("Invalid budget identity"))?;
    validate_name(
        state.budget["name"]
            .as_str()
            .ok_or_else(|| invalid("Missing budget name"))?,
    )
    .map_err(|_| invalid("Invalid budget name"))?;
    if !matches!(
        state.budget["budget_type"].as_str(),
        Some("tracking" | "envelope")
    ) {
        return Err(invalid("Unsupported budget method"));
    }
    crate::concurrency::validate_import_clock(&state.clock)
        .map_err(|_| invalid("Invalid preserved edit clock"))?;
    if crate::concurrency::import_clock_ahead(&state.clock)? {
        return Err(invalid(
            "The archive's edit clock is ahead of server time; correct time before restoring",
        ));
    }
    if state
        .tables
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != TABLES.iter().copied().collect()
    {
        return Err(invalid("Unsupported or missing budget domain"));
    }
    let blobs = entries
        .into_iter()
        .filter(|(name, _)| name.starts_with("blobs/"))
        .collect();
    let mut validated = Validated {
        state,
        blobs,
        legacy_category_metadata: matches!(manifest["schema_version"].as_u64(), Some(6 | 7)),
    };
    let mut staging = Connection::open_in_memory().map_err(Error::internal)?;
    staging.execute_batch("PRAGMA foreign_keys=ON;")?;
    let temp = tempfile::tempdir().map_err(Error::internal)?;
    crate::migrations::apply(&mut staging, temp.path())?;
    staging.execute(
        "INSERT INTO budgets(id,name,budget_type) VALUES(?1,?2,?3)",
        params![
            budget,
            validated.state.budget["name"].as_str(),
            validated.state.budget["budget_type"].as_str()
        ],
    )?;
    install_tables(&staging, &validated)
        .map_err(|e| invalid(format!("Archive data failed validation: {}", e.message)))?;
    if manifest["schema_version"].as_u64().unwrap() < 9 {
        crate::imports::backfill_payees_tags(&staging).map_err(|e| invalid(e.message))?;
        // Only newly supported metadata is backfilled; old native fields, exact
        // receipts and original source archives remain the supplied snapshot.
        for table in ["payees", "tags", "transactions", "field_versions"] {
            let mut query = staging.prepare(&format!(
                "SELECT * FROM {table} WHERE budget_id=?1 ORDER BY rowid"
            ))?;
            let columns = query
                .column_names()
                .iter()
                .map(|v| (*v).to_owned())
                .collect::<Vec<_>>();
            let rows = query
                .query_map([&budget], |r| {
                    (0..columns.len())
                        .map(|i| match r.get_ref(i)? {
                            ValueRef::Null => Ok(Value::Null),
                            ValueRef::Integer(v) => Ok(json!(v)),
                            ValueRef::Text(v) => Ok(json!(String::from_utf8_lossy(v))),
                            _ => Err(rusqlite::Error::InvalidQuery),
                        })
                        .collect::<Result<Vec<_>, _>>()
                })?
                .collect::<Result<Vec<_>, _>>()?;
            validated
                .state
                .tables
                .insert(table.into(), Table { columns, rows });
        }
        validated.state.clock = staging.query_row(
            "SELECT clock FROM budget_sync WHERE budget_id=?1",
            [&budget],
            |r| r.get(0),
        )?;
    }
    validate_financial_state(&staging, &budget)?;
    let mut clocks = staging.prepare("SELECT clock FROM field_versions WHERE budget_id=?1")?;
    for clock in clocks.query_map([&budget], |r| r.get::<_, String>(0))? {
        let clock = clock?;
        crate::concurrency::validate_import_clock(&clock)
            .map_err(|_| invalid("Invalid preserved field ordering"))?;
        if crate::concurrency::import_clock_ahead(&clock)? {
            return Err(invalid(
                "An archived field clock is ahead of server time; correct time before restoring",
            ));
        }
    }
    for receipt in &validated.state.receipts {
        super::verify_receipt_budget(&staging, &budget, receipt)?;
        serde_json::from_str::<Value>(&receipt.response)
            .map_err(|_| invalid("Invalid operation outcome"))?;
    }
    Ok(validated)
}
pub fn install_tables(db: &Connection, archive: &Validated) -> Result<(), Error> {
    for table in TABLES {
        let source = &archive.state.tables[*table];
        let allowed = db
            .prepare(&format!("SELECT * FROM {table} LIMIT 0"))?
            .column_names()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        if source.columns != allowed {
            return Err(invalid(format!("Unsupported columns in {table}")));
        }
        let budget_index = source
            .columns
            .iter()
            .position(|s| s == "budget_id")
            .ok_or_else(|| invalid("Domain is missing budget scope"))?;
        let sql = format!(
            "INSERT INTO {table}({}) VALUES({})",
            source.columns.join(","),
            (1..=source.columns.len())
                .map(|i| format!("?{i}"))
                .collect::<Vec<_>>()
                .join(",")
        );
        for row in &source.rows {
            if row.len() != source.columns.len() || row[budget_index] != archive.state.budget["id"]
            {
                return Err(invalid("Archive row belongs to a different budget"));
            }
            let values = row
                .iter()
                .enumerate()
                .map(|(index, value)| match value {
                    Value::Null => Ok(SqlValue::Null),
                    Value::String(v) => Ok(SqlValue::Text(v.clone())),
                    Value::Number(v)
                        if ["categories", "category_groups"].contains(table)
                            && source.columns[index] == "sort_order" =>
                    {
                        v.as_f64()
                            .filter(|n| n.is_finite() && n.abs() <= 9007199254740991.0)
                            .map(SqlValue::Real)
                            .ok_or_else(|| invalid("Invalid category order"))
                    }
                    Value::Number(v) => v
                        .as_i64()
                        .map(SqlValue::Integer)
                        .ok_or_else(|| invalid("Budget numbers must be exact integers")),
                    Value::Object(v) if v.len() == 1 && v.contains_key("$blob") => archive
                        .blobs
                        .get(v["$blob"].as_str().unwrap_or(""))
                        .cloned()
                        .map(SqlValue::Blob)
                        .ok_or_else(|| invalid("Missing referenced binary source")),
                    _ => Err(invalid("Unsupported archive value")),
                })
                .collect::<Result<Vec<_>, _>>()?;
            if *table == "staged_imports" {
                let id = &values[source.columns.iter().position(|c| c == "id").unwrap()];
                let exists = db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM staged_imports WHERE id=?1)",
                    [id],
                    |r| r.get::<_, bool>(0),
                )?;
                if exists {
                    let existing: Vec<SqlValue> =
                        db.query_row("SELECT * FROM staged_imports WHERE id=?1", [id], |r| {
                            (0..source.columns.len()).map(|i| r.get(i)).collect()
                        })?;
                    if existing != values {
                        return Err(invalid(
                            "Source archive identity conflicts with retained provenance",
                        ));
                    }
                    continue;
                }
            }
            db.execute(&sql, rusqlite::params_from_iter(values))?;
        }
    }
    crate::register_settings::enrich_legacy_imports(db, archive.state.budget["id"].as_str())?;
    if archive.legacy_category_metadata {
        crate::categories::enrich_legacy_imports(db, archive.state.budget["id"].as_str())?;
    }
    Ok(())
}
pub fn validate_financial_state(db: &Connection, budget: &str) -> Result<(), Error> {
    crate::transfers::validate(db, budget)?;
    for table in [
        "accounts",
        "account_groups",
        "payees",
        "categories",
        "category_groups",
    ] {
        let mut rows = db.prepare(&format!("SELECT id,name FROM {table} WHERE budget_id=?1"))?;
        for row in rows.query_map([budget], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (id, name) = row?;
            if table == "category_groups" {
                crate::categories::validate_group_id(&id)
            } else {
                crate::models::validate_id(&id)
            }
            .map_err(|e| invalid(e.message))?;
            validate_name(&name).map_err(|e| invalid(e.message))?;
        }
    }
    crate::payees_tags::validate_archive(db, budget).map_err(|e| invalid(e.message))?;
    let invalid_groups:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM accounts a LEFT JOIN account_groups g ON g.budget_id=a.budget_id AND g.id=a.group_id WHERE a.budget_id=?1 AND a.group_id IS NOT NULL AND (g.id IS NULL OR g.tombstone=1))",[budget],|r|r.get(0))?;
    if invalid_groups {
        return Err(invalid("Invalid account group reference"));
    }
    let visible_deleted:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM transactions t JOIN accounts a ON a.budget_id=t.budget_id AND a.id=t.account_id WHERE t.budget_id=?1 AND t.tombstone=0 AND a.tombstone=1)",[budget],|r|r.get(0))?;
    if visible_deleted {
        return Err(invalid("Deleted account has visible history"));
    }
    crate::categories::validate_archive(db, budget).map_err(|e| invalid(e.message))?;
    let mut dates = db.prepare("SELECT last_reconciled FROM accounts WHERE budget_id=?1")?;
    for date in dates.query_map([budget], |r| r.get::<_, Option<String>>(0))? {
        crate::reconciliation::source_timestamp(&json!(date?)).map_err(|e| invalid(e.message))?;
    }
    crate::splits::validate(db, budget).map_err(|e| invalid(e.message))?;
    crate::register_settings::validate_archive(db, budget).map_err(|e| invalid(e.message))?;
    let mut notes = db.prepare("SELECT notes FROM accounts WHERE budget_id=?1")?;
    for text in notes.query_map([budget], |r| r.get::<_, String>(0))? {
        if text?.chars().count() > 10000 {
            return Err(invalid("Account notes exceed 10000 characters"));
        }
    }
    let mut ids = db.prepare("SELECT id FROM transactions WHERE budget_id=?1")?;
    for id in ids.query_map([budget], |r| r.get::<_, String>(0))? {
        let row = ledger::get_any(db, budget, &id?)?;
        ledger::validate_patch(&row, &json!({"notes":row["notes"]}))
            .map_err(|e| invalid(e.message))?;
    }
    let mut ids = db.prepare("SELECT id FROM accounts WHERE budget_id=?1")?;
    for id in ids.query_map([budget], |r| r.get::<_, String>(0))? {
        ledger::refresh_account(db, budget, &id?)?;
    }
    ledger::validate_budget_total(db, budget)?;
    let values = crate::preferences::get_preferences(db, budget)?;
    let mut settings = values
        .as_object()
        .cloned()
        .ok_or_else(|| invalid("Invalid shared preferences"))?;
    settings.remove("revision");
    db.execute_batch("SAVEPOINT validate_preferences")?;
    let valid = crate::preferences::apply_changes(db, budget, &settings);
    db.execute_batch("ROLLBACK TO validate_preferences; RELEASE validate_preferences")?;
    valid.map_err(|e| invalid(e.message))?;
    Ok(())
}
