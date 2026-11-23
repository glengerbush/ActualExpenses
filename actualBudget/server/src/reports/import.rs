use super::*;
/// Source fields retain their pinned names, values and entity identity.
/// Unknown widget metadata survives unchanged; its renderer reports availability.
pub fn convert(source: &Value) -> Result<Value, Error> {
    let mut documents = vec![];
    for (table, kind) in [
        ("dashboard_pages", "page"),
        ("dashboard", "widget"),
        ("custom_reports", "custom_report"),
    ] {
        for row in source[table].as_array().into_iter().flatten() {
            let mut config = row
                .as_object()
                .ok_or_else(|| Error::invalid("Invalid source reporting document"))?
                .clone();
            let id = config
                .remove("id")
                .and_then(|v| v.as_str().map(str::to_owned))
                .ok_or_else(|| Error::invalid("Missing source reporting ID"))?;
            let tombstone = config
                .remove("tombstone")
                .is_some_and(|v| v == true || v == 1);
            for key in ["meta", "conditions", "metadata", "selected_categories"] {
                if let Some(Value::String(text)) = config.get(key)
                    && !text.is_empty()
                {
                    let parsed: Value = serde_json::from_str(text)
                        .map_err(|_| Error::invalid("Invalid source reporting JSON"))?;
                    config.insert(key.into(), parsed);
                }
            }
            let d = Document {
                id,
                kind: kind.into(),
                revision: 1,
                config: Value::Object(config.clone()),
                tombstone: tombstone
                    || (kind == "widget" && retired_widget(&Value::Object(config.clone()))),
            };
            validate(&d)?;
            documents.push(d);
        }
    }
    Ok(json!(documents))
}
pub fn install(db: &Connection, b: &str, data: &Value) -> Result<(), Error> {
    for row in data.as_array().into_iter().flatten() {
        let d: Document = serde_json::from_value(row.clone()).map_err(Error::internal)?;
        validate(&d)?;
        db.execute("INSERT OR IGNORE INTO reporting_documents(budget_id,id,kind,config,revision,tombstone) VALUES(?1,?2,?3,?4,?5,?6)",params![b,d.id,d.kind,d.config.to_string(),d.revision,d.tombstone])?;
    }
    validate_archive(db, b)
}
pub fn import_clock(db: &Connection, b: &str, row: &Value) -> Result<(), Error> {
    if ["dashboard_pages", "dashboard", "custom_reports"]
        .contains(&row["dataset"].as_str().unwrap_or(""))
    {
        sync::seed_field_version(
            db,
            b,
            "reporting_documents",
            row["row"].as_str().unwrap_or(""),
            row["column"].as_str().unwrap_or(""),
            row["timestamp"].as_str().unwrap_or(""),
        )?;
    }
    Ok(())
}
