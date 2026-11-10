//! Definition-only source migration. Preserve executable semantics and targets;
//! never accept a lossy renderer merely because today's balance hides the loss.
use super::*;
use rusqlite::{OptionalExtension, params};
fn pools(db: &Connection, b: &str) -> Result<Vec<Value>, Error> {
    let mut q = db.prepare(
        "SELECT id,name,tombstone FROM budget_cleanup_groups WHERE budget_id=?1 ORDER BY rowid",
    )?;
    q.query_map([b],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"deleted":r.get::<_,bool>(2)?})))?.collect::<Result<Vec<_>,_>>().map_err(Error::from)
}
fn schedule(db: &Connection, b: &str, t: &Value) -> Result<(String, String), Error> {
    let id = t["scheduleId"].as_str();
    db.query_row("SELECT id,name FROM schedules WHERE budget_id=?1 AND tombstone=0 AND ((?2 IS NOT NULL AND id=?2) OR (?2 IS NULL AND trim(name)=trim(?3))) ORDER BY rowid LIMIT 1",params![b,id,t["name"].as_str()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or_else(||Error::invalid("The schedule reference cannot be migrated until it is repaired"))
}
fn income(db: &Connection, b: &str, raw: &str) -> Result<String, Error> {
    let lower = raw.to_lowercase();
    if matches!(lower.as_str(), "all income" | "available funds") {
        return Ok(lower);
    }
    let mut q=db.prepare("SELECT c.id,c.name FROM categories c LEFT JOIN category_groups g ON g.budget_id=c.budget_id AND g.id=c.group_id WHERE c.budget_id=?1 AND c.tombstone=0 AND c.is_income=1 ORDER BY COALESCE(g.sort_order,0),COALESCE(c.sort_order,0),c.id")?;
    for r in q.query_map([b], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (id, name) = r?;
        if id == raw || name.to_lowercase() == lower {
            return Ok(id);
        }
    }
    Err(Error::invalid(
        "An income reference is unavailable; repair it before migration",
    ))
}
fn canonical_number(v: &Value) -> Value {
    match v {
        Value::Array(a) => json!(a.iter().map(canonical_number).collect::<Vec<_>>()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), canonical_number(v)))
                .collect(),
        ),
        Value::Number(n) => {
            let n = n.as_f64().unwrap();
            if n.fract() == 0.0 && n.abs() <= crate::models::MAX_MONEY as f64 {
                json!(n as i64)
            } else {
                v.clone()
            }
        }
        _ => v.clone(),
    }
}
fn canonical(db: &Connection, b: &str, input: &[Value]) -> Result<Value, Error> {
    validation::templates(input)?;
    let mut cap = Value::Null;
    let mut regular = vec![];
    let mut goals = vec![];
    let mut remainder = vec![];
    let mut has_regular = false;
    for original in input {
        let mut t = original.clone();
        let kind = t["type"].as_str().unwrap().to_owned();
        let limit = if kind == "limit" {
            t.clone()
        } else if matches!(kind.as_str(), "simple" | "periodic" | "remainder") {
            t["limit"].clone()
        } else {
            Value::Null
        };
        if !limit.is_null() {
            if !cap.is_null() {
                return Err(Error::invalid(
                    "Multiple limits need review before migration",
                ));
            }
            cap = json!({"amount":limit["amount"],"period":limit["period"],"hold":limit["hold"]==true,"start":if limit["period"]=="weekly"{limit["start"].clone()}else{Value::Null}});
        }
        let o = t.as_object_mut().unwrap();
        o.remove("description");
        o.remove("limit");
        if kind == "limit" {
            continue;
        }
        if kind == "goal" {
            o.remove("priority");
            goals.push(t);
            continue;
        }
        if kind == "remainder" {
            o.remove("priority");
            remainder.push(t);
            continue;
        }
        has_regular = true;
        if kind == "simple" {
            if t["monthly"].is_null() {
                t = json!({"type":"refill","directive":"template","priority":t["priority"]});
            } else if t["monthly"].as_f64() == Some(0.0) {
                continue;
            }
        }
        if kind == "schedule" {
            let (id, _) = schedule(db, b, &t)?;
            t["scheduleId"] = json!(id);
            t["full"] = json!(t["full"] == true);
            t.as_object_mut().unwrap().remove("name");
        }
        if kind == "percentage" {
            t["category"] = json!(income(db, b, t["category"].as_str().unwrap())?);
        }
        for key in ["annual", "repeat", "from", "adjustment", "adjustmentType"] {
            if t[key].is_null() {
                t.as_object_mut().unwrap().remove(key);
            }
        }
        if matches!(kind.as_str(), "by" | "spend") {
            let period = if t["annual"] == true {
                Some(t["repeat"].as_i64().unwrap_or(1) * 12)
            } else {
                t["repeat"].as_i64()
            };
            t.as_object_mut().unwrap().remove("annual");
            t.as_object_mut().unwrap().remove("repeat");
            t["repeat_period"] = json!(period);
        }
        regular.push(t);
    }
    Ok(canonical_number(
        &json!({"cap":cap,"regular":regular,"has_regular":has_regular,"goals":goals,"remainder":remainder}),
    ))
}
fn to_ui(db: &Connection, b: &str, templates: &[Value]) -> Result<Vec<Value>, Error> {
    let mut out = vec![];
    for t in templates {
        let kind = t["type"].as_str().unwrap_or("");
        if kind == "schedule" {
            let (id, name) = schedule(db, b, t)?;
            let mut t = t.clone();
            t["scheduleId"] = json!(id);
            t["name"] = json!(name);
            out.push(t);
            continue;
        }
        if kind == "simple" {
            let monthly = t["monthly"].as_f64();
            if !t["limit"].is_null() {
                let mut limit = t["limit"].clone();
                limit["type"] = json!("limit");
                limit["directive"] = json!("template");
                limit["priority"] = Value::Null;
                if monthly.is_none_or(|n| n == 0.0) && !t["description"].is_null() {
                    limit["description"] = t["description"].clone();
                }
                out.push(limit);
                if monthly.is_none() {
                    out.push(
                        json!({"type":"refill","directive":"template","priority":t["priority"]}),
                    );
                }
            }
            // Keep a monthly directive monthly. Minting a new starting date would change
            // historical-month behavior; zero lines also carry a meaningful goal indicator.
            if monthly.is_some() {
                let mut copy = t.clone();
                copy.as_object_mut().unwrap().remove("limit");
                out.push(copy);
            }
            continue;
        }
        if matches!(kind, "periodic" | "remainder") && !t["limit"].is_null() {
            let mut copy = t.clone();
            copy.as_object_mut().unwrap().remove("limit");
            out.push(copy);
            let mut limit = t["limit"].clone();
            limit["type"] = json!("limit");
            limit["directive"] = json!("template");
            limit["priority"] = Value::Null;
            out.push(limit);
            continue;
        }
        let mut t = t.clone();
        if kind == "percentage" {
            t["category"] = json!(income(db, b, t["category"].as_str().unwrap_or(""))?);
        }
        out.push(t);
    }
    Ok(out)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Preview {
    month: String,
    snapshot: String,
    category_id: String,
    target_source: String,
    notes: Option<String>,
}
pub(crate) async fn preview(
    State(store): State<Store>,
    Path(b): Path<String>,
    input: Result<Json<Preview>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>, Error> {
    let Json(body) = input.map_err(crate::api::json_error)?;
    crate::models::validate_id(&b)?;
    let body = json!({"month":body.month,"snapshot":body.snapshot,"category_id":body.category_id,"target_source":body.target_source,"notes":body.notes});
    store
        .run(move |db| {
            let tx = db.transaction()?;
            crate::concurrency::check_snapshot(&tx, &b, body["snapshot"].as_str())?;
            Ok(json!({"data":plan(&tx,&b,&body)?}))
        })
        .await
        .map(Json)
}
pub(super) fn plan(db: &Connection, b: &str, body: &Value) -> Result<Value, Error> {
    let month = body["month"]
        .as_str()
        .ok_or_else(|| Error::invalid("month is required"))?;
    super::super::month(month)?;
    let id = body["category_id"]
        .as_str()
        .ok_or_else(|| Error::invalid("category_id is required"))?;
    crate::models::validate_id(id)?;
    let target = body["target_source"]
        .as_str()
        .filter(|s| matches!(*s, "notes" | "ui"))
        .ok_or_else(|| Error::invalid("Choose notes or ui migration target"))?;
    let old = configuration::category(db, b, id)?;
    let original = old["templates"].as_array().unwrap().clone();
    let mut pools = pools(db, b)?;
    let original_cleanup = if old["source"] == "ui" {
        old["cleanup"].as_array().unwrap().clone()
    } else {
        cleanup::notes(b, old["notes"].as_str().unwrap_or(""), &mut pools)?
    };
    let mut issues = vec![];
    let rendered = match render::templates(db, b, &original).and_then(|text| {
        let cleanup = render::cleanup(&original_cleanup, &pools)?;
        Ok([text, cleanup]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n"))
    }) {
        Ok(s) => s,
        Err(e) => {
            issues.push(e.message);
            String::new()
        }
    };
    let notes = if target == "notes" {
        body["notes"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| render::merge(old["notes"].as_str().unwrap_or(""), &rendered))
    } else {
        if !body["notes"].is_null() {
            return Err(Error::invalid(
                "Notes text can only be reviewed when migrating to notes",
            ));
        }
        old["notes"].as_str().unwrap_or("").into()
    };
    if notes.chars().count() > 10000 {
        return Err(Error::invalid("Proposed notes exceed10000 characters"));
    }
    let parsed = parse::notes(&notes)?;
    if target == "notes" && !parsed["errors"].as_array().unwrap().is_empty() {
        issues.push(
            "Proposed notes contain invalid template directives. Repair them before migration."
                .into(),
        );
    }
    let templates = if target == "ui" {
        match to_ui(db, b, &original) {
            Ok(t) => t,
            Err(e) => {
                issues.push(e.message);
                original.clone()
            }
        }
    } else {
        parsed["templates"].as_array().unwrap().clone()
    };
    let cleanup = if target == "ui" {
        original_cleanup.clone()
    } else {
        cleanup::notes(b, &notes, &mut pools)?
    };
    match(canonical(db,b,&original),canonical(db,b,&templates)){(Ok(a),Ok(c))if a==c=>{},(Ok(_),Ok(_))=>issues.push("This representation changes an allocation rule or goal indicator. Keep the current source, or edit the automation as a separate change.".into()),(Err(e),_)|(_,Err(e))=>issues.push(e.message)}
    if canonical_number(&json!(original_cleanup)) != canonical_number(&json!(cleanup)) {
        issues.push(
            "A cleanup role, weight or pool would change. Repair its reference before migrating."
                .into(),
        );
    }
    let before_plan = engine::compute(db, b, &json!({"month":month,"force":true}))?;
    let mut overrides = json!({});
    overrides[id] = json!(templates);
    let after_plan = engine::compute(
        db,
        b,
        &json!({"month":month,"force":true,"template_overrides":overrides}),
    )?;
    for value in [&before_plan, &after_plan] {
        for error in value["errors"].as_array().unwrap() {
            issues.push(format!(
                "{}: {}",
                error["name"].as_str().unwrap_or("Category"),
                error["error"].as_str().unwrap_or("Cannot verify this plan")
            ));
        }
    }
    let mut before = Value::Null;
    let mut after = Value::Null;
    for (a, c) in before_plan["categories"]
        .as_array()
        .unwrap()
        .iter()
        .zip(after_plan["categories"].as_array().unwrap())
    {
        let pick = |r: &Value| json!({"budgeted":r["budgeted"],"goal":r["goal"],"long_goal":r["long_goal"]});
        if pick(a) != pick(c) {
            issues.push(format!(
                "{} would have different allocations or goal indicators.",
                a["name"].as_str().unwrap_or("A category")
            ));
        }
        if a["id"] == id {
            before = pick(a);
            after = pick(c);
        }
    }
    issues.sort();
    issues.dedup();
    Ok(
        json!({"category_id":id,"category_name":old["name"],"current_source":old["source"],"target_source":target,"old_notes":old["notes"],"notes":notes,"rendered_directives":rendered,"templates":templates,"cleanup":cleanup,"pools":pools.iter().filter(|p|p["deleted"]!=true).map(|p|json!({"id":p["id"],"name":p["name"]})).collect::<Vec<_>>(),"compatible":issues.is_empty(),"issues":issues,"before":before,"after":after,"snapshot":before_plan["snapshot"],"epoch":before_plan["epoch"],"method":before_plan["method"]}),
    )
}
pub(super) fn apply(db: &Connection, b: &str, body: &Value, clock: &str) -> Result<Value, Error> {
    let plan = plan(db, b, body)?;
    if plan["compatible"] != true {
        return Err(Error::invalid(format!(
            "Migration would change saved behavior: {}",
            plan["issues"]
        )));
    }
    let id = plan["category_id"].as_str().unwrap();
    let mut pools = plan["pools"].as_array().unwrap().clone();
    for pool in &mut pools {
        pool["deleted"] = json!(false);
    }
    cleanup::persist_definitions(
        db,
        b,
        &json!({"cleanup_definitions":[],"pools":pools}),
        clock,
    )?;
    if plan["target_source"] == "notes" {
        super::super::actions::apply(
            db,
            b,
            &json!({"action":"category_notes","month":body["month"],"category":id,"notes":plan["notes"]}),
            clock,
        )?;
    }
    let config = configuration::configure(
        db,
        b,
        &json!({"category_id":id,"source":plan["target_source"],"templates":plan["templates"],"cleanup":plan["cleanup"]}),
        clock,
    )?;
    Ok(
        json!({"category":config["category"],"snapshot":config["snapshot"],"epoch":config["epoch"],"migration":plan}),
    )
}
