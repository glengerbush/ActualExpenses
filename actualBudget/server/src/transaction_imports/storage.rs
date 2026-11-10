//! Source metadata and independent account preference cells retain provenance.
use super::*;
const PREFIXES: &[&str] = &[
    "csv-mappings-",
    "csv-delimiter-",
    "csv-encoding-",
    "csv-has-header-",
    "csv-skip-start-lines-",
    "csv-skip-end-lines-",
    "csv-skip-lines-",
    "csv-in-out-mode-",
    "csv-out-value-",
    "parse-date-",
    "flip-amount-",
    "import-notes-",
    "import-reimport-deleted-",
    "ofx-fallback-missing-payee-",
    "ofx-swap-payee-memo-",
    "qif-swap-payee-memo-",
    "camt-swap-payee-memo-",
];
pub(super) fn preference_parts(key: &str) -> Option<(&str, String)> {
    for prefix in PREFIXES {
        let Some(rest) = key.strip_prefix(prefix) else {
            continue;
        };
        let id = rest.get(..36)?;
        if crate::models::validate_id(id).is_err() {
            return None;
        }
        let suffix = &rest[36..];
        let per_format = matches!(*prefix, "parse-date-" | "flip-amount-" | "import-notes-");
        if (per_format && matches!(suffix, "-csv" | "-qif" | "-ofx" | "-qfx" | "-xml"))
            || (!per_format && suffix.is_empty())
        {
            return Some((id, format!("{}{}", prefix.trim_end_matches('-'), suffix)));
        }
    }
    None
}
pub(crate) fn imported_preference(key: &str) -> bool {
    preference_parts(key).is_some()
}
pub(crate) fn convert(source: &Value) -> Result<Value, Error> {
    let accounts = source["accounts"]
        .as_array()
        .ok_or_else(|| Error::invalid("Import account definitions are missing"))?;
    let mut settings = vec![];
    let mut metadata = vec![];
    for row in source["preferences"].as_array().into_iter().flatten() {
        let Some(key) = row["id"].as_str().filter(|key| imported_preference(key)) else {
            continue;
        };
        let (account, option_key) = preference_parts(key).unwrap();
        if !accounts.iter().any(|a| a["id"] == account) {
            return Err(Error::invalid(
                "An import preference refers to an unavailable account",
            ));
        }
        if row["value"].to_string().len() > 64000 {
            return Err(Error::invalid("Import preference exceeds64000 bytes"));
        }
        settings.push(json!({"id":key,"account_id":account,"option_key":option_key,"value":row["value"],"tombstone":row["tombstone"]==1||row["tombstone"]==true}));
    }
    for row in source["transactions"].as_array().into_iter().flatten() {
        for field in ["financial_id", "raw_synced_data", "type"] {
            if !row[field].is_null() && row[field].as_str().is_none_or(|v| v.len() > MAX_FILE) {
                return Err(Error::invalid(format!(
                    "Invalid retained transaction {field}"
                )));
            }
        }
        let pending = match &row["pending"] {
            Value::Null => false,
            Value::Bool(v) => *v,
            Value::Number(n) if n.as_i64() == Some(0) => false,
            Value::Number(n) if n.as_i64() == Some(1) => true,
            _ => return Err(Error::invalid("Invalid source pending status")),
        };
        if row["financial_id"].is_null()
            && row["raw_synced_data"].is_null()
            && row["type"].is_null()
            && !pending
        {
            continue;
        }
        metadata.push(json!({"id":row["id"],"imported_id":row["financial_id"],"raw_synced_data":row["raw_synced_data"],"pending":pending,"import_type":row["type"]}));
    }
    Ok(json!({"settings":settings,"metadata":metadata}))
}
pub(crate) fn install(db: &Connection, b: &str, data: &Value) -> Result<(), Error> {
    for row in data["settings"].as_array().into_iter().flatten() {
        db.execute("INSERT OR IGNORE INTO transaction_import_settings(budget_id,id,account_id,option_key,value,tombstone) VALUES(?1,?2,?3,?4,?5,?6)",params![b,row["id"].as_str(),row["account_id"].as_str(),row["option_key"].as_str(),row["value"].to_string(),row["tombstone"]==true])?;
    }
    for row in data["metadata"].as_array().into_iter().flatten() {
        db.execute("INSERT OR IGNORE INTO transaction_import_metadata(budget_id,id,imported_id,raw_synced_data,pending,import_type) SELECT ?1,?2,?3,?4,?5,?6 WHERE EXISTS(SELECT 1 FROM transactions WHERE budget_id=?1 AND id=?2)",params![b,row["id"].as_str(),row["imported_id"].as_str(),row["raw_synced_data"].as_str(),row["pending"]==true,row["import_type"].as_str()])?;
    }
    Ok(())
}
pub(crate) fn import_clock(db: &Connection, b: &str, row: &Value) -> Result<(), Error> {
    let dataset = row["dataset"].as_str().unwrap_or("");
    let column = row["column"].as_str().unwrap_or("");
    let id = row["row"].as_str().unwrap_or("");
    let (entity, field) = match (dataset, column) {
        ("preferences", "value") if imported_preference(id) => {
            ("transaction_import_settings", "value")
        }
        ("transactions", "financial_id") => ("transaction_import_metadata", "imported_id"),
        ("transactions", "raw_synced_data") => ("transaction_import_metadata", "raw_synced_data"),
        ("transactions", "pending") => ("transaction_import_metadata", "pending"),
        ("transactions", "type") => ("transaction_import_metadata", "import_type"),
        _ => return Ok(()),
    };
    let exists:bool=db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {entity} WHERE budget_id=?1 AND id=?2) AND NOT EXISTS(SELECT 1 FROM field_versions WHERE budget_id=?1 AND entity=?3 AND entity_id=?2 AND field=?4)"),params![b,id,entity,field],|r|r.get(0))?;
    if exists {
        sync::seed_field_version(
            db,
            b,
            entity,
            id,
            field,
            row["timestamp"]
                .as_str()
                .ok_or_else(|| Error::invalid("Source field clock is missing"))?,
        )?;
    }
    Ok(())
}
pub(crate) fn record(db: &Connection, b: &str, entity: &str, id: &str) -> Result<Value, Error> {
    match entity{
 "transaction_import_metadata"=>db.query_row("SELECT imported_id,raw_synced_data,pending,import_type,tombstone FROM transaction_import_metadata WHERE budget_id=?1 AND id=?2",params![b,id],|r|Ok(json!({"id":id,"imported_id":r.get::<_,Option<String>>(0)?,"raw_synced_data":r.get::<_,Option<String>>(1)?,"pending":r.get::<_,bool>(2)?,"import_type":r.get::<_,Option<String>>(3)?,"deleted":r.get::<_,bool>(4)?}))).optional()?.ok_or_else(Error::missing),
 "transaction_import_settings"=>db.query_row("SELECT account_id,option_key,value,tombstone FROM transaction_import_settings WHERE budget_id=?1 AND id=?2",params![b,id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,bool>(3)?))).optional()?.map(|(account,key,value,deleted)|Ok(json!({"id":id,"account_id":account,"option_key":key,"value":serde_json::from_str::<Value>(&value).map_err(Error::internal)?,"deleted":deleted}))).unwrap_or_else(||Err(Error::missing())),_=>Err(Error::internal("Unknown transaction import entity"))}
}
pub(crate) fn decorate_many(db: &Connection, b: &str, rows: &mut [Value]) -> Result<(), Error> {
    for batch in rows.chunks_mut(500) {
        if batch.is_empty() {
            continue;
        }
        let positions = batch
            .iter()
            .enumerate()
            .map(|(i, r)| (r["id"].as_str().unwrap().to_owned(), i))
            .collect::<BTreeMap<_, _>>();
        let mut values = vec![b.to_owned()];
        values.extend(batch.iter().map(|r| r["id"].as_str().unwrap().to_owned()));
        let marks = (2..=values.len())
            .map(|n| format!("?{n}"))
            .collect::<Vec<_>>()
            .join(",");
        let mut q=db.prepare(&format!("SELECT id,imported_id,raw_synced_data,pending,import_type FROM transaction_import_metadata WHERE budget_id=?1 AND tombstone=0 AND id IN ({marks})"))?;
        for r in q.query_map(rusqlite::params_from_iter(values.iter()), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, bool>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })? {
            let (id, imported, raw, pending, kind) = r?;
            let row = &mut batch[positions[&id]];
            for (key, value) in [
                ("imported_id", imported),
                ("raw_synced_data", raw),
                ("import_type", kind),
            ] {
                if let Some(value) = value.filter(|s| !s.is_empty()) {
                    row[key] = json!(value);
                }
            }
            if pending {
                row["pending"] = json!(true);
            }
        }
    }
    Ok(())
}
pub(crate) fn enrich(db: &Connection, only: Option<&str>) -> Result<(), Error> {
    let mut q=db.prepare("SELECT s.budget_id,s.original FROM staged_imports s JOIN imported_sources i ON i.import_id=s.id WHERE s.status='activated' AND (?1 IS NULL OR s.budget_id=?1) UNION SELECT budget_id,original FROM budget_provenance WHERE kind='actual-import' AND original IS NOT NULL AND (?1 IS NULL OR budget_id=?1)")?;
    let originals = q
        .query_map([only], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (b, bytes) in originals {
        let Ok((_, data)) = crate::imports::conversion::inspect(&bytes) else {
            continue;
        };
        install(db, &b, &data["transaction_imports"])?;
        for clock in data["clocks"].as_array().into_iter().flatten() {
            import_clock(db, &b, clock)?;
        }
    }
    let mut q=db.prepare("SELECT id,original,archive_sha256 FROM staged_imports WHERE status<>'activated' AND (?1 IS NULL OR budget_id=?1)")?;
    let stages = q
        .query_map([only], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, bytes, hash) in stages {
        let Ok((mut report, data)) = crate::imports::conversion::inspect(&bytes) else {
            continue;
        };
        report["archive_sha256"] = json!(hash);
        let blocked = report["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["blocking"] == true);
        db.execute(
            "UPDATE staged_imports SET status=?2,report=?3,conversion=?4 WHERE id=?1",
            params![
                id,
                if blocked { "blocked" } else { "ready" },
                report.to_string(),
                data.to_string()
            ],
        )?;
    }
    Ok(())
}

pub(crate) fn archive_tables() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            "transaction_import_files",
            vec![
                "budget_id",
                "id",
                "account_id",
                "filename",
                "sha256",
                "original",
                "created_at",
            ],
        ),
        (
            "transaction_import_metadata",
            vec![
                "budget_id",
                "id",
                "imported_id",
                "raw_synced_data",
                "pending",
                "import_type",
                "tombstone",
            ],
        ),
        (
            "transaction_import_settings",
            vec![
                "budget_id",
                "id",
                "account_id",
                "option_key",
                "value",
                "tombstone",
            ],
        ),
    ]
}

