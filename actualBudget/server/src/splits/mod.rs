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
        Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::{HeaderMap, StatusCode},
    routing::post,
};
use rusqlite::{Connection, OptionalExtension, params};
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer_account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterpart_id: Option<String>,
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
}
pub fn router() -> Router<Store> {
    Router::new().route(
        "/api/v1/budgets/{budget}/transactions/{id}/splits",
        post(change).get(view),
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
        || input.children.len() > 100
        || input.detach.len() > 100
        || input.reconciled_versions.len() > 101
    {
        return Err(Error::invalid(
            "Invalid split snapshot or more than 100 children",
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
pub fn decorate(db: &Connection, budget: &str, mut row: Value) -> Result<Value, Error> {
    if row["is_parent"] == true && row["parent_id"].is_null() {
        let id = row["id"]
            .as_str()
            .ok_or_else(|| Error::internal("Missing split ID"))?;
        let mut q=db.prepare("SELECT id FROM transactions WHERE budget_id=?1 AND parent_id=?2 AND tombstone=0 ORDER BY sort_order DESC,id ASC")?;
        let ids = q
            .query_map(params![budget, id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        row["children"] = json!(
            ids.iter()
                .map(|id| ledger::get(db, budget, id))
                .collect::<Result<Vec<_>, _>>()?
        );
    }
    if row["is_parent"] == false
        && row["parent_id"].is_null()
        && let Some(object) = row.as_object_mut()
    {
        object.remove("is_parent");
        object.remove("parent_id");
    }
    Ok(row)
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
        let mut cents = total as i128
            - children
                .iter()
                .enumerate()
                .filter(|(i, _)| !indices.contains(i))
                .map(|(_, c)| c.amount as i128)
                .sum::<i128>()
            - amounts.iter().sum::<i128>();
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
    for field in [
        "account_id",
        "amount",
        "date",
        "cleared",
        "notes",
        "payee_id",
        "category_id",
        "reconciled",
        "starting_balance",
        "tombstone",
        "splits",
    ] {
        concurrency::seed_field_version(db, budget, "transactions", id, field, clock)?;
    }
    concurrency::mark(db, budget, "transactions", id)
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
    if !parent["transfer_id"].is_null() {
        return Err(Error::invalid("A transfer cannot become a split parent"));
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
    crate::transfers::composition::guard_children(db, budget, &old, clock)?;
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
            for (index, child) in input.children.iter_mut().enumerate() {
                child.payee_id = child
                    .payee_id
                    .as_deref()
                    .map(|payee| crate::payees_tags::canonical(db, budget, payee))
                    .transpose()?;
                if child.payee_id == previous_payee && previous_payee != current_payee {
                    child.payee_id = current_payee.clone();
                }
                validate_id(&child.id)?;
                if child.id == id || !ids.insert(child.id.clone()) {
                    return Err(Error::invalid("Split identifiers must be distinct"));
                }
                let previous: Option<(Option<String>, bool)> = db
                    .query_row(
                        "SELECT parent_id,tombstone FROM transactions WHERE budget_id=?1 AND id=?2",
                        params![budget, child.id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                if previous
                    .as_ref()
                    .is_some_and(|(owner, deleted)| owner.as_deref() != Some(id) || *deleted)
                {
                    return Err(Error::conflict(
                        "Split ID already belongs to another or deleted transaction",
                    ));
                }
                let existing = old.iter().find(|previous| previous["id"] == child.id);
                // Older clients may retain the transfer payee without the new fields.
                if child.transfer_account_id.is_none()
                    && existing.is_some_and(|row| {
                        !row["transfer_id"].is_null()
                            && row["payee_id"].as_str() == child.payee_id.as_deref()
                    })
                {
                    let peer = existing.unwrap()["transfer_id"].as_str().unwrap();
                    let other = ledger::get(db, budget, peer)?;
                    child.transfer_account_id = other["account_id"].as_str().map(str::to_owned);
                    child.counterpart_id = Some(peer.into());
                }
                if child.transfer_account_id.is_some() != child.counterpart_id.is_some() {
                    return Err(Error::invalid(
                        "A split transfer needs a destination and counterpart identity",
                    ));
                }
                if child.transfer_account_id.is_none()
                    && existing.is_some_and(|row| !row["transfer_id"].is_null())
                {
                    crate::transfers::composition::remove_child(db, budget, &child.id, clock)?;
                }
                // Transfer identities are owned by the pair operation below.
                if child.transfer_account_id.is_some() {
                    child.payee_id = None;
                }
                child.category_id =
                    crate::categories::resolve(db, budget, child.category_id.clone())?;
                let retained_payee = old
                    .iter()
                    .find(|previous| previous["id"] == child.id)
                    .and_then(|previous| previous["retained_payee_id"].as_str());
                ledger::validate_payee_selection(
                    db,
                    budget,
                    child.payee_id.as_deref(),
                    retained_payee,
                )?;
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
                ledger::validate_references(db, budget, &row, None)?;
                // Moving an existing allocation off budget retains its latent category.
                // Only a newly selected category is disallowed there.
                let retained_category = old.iter().any(|previous| {
                    previous["id"] == child.id
                        && previous["category_id"].as_str() == child.category_id.as_deref()
                });
                if child.category_id.is_some()
                    && !retained_category
                    && db.query_row(
                        "SELECT off_budget FROM accounts WHERE budget_id=?1 AND id=?2",
                        params![budget, account],
                        |r| r.get::<_, bool>(0),
                    )?
                {
                    return Err(Error::invalid("Off-budget splits cannot have categories"));
                }
                db.execute("INSERT INTO transactions(budget_id,id,account_id,amount,date,cleared,notes,payee_id,category_id,sort_order,parent_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(budget_id,id) DO UPDATE SET account_id=excluded.account_id,amount=excluded.amount,date=excluded.date,cleared=excluded.cleared,notes=excluded.notes,payee_id=excluded.payee_id,category_id=excluded.category_id,sort_order=excluded.sort_order",params![budget,child.id,account,child.amount,parent["date"].as_str(),parent["cleared"].as_bool(),child.notes,child.payee_id,child.category_id,-(index as i64)-1,id])?;
                db.execute(
                    "UPDATE transactions SET reconciled=?3 WHERE budget_id=?1 AND id=?2",
                    params![budget, child.id, parent["reconciled"].as_bool()],
                )?;
                stamp(db, budget, &child.id, clock)?;
                if let (Some(destination), Some(peer)) =
                    (&child.transfer_account_id, &child.counterpart_id)
                {
                    crate::transfers::composition::save_child(
                        db,
                        budget,
                        &child.id,
                        destination,
                        peer,
                        clock,
                    )?;
                }
            }
            for child in &old {
                let cid = child["id"].as_str().unwrap();
                if !ids.contains(cid) {
                    remove(db, budget, cid, clock)?;
                }
            }
            db.execute(
                "UPDATE transactions SET is_parent=1 WHERE budget_id=?1 AND id=?2",
                params![budget, id],
            )?;
        }
        Action::Recombine => {
            for child in &old {
                remove(db, budget, child["id"].as_str().unwrap(), clock)?;
            }
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
            if chosen
                .iter()
                .any(|id| !old.iter().any(|c| c["id"].as_str() == Some(id)))
            {
                return Err(Error::invalid("Unknown child to unsplit"));
            }
            if old.len() == 1 {
                db.execute("UPDATE transactions SET is_parent=0,category_id=?3 WHERE budget_id=?1 AND id=?2",params![budget,id,old[0]["category_id"].as_str()])?;
                crate::transfers::composition::promote_only_child(db, budget, id, &old[0], clock)?;
                remove(db, budget, old[0]["id"].as_str().unwrap(), clock)?;
            } else {
                let remaining: Vec<&Value> = old
                    .iter()
                    .filter(|c| !chosen.is_empty() && !chosen.contains(c["id"].as_str().unwrap()))
                    .collect();
                let all = remaining.len() <= 1;
                for child in &old {
                    let cid = child["id"].as_str().unwrap();
                    if all || chosen.is_empty() || chosen.contains(cid) {
                        db.execute("UPDATE transactions SET parent_id=NULL,sort_order=?3 WHERE budget_id=?1 AND id=?2",params![budget,cid,parent["sort_order"].as_i64()])?;
                        stamp(db, budget, cid, clock)?;
                    }
                }
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
    crate::transfers::validate(db, budget)?;
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
pub fn remove(db: &Connection, budget: &str, id: &str, clock: &str) -> Result<(), Error> {
    crate::transfers::composition::remove_child(db, budget, id, clock)?;
    db.execute(
        "UPDATE transactions SET tombstone=1 WHERE budget_id=?1 AND id=?2",
        params![budget, id],
    )?;
    stamp(db, budget, id, clock)
}

/// Validate restored/imported relationships before any budget becomes visible.
pub fn validate(db: &Connection, budget: &str) -> Result<(), Error> {
    let mut q = db.prepare(
        "SELECT id,parent_id,is_parent FROM transactions WHERE budget_id=?1 AND tombstone=0",
    )?;
    for row in q.query_map([budget], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, bool>(2)?,
        ))
    })? {
        let (id, parent, is_parent) = row?;
        if let Some(parent) = parent {
            validate_id(&parent)?;
            if is_parent || parent == id {
                return Err(Error::invalid("Nested or cyclic split"));
            }
            let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM transactions c JOIN transactions p ON p.budget_id=c.budget_id AND p.id=c.parent_id WHERE c.budget_id=?1 AND c.id=?2 AND p.is_parent=1 AND p.parent_id IS NULL AND p.tombstone=0 AND c.account_id=p.account_id AND c.date=p.date AND c.cleared=p.cleared AND c.reconciled=p.reconciled AND c.starting_balance=p.starting_balance)",params![budget,id],|r|r.get(0))?;
            if !valid {
                return Err(Error::invalid(
                    "Split child has a missing or inconsistent parent",
                ));
            }
        }
        if is_parent {
            let parent = ledger::get(db, budget, &id)?;
            let children = parent["children"].as_array().unwrap();
            if children.is_empty()
                || children
                    .iter()
                    .map(|c| c["amount"].as_i64().unwrap() as i128)
                    .sum::<i128>()
                    != parent["amount"].as_i64().unwrap() as i128
            {
                return Err(Error::invalid(
                    "Split child amounts must equal their parent total",
                ));
            }
        }
    }
    Ok(())
}
