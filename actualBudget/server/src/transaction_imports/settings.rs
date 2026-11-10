//! Remembered account defaults do not retarget an already captured import.
use super::*;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct QueryOptions {
    #[serde(default = "csv")]
    format: String,
}
fn csv() -> String {
    "csv".into()
}
pub(super) fn defaults(
    db: &Connection,
    b: &str,
    account: &str,
    format: &str,
) -> Result<Value, Error> {
    crate::ledger::account_exists(db, b, account)?;
    if !matches!(format, "csv" | "tsv" | "ofx" | "qfx" | "qif" | "xml") {
        return Err(Error::invalid("Choose a supported import format"));
    }
    let kind = if format == "tsv" { "csv" } else { format };
    let mut raw = serde_json::Map::new();
    let mut q=db.prepare("SELECT id,value FROM transaction_import_settings WHERE budget_id=?1 AND account_id=?2 AND tombstone=0 ORDER BY id")?;
    for row in q.query_map(params![b, account], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (id, value) = row?;
        raw.insert(
            id,
            serde_json::from_str::<Value>(&value).map_err(Error::internal)?,
        );
    }
    let text = |key: &str| raw.get(key).and_then(Value::as_str);
    let key = |prefix: &str| format!("{prefix}-{account}");
    let per_format = |prefix: &str| format!("{prefix}-{account}-{kind}");
    let mut options = parse::Options::default();
    let mut mapping = parse::Mapping::default();
    let mut errors = vec![];
    if format == "tsv" {
        options.delimiter = "\t".into();
    }
    if let Some(value) = text(&key("csv-delimiter")) {
        options.delimiter = value.into();
    }
    if let Some(value) = text(&key("csv-encoding")) {
        options.encoding = value.into();
    }
    options.has_header_row = text(&key("csv-has-header")) != Some("false");
    let integer = |key: &str| text(key).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    options.skip_start_lines = if raw.contains_key(&key("csv-skip-start-lines")) {
        integer(&key("csv-skip-start-lines"))
    } else {
        integer(&key("csv-skip-lines"))
    };
    options.skip_end_lines = integer(&key("csv-skip-end-lines"));
    options.in_out_mode = text(&key("csv-in-out-mode")) == Some("true");
    options.out_value = text(&key("csv-out-value")).unwrap_or("").into();
    if let Some(value) = text(&per_format("parse-date")) {
        options.date_format = value.into();
    }
    options.flip_amount = text(&per_format("flip-amount")) == Some("true");
    options.import_notes = text(&per_format("import-notes")) != Some("false");
    options.reimport_deleted = text(&key("import-reimport-deleted")).unwrap_or("true") == "true";
    options.fallback_missing_payee_to_memo =
        text(&key("ofx-fallback-missing-payee")) != Some("false");
    let swap = match kind {
        "ofx" | "qfx" => "ofx-swap-payee-memo",
        "qif" => "qif-swap-payee-memo",
        "xml" => "camt-swap-payee-memo",
        _ => "",
    };
    options.swap_payee_and_memo = !swap.is_empty() && text(&key(swap)) == Some("true");
    if matches!(kind, "csv")
        && let Some(value) = text(&key("csv-mappings"))
    {
        let parsed = serde_json::from_str::<Value>(value)
            .map(|mut v| {
                if let Some(map) = v.as_object_mut()
                    && let Some(value) = map.remove("inOut")
                {
                    map.insert("in_out".into(), value);
                }
                v
            })
            .ok()
            .and_then(|v| serde_json::from_value::<parse::Mapping>(v).ok());
        match parsed{Some(value)=>mapping=value,None=>errors.push("The saved column mapping needs review. Its original value is retained; choose columns explicitly before saving new defaults.")}
    }
    options.split_mode = mapping.inflow.as_deref().is_some_and(|s| !s.is_empty())
        || mapping.outflow.as_deref().is_some_and(|s| !s.is_empty());
    let mut clocks = serde_json::Map::new();
    let mut q=db.prepare("SELECT v.entity_id,v.field,v.clock FROM field_versions v JOIN transaction_import_settings s ON s.budget_id=v.budget_id AND s.id=v.entity_id WHERE v.budget_id=?1 AND v.entity='transaction_import_settings' AND s.account_id=?2")?;
    for row in q.query_map(params![b, account], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (id, field, clock) = row?;
        let row = clocks.entry(id).or_insert(json!({}));
        row[field] = json!(clock);
    }
    Ok(
        json!({"format":format,"options":options,"mapping":mapping,"raw_preferences":raw,"field_versions":clocks,"errors":errors,"snapshot":sync::cursor(db,b)?,"epoch":crate::lifecycle::epoch(db,b)?}),
    )
}
pub(super) async fn read(
    State(store): State<Store>,
    Path((b, account)): Path<(String, String)>,
    query: Result<Query<QueryOptions>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    crate::models::validate_id(&account)?;
    let Query(query) = query.map_err(|e| Error::invalid(e.body_text()))?;
    store
        .run(move |db| {
            let tx = db.transaction()?;
            Ok(json!({"data":defaults(&tx,&b,&account,&query.format)?}))
        })
        .await
        .map(Json)
}
