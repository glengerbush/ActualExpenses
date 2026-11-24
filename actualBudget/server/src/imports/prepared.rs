//! Retained original bytes are authoritative; callers never repair saved conversions.
use super::*;

pub(super) enum Source<'a> {
    Actual,
    Ynab { kind: &'a str, filename: &'a str },
}
impl Source<'_> {
    fn inspect(&self, bytes: &[u8]) -> Result<(Value, Value), Error> {
        match self {
            Self::Actual => conversion::inspect(bytes),
            Self::Ynab { kind, filename } => ynab::inspect(kind, filename, bytes),
        }
    }
    fn matches(&self, report: &Value) -> bool {
        match self {
            Self::Actual => !matches!(report["source_format"].as_str(), Some("ynab4" | "nynab")),
            Self::Ynab { kind, .. } => report["source_format"] == *kind,
        }
    }
}
fn source(report: &Value) -> Result<Source<'_>, Error> {
    Ok(match report["source_format"].as_str() {
        Some(kind @ ("ynab4" | "nynab")) => Source::Ynab {
            kind,
            filename: report["source"]["filename"]
                .as_str()
                .ok_or_else(|| Error::invalid("Original filename is missing"))?,
        },
        _ => Source::Actual,
    })
}
fn status(report: &Value) -> &'static str {
    if report["requirements"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|r| r["blocking"] == true))
    {
        "blocked"
    } else {
        "ready"
    }
}
// Both supported patches write this complete shape. Older retained stages need
// explicit re-review, never a mixture of cached and freshly converted domains.
fn current(plan: &Value) -> bool {
    [
        "account_groups",
        "accounts",
        "category_groups",
        "categories",
        "payees",
        "transactions",
    ]
    .iter()
    .all(|name| plan[*name].is_array())
        && plan.get("register_settings").is_some()
        && [
            "notes_version",
            "transfer_version",
            "budgeting_version",
            "schedules_version",
            "transaction_imports_version",
            "automations_version",
        ]
        .iter()
        .all(|name| plan[*name] == 1)
        && plan["rules_version"] == capabilities::RULES_VERSION
}
pub(super) fn stage(
    db: &Connection,
    source: Source<'_>,
    bytes: &[u8],
    hash: &str,
) -> Result<Value, Error> {
    if let Some(id) = db
        .query_row(
            "SELECT id FROM staged_imports WHERE archive_sha256=?1",
            [hash],
            |r| r.get::<_, String>(0),
        )
        .optional()?
    {
        let existing = read_import(db, &id)?;
        if !source.matches(&existing["report"]) {
            return Err(Error::conflict(
                "These original bytes were already staged using another source format. Review the existing import.",
            ));
        }
        if existing["status"] != "activated" {
            let plan: String = db.query_row(
                "SELECT conversion FROM staged_imports WHERE id=?1",
                [&id],
                |r| r.get(0),
            )?;
            let plan: Value = serde_json::from_str(&plan).map_err(Error::internal)?;
            if !current(&plan) {
                // store::write has already replayed any original receipt. Only a
                // new upload intent may replace a stale review, as a whole.
                let (mut report, conversion) = source.inspect(bytes)?;
                report["archive_sha256"] = json!(hash);
                db.execute(
                    "UPDATE staged_imports SET status=?2,report=?3,conversion=?4 WHERE id=?1",
                    params![
                        id,
                        status(&report),
                        report.to_string(),
                        conversion.to_string()
                    ],
                )?;
            }
        }
        return read_import(db, &id);
    }
    let (mut report, conversion) = source.inspect(bytes)?;
    report["archive_sha256"] = json!(hash);
    let id = Uuid::new_v4().to_string();
    let budget = db
        .query_row("SELECT id FROM budgets WHERE deleted=0", [], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    db.execute("INSERT INTO staged_imports(id,archive_sha256,original,budget_id,status,report,conversion) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![id,hash,bytes,budget,status(&report),report.to_string(),conversion.to_string()])?;
    read_import(db, &id)
}

pub(super) fn load(db: &Connection, stage: &Value) -> Result<Value, Error> {
    if stage["status"] == "blocked" {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "import_requires_conversion",
            "Import contains unsupported data; review its conversion requirements",
        ));
    }
    let (bytes, saved): (Vec<u8>, String) = db.query_row(
        "SELECT original,conversion FROM staged_imports WHERE id=?1",
        [stage["id"].as_str().ok_or_else(Error::missing)?],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let saved: Value = serde_json::from_str(&saved).map_err(Error::internal)?;
    if stage["status"] != "activated" && !current(&saved) {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "import_requires_review",
            "This saved import review is no longer supported. Upload the original file again to review it; the original bytes have been retained.",
        ));
    }
    // Revalidate the source for activation and merge alike. Activated stages stay
    // immutable: merging their original file is a new import, not a stage upgrade.
    let (report, plan) = source(&stage["report"])?.inspect(&bytes)?;
    if status(&report) == "blocked" {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "import_requires_conversion",
            "Import contains unsupported data; review its conversion requirements",
        ));
    }
    Ok(plan)
}
