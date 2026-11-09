//! A split is one financial aggregate. Its children carry allocations, never a
//! second account balance. Replacement and its receipt share one transaction.
use crate::{
    api, concurrency,
    error::Error,
    ledger,
    models::{MAX_MONEY, validate_id},
    store::Store,
};
use axum::{
    Json, Router,
    extract::{
        DefaultBodyLimit, Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::{HeaderMap, StatusCode},
    routing::post,
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Child {
    pub id: String,
    pub amount: i64,
    #[serde(default)]
    pub payee_id: Option<String>,
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub notes: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Changes {
    #[serde(default)]
    pub upsert: Vec<Child>,
    #[serde(default)]
    pub remove: Vec<String>,
}
#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Allocation {
    #[default]
    Exact,
    Even,
    Proportional,
    Auto,
}
#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    #[default]
    Replace,
    Recombine,
    Unsplit,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub snapshot: String,
    #[serde(default)]
    pub action: Action,
    #[serde(default)]
    pub children: Vec<Child>,
    #[serde(default)]
    pub allocation: Allocation,
    #[serde(default)]
    pub detach: Vec<String>,
    #[serde(default)]
    pub fields: Option<Value>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_reconciled: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub reconciled_versions: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconciliation_token: Option<String>,
    // Omitting this new field preserves normalized legacy receipt payloads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<Changes>,
}
// The Actual importer accepts 100,000 rows per table. Keep allocations editable
// at that scale; sparse edits avoid retransmitting unchanged notes/references.
const MAX_CHILDREN: usize = 100_000;
const MAX_BODY: usize = 64 * 1024 * 1024;

pub fn router() -> Router<Store> {
    Router::new().route(
        "/api/v1/budgets/{budget}/transactions/{id}/splits",
        post(change)
            .get(view)
            .layer(DefaultBodyLimit::max(MAX_BODY)),
    )
}
async fn view(
    State(store): State<Store>,
    Path((budget, id)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    validate_id(&budget)?;
    validate_id(&id)?;
    Ok(Json(store.run(move|db|{let tx=db.transaction()?;Ok(json!({"data":ledger::get(&tx,&budget,&id)?,"snapshot":concurrency::cursor(&tx,&budget)?,"epoch":crate::lifecycle::epoch(&tx,&budget)?,"reconciliation_token":crate::reconciliation::protection_token(&budget,&crate::reconciliation::members(&tx,&budget,&id)?)}))}).await?))
}
async fn change(
    State(store): State<Store>,
    path: Result<Path<(String, String)>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<Change>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let Path((budget, id)) = path.map_err(|_| Error::invalid("Invalid split path"))?;
    validate_id(&budget)?;
    validate_id(&id)?;
    let Json(input) = input.map_err(api::json_error)?;
    if input.snapshot.len() != 50
        || input.children.len() > MAX_CHILDREN
        || input.detach.len() > MAX_CHILDREN
        || input.reconciled_versions.len() > 101
        || input.changes.as_ref().is_some_and(|changes| {
            changes.upsert.len() > MAX_CHILDREN || changes.remove.len() > MAX_CHILDREN
        })
    {
        return Err(Error::invalid(
            "Invalid split snapshot or more than 100000 children",
        ));
    }
    if input.changes.is_some()
        && (!matches!(input.action, Action::Replace)
            || !input.children.is_empty()
            || !input.detach.is_empty())
    {
        return Err(Error::invalid(
            "Sparse changes require replace without children or detach",
        ));
    }
    let key = api::idempotency_key(&headers)?;
    let clock = concurrency::header(&headers)?;
    let cancel = concurrency::cancel_header(&headers)?;
    let epoch = crate::lifecycle::epoch_header(&headers)?;
    let payload = serde_json::to_string(&input).map_err(Error::internal)?;
    store
        .run(move |db| {
            concurrency::write(
                db,
                &format!("POST /budgets/{budget}/transactions/{id}/splits"),
                &key,
                &payload,
                clock.as_deref(),
                StatusCode::OK,
                |db| {
                    concurrency::execute(
                        db,
                        &budget,
                        clock.as_deref(),
                        cancel,
                        epoch.as_deref(),
                        concurrency::Mutation::Split(id, input),
                    )
                },
            )
        })
        .await
}
fn checked(n: i128) -> Result<i64, Error> {
    if n.abs() > MAX_MONEY as i128 {
        return Err(Error::invalid(
            "Split amount exceeds the exact integer range",
        ));
    }
    Ok(n as i64)
}
fn floor(n: i128, d: i128) -> i128 {
    let q = n / d;
    let r = n % d;
    if r != 0 && (r < 0) != (d < 0) {
        q - 1
    } else {
        q
    }
}
fn allocate(children: &mut [Child], total: i64, allocation: &Allocation) -> Result<(), Error> {
    for child in children.iter() {
        checked(child.amount as i128)?;
    }
    let sum: i128 = children.iter().map(|c| c.amount as i128).sum();
    let remaining = total as i128 - sum;
    let empty: Vec<usize> = children
        .iter()
        .enumerate()
        .filter(|(_, c)| c.amount == 0)
        .map(|(i, _)| i)
        .collect();
    let even = matches!(allocation, Allocation::Even)
        || matches!(allocation, Allocation::Auto) && !empty.is_empty();
    if !matches!(allocation, Allocation::Exact) && remaining != 0 {
        let indices = if even {
            empty
        } else {
            (0..children.len()).collect()
        };
        if indices.is_empty() {
            return Err(Error::invalid("Even allocation needs an empty split"));
        }
        if !even && sum == 0 {
            return Err(Error::invalid(
                "Proportional allocation requires a nonzero child total",
            ));
        }
        let mut amounts = Vec::new();
        for &i in &indices {
            let addition = if even {
                floor(remaining, indices.len() as i128)
            } else {
                floor(children[i].amount as i128 * remaining, sum)
            };
            amounts.push(children[i].amount as i128 + addition);
        }
        // Even targets zero allocations; proportional targets every allocation.
        // Avoid an O(children²) membership scan for large imported aggregates.
        let untouched = if even { sum } else { 0 };
        let mut cents = total as i128 - untouched - amounts.iter().sum::<i128>();
        for amount in &mut amounts {
            if cents > 0 {
                *amount += 1;
                cents -= 1;
            }
        }
        for (i, amount) in indices.into_iter().zip(amounts) {
            children[i].amount = checked(amount)?;
        }
    }
    if children.iter().map(|c| c.amount as i128).sum::<i128>() != total as i128 {
        return Err(Error::invalid(
            "Split amounts must equal the transaction total. Distribute the remainder before saving.",
        ));
    }
    Ok(())
}
pub fn stamp(db: &Connection, budget: &str, id: &str, clock: &str) -> Result<(), Error> {
    stamp_many(db, budget, &[id], clock)
}

/// Bound each SQL statement to 501 parameters. One sequence per member preserves
/// change-feed pagination while field versions are written as a set.
pub fn stamp_many(db: &Connection, budget: &str, ids: &[&str], clock: &str) -> Result<(), Error> {
    concurrency::parse_clock(clock)?;
    db.execute(
        "UPDATE budget_sync SET clock=?2 WHERE budget_id=?1 AND clock<?2",
        params![budget, clock],
    )?;
    for batch in ids.chunks(499) {
        let values = (3..=batch.len() + 2)
            .map(|n| format!("(?{n})"))
            .collect::<Vec<_>>()
            .join(",");
        let mut parameters = vec![budget, clock];
        parameters.extend(batch.iter().copied());
        db.execute(&format!("WITH members(id) AS (VALUES {values}), fields(field) AS (VALUES ('account_id'),('amount'),('date'),('cleared'),('notes'),('payee_id'),('category_id'),('reconciled'),('starting_balance'),('tombstone'),('splits')) INSERT INTO field_versions(budget_id,entity,entity_id,field,clock) SELECT ?1,'transactions',members.id,fields.field,?2 FROM members CROSS JOIN fields WHERE true ON CONFLICT(budget_id,entity,entity_id,field) DO UPDATE SET clock=excluded.clock WHERE excluded.clock>field_versions.clock"),rusqlite::params_from_iter(parameters.iter()))?;
        db.execute(&format!("WITH members(id) AS (VALUES {values}) INSERT INTO budget_changes(budget_id,entity,entity_id,sequence) SELECT ?1,'transactions',id,(SELECT sequence FROM budget_sync WHERE budget_id=?1)+ROW_NUMBER() OVER (ORDER BY id) FROM members WHERE ?2 IS NOT NULL ON CONFLICT(budget_id,entity,entity_id) DO UPDATE SET sequence=excluded.sequence"),rusqlite::params_from_iter(parameters.iter()))?;
        db.execute(
            "UPDATE budget_sync SET sequence=sequence+?2 WHERE budget_id=?1",
            params![budget, batch.len() as i64],
        )?;
    }
    Ok(())
}
pub fn apply(
    db: &Connection,
    budget: &str,
    id: &str,
    mut input: Change,
    clock: &str,
) -> Result<Value, Error> {
    concurrency::check_snapshot(db, budget, Some(&input.snapshot))?;
    let mut parent = ledger::get(db, budget, id)?;
    if !parent["parent_id"].is_null() {
        return Err(Error::invalid("Edit children through their parent split"));
    }
    if !input.confirm_reconciled
        && (!input.reconciled_versions.is_empty() || input.reconciliation_token.is_some())
    {
        return Err(Error::invalid(
            "Reconciliation proof requires confirm_reconciled=true",
        ));
    }
    let members = crate::reconciliation::members(db, budget, id)?;
    if let Some(token) = input.reconciliation_token.as_deref() {
        crate::reconciliation::verify_token(budget, &members, token, clock)?;
    } else {
        crate::reconciliation::protect_members(
            &members,
            input
                .confirm_reconciled
                .then_some(&input.reconciled_versions),
            clock,
        )?;
    }
    if parent["starting_balance"] == true {
        return Err(Error::invalid("Opening balances cannot be split"));
    }
    let newer_child:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM transactions c JOIN field_versions v ON v.budget_id=c.budget_id AND v.entity='transactions' AND v.entity_id=c.id WHERE c.budget_id=?1 AND c.parent_id=?2 AND c.tombstone=0 AND v.clock>?3)",params![budget,id,clock],|r|r.get(0))?;
    if newer_child
        || parent["field_versions"]
            .as_object()
            .is_some_and(|versions| {
                versions
                    .values()
                    .any(|v| v.as_str().is_some_and(|value| value > clock))
            })
    {
        return Err(Error::conflict(
            "A newer transaction edit prevents this split change. Refresh first.",
        ));
    }
    let old: Vec<Value> = parent["children"].as_array().cloned().unwrap_or_default();
    if let Some(changes) = input.changes.take() {
        let old_ids: BTreeSet<&str> = old.iter().map(|c| c["id"].as_str().unwrap()).collect();
        let mut remove = BTreeSet::new();
        for id in &changes.remove {
            if !old_ids.contains(id.as_str()) || !remove.insert(id.as_str()) {
                return Err(Error::invalid("Unknown or duplicate child to remove"));
            }
        }
        let mut replacements = BTreeMap::new();
        let mut appended = Vec::new();
        for child in changes.upsert {
            if remove.contains(child.id.as_str()) || replacements.contains_key(&child.id) {
                return Err(Error::invalid(
                    "Split changes must have distinct identifiers",
                ));
            }
            if !old_ids.contains(child.id.as_str()) {
                appended.push(child.id.clone());
            }
            replacements.insert(child.id.clone(), child);
        }
        input.children = old
            .iter()
            .filter(|c| !remove.contains(c["id"].as_str().unwrap()))
            .map(|c| {
                replacements
                    .remove(c["id"].as_str().unwrap())
                    .unwrap_or_else(|| Child {
                        id: c["id"].as_str().unwrap().into(),
                        amount: c["amount"].as_i64().unwrap(),
                        payee_id: c["retained_payee_id"].as_str().map(str::to_owned),
                        category_id: c["category_id"].as_str().map(str::to_owned),
                        notes: c["notes"].as_str().unwrap().into(),
                    })
            })
            .collect();
        for id in appended {
            input.children.push(replacements.remove(&id).unwrap());
        }
    }
    if input.children.len() > MAX_CHILDREN {
        return Err(Error::invalid("A split supports at most 100000 children"));
    }
    let old_by_id: BTreeMap<&str, &Value> =
        old.iter().map(|c| (c["id"].as_str().unwrap(), c)).collect();
    let previous_payee = parent["retained_payee_id"].as_str().map(str::to_owned);
    if let Some(fields) = input.fields.take() {
        if matches!(input.action, Action::Unsplit)
            && fields.as_object().is_none_or(|fields| {
                fields.iter().any(|(key, value)| {
                    let previous = if key == "payee_id" {
                        parent.get("retained_payee_id").unwrap_or(&parent[key])
                    } else {
                        &parent[key]
                    };
                    previous != value
                })
            })
        {
            return Err(Error::invalid(
                "Save transaction edits before unsplitting children",
            ));
        }
        parent = if input.confirm_reconciled {
            let version = parent["field_versions"]["reconciled"]
                .as_str()
                .unwrap_or(concurrency::ZERO);
            ledger::update_confirmed(
                db,
                budget,
                id,
                fields,
                crate::reconciliation::Confirmation { version, clock },
            )?
        } else {
            ledger::update(db, budget, id, fields)?
        };
    }
    let account = parent["account_id"].as_str().unwrap();
    let total = parent["amount"].as_i64().unwrap();
    let current_payee = parent["retained_payee_id"].as_str().map(str::to_owned);
    match input.action {
        Action::Replace => {
            if input.children.is_empty() {
                return Err(Error::invalid(
                    "Add at least one split, or recombine the transaction",
                ));
            }
            allocate(&mut input.children, total, &input.allocation)?;
            let mut ids = BTreeSet::new();
            ledger::account_exists(db, budget, account)?;
            let off_budget: bool = db.query_row(
                "SELECT off_budget FROM accounts WHERE budget_id=?1 AND id=?2",
                params![budget, account],
                |r| r.get(0),
            )?;
            let mut payees = BTreeMap::new();
            let mut categories = BTreeMap::new();
            let mut selections = BTreeSet::new();
            // Existing members are known to be live and owned by this parent.
            // Check incoming IDs in bounded batches, including tombstones.
            let incoming: Vec<&str> = input
                .children
                .iter()
                .map(|c| c.id.as_str())
                .filter(|cid| !old_by_id.contains_key(cid))
                .collect();
            for batch in incoming.chunks(500) {
                let placeholders = (2..=batch.len() + 1)
                    .map(|n| format!("?{n}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let mut parameters = vec![budget];
                parameters.extend(batch.iter().copied());
                let exists: bool = db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM transactions WHERE budget_id=?1 AND id IN ({placeholders}))"),rusqlite::params_from_iter(parameters.iter()),|r|r.get(0))?;
                if exists {
                    return Err(Error::conflict(
                        "Split ID already belongs to another or deleted transaction",
                    ));
                }
            }
            for (index, child) in input.children.iter_mut().enumerate() {
                if let Some(payee) = &child.payee_id {
                    if !payees.contains_key(payee) {
                        payees.insert(
                            payee.clone(),
                            crate::payees_tags::canonical(db, budget, payee)?,
                        );
                    }
                    child.payee_id = Some(payees[payee].clone());
                }
                if child.payee_id == previous_payee && previous_payee != current_payee {
                    child.payee_id = current_payee.clone();
                }
                validate_id(&child.id)?;
                if child.id == id || !ids.insert(child.id.clone()) {
                    return Err(Error::invalid("Split identifiers must be distinct"));
                }
                if let Some(category) = &child.category_id {
                    if !categories.contains_key(category) {
                        categories.insert(
                            category.clone(),
                            crate::categories::resolve(db, budget, Some(category.clone()))?,
                        );
                    }
                    child.category_id = categories[category].clone();
                }
                let previous = old_by_id.get(child.id.as_str());
                let retained_payee =
                    previous.and_then(|previous| previous["retained_payee_id"].as_str());
                if selections.insert((child.payee_id.clone(), retained_payee.map(str::to_owned))) {
                    ledger::validate_payee_selection(
                        db,
                        budget,
                        child.payee_id.as_deref(),
                        retained_payee,
                    )?;
                }
                let row = ledger::NewTransaction {
                    id: child.id.clone(),
                    account_id: account.into(),
                    amount: child.amount,
                    date: parent["date"].as_str().unwrap().into(),
                    cleared: parent["cleared"].as_bool().unwrap(),
                    notes: child.notes.clone(),
                    payee_id: child.payee_id.clone(),
                    category_id: child.category_id.clone(),
                    new_payee: None,
                    new_category: None,
                };
                row.validate()?;
                // Moving an existing allocation off budget retains its latent category.
                // Only a newly selected category is disallowed there.
                let retained_category = previous.is_some_and(|previous| {
                    previous["category_id"].as_str() == child.category_id.as_deref()
                });
                if child.category_id.is_some() && !retained_category && off_budget {
                    return Err(Error::invalid("Off-budget splits cannot have categories"));
                }
                db.prepare_cached("INSERT INTO transactions(budget_id,id,account_id,amount,date,cleared,notes,payee_id,category_id,sort_order,parent_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(budget_id,id) DO UPDATE SET account_id=excluded.account_id,amount=excluded.amount,date=excluded.date,cleared=excluded.cleared,notes=excluded.notes,payee_id=excluded.payee_id,category_id=excluded.category_id,sort_order=excluded.sort_order")?.execute(params![budget,child.id,account,child.amount,parent["date"].as_str(),parent["cleared"].as_bool(),child.notes,child.payee_id,child.category_id,-(index as i64)-1,id])?;
                db.execute(
                    "UPDATE transactions SET reconciled=?3 WHERE budget_id=?1 AND id=?2",
                    params![budget, child.id, parent["reconciled"].as_bool()],
                )?;
            }
            stamp_many(
                db,
                budget,
                &input
                    .children
                    .iter()
                    .map(|c| c.id.as_str())
                    .collect::<Vec<_>>(),
                clock,
            )?;
            let removed: Vec<&str> = old
                .iter()
                .map(|child| child["id"].as_str().unwrap())
                .filter(|cid| !ids.contains(*cid))
                .collect();
            remove_many(db, budget, &removed, clock)?;
            db.execute(
                "UPDATE transactions SET is_parent=1 WHERE budget_id=?1 AND id=?2",
                params![budget, id],
            )?;
        }
        Action::Recombine => {
            remove_many(
                db,
                budget,
                &old.iter()
                    .map(|child| child["id"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                clock,
            )?;
            db.execute(
                "UPDATE transactions SET is_parent=0 WHERE budget_id=?1 AND id=?2",
                params![budget, id],
            )?;
        }
        Action::Unsplit => {
            if old.is_empty() {
                return Err(Error::invalid("This transaction has no splits"));
            }
            let chosen: BTreeSet<&str> = input.detach.iter().map(String::as_str).collect();
            if chosen.iter().any(|id| !old_by_id.contains_key(id)) {
                return Err(Error::invalid("Unknown child to unsplit"));
            }
            if old.len() == 1 {
                db.execute("UPDATE transactions SET is_parent=0,category_id=?3 WHERE budget_id=?1 AND id=?2",params![budget,id,old[0]["category_id"].as_str()])?;
                remove(db, budget, old[0]["id"].as_str().unwrap(), clock)?;
            } else {
                let remaining: Vec<&Value> = old
                    .iter()
                    .filter(|c| !chosen.is_empty() && !chosen.contains(c["id"].as_str().unwrap()))
                    .collect();
                let all = remaining.len() <= 1;
                let mut detached = Vec::new();
                for child in &old {
                    let cid = child["id"].as_str().unwrap();
                    if all || chosen.is_empty() || chosen.contains(cid) {
                        db.prepare_cached("UPDATE transactions SET parent_id=NULL,sort_order=?3 WHERE budget_id=?1 AND id=?2")?.execute(params![budget,cid,parent["sort_order"].as_i64()])?;
                        detached.push(cid);
                    }
                }
                stamp_many(db, budget, &detached, clock)?;
                if all {
                    remove(db, budget, id, clock)?;
                } else {
                    db.execute(
                        "UPDATE transactions SET amount=?3 WHERE budget_id=?1 AND id=?2",
                        params![
                            budget,
                            id,
                            checked(
                                remaining
                                    .iter()
                                    .map(|c| c["amount"].as_i64().unwrap() as i128)
                                    .sum()
                            )?
                        ],
                    )?;
                }
            }
        }
    }
    stamp(db, budget, id, clock)?;
    ledger::refresh_account(db, budget, account)?;
    ledger::validate_budget_total(db, budget)?;
    validate(db, budget)?;
    let deleted: bool = db.query_row(
        "SELECT tombstone FROM transactions WHERE budget_id=?1 AND id=?2",
        params![budget, id],
        |r| r.get(0),
    )?;
    if deleted {
        Ok(json!({"id":id,"deleted":true}))
    } else {
        ledger::get(db, budget, id)
    }
}
fn remove(db: &Connection, budget: &str, id: &str, clock: &str) -> Result<(), Error> {
    remove_many(db, budget, &[id], clock)
}
fn remove_many(db: &Connection, budget: &str, ids: &[&str], clock: &str) -> Result<(), Error> {
    for batch in ids.chunks(500) {
        let placeholders = (2..=batch.len() + 1)
            .map(|n| format!("?{n}"))
            .collect::<Vec<_>>()
            .join(",");
        let mut parameters = vec![budget];
        parameters.extend(batch.iter().copied());
        db.execute(
            &format!(
                "UPDATE transactions SET tombstone=1 WHERE budget_id=?1 AND id IN ({placeholders})"
            ),
            rusqlite::params_from_iter(parameters.iter()),
        )?;
    }
    stamp_many(db, budget, ids, clock)
}

/// Validate restored/imported relationships before any budget becomes visible.
pub fn validate(db: &Connection, budget: &str) -> Result<(), Error> {
    // Validate membership in one join, and sum in i128 (SQLite SUM can overflow
    // before offsetting allocations cancel). No per-child reads/hydration.
    let mut q = db.prepare("SELECT c.id,c.parent_id,c.is_parent,c.amount,COALESCE(p.is_parent=1 AND p.parent_id IS NULL AND p.tombstone=0 AND c.account_id=p.account_id AND c.date=p.date AND c.cleared=p.cleared AND c.reconciled=p.reconciled AND c.starting_balance=p.starting_balance,0) FROM transactions c LEFT JOIN transactions p ON p.budget_id=c.budget_id AND p.id=c.parent_id WHERE c.budget_id=?1 AND c.tombstone=0")?;
    let mut sums = BTreeMap::<String, (usize, i128)>::new();
    let mut parents = Vec::new();
    for row in q.query_map([budget], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, bool>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, bool>(4)?,
        ))
    })? {
        let (id, parent, is_parent, amount, valid) = row?;
        if let Some(parent) = parent {
            validate_id(&parent)?;
            if is_parent || parent == id || !valid {
                return Err(Error::invalid(
                    "Split child has a missing or inconsistent parent",
                ));
            }
            let sum = sums.entry(parent).or_default();
            sum.0 += 1;
            sum.1 += amount as i128;
        } else if is_parent {
            parents.push((id, amount));
        }
    }
    for (id, amount) in parents {
        if sums
            .get(&id)
            .is_none_or(|(count, sum)| *count == 0 || *sum != amount as i128)
        {
            return Err(Error::invalid(
                "Split child amounts must equal their parent total",
            ));
        }
    }
    Ok(())
}
