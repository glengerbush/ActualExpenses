//! Exact source-record contributions; routing constraints never become purchases.
use super::*;
use graph::{HIDDEN, Source, exact};
use std::collections::BTreeMap;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Link {
    source: String,
    target: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Detail {
    query: Request,
    snapshot: String,
    link: Link,
    #[serde(default)]
    offset: usize,
    #[serde(default = "limit")]
    limit: usize,
}
fn limit() -> usize {
    50
}
struct Row {
    value: Value,
    date: String,
    id: String,
}
fn push(
    rows: &mut Vec<Row>,
    id: String,
    date: String,
    basis: &str,
    amount: i128,
    original: i64,
    label: &str,
) -> Result<usize, Error> {
    if rows.len() >= 100000 {
        return Err(Error::invalid(
            "This flow exceeds100000 contribution rows; narrow dates or filters before inspecting it",
        ));
    }
    let index = rows.len();
    rows.push(Row{value:json!({"id":id,"basis":basis,"amount":exact(amount)?,"original_amount":original,"label_key":label}),date,id});
    Ok(index)
}
fn entry_rows(
    db: &Connection,
    b: &str,
    options: &Options,
    base: &data::Base,
    sources: &BTreeMap<Source, i64>,
    rows: &mut Vec<Row>,
) -> Result<(), Error> {
    let mut grouped = BTreeMap::<(String, String, Option<String>), i128>::new();
    let mut received = BTreeMap::<String, i128>::new();
    let mut planned = BTreeMap::<String, i128>::new();
    for (source, factor) in sources {
        if let Source::Entry(index) = source {
            let entry = base
                .entries
                .get(*index)
                .ok_or_else(|| Error::internal("Missing source aggregate"))?;
            if base.context.is_some() {
                let map = if entry.income {
                    &mut received
                } else {
                    &mut planned
                };
                *map.entry(entry.category.clone()).or_default() += *factor as i128;
            } else {
                let key = (
                    entry.category.clone(),
                    entry.account.clone(),
                    if entry.income {
                        Some(entry.payee.clone())
                    } else {
                        None
                    },
                );
                *grouped.entry(key).or_default() +=
                    *factor as i128 * if entry.negative { -1 } else { 1 };
            }
        }
    }
    if !grouped.is_empty() || !received.is_empty() {
        let mut actual_options = options.clone();
        if base.context.is_some() {
            actual_options.conditions.clear();
            actual_options.conditions_op = "and".into();
        }
        let (mut values, mut where_clause) = data::clauses(db, b, &actual_options)?;
        let categories = grouped
            .keys()
            .map(|key| key.0.clone())
            .chain(received.keys().cloned())
            .collect::<std::collections::BTreeSet<_>>();
        let slots = categories
            .into_iter()
            .map(|id| {
                values.push(SqlValue::Text(id));
                format!("?{}", values.len())
            })
            .collect::<Vec<_>>()
            .join(",");
        where_clause.push_str(&format!(" AND c.id IN ({slots})"));
        if base.context.is_some() {
            where_clause.push_str(" AND a.off_budget=0");
        }
        data::input_bound(db, &values, &where_clause)?;
        let mut stmt=db.prepare(&format!("SELECT t.id,t.date,t.amount,t.account_id,c.id,p.id {JOINS} WHERE {where_clause} ORDER BY t.date,t.id LIMIT 100001"))?;
        let mut input = stmt.query(params_from_iter(values))?;
        while let Some(row) = input.next()? {
            let Some(category) = row.get::<_, Option<String>>(4)? else {
                continue;
            };
            let account: String = row.get(3)?;
            let payee = row.get::<_, Option<String>>(5)?.unwrap_or_default();
            let factor = if base.context.is_some() {
                received.get(&category).copied().unwrap_or(0)
            } else {
                grouped
                    .get(&(category.clone(), account.clone(), Some(payee)))
                    .copied()
                    .unwrap_or(0)
                    + grouped
                        .get(&(category.clone(), account.clone(), None))
                        .copied()
                        .unwrap_or(0)
            };
            if factor == 0 {
                continue;
            }
            let amount: i64 = row.get(2)?;
            let id: String = row.get(0)?;
            let date: String = row.get(1)?;
            let index = push(
                rows,
                id,
                date.clone(),
                "recorded_entry",
                amount as i128 * factor,
                amount,
                "Recorded entry",
            )?;
            rows[index].value["date"] = json!(date);
            rows[index].value["account_id"] = json!(account);
            rows[index].value["category_id"] = json!(category);
        }
    }
    if !planned.is_empty() {
        let method: String =
            db.query_row("SELECT budget_type FROM budgets WHERE id=?1", [b], |r| {
                r.get(0)
            })?;
        let values = [b, &method, &options.start_date[..7], &options.end_date[..7]];
        let count:i64=db.query_row("SELECT count(*) FROM (SELECT id FROM budget_cells WHERE budget_id=?1 AND method=?2 AND tombstone=0 AND month>=?3 AND month<=?4 AND amount<>0 LIMIT 100001)",values,|r|r.get(0))?;
        if count > 100000 {
            return Err(Error::invalid("Sankey planned detail exceeds100000 inputs"));
        }
        let mut stmt=db.prepare("SELECT p.id,p.month,p.category_id,c.name,p.amount FROM budget_cells p JOIN categories c ON c.budget_id=p.budget_id AND c.id=p.category_id WHERE p.budget_id=?1 AND p.method=?2 AND p.tombstone=0 AND p.month>=?3 AND p.month<=?4 AND p.amount<>0 ORDER BY p.month,p.id LIMIT 100001")?;
        let mut input = stmt.query(values)?;
        while let Some(row) = input.next()? {
            let category: String = row.get(2)?;
            let factor = planned.get(&category).copied().unwrap_or(0);
            if factor == 0 {
                continue;
            }
            let amount: i64 = row.get(4)?;
            let id: String = row.get(0)?;
            let month: String = row.get(1)?;
            let index = push(
                rows,
                id,
                month.clone() + "-01",
                "budget_allocation",
                amount as i128 * factor,
                amount,
                "Planned allocation",
            )?;
            rows[index].value["month"] = json!(month);
            rows[index].value["category_id"] = json!(category);
            rows[index].value["category_name"] = json!(row.get::<_, String>(3)?);
        }
    }
    Ok(())
}
fn transfer_rows(
    db: &Connection,
    b: &str,
    options: &Options,
    base: &data::Base,
    sources: &BTreeMap<Source, i64>,
    rows: &mut Vec<Row>,
) -> Result<(), Error> {
    let mut selected = BTreeMap::<(String, String), (String, i128)>::new();
    for (source, factor) in sources {
        if let Source::Transfer(index) = source {
            let pair = base
                .transfers
                .get(*index)
                .ok_or_else(|| Error::internal("Missing transfer aggregate"))?;
            let key = if pair.from < pair.to {
                (pair.from.clone(), pair.to.clone())
            } else {
                (pair.to.clone(), pair.from.clone())
            };
            selected
                .entry(key)
                .and_modify(|(_, n)| *n += *factor as i128)
                .or_insert((pair.from.clone(), *factor as i128));
        }
    }
    if selected.is_empty() {
        return Ok(());
    }
    let (values, where_clause) = data::clauses(db, b, options)?;
    let input = data::transfer_rows(db, &values, &where_clause)?;
    let by_id = input
        .iter()
        .map(|row| (row.id.clone(), row))
        .collect::<BTreeMap<_, _>>();
    for from in &input {
        let Some(to) = by_id.get(&from.peer) else {
            continue;
        };
        if from.account > to.account {
            continue;
        }
        let Some((net_from, factor)) = selected.get(&(from.account.clone(), to.account.clone()))
        else {
            continue;
        };
        if *factor == 0 {
            continue;
        }
        let amount = from.amount as i128 * if net_from == &from.account { -1 } else { 1 } * factor;
        let index = push(
            rows,
            from.id.clone(),
            from.date.clone(),
            "net_transfer",
            amount,
            from.amount,
            "Linked transfer",
        )?;
        rows[index].value["date"] = json!(from.date);
        rows[index].value["account_id"] = json!(from.account);
        rows[index].value["account_name"] = json!(from.name);
        rows[index].value["peer_id"] = json!(to.id);
        rows[index].value["peer_account_id"] = json!(to.account);
        rows[index].value["peer_account_name"] = json!(to.name);
    }
    Ok(())
}
fn context_rows(
    base: &data::Base,
    sources: &BTreeMap<Source, i64>,
    rows: &mut Vec<Row>,
) -> Result<(), Error> {
    for (source, factor) in sources {
        if let Source::Context(kind) = source {
            let context = base
                .context
                .as_ref()
                .ok_or_else(|| Error::internal("Missing budget context"))?;
            let (amount, month, label) = match kind.as_str() {
                "to_budget" => (
                    context.to_budget,
                    &context.end,
                    if context.to_budget > 0 {
                        "To budget"
                    } else {
                        "Overbudgeted"
                    },
                ),
                "previous" => (
                    context.previous,
                    &context.start,
                    "Funds from previous month",
                ),
                "next" => (context.next, &context.end, "Funds held forward"),
                "overspent" => (context.overspent, &context.end, "Overspending adjustment"),
                _ => return Err(Error::internal("Unknown budget context")),
            };
            if *factor == 0 {
                continue;
            }
            let index = push(
                rows,
                format!("context:{kind}:{month}"),
                month.clone() + "-01",
                "budget_context",
                amount as i128 * *factor as i128,
                amount,
                label,
            )?;
            rows[index].value["month"] = json!(month);
        }
    }
    Ok(())
}
/// Enrich only the requested page; large notes never enter the aggregate cache
/// or a complete-history vector while counting/summing contribution records.
fn enrich(db: &Connection, b: &str, rows: &mut [Row]) -> Result<(), Error> {
    let ids = rows
        .iter()
        .filter(|row| {
            matches!(
                row.value["basis"].as_str(),
                Some("recorded_entry" | "net_transfer")
            )
        })
        .map(|row| row.id.clone())
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(());
    }
    let mut values = vec![SqlValue::Text(b.into())];
    let slots = ids
        .iter()
        .map(|id| {
            values.push(SqlValue::Text(id.clone()));
            format!("?{}", values.len())
        })
        .collect::<Vec<_>>()
        .join(",");
    let mut stmt=db.prepare(&format!("SELECT t.id,a.name,c.name,coalesce(y.name,p.name,''),t.notes,t.parent_id,t.transfer_id {JOINS} WHERE t.budget_id=?1 AND t.id IN ({slots})"))?;
    let mut input = stmt.query(params_from_iter(values))?;
    while let Some(row) = input.next()? {
        let id: String = row.get(0)?;
        for record in rows.iter_mut().filter(|record| record.id == id) {
            record.value["account_name"] = json!(row.get::<_, String>(1)?);
            record.value["category_name"] = json!(row.get::<_, Option<String>>(2)?);
            record.value["payee_name"] = json!(row.get::<_, String>(3)?);
            record.value["notes"] = json!(row.get::<_, String>(4)?);
            record.value["parent_id"] = json!(row.get::<_, Option<String>>(5)?);
            record.value["transfer_id"] = json!(row.get::<_, Option<String>>(6)?);
        }
    }
    Ok(())
}
fn detail(
    db: &Connection,
    b: &str,
    input: &Detail,
    evaluation: &Evaluation,
) -> Result<Value, Error> {
    let key = (input.link.source.clone(), input.link.target.clone());
    if key.0.ends_with(HIDDEN) || key.1.ends_with(HIDDEN) {
        return Err(Error::invalid(
            "Hidden routing links are not monetary records",
        ));
    }
    let (expected, members) = if let Some(amount) = evaluation.projection.graph.edge(&key.0, &key.1)
    {
        (
            amount,
            evaluation
                .projection
                .graph
                .members
                .get(&key)
                .cloned()
                .unwrap_or_default(),
        )
    } else if let Some(amount) = evaluation.graph.edge(&key.0, &key.1) {
        (amount, vec![key.clone()])
    } else {
        return Err(Error::invalid(
            "Unknown flow; select a current base or grouped link",
        ));
    };
    let mut sources = BTreeMap::<Source, i64>::new();
    for pair in members {
        for annotation in evaluation
            .graph
            .annotations
            .get(&pair)
            .into_iter()
            .flatten()
        {
            *sources.entry(annotation.source.clone()).or_default() += annotation.factor;
        }
    }
    let mut rows = vec![];
    entry_rows(
        db,
        b,
        &input.query.options,
        &evaluation.base,
        &sources,
        &mut rows,
    )?;
    transfer_rows(
        db,
        b,
        &input.query.options,
        &evaluation.base,
        &sources,
        &mut rows,
    )?;
    context_rows(&evaluation.base, &sources, &mut rows)?;
    let total = rows
        .iter()
        .map(|row| row.value["amount"].as_i64().unwrap() as i128)
        .sum::<i128>();
    if total != expected {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "flow_not_reconciled",
            "This flow cannot be reconciled against its source records. Recalculate and review the account register or budget plan.",
        ));
    }
    rows.sort_by(|a, b| {
        a.date
            .cmp(&b.date)
            .then(a.id.cmp(&b.id))
            .then(a.value["basis"].as_str().cmp(&b.value["basis"].as_str()))
    });
    let count = rows.len();
    let kinds = rows
        .iter()
        .filter_map(|row| row.value["basis"].as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let basis = if kinds.len() == 1 {
        kinds.first().copied().unwrap_or("mixed").to_owned()
    } else {
        "mixed".into()
    };
    let mut page = rows
        .into_iter()
        .skip(input.offset)
        .take(input.limit)
        .collect::<Vec<_>>();
    enrich(db, b, &mut page)?;
    let value = json!({"rows":page.into_iter().map(|row|row.value).collect::<Vec<_>>(),"total_count":count,"total_amount":exact(total)?,"offset":input.offset,"limit":input.limit,"has_more":input.offset.saturating_add(input.limit)<count,"snapshot":sync::cursor(db,b)?,"link":{"source":key.0,"target":key.1},"basis":basis,"assumptions":["Amounts are signed contributions to this flow. Refunds and opposing transfers can reduce its total; original recorded or planned amounts remain separate. Budget context rows are derived and are not purchases."]});
    if value.to_string().len() > 8 * 1024 * 1024 {
        return Err(Error::invalid(
            "Sankey detail exceeds8MiB; request fewer rows",
        ));
    }
    Ok(value)
}
pub(super) async fn read(
    State(store): State<Store>,
    Path(b): Path<String>,
    Extension(cache): Extension<Cache>,
    input: Result<Json<Detail>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    let Json(input) = input.map_err(crate::api::json_error)?;
    validate(&input.query)?;
    if input.snapshot.len() > 256
        || serde_json::to_vec(&input).map_err(Error::internal)?.len() > 65536
    {
        return Err(Error::invalid("Sankey detail query exceeds64KiB"));
    }
    if input.limit == 0
        || input.limit > 200
        || input.offset > 100000
        || input.link.source.len() > 512
        || input.link.target.len() > 512
    {
        return Err(Error::invalid(
            "Choose a flow, offset up to100000 and limit1–200",
        ));
    }
    let value = store
        .run(move |db| {
            let tx = db.transaction()?;
            super::super::sql_work::run(&tx, || {
                crate::register::with_report_predicates(&tx, || {
                    sync::check_snapshot(&tx, &b, Some(&input.snapshot))?;
                    let evaluation = evaluate(&tx, &b, &input.query, &cache)?;
                    detail(&tx, &b, &input, &evaluation)
                })
            })
        })
        .await?;
    Ok(Json(json!({"data":value})))
}