pub(crate) fn validate(db: &Connection, b: &str) -> Result<(), Error> {
    let mut q = db.prepare(
        "SELECT id,filename,sha256,original FROM transaction_import_files WHERE budget_id=?1",
    )?;
    for r in q.query_map([b], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Vec<u8>>(3)?,
        ))
    })? {
        let (id, name, hash, bytes) = r?;
        crate::models::validate_id(&id)?;
        crate::models::validate_name(&name)?;
        if bytes.is_empty()
            || bytes.len() > MAX_FILE
            || format!("{:x}", Sha256::digest(&bytes)) != hash
        {
            return Err(Error::invalid(
                "Retained import file size or checksum is invalid",
            ));
        }
    }
    let mut q = db.prepare(
        "SELECT id,account_id,option_key,value FROM transaction_import_settings WHERE budget_id=?1",
    )?;
    for r in q.query_map([b], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })? {
        let (id, account, key, value) = r?;
        let Some((expected, kind)) = preference_parts(&id) else {
            return Err(Error::invalid("Unsupported remembered import preference"));
        };
        if expected != account
            || kind != key
            || value.len() > 64000
            || serde_json::from_str::<Value>(&value).is_err()
        {
            return Err(Error::invalid("Remembered import preference is invalid"));
        }
    }
    let mut q=db.prepare("SELECT imported_id,raw_synced_data,import_type FROM transaction_import_metadata WHERE budget_id=?1")?;
    for r in q.query_map([b], |r| {
        Ok((
            r.get::<_, Option<String>>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })? {
        let (id, raw, kind) = r?;
        if id
            .iter()
            .chain(raw.iter())
            .chain(kind.iter())
            .any(|v| v.len() > MAX_FILE)
        {
            return Err(Error::invalid("Retained bank metadata exceeds its limit"));
        }
    }
    Ok(())
}

/// File reconciliation replaces the bank ID, while retaining an already saved
/// raw bank payload. Financial visibility/tombstones remain ledger-owned.
pub(super) fn save(
    db: &Connection,
    b: &str,
    id: &str,
    input: &Value,
    clock: &str,
) -> Result<bool, Error> {
    let old=db.query_row("SELECT imported_id,raw_synced_data,pending,import_type,tombstone FROM transaction_import_metadata WHERE budget_id=?1 AND id=?2",params![b,id],|r|Ok(json!({"imported_id":r.get::<_,Option<String>>(0)?,"raw_synced_data":r.get::<_,Option<String>>(1)?,"pending":r.get::<_,bool>(2)?,"import_type":r.get::<_,Option<String>>(3)?,"tombstone":r.get::<_,bool>(4)?}))).optional()?;
    let before=old.clone().unwrap_or(json!({"imported_id":null,"raw_synced_data":null,"pending":false,"import_type":null,"tombstone":false}));
    let mut after = before.clone();
    after["imported_id"] = input["imported_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| json!(s))
        .unwrap_or(Value::Null);
    if after["raw_synced_data"].is_null() {
        after["raw_synced_data"] = input["raw_synced_data"].clone();
    }
    after["tombstone"] = json!(false);
    let fields = [
        "imported_id",
        "raw_synced_data",
        "pending",
        "import_type",
        "tombstone",
    ];
    let changed = fields
        .into_iter()
        .filter(|f| before[*f] != after[*f])
        .collect::<Vec<_>>();
    if changed.is_empty() {
        return Ok(false);
    }
    for field in &changed {
        guard(db, b, "transaction_import_metadata", id, field, clock)?;
    }
    for field in ["imported_id", "raw_synced_data", "import_type"] {
        if !after[field].is_null() && after[field].as_str().is_none_or(|s| s.len() > MAX_FILE) {
            return Err(Error::invalid("Invalid retained bank metadata"));
        }
    }
    db.execute("INSERT INTO transaction_import_metadata(budget_id,id,imported_id,raw_synced_data,pending,import_type) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(budget_id,id) DO UPDATE SET imported_id=excluded.imported_id,raw_synced_data=excluded.raw_synced_data,pending=excluded.pending,import_type=excluded.import_type,tombstone=0",params![b,id,after["imported_id"].as_str(),after["raw_synced_data"].as_str(),after["pending"]==true,after["import_type"].as_str()])?;
    for field in changed {
        sync::seed_field_version(db, b, "transaction_import_metadata", id, field, clock)?;
    }
    sync::mark(db, b, "transaction_import_metadata", id)?;
    Ok(true)
}
