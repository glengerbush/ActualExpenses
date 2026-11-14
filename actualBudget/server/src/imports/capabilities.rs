//! Re-evaluate unactivated source plans when a previously gated feature ships.
//! Original bytes and operation receipts remain immutable; live budgets are not
//! re-imported, so user edits never get replaced by historical source rules.
use super::*;

pub(crate) const RULES_VERSION: i64 = 3;

pub(crate) fn refresh_rules(db: &Connection, budget: Option<&str>) -> Result<(), Error> {
    let mut query = db.prepare(
        "SELECT id FROM staged_imports
         WHERE status<>'activated' AND (?1 IS NULL OR budget_id=?1)
         AND COALESCE(json_extract(report,'$.source_format'),'actual')='actual'
         AND COALESCE(json_extract(conversion,'$.rules_version'),0)<?2",
    )?;
    let stages = query
        .query_map(params![budget, RULES_VERSION], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for id in stages {
        // Load one source at a time rather than retaining every uploaded ZIP.
        let (original, hash) = db.query_row(
            "SELECT original,archive_sha256 FROM staged_imports WHERE id=?1",
            [&id],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?)),
        )?;
        let (mut report, conversion) = conversion::inspect(&original)?;
        report["archive_sha256"] = json!(hash);
        let blocked = report["requirements"]
            .as_array()
            .ok_or_else(|| Error::internal("Missing source conversion requirements"))?
            .iter()
            .any(|requirement| requirement["blocking"] == true);
        db.execute(
            "UPDATE staged_imports SET status=?2,report=?3,conversion=?4 WHERE id=?1",
            params![
                id,
                if blocked { "blocked" } else { "ready" },
                report.to_string(),
                conversion.to_string()
            ],
        )?;
    }
    Ok(())
}
