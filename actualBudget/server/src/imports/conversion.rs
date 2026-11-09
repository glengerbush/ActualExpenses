use crate::{
    error::Error,
    ledger,
    models::{MAX_MONEY, validate_date, validate_id, validate_name},
    preferences,
};
use rusqlite::{Connection, OpenFlags, params, types::ValueRef};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read, Write},
};
const MAX_EXPANDED: u64 = 64 * 1024 * 1024;
fn invalid(message: impl Into<String>) -> Error {
    Error::new(
        axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_archive",
        message,
    )
}
fn source_error(_: impl std::fmt::Display) -> Error {
    invalid("The archive contains an invalid Actual SQLite database")
}
fn array<'a>(value: &'a Value, key: &str) -> &'a Vec<Value> {
    value[key].as_array().unwrap()
}
fn text<'a>(row: &'a Value, key: &str) -> Result<&'a str, Error> {
    row[key]
        .as_str()
        .ok_or_else(|| invalid(format!("Missing or invalid {key}")))
}
fn flag(row: &Value, key: &str) -> Result<bool, Error> {
    match row[key].as_i64() {
        Some(0) | None if row[key].is_null() || row[key] == 0 => Ok(false),
        Some(1) => Ok(true),
        _ => Err(invalid(format!("Invalid {key} flag"))),
    }
}
fn rows(db: &Connection, table: &str) -> Result<Vec<Value>, Error> {
    let quoted = format!("\"{}\"", table.replace('"', "\"\""));
    let mut statement = db
        .prepare(&format!("SELECT * FROM {quoted} LIMIT 100001"))
        .map_err(source_error)?;
    let names = statement
        .column_names()
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut cursor = statement.query([]).map_err(source_error)?;
    let mut result = Vec::new();
    while let Some(row) = cursor.next().map_err(source_error)? {
        let mut value = Map::new();
        for (index, name) in names.iter().enumerate() {
            let cell = match row.get_ref(index).map_err(source_error)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => json!(n),
                ValueRef::Real(n) => {
                    if !n.is_finite() {
                        return Err(invalid("Nonfinite source value"));
                    }
                    json!(n)
                }
                ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
                    json!(std::str::from_utf8(bytes).map_err(source_error)?)
                }
            };
            value.insert(name.clone(), cell);
        }
        result.push(Value::Object(value));
        if result.len() > 100000 {
            return Err(invalid("This import supports at most100000 rows per table"));
        }
    }
    Ok(result)
}
fn requirement(
    requirements: &mut Vec<Value>,
    feature: &str,
    count: usize,
    blocking: bool,
    reason: &str,
) {
    if let Some(existing) = requirements.iter_mut().find(|item| {
        item["feature"] == feature && item["blocking"] == blocking && item["reason"] == reason
    }) {
        existing["count"] = json!(existing["count"].as_u64().unwrap() + count as u64);
        return;
    }
    if count > 0 {
        requirements
            .push(json!({"feature":feature,"count":count,"blocking":blocking,"reason":reason}));
    }
}
fn unpack(bytes: &[u8]) -> Result<(Vec<u8>, Value, Vec<String>), Error> {
    // zip-rs indexes entries by name, so exact duplicates can be collapsed before
    // iteration. Validate the original directory count rather than trusting len().
    // This bounded first converter explicitly excludes ZIP64/multi-disk archives.
    let end = (0..bytes.len().saturating_sub(21))
        .rev()
        .take(65536)
        .find(|offset| {
            bytes.get(*offset..*offset + 4) == Some(b"PK\x05\x06")
                && *offset
                    + 22
                    + u16::from_le_bytes([bytes[*offset + 20], bytes[*offset + 21]]) as usize
                    == bytes.len()
        })
        .ok_or_else(|| invalid("Invalid ZIP directory"))?;
    let count = u16::from_le_bytes([bytes[end + 10], bytes[end + 11]]) as usize;
    if bytes[end + 4..end + 8] != [0, 0, 0, 0]
        || count > 128
        || (end >= 20 && &bytes[end - 20..end - 16] == b"PK\x06\x07")
        || u16::from_le_bytes([bytes[end + 8], bytes[end + 9]]) as usize != count
    {
        return Err(invalid(
            "Duplicate entries, ZIP64 and multi-disk archives are not supported",
        ));
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| invalid("Expected an unencrypted Actual ZIP export"))?;
    if count != zip.len() {
        return Err(invalid("Archive has duplicate entry names"));
    }
    let mut entries = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    for index in 0..zip.len() {
        let mut file = zip
            .by_index(index)
            .map_err(|_| invalid("Encrypted or corrupt ZIP entries are not supported"))?;
        let name = file.name().to_owned();
        if name.is_empty()
            || name.contains(['\0', '\\', ':'])
            || name.starts_with('/')
            || name.split('/').any(|p| p == ".." || p == ".")
            || !seen.insert(name.to_lowercase())
        {
            return Err(invalid("Archive has unsafe or duplicate entry names"));
        }
        if file
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(invalid("Archive symbolic links are not supported"));
        }
        if file.encrypted() {
            return Err(invalid("Encrypted input is not supported"));
        }
        if file.size() > MAX_EXPANDED - total {
            return Err(invalid("Expanded archive exceeds64 MiB"));
        }
        let mut content = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_EXPANDED - total + 1)
            .read_to_end(&mut content)
            .map_err(|_| invalid("ZIP entry failed integrity validation"))?;
        total += content.len() as u64;
        if total > MAX_EXPANDED {
            return Err(invalid("Expanded archive exceeds64 MiB"));
        }
        if !file.is_dir() {
            entries.insert(name, content);
        }
    }
    let pairs = entries
        .keys()
        .filter(|name| *name == "db.sqlite" || name.ends_with("/db.sqlite"))
        .filter_map(|name| {
            let prefix = name.strip_suffix("db.sqlite").unwrap();
            entries
                .contains_key(&format!("{prefix}metadata.json"))
                .then(|| prefix.to_owned())
        })
        .collect::<Vec<_>>();
    if pairs.len() != 1 {
        return Err(invalid(
            "Archive must contain exactly one matching db.sqlite and metadata.json",
        ));
    }
    let prefix = &pairs[0];
    let metadata = entries.remove(&format!("{prefix}metadata.json")).unwrap();
    if metadata.len() > 1024 * 1024 {
        return Err(invalid("Metadata exceeds1 MiB"));
    }
    let metadata: Value =
        serde_json::from_slice(&metadata).map_err(|_| invalid("Invalid metadata.json"))?;
    if metadata
        .get("encryptKeyId")
        .is_some_and(|value| !value.is_null() && value != "")
        || !metadata["encryptMeta"].is_null()
    {
        return Err(invalid(
            "Encrypted budgets are not supported; export an unencrypted budget",
        ));
    }
    let database = entries.remove(&format!("{prefix}db.sqlite")).unwrap();
    if !database.starts_with(b"SQLite format 3\0") {
        return Err(invalid("Database is corrupt or encrypted"));
    }
    Ok((database, metadata, entries.into_keys().collect()))
}
pub fn inspect(bytes: &[u8]) -> Result<(Value, Value), Error> {
    let (database, metadata, extra) = unpack(bytes)?;
    let name = text(&metadata, "budgetName")?;
    validate_name(name).map_err(|_| invalid("Invalid source budget name"))?;
    let local_id = text(&metadata, "id")?;
    if local_id.is_empty() || local_id.len() > 1024 {
        return Err(invalid("Invalid source local identity"));
    }
    let mut temporary = tempfile::NamedTempFile::new().map_err(Error::internal)?;
    temporary.write_all(&database).map_err(Error::internal)?;
    temporary.flush().map_err(Error::internal)?;
    let db = Connection::open_with_flags(
        temporary.path(),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(source_error)?;
    db.set_limit(
        rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
        16 * 1024 * 1024,
    )
    .map_err(source_error)?;
    db.set_limit(
        rusqlite::limits::Limit::SQLITE_LIMIT_SQL_LENGTH,
        1024 * 1024,
    )
    .map_err(source_error)?;
    db.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_COLUMN, 256)
        .map_err(source_error)?;
    let started = std::time::Instant::now();
    db.progress_handler(
        10000,
        Some(move || started.elapsed() > std::time::Duration::from_secs(10)),
    )
    .map_err(source_error)?;
    db.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; PRAGMA cache_size=-2048;")
        .map_err(source_error)?;
    let mut catalog = db.prepare("PRAGMA table_list").map_err(source_error)?;
    let kinds = catalog
        .query_map([], |row| row.get::<_, String>(2))
        .map_err(source_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(source_error)?;
    if kinds
        .iter()
        .any(|kind| kind == "virtual" || kind == "shadow")
    {
        return Err(invalid("Virtual source tables are not supported"));
    }
    let integrity: String = db
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(source_error)?;
    if integrity != "ok" {
        return Err(invalid("SQLite integrity check failed"));
    }
    let mut statement = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name LIMIT 129")
        .map_err(source_error)?;
    let tables = statement
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(source_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(source_error)?;
    if tables.len() > 128 {
        return Err(invalid("Source database contains too many tables"));
    }
    let accepted_columns: BTreeMap<String, Vec<String>> =
        serde_json::from_str(include_str!("source-columns-26.10.json")).map_err(Error::internal)?;
    let mut source = Map::new();
    let mut counts = Map::new();
    let mut requirements = Vec::new();
    const LOADED: &[&str] = &[
        "accounts",
        "transactions",
        "payees",
        "payee_mapping",
        "categories",
        "category_groups",
        "category_mapping",
        "preferences",
        "notes",
        "messages_crdt",
        "messages_pending",
    ];
    const NONBLOCKING: &[&str] = &[
        "__meta__",
        "__migrations__",
        "account_groups",
        "category_groups",
        "category_mapping",
        "payee_mapping",
        "dashboard",
        "dashboard_pages",
        "kvcache",
        "kvcache_key",
        "messages_clock",
        "messages_crdt",
        "sqlite_stat1",
        "sqlite_stat4",
    ];
    for table in &tables {
        let quoted = format!("\"{}\"", table.replace('"', "\"\""));
        let mut extended = db
            .prepare(&format!("PRAGMA table_xinfo({quoted})"))
            .map_err(source_error)?;
        let hidden = extended
            .query_map([], |row| row.get::<_, i64>(6))
            .map_err(source_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(source_error)?;
        if hidden.iter().any(|value| *value != 0) {
            return Err(invalid("Generated source columns are not supported"));
        }
        let count: i64 = db
            .query_row(&format!("SELECT COUNT(*) FROM {quoted}"), [], |r| r.get(0))
            .map_err(source_error)?;
        counts.insert(table.clone(), json!(count));
        if LOADED.contains(&table.as_str()) {
            let mut info = db
                .prepare(&format!("PRAGMA table_info({quoted})"))
                .map_err(source_error)?;
            let present = info
                .query_map([], |r| r.get::<_, String>(1))
                .map_err(source_error)?
                .collect::<Result<BTreeSet<_>, _>>()
                .map_err(source_error)?;
            let accepted = accepted_columns
                .get(table)
                .ok_or_else(|| invalid("Unsupported source table"))?
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            for column in present.symmetric_difference(&accepted) {
                requirement(
                    &mut requirements,
                    &format!("source_schema:{table}:{column}"),
                    1,
                    true,
                    "Source column differs from the pinned format and needs explicit conversion",
                );
            }
            source.insert(table.clone(), json!(rows(&db, table)?));
        }
        if ![
            "accounts",
            "transactions",
            "payees",
            "categories",
            "preferences",
            "notes",
        ]
        .contains(&table.as_str())
        {
            requirement(
                &mut requirements,
                table,
                count as usize,
                !NONBLOCKING.contains(&table.as_str()),
                if NONBLOCKING.contains(&table.as_str()) {
                    "Original source records retained; feature metadata is not activated by this ledger converter"
                } else {
                    "A feature converter is required before activation; original records retained"
                },
            );
        }
    }
    for table in LOADED {
        if !source.contains_key(*table) {
            return Err(invalid(format!("Pinned Actual table is missing: {table}")));
        }
    }
    requirement(
        &mut requirements,
        "extra_archive_entries",
        extra.len(),
        true,
        "Additional archive entries require explicit conversion; original bytes retained",
    );
    let source = Value::Object(source);
    for table in ["category_groups", "preferences", "notes"] {
        let mut identities = BTreeSet::new();
        for row in array(&source, table) {
            if !identities.insert(text(row, "id")?) {
                return Err(invalid(format!("Duplicate {table} identity")));
            }
        }
    }
    let mut converted = json!({"name":name,"budget_type":"envelope","accounts":[],"payees":[],"categories":[],"transactions":[],"clocks":[]});
    let mut accounts = BTreeMap::new();
    for row in array(&source, "accounts") {
        let id = text(row, "id")?;
        validate_id(id).map_err(|_| invalid("Unsupported source account identity"))?;
        if accounts.insert(id.to_owned(), row).is_some() {
            return Err(invalid("Duplicate account identity"));
        }
        let name = text(row, "name")?;
        validate_name(name).map_err(|_| invalid("Invalid source account name"))?;
        let unsupported = [
            "balance_current",
            "balance_available",
            "balance_limit",
            "mask",
            "official_name",
            "subtype",
            "bank",
            "account_id",
            "account_sync_source",
            "account_group_id",
            "type",
            "last_sync",
            "last_reconciled",
            "bank_sync_status",
        ]
        .iter()
        .any(|field| !row[*field].is_null());
        requirement(
            &mut requirements,
            "account_configuration",
            usize::from(unsupported || flag(row, "tombstone")?),
            true,
            "Bank, group or deleted-account data needs its feature converter",
        );
        let notes = array(&source, "notes")
            .iter()
            .find(|note| note["id"] == id)
            .and_then(|note| note["note"].as_str())
            .unwrap_or("");
        converted["accounts"].as_array_mut().unwrap().push(json!({"id":id,"name":name,"off_budget":flag(row,"offbudget")?,"closed":flag(row,"closed")?,"notes":notes}));
    }
    let other_notes = array(&source, "notes")
        .iter()
        .filter(|row| !accounts.contains_key(row["id"].as_str().unwrap_or("")))
        .count();
    requirement(
        &mut requirements,
        "notes",
        other_notes,
        true,
        "Category or budget notes may contain templates and require their feature converter",
    );
    let groups = array(&source, "category_groups")
        .iter()
        .map(|row| text(row, "id"))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let mut category_ids = BTreeSet::new();
    for row in array(&source, "categories") {
        let id = text(row, "id")?;
        validate_id(id).map_err(|_| invalid("Unsupported source category identity"))?;
        if !category_ids.insert(id) {
            return Err(invalid("Duplicate category identity"));
        }
        if !groups.contains(text(row, "cat_group")?) {
            return Err(invalid("Category refers to a missing group"));
        }
        validate_name(text(row, "name")?).map_err(|_| invalid("Invalid source category name"))?;
        let unsupported = flag(row, "tombstone")?
            || flag(row, "hidden")?
            || !row["goal_def"].is_null()
            || !row["cleanup_def"].is_null()
            || row["template_settings"] != "{\"source\": \"notes\"}";
        requirement(
            &mut requirements,
            "category_configuration",
            usize::from(unsupported),
            true,
            "Hidden/deleted categories, goals or templates require their converter",
        );
        converted["categories"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":id,"name":row["name"],"is_income":flag(row,"is_income")?}));
    }
    let mut payee_ids = BTreeSet::new();
    let mut transfer_payees = BTreeSet::new();
    for row in array(&source, "payees") {
        let id = text(row, "id")?;
        validate_id(id).map_err(|_| invalid("Unsupported source payee identity"))?;
        if !payee_ids.insert(id) {
            return Err(invalid("Duplicate payee identity"));
        }
        let name = if let Some(account) = row["transfer_acct"].as_str() {
            text(
                accounts
                    .get(account)
                    .ok_or_else(|| invalid("Transfer payee references missing account"))?,
                "name",
            )?
        } else {
            text(row, "name")?
        };
        validate_name(name).map_err(|_| invalid("Invalid source payee name"))?;
        requirement(
            &mut requirements,
            "payee_configuration",
            usize::from(
                flag(row, "tombstone")? || flag(row, "favorite")? || !row["category"].is_null(),
            ),
            true,
            "Deleted/favorite payee or legacy category association requires its converter",
        );
        if !row["transfer_acct"].is_null() {
            transfer_payees.insert(id);
            continue;
        }
        converted["payees"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":id,"name":name}));
    }
    requirement(
        &mut requirements,
        "transfer_payees",
        transfer_payees.len(),
        false,
        "Automatic transfer payees retained in original source and excluded from ordinary payee selection until transfers are supported",
    );
    let aliases = |table: &str, column: &str| -> Result<BTreeMap<String, String>, Error> {
        let mut map = BTreeMap::new();
        for row in array(&source, table) {
            let key = text(row, "id")?.to_owned();
            if map.insert(key, text(row, column)?.to_owned()).is_some() {
                return Err(invalid("Duplicate alias identity"));
            }
        }
        Ok(map)
    };
    let payee_map = aliases("payee_mapping", "targetId")?;
    let category_map = aliases("category_mapping", "transferId")?;
    let resolve = |value: &Value,
                   map: &BTreeMap<String, String>,
                   ids: &BTreeSet<&str>|
     -> Result<Value, Error> {
        if value.is_null() {
            return Ok(Value::Null);
        }
        let mut id = value
            .as_str()
            .ok_or_else(|| invalid("Invalid reference identity"))?
            .to_owned();
        let mut visited = BTreeSet::new();
        for _ in 0..128 {
            if !visited.insert(id.clone()) {
                return Err(invalid("Alias cycle"));
            }
            match map.get(&id) {
                Some(target) if target != &id => id = target.clone(),
                _ => {
                    if !ids.contains(id.as_str()) {
                        return Err(invalid("Transaction reference is missing"));
                    }
                    return Ok(json!(id));
                }
            }
        }
        Err(invalid("Alias chain exceeds supported depth"))
    };
    for id in payee_map.keys() {
        if resolve(&json!(id), &payee_map, &payee_ids).is_err() {
            requirement(
                &mut requirements,
                "unreferenced_payee_alias",
                1,
                false,
                "Obsolete source alias retained; converted transactions must still resolve to an existing payee",
            );
        }
    }
    for id in category_map.keys() {
        if resolve(&json!(id), &category_map, &category_ids).is_err() {
            requirement(
                &mut requirements,
                "unreferenced_category_alias",
                1,
                false,
                "Obsolete source alias retained; converted transactions must still resolve to an existing category",
            );
        }
    }
    let mut transactions = array(&source, "transactions").clone();
    if transactions
        .iter()
        .any(|row| !row["sort_order"].is_null() && row["sort_order"].as_f64().is_none())
    {
        return Err(invalid("Invalid source transaction sort order"));
    }
    transactions.sort_by(|a, b| {
        // SQLite sorts NULL below numeric values. Finite partial comparison
        // also keeps -0.0 and +0.0 equal, unlike total_cmp.
        a["sort_order"]
            .as_f64()
            .partial_cmp(&b["sort_order"].as_f64())
            .expect("source numbers validated finite")
            .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
    });
    let mut transaction_ids = BTreeSet::new();
    let mut totals: BTreeMap<String, [i128; 3]> =
        accounts.keys().map(|id| (id.clone(), [0; 3])).collect();
    let mut rank = 0_i64;
    let mut previous_order = None;
    for row in &transactions {
        let order = row["sort_order"].as_f64();
        if previous_order != Some(order) {
            rank += 1;
            previous_order = Some(order);
        }
        let id = text(row, "id")?;
        validate_id(id).map_err(|_| invalid("Unsupported source transaction identity"))?;
        if !transaction_ids.insert(id) {
            return Err(invalid("Duplicate transaction identity"));
        }
        let account = text(row, "acct")?;
        let sums = totals
            .get_mut(account)
            .ok_or_else(|| invalid("Transaction account is missing"))?;
        let amount = row["amount"]
            .as_i64()
            .filter(|v| (-MAX_MONEY..=MAX_MONEY).contains(v))
            .ok_or_else(|| invalid("Invalid transaction amount"))?;
        let integer = row["date"]
            .as_i64()
            .filter(|v| *v >= 10101 && *v <= 99991231)
            .ok_or_else(|| invalid("Invalid transaction date"))?;
        let date = format!(
            "{:04}-{:02}-{:02}",
            integer / 10000,
            integer / 100 % 100,
            integer % 100
        );
        validate_date(&date).map_err(|_| invalid("Invalid transaction date"))?;
        let cleared = flag(row, "cleared")?;
        let tombstone = flag(row, "tombstone")?;
        let unsupported = flag(row, "isParent")?
            || flag(row, "isChild")?
            || flag(row, "pending")?
            || [
                "transferred_id",
                "parent_id",
                "schedule",
                "financial_id",
                "raw_synced_data",
                "imported_description",
                "type",
                "location",
                "error",
            ]
            .iter()
            .any(|field| !row[*field].is_null());
        requirement(
            &mut requirements,
            "transaction_features",
            usize::from(unsupported),
            true,
            "Split, transfer, schedule or imported/bank metadata requires its converter",
        );
        if !tombstone {
            sums[0] += amount as i128;
            sums[if cleared { 1 } else { 2 }] += amount as i128;
        }
        let mapped_payee = resolve(&row["description"], &payee_map, &payee_ids)?;
        requirement(
            &mut requirements,
            "transfer_payee_transaction",
            usize::from(
                mapped_payee
                    .as_str()
                    .is_some_and(|id| transfer_payees.contains(id)),
            ),
            true,
            "Transaction uses a transfer payee and requires transfer conversion",
        );
        converted["transactions"].as_array_mut().unwrap().push(json!({"id":id,"account_id":account,"amount":amount,"date":date,"cleared":cleared,"reconciled":flag(row,"reconciled")?,"tombstone":tombstone,"starting_balance":flag(row,"starting_balance_flag")?,"payee_id":resolve(&row["description"],&payee_map,&payee_ids)?,"category_id":resolve(&row["category"],&category_map,&category_ids)?,"notes":row["notes"].as_str().unwrap_or(""),"sort_order":rank}));
    }
    let balances=totals.into_iter().map(|(account_id,sums)|{if sums.iter().any(|n|n.abs()>MAX_MONEY as i128){return Err(invalid("Account balance exceeds safe integer range"));}Ok(json!({"account_id":account_id,"total":sums[0] as i64,"cleared":sums[1] as i64,"uncleared":sums[2] as i64}))}).collect::<Result<Vec<_>,Error>>()?;
    let synced = array(&source, "preferences")
        .iter()
        .map(|row| Ok((text(row, "id")?.to_owned(), row["value"].clone())))
        .collect::<Result<Map<_, _>, Error>>()?;
    if let Some(kind) = synced.get("budgetType") {
        if !["tracking", "envelope"].contains(&kind.as_str().unwrap_or("")) {
            return Err(invalid("Unsupported budgeting method"));
        }
        converted["budget_type"] = kind.clone();
    }
    let prefs = preferences::convert_actual(&json!({"synced":synced}))?;
    converted["preferences"] = prefs["shared"].clone();
    for (key, _) in prefs["unmapped"]["synced"].as_object().unwrap() {
        if key != "budgetType" {
            requirement(
                &mut requirements,
                &format!("preference:{key}"),
                1,
                true,
                "Preference needs conversion before activation",
            );
        }
    }
    let mut clocks = BTreeMap::<(String, String, String), Value>::new();
    let mut timestamps = BTreeSet::new();
    for row in array(&source, "messages_crdt") {
        let key = (
            text(row, "dataset")?.to_owned(),
            text(row, "row")?.to_owned(),
            text(row, "column")?.to_owned(),
        );
        let order = text(row, "timestamp")?;
        if !timestamps.insert(order) {
            return Err(invalid("Duplicate source logical timestamp"));
        }
        crate::concurrency::validate_import_clock(order)
            .map_err(|_| invalid("Invalid source logical edit order"))?;
        if crate::concurrency::import_clock_ahead(order)? {
            requirement(
                &mut requirements,
                "future_source_clock",
                1,
                true,
                "Source clock is more than five minutes ahead; correct the source clock and export again without rewriting edit order",
            );
        }
        if clocks
            .get(&key)
            .is_none_or(|old| old["timestamp"].as_str().unwrap() < order)
        {
            clocks.insert(key, row.clone());
        }
    }
    converted["clocks"] = json!(clocks.into_values().collect::<Vec<_>>());
    converted["balances"] = json!(balances);
    let report = json!({"source":metadata,"counts":counts,"accounts":converted["accounts"],"balances":converted["balances"],"source_field_clock_count":converted["clocks"].as_array().unwrap().len(),"requirements":requirements,"preferences":prefs,"source_preferences":source["preferences"],"extra_archive_entries":extra,"checks":{"sqlite_integrity":true,"identifiers":true,"converted_relationships":true,"financial_totals":true}});
    Ok((report, converted))
}
pub fn install(db: &Connection, budget: &str, data: &Value) -> Result<(), Error> {
    db.execute(
        "INSERT INTO budgets(id,name,budget_type) VALUES(?1,?2,?3)",
        params![budget, data["name"].as_str(), data["budget_type"].as_str()],
    )?;
    for row in array(data, "accounts") {
        db.execute("INSERT INTO accounts(budget_id,id,name,notes,off_budget,closed) VALUES(?1,?2,?3,?4,?5,?6)",params![budget,row["id"].as_str(),row["name"].as_str(),row["notes"].as_str(),row["off_budget"].as_bool(),row["closed"].as_bool()])?;
    }
    for row in array(data, "payees") {
        db.execute(
            "INSERT INTO payees(budget_id,id,name) VALUES(?1,?2,?3)",
            params![budget, row["id"].as_str(), row["name"].as_str()],
        )?;
    }
    for row in array(data, "categories") {
        db.execute(
            "INSERT INTO categories(budget_id,id,name,is_income) VALUES(?1,?2,?3,?4)",
            params![
                budget,
                row["id"].as_str(),
                row["name"].as_str(),
                row["is_income"].as_bool()
            ],
        )?;
    }
    for row in array(data, "transactions") {
        db.execute("INSERT INTO transactions(budget_id,id,account_id,amount,date,cleared,reconciled,starting_balance,payee_id,category_id,notes,tombstone,sort_order) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",params![budget,row["id"].as_str(),row["account_id"].as_str(),row["amount"].as_i64(),row["date"].as_str(),row["cleared"].as_bool(),row["reconciled"].as_bool(),row["starting_balance"].as_bool(),row["payee_id"].as_str(),row["category_id"].as_str(),row["notes"].as_str(),row["tombstone"].as_bool(),row["sort_order"].as_i64()])?;
    }
    for row in array(data, "accounts") {
        ledger::refresh_account(db, budget, row["id"].as_str().unwrap())?;
    }
    ledger::validate_budget_total(db, budget)?;
    for expected in array(data, "balances") {
        let actual = ledger::cached_balances(db, budget, expected["account_id"].as_str().unwrap())?;
        if serde_json::to_value(actual).map_err(Error::internal)?
            != json!({"total":expected["total"],"cleared":expected["cleared"],"uncleared":expected["uncleared"]})
        {
            return Err(invalid("Converted financial totals differ from source"));
        }
    }
    db.execute(
        "INSERT INTO budget_preferences(budget_id,settings,revision) VALUES(?1,?2,0)",
        params![budget, data["preferences"].to_string()],
    )?;
    for row in array(data, "clocks") {
        let dataset = row["dataset"].as_str().unwrap();
        let field = row["column"].as_str().unwrap();
        let mapped = match (dataset, field) {
            ("transactions", "acct") => Some("account_id"),
            ("transactions", "description") => Some("payee_id"),
            ("transactions", "category") => Some("category_id"),
            ("transactions", "starting_balance_flag") => Some("starting_balance"),
            (
                "transactions",
                "amount" | "date" | "notes" | "cleared" | "reconciled" | "tombstone",
            ) => Some(field),
            ("accounts", "offbudget") => Some("off_budget"),
            ("accounts", "name" | "closed")
            | ("payees", "name")
            | ("categories", "name" | "is_income") => Some(field),
            _ => None,
        };
        if let Some(mapped) = mapped {
            crate::concurrency::seed_field_version(
                db,
                budget,
                dataset,
                row["row"].as_str().unwrap(),
                mapped,
                row["timestamp"].as_str().unwrap(),
            )?;
        }
        db.execute("INSERT INTO imported_field_clocks(budget_id,dataset,row_id,column_name,edit_order,serialized_value) VALUES(?1,?2,?3,?4,?5,?6)",params![budget,row["dataset"].as_str(),row["row"].as_str(),row["column"].as_str(),row["timestamp"].as_str(),row["value"].as_str()])?;
    }
    Ok(())
}
