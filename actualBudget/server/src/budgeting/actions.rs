use super::*;
use axum::{
    extract::rejection::JsonRejection,
    http::{HeaderMap, StatusCode},
};
use chrono::Datelike;
use rusqlite::OptionalExtension;
fn required<'a>(body: &'a Value, key: &str) -> Result<&'a str, Error> {
    body[key]
        .as_str()
        .ok_or_else(|| Error::invalid(format!("{key} is required")))
}
fn amount(body: &Value) -> Result<i64, Error> {
    let n = body["amount"]
        .as_i64()
        .ok_or_else(|| Error::invalid("Amount must be an exact integer"))?;
    money(n as i128)
}
fn cell_id(method: &str, m: &str, category: &str) -> String {
    format!("{method}:{m}:{category}")
}
// Every writer runs inside the receipt transaction. Refusing any stale field
// rolls back the complete action, including earlier move/copy destinations.
pub(super) fn guard_field(
    db: &Connection,
    b: &str,
    entity: &str,
    id: &str,
    field: &str,
    clock: &str,
) -> Result<(), Error> {
    let newer: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM field_versions WHERE budget_id=?1 AND entity=?2 AND entity_id=?3 AND field=?4 AND clock>=?5)", params![b,entity,id,field,clock], |r| r.get(0))?;
    if newer {
        return Err(Error::conflict(
            "A newer field value prevents this budget action. Refresh and review the plan.",
        ));
    }
    Ok(())
}
pub(super) fn ensure_cell(
    db: &Connection,
    b: &str,
    kind: &str,
    m: &str,
    c: &str,
) -> Result<String, Error> {
    let id = cell_id(kind, m, c);
    db.execute("INSERT INTO budget_cells(budget_id,id,method,month,category_id) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(budget_id,id) DO UPDATE SET tombstone=0",params![b,id,kind,m,c])?;
    Ok(id)
}
pub(super) fn put(
    db: &Connection,
    b: &str,
    kind: &str,
    m: &str,
    c: &str,
    n: i128,
    clock: &str,
) -> Result<(), Error> {
    let n = money(n)?;
    let id = cell_id(kind, m, c);
    guard_field(db, b, "budget_cells", &id, "amount", clock)?;
    guard_field(db, b, "budget_cells", &id, "tombstone", clock)?;
    ensure_cell(db, b, kind, m, c)?;
    db.execute(
        "UPDATE budget_cells SET amount=?3 WHERE budget_id=?1 AND id=?2",
        params![b, id, n],
    )?;
    sync::seed_field_version(db, b, "budget_cells", &id, "amount", clock)?;
    sync::mark(db, b, "budget_cells", &id)
}
fn ensure_month(db: &Connection, b: &str, m: &str) -> Result<(), Error> {
    db.execute("INSERT INTO budget_months(budget_id,id) VALUES(?1,?2) ON CONFLICT(budget_id,id) DO UPDATE SET tombstone=0",params![b,m])?;
    Ok(())
}
fn put_month(
    db: &Connection,
    b: &str,
    m: &str,
    field: &str,
    value: Value,
    clock: &str,
) -> Result<(), Error> {
    guard_field(db, b, "budget_months", m, field, clock)?;
    guard_field(db, b, "budget_months", m, "tombstone", clock)?;
    ensure_month(db, b, m)?;
    match field {
        "notes" => {
            db.execute(
                "UPDATE budget_months SET notes=?3 WHERE budget_id=?1 AND id=?2",
                params![b, m, value.as_str()],
            )?;
        }
        "buffered" => {
            db.execute(
                "UPDATE budget_months SET buffered=?3 WHERE budget_id=?1 AND id=?2",
                params![b, m, value.as_i64()],
            )?;
        }
        _ => return Err(Error::internal("Unknown month field")),
    };
    sync::seed_field_version(db, b, "budget_months", m, field, clock)?;
    sync::mark(db, b, "budget_months", m)
}
fn find<'a>(data: &'a Value, id: &str) -> Result<&'a Value, Error> {
    data["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .ok_or_else(|| Error::invalid("Choose an active category"))
}
fn selectable<'a>(data: &'a Value, id: &str, tracking: bool) -> Result<&'a Value, Error> {
    crate::models::validate_id(id)?;
    let c = find(data, id)?;
    if !tracking && c["is_income"] == true {
        return Err(Error::invalid("Envelope income is received, not allocated"));
    }
    Ok(c)
}
fn horizon(m: i32) -> i32 {
    let now = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now());
    let end = now.year() * 12 + now.month0() as i32 + 12;
    end.max(m)
}
fn movement_note(
    db: &Connection,
    b: &str,
    m: &str,
    data: &Value,
    route: (&str, &str),
    n: i64,
    clock: &str,
) -> Result<(), Error> {
    let (from, to) = route;
    let label = |id: &str| -> Result<String, Error> {
        Ok(if id == "to-budget" {
            "To Budget".into()
        } else if id == "overbudgeted" {
            "Overbudgeted".into()
        } else {
            find(data, id)?["name"].as_str().unwrap().into()
        })
    };
    let old = data["notes"].as_str().unwrap_or("");
    let separator = if old.is_empty() { "" } else { "\n" };
    let prefs = crate::preferences::get_preferences(db, b)?;
    // Pinned supported currencies use either zero or two fractional digits.
    let digits = if matches!(prefs["currency_code"].as_str(), Some("IRR" | "JPY" | "KRW")) {
        0
    } else {
        2
    };
    let shown = if digits == 0 {
        n.to_string()
    } else {
        format!("{}.{:02}", n / 100, n.abs() % 100)
    };
    let note = format!(
        "{old}{separator}- Reassigned {shown} from {} → {} on {}",
        label(from)?,
        label(to)?,
        chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).date_naive()
    );
    if note.chars().count() > 10000 {
        return Err(Error::invalid("Budget notes exceed limit"));
    }
    put_month(db, b, m, "notes", json!(note), clock)
}
pub(super) async fn mutate(
    State(store): State<Store>,
    Path(b): Path<String>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    crate::models::validate_id(&b)?;
    let Json(body) = input.map_err(crate::api::json_error)?;
    let key = crate::api::idempotency_key(&headers)?;
    let owner = crate::history::owner_header(&headers)?;
    let proposed = sync::header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let cancel = sync::cancel_header(&headers)?;
    let lookup = sync::receipt_only_header(&headers)?;
    let payload = body.to_string();
    store.run(move|db|{let scope=format!("POST /budgets/{b}/budget");if lookup{return sync::receipt_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref());}sync::write_owned(db,&scope,&key,&payload,proposed.as_deref(),StatusCode::OK,owner.as_ref(),|db|{
  if cancel{return sync::canceled(db,&b);}crate::lifecycle::require_epoch(db,&b,epoch.as_deref())?;sync::check_snapshot(db,&b,Some(required(&body,"snapshot")?))?;
  if body.as_object().is_none_or(|o|o.keys().any(|k|!["snapshot","action","month","category","amount","enabled","from","to","method","notes"].contains(&k.as_str()))){return Err(Error::invalid("Unknown budgeting field"));}
  let m=required(&body,"month")?;month(m)?;let clock=sync::clock(db,&b,proposed.as_deref())?;apply(db,&b,&body,&clock)?;
  Ok(json!({"response_status":200,"response_body":{"data":view(db,&b,m,1)?,"edit_clock":clock,"cursor":sync::cursor(db,&b)?}}))
 })}).await
}
pub fn apply(db: &Connection, b: &str, body: &Value, clock: &str) -> Result<(), Error> {
    let m = required(body, "month")?;
    let month_number = month(m)?;
    let before = view(db, b, m, 1)?;
    let data = &before["months"][m];
    let kind = before["method"].as_str().unwrap();
    let tracking = kind == "tracking";
    let action = required(body, "action")?;
    let specific: &[&str] = match action {
        "set" => &["category", "amount"],
        "carryover" => &["category", "enabled"],
        "copy_previous" => &["category"],
        "copy_year" => &["category"],
        "zero" | "reset_hold" => &[],
        "hold" => &["amount"],
        "move" | "cover" => &["from", "to", "amount"],
        "cover_overbudgeted" => &["from", "amount"],
        "notes" => &["notes"],
        "category_notes" => &["category", "notes"],
        "method" => &["method"],
        _ => &[],
    };
    if body.as_object().unwrap().keys().any(|k| {
        !["action", "month", "snapshot"].contains(&k.as_str()) && !specific.contains(&k.as_str())
    }) {
        return Err(Error::invalid(
            "Field does not belong to this budget action",
        ));
    }
    match action {
        "set" => {
            let c = required(body, "category")?;
            selectable(data, c, tracking)?;
            put(db, b, kind, m, c, amount(body)? as i128, clock)?;
        }
        "carryover" => {
            let c = required(body, "category")?;
            crate::models::validate_id(c)?;
            find(data, c)?;
            let flag = body["enabled"]
                .as_bool()
                .ok_or_else(|| Error::invalid("Choose whether to carry over"))?;
            for next in month_number..=horizon(month_number) {
                let next = month_name(next)?;
                let id = cell_id(kind, &next, c);
                guard_field(db, b, "budget_cells", &id, "carryover", clock)?;
                guard_field(db, b, "budget_cells", &id, "tombstone", clock)?;
                ensure_cell(db, b, kind, &next, c)?;
                db.execute(
                    "UPDATE budget_cells SET carryover=?3 WHERE budget_id=?1 AND id=?2",
                    params![b, id, flag],
                )?;
                sync::seed_field_version(db, b, "budget_cells", &id, "carryover", clock)?;
                sync::mark(db, b, "budget_cells", &id)?;
            }
        }
        "copy_previous" => {
            let prev = month_name(month_number - 1)?;
            if let Some(c) = body["category"].as_str() {
                selectable(data, c, tracking)?;
                let previous_view = view(db, b, &prev, 1)?;
                let value = find(&previous_view["months"][prev.as_str()], c)?["budgeted"]
                    .as_i64()
                    .unwrap();
                put(db, b, kind, m, c, value as i128, clock)?;
            } else {
                let mut q=db.prepare("SELECT category_id,amount FROM budget_cells WHERE budget_id=?1 AND method=?2 AND month=?3 AND tombstone=0")?;
                let rows = q
                    .query_map(params![b, kind, prev], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                for (c, n) in rows {
                    if let Ok(cat) = selectable(data, &c, tracking)
                        && cat["hidden"] != true
                        && cat["group_hidden"] != true
                    {
                        put(db, b, kind, m, &c, n as i128, clock)?;
                    }
                }
            }
        }
        "copy_year" => {
            let c = required(body, "category")?;
            let cat = selectable(data, c, tracking)?;
            let n = cat["budgeted"].as_i64().unwrap();
            let end = (month_number / 12 * 12 + 11).min(horizon(month_number));
            for next in month_number + 1..=end {
                put(db, b, kind, &month_name(next)?, c, n as i128, clock)?;
            }
        }
        "zero" => {
            for cat in data["categories"].as_array().unwrap() {
                if tracking || cat["is_income"] != true {
                    put(db, b, kind, m, cat["id"].as_str().unwrap(), 0, clock)?;
                }
            }
        }
        "hold" | "reset_hold" => {
            if tracking {
                return Err(Error::invalid("Income holds belong to envelope budgeting"));
            }
            let manual = data["summary"]["buffered"].as_i64().unwrap();
            let available = data["summary"]["to_budget"].as_i64().unwrap();
            let n = if action == "reset_hold" {
                0
            } else if available > 0 {
                money(
                    manual as i128
                        + (amount(body)? as i128)
                            .max(-(manual as i128))
                            .min(available as i128),
                )?
            } else {
                manual
            };
            put_month(db, b, m, "buffered", json!(n), clock)?;
        }
        "move" | "cover" | "cover_overbudgeted" => {
            if tracking {
                return Err(Error::invalid("Move available money in an envelope budget"));
            }
            let from = required(body, "from")?;
            let to = if action == "cover_overbudgeted" {
                "overbudgeted"
            } else {
                required(body, "to")?
            };
            if from == to {
                return Err(Error::invalid("Choose different source and destination"));
            }
            let source = if from == "to-budget" {
                None
            } else {
                Some(selectable(data, from, false)?)
            };
            let target = if to == "to-budget" || to == "overbudgeted" {
                None
            } else {
                Some(selectable(data, to, false)?)
            };
            let available = data["summary"]["to_budget"].as_i64().unwrap();
            let source_left = source.map_or(available, |c| c["balance"].as_i64().unwrap());
            let n = if action == "move" {
                let n = amount(body)?;
                if n < 0 {
                    return Err(Error::invalid("Move a nonnegative amount"));
                }
                if source.is_none() {
                    n.min(available.max(0))
                } else {
                    n
                }
            } else {
                let deficit = if action == "cover_overbudgeted" {
                    available
                } else {
                    target.ok_or_else(|| Error::invalid("Choose an overspent category"))?["balance"]
                        .as_i64()
                        .unwrap()
                };
                let requested = if body["amount"].is_null() || body["amount"] == 0 {
                    (-deficit).max(0)
                } else {
                    amount(body)?
                };
                if requested < 0 {
                    return Err(Error::invalid("Cover a nonnegative amount"));
                }
                if requested == 0 || source_left <= 0 {
                    0
                } else {
                    requested.min(source_left)
                }
            };
            if n > 0 {
                if let Some(c) = source {
                    put(
                        db,
                        b,
                        kind,
                        m,
                        from,
                        c["budgeted"].as_i64().unwrap() as i128 - n as i128,
                        clock,
                    )?;
                }
                if let Some(c) = target {
                    put(
                        db,
                        b,
                        kind,
                        m,
                        to,
                        c["budgeted"].as_i64().unwrap() as i128 + n as i128,
                        clock,
                    )?;
                }
                movement_note(db, b, m, data, (from, to), n, clock)?;
            }
        }
        "category_notes" => {
            let id = required(body, "category")?;
            crate::models::validate_id(id)?;
            find(data, id)?;
            let notes = required(body, "notes")?;
            if notes.chars().count() > 10000 {
                return Err(Error::invalid("Category notes exceed limit"));
            }
            guard_field(db, b, "categories", id, "notes", clock)?;
            db.execute(
                "UPDATE categories SET notes=?3 WHERE budget_id=?1 AND id=?2",
                params![b, id, notes],
            )?;
            sync::seed_field_version(db, b, "categories", id, "notes", clock)?;
            sync::mark(db, b, "categories", id)?;
        }
        "notes" => {
            let note = required(body, "notes")?;
            if note.chars().count() > 10000 {
                return Err(Error::invalid("Budget notes exceed limit"));
            }
            put_month(db, b, m, "notes", json!(note), clock)?;
        }
        "method" => {
            let method = required(body, "method")?;
            if !["tracking", "envelope"].contains(&method) {
                return Err(Error::invalid("Unknown budgeting method"));
            }
            guard_field(db, b, "budget_settings", "method", "method", clock)?;
            db.execute(
                "UPDATE budget_settings SET method=?2 WHERE budget_id=?1 AND id='method'",
                params![b, method],
            )?;
            sync::seed_field_version(db, b, "budget_settings", "method", "method", clock)?;
            sync::mark(db, b, "budget_settings", "method")?;
        }
        _ => return Err(Error::invalid("Unknown budgeting action")),
    }
    // Validate propagation before the receipt transaction commits. A valid source
    // allocation must not leave an overflowing later carry or copy destination.
    let latest: Option<String> = db.query_row(
        "SELECT MAX(month) FROM budget_cells WHERE budget_id=?1 AND tombstone=0",
        [b],
        |r| r.get(0),
    )?;
    let end = latest
        .as_deref()
        .map(month)
        .transpose()?
        .unwrap_or(month_number)
        .max(month_number);
    view(db, b, &month_name(end)?, 1)?;
    Ok(())
}

/// Category deletion keeps retired plans but removes them from active finance.
/// Pinned transfer adds active-method expense allocations, not carry flags.
pub fn category_deleted(
    db: &Connection,
    b: &str,
    id: &str,
    target: Option<&str>,
    income: bool,
    clock: &str,
) -> Result<(), Error> {
    let newer:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM budget_cells c JOIN field_versions v ON v.budget_id=c.budget_id AND v.entity='budget_cells' AND v.entity_id=c.id WHERE c.budget_id=?1 AND (c.category_id=?2 OR c.category_id=?3) AND v.clock>?4)",params![b,id,target,clock],|r|r.get(0))?;
    if newer {
        return Err(Error::conflict(
            "A newer allocation prevents removing this category. Refresh and review the plan.",
        ));
    }
    let kind: String = db.query_row("SELECT budget_type FROM budgets WHERE id=?1", [b], |r| {
        r.get(0)
    })?;
    let mut q=db.prepare("SELECT month,amount FROM budget_cells WHERE budget_id=?1 AND method=?2 AND category_id=?3 AND tombstone=0")?;
    let rows = q
        .query_map(params![b, kind, id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if !income {
        if let Some(target) = target {
            for (m, n) in rows {
                let old:Option<i64>=db.query_row("SELECT amount FROM budget_cells WHERE budget_id=?1 AND method=?2 AND month=?3 AND category_id=?4 AND tombstone=0",params![b,kind,m,target],|r|r.get(0)).optional()?;
                put(
                    db,
                    b,
                    &kind,
                    &m,
                    target,
                    old.unwrap_or(0) as i128 + n as i128,
                    clock,
                )?;
            }
        } else if rows.iter().any(|(_, n)| *n != 0) {
            return Err(Error::invalid(
                "Choose a replacement for allocated category money",
            ));
        }
    }
    let mut q = db.prepare(
        "SELECT id FROM budget_cells WHERE budget_id=?1 AND category_id=?2 AND tombstone=0",
    )?;
    let ids = q
        .query_map(params![b, id], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in ids {
        db.execute(
            "UPDATE budget_cells SET tombstone=1 WHERE budget_id=?1 AND id=?2",
            params![b, id],
        )?;
        sync::seed_field_version(db, b, "budget_cells", &id, "tombstone", clock)?;
        sync::mark(db, b, "budget_cells", &id)?;
    }
    Ok(())
}
