//! Conservative maintenance: previews never mutate authority. Only source-backed,
//! unambiguous canonicalizations run, after a normally restorable native backup.
use crate::{
    api, concurrency,
    error::Error,
    ledger, lifecycle,
    store::{self, Store},
};
use axum::{
    Json, Router,
    extract::{
        Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::{HeaderMap, StatusCode},
    routing::get,
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Serialize)]
struct Repair {
    id: String,
    field: &'static str,
    before: Value,
    after: Value,
    reason: &'static str,
}
#[derive(Serialize)]
struct Issue {
    domain: &'static str,
    id: String,
    message: String,
    blocking: bool,
}
#[derive(Serialize)]
struct Account {
    id: String,
    name: String,
    before: Value,
    after: Value,
}
#[derive(Serialize)]
struct Preview {
    epoch: String,
    snapshot: String,
    preview_token: String,
    name: String,
    accounts: Vec<Account>,
    repairs: Vec<Repair>,
    issues: Vec<Issue>,
    can_apply: bool,
    backup_restorable: bool,
}
struct Scan<'a> {
    db: &'a Connection,
    budget: &'a str,
    rows: BTreeMap<String, Value>,
    repairs: Vec<Repair>,
    issues: Vec<Issue>,
    protected: BTreeMap<String, bool>,
    ordering_hash: String,
}
impl Scan<'_> {
    fn issue(
        &mut self,
        domain: &'static str,
        id: &str,
        message: impl Into<String>,
        blocking: bool,
    ) {
        self.issues.push(Issue {
            domain,
            id: id.into(),
            message: message.into(),
            blocking,
        });
    }
    fn repair(
        &mut self,
        row: &Value,
        field: &'static str,
        after: Value,
        reason: &'static str,
    ) -> Result<(), Error> {
        let id = row["id"].as_str().unwrap();
        if row[field] == after {
            return Ok(());
        }
        if !self.protected.contains_key(id) {
            let members = crate::transfers::protection::members(self.db, self.budget, &[id])?;
            let protected = members.iter().any(|r| r["reconciled"] == true);
            for member in members {
                self.protected
                    .insert(member["id"].as_str().unwrap().into(), protected);
            }
        }
        if self.protected[id] {
            self.issue("protection",id,"A reconciled connected entry protects this graph. Review it in its register; no automatic repair.",false);
        } else {
            self.repairs.push(Repair {
                id: id.into(),
                field,
                before: row[field].clone(),
                after,
                reason,
            });
        }
        Ok(())
    }
}
// Delivered domains register complete diagnostics here; no unfinished repair actions.
type Check = fn(&mut Scan<'_>) -> Result<(), Error>;
const CHECKS: &[Check] = &[references, splits, transfers, ordering];
fn references(scan: &mut Scan<'_>) -> Result<(), Error> {
    for (table, field) in [
        ("accounts", "account_id"),
        ("payees", "payee_id"),
        ("categories", "category_id"),
    ] {
        let mut q=scan.db.prepare(&format!("SELECT t.id FROM transactions t LEFT JOIN {table} r ON r.budget_id=t.budget_id AND r.id=t.{field} WHERE t.budget_id=?1 AND t.{field} IS NOT NULL AND r.id IS NULL ORDER BY t.id"))?;
        for id in q.query_map([scan.budget], |r| r.get::<_, String>(0))? {
            scan.issue("references",&id?,format!("Missing {table} reference. Keep the original purchase and review its source; no reference is guessed."),true);
        }
    }
    Ok(())
}
fn splits(scan: &mut Scan<'_>) -> Result<(), Error> {
    let rows = scan.rows.values().cloned().collect::<Vec<_>>();
    let mut sums = BTreeMap::<String, (usize, i128)>::new();
    for row in &rows {
        let id = row["id"].as_str().unwrap();
        if let Some(parent) = row["parent_id"].as_str() {
            let Some(p) = scan.rows.get(parent).cloned() else {
                scan.issue("splits",id,"Missing/deleted split parent. This purchase is retained for review, never deleted automatically.",true);
                continue;
            };
            let sum = sums.entry(parent.into()).or_default();
            sum.0 += 1;
            sum.1 += row["amount"].as_i64().unwrap() as i128;
            if p["is_parent"] != true
                || row["is_parent"] == true
                || p["parent_id"].is_string()
                || [
                    "account_id",
                    "date",
                    "cleared",
                    "reconciled",
                    "starting_balance",
                ]
                .iter()
                .any(|f| row[f] != p[f])
            {
                scan.issue("splits",id,"Split membership or inherited state is inconsistent. Review the complete aggregate; no amount or membership is guessed.",true);
            }
            if row["payee_id"].is_null()
                && p["payee_id"].is_string()
                && row["transfer_id"].is_null()
            {
                let ordinary =
                    p["payee_transfer_account_id"].is_null() && p["payee_deleted"] == false;
                if ordinary {
                    scan.repair(
                        row,
                        "payee_id",
                        p["payee_id"].clone(),
                        "Inherit the split parent's ordinary payee for its blank child",
                    )?;
                }
            }
        }
        if row["is_parent"] == true {
            scan.repair(
                row,
                "category_id",
                Value::Null,
                "Allocations belong to split children, not the parent",
            )?;
        }
    }
    for row in &rows {
        let id = row["id"].as_str().unwrap();
        if row["is_parent"] == true
            && sums
                .get(id)
                .is_none_or(|(n, sum)| *n == 0 || *sum != row["amount"].as_i64().unwrap() as i128)
        {
            scan.issue("splits",id,"Split allocations do not equal their parent. Purchases and amounts are retained; review the original allocation.",true);
        }
    }
    Ok(())
}
fn transfers(scan: &mut Scan<'_>) -> Result<(), Error> {
    let rows = scan.rows.values().cloned().collect::<Vec<_>>();
    for row in &rows {
        let id = row["id"].as_str().unwrap();
        let transfer_payee = row["payee_transfer_account_id"].as_str();
        let Some(peer) = row["transfer_id"].as_str() else {
            if transfer_payee.is_some() {
                scan.issue(
                    "transfers",
                    id,
                    "Transfer payee has no linked purchase. No partner is synthesized.",
                    true,
                );
            }
            continue;
        };
        let Some(other) = scan.rows.get(peer) else {
            scan.issue(
                "transfers",
                id,
                "Missing/deleted transfer partner. Original purchase retained for review.",
                true,
            );
            continue;
        };
        if other["transfer_id"] != id
            || row["amount"].as_i64().unwrap() as i128
                != -(other["amount"].as_i64().unwrap() as i128)
            || other["account_id"] == row["account_id"]
            || row["is_parent"] == true
            || other["is_parent"] == true
            || transfer_payee != other["account_id"].as_str()
        {
            scan.issue("transfers",id,"Transfer links, amounts or account references disagree. No purchase is removed or guessed.",true);
            continue;
        }
        let same = row["account_off_budget"].is_boolean()
            && row["account_off_budget"] == other["account_off_budget"];
        if same {
            scan.repair(
                row,
                "category_id",
                Value::Null,
                "Transfers within the same budget class have no spending category",
            )?;
        }
    }
    Ok(())
}
fn ordering(scan: &mut Scan<'_>) -> Result<(), Error> {
    let current: String = scan.db.query_row(
        "SELECT clock FROM budget_sync WHERE budget_id=?1",
        [scan.budget],
        |r| r.get(0),
    )?;
    if concurrency::validate_import_clock(&current).is_err()
        || concurrency::import_clock_ahead(&current).unwrap_or(true)
    {
        scan.issue("ordering","budget","The budget clock is invalid or ahead of this server. Correct the clock/source before repair; pending request clocks are never rewritten.",true);
    }
    let mut hash = Sha256::new();
    let mut query=scan.db.prepare("SELECT entity,entity_id,field,clock FROM field_versions WHERE budget_id=?1 ORDER BY entity,entity_id,field")?;
    for row in query.query_map([scan.budget], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })? {
        let (entity, id, field, clock) = row?;
        hash.update(json!([entity, id, field, clock]).to_string().as_bytes());
        if concurrency::validate_import_clock(&clock).is_err() || clock > current {
            scan.issue("ordering",&id,format!("{entity}.{field} has an invalid or unowned field clock. Review source ordering; no pending clock or receipt is reset."),true);
        }
    }
    scan.ordering_hash = format!("{:x}", hash.finalize());
    Ok(())
}
fn preview(db: &Connection, budget: &str) -> Result<Preview, Error> {
    let metadata = lifecycle::metadata(db, budget, false)?;
    let mut query=db.prepare("SELECT t.id,t.account_id,t.amount,t.date,t.cleared,t.reconciled,t.starting_balance,t.payee_id,t.category_id,t.parent_id,t.is_parent,t.transfer_id,y.transfer_account_id,y.tombstone,a.off_budget FROM transactions t LEFT JOIN payees y ON y.budget_id=t.budget_id AND y.id=t.payee_id LEFT JOIN accounts a ON a.budget_id=t.budget_id AND a.id=t.account_id WHERE t.budget_id=?1 AND t.tombstone=0 ORDER BY t.id")?;
    let rows=query.query_map([budget],|r|Ok(json!({"id":r.get::<_,String>(0)?,"account_id":r.get::<_,String>(1)?,"amount":r.get::<_,i64>(2)?,"date":r.get::<_,String>(3)?,"cleared":r.get::<_,bool>(4)?,"reconciled":r.get::<_,bool>(5)?,"starting_balance":r.get::<_,bool>(6)?,"payee_id":r.get::<_,Option<String>>(7)?,"category_id":r.get::<_,Option<String>>(8)?,"parent_id":r.get::<_,Option<String>>(9)?,"is_parent":r.get::<_,bool>(10)?,"transfer_id":r.get::<_,Option<String>>(11)?,"payee_transfer_account_id":r.get::<_,Option<String>>(12)?,"payee_deleted":r.get::<_,Option<bool>>(13)?,"account_off_budget":r.get::<_,Option<bool>>(14)?})))?.collect::<Result<Vec<_>,_>>()?;
    let source = json!(rows);
    let mut scan = Scan {
        db,
        budget,
        rows: rows
            .into_iter()
            .map(|v| (v["id"].as_str().unwrap().into(), v))
            .collect(),
        repairs: vec![],
        issues: vec![],
        protected: BTreeMap::new(),
        ordering_hash: String::new(),
    };
    for check in CHECKS {
        check(&mut scan)?;
    }
    let mut accounts = vec![];
    let mut q = db.prepare("SELECT id,name FROM accounts WHERE budget_id=?1 ORDER BY id")?;
    let mut calculated = [0_i128; 3];
    for pair in q.query_map([budget], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (id, name) = pair?;
        let after = serde_json::to_value(ledger::calculated_balances(db, budget, &id)?)
            .map_err(Error::internal)?;
        for (slot, key) in calculated.iter_mut().zip(["total", "cleared", "uncleared"]) {
            *slot += after[key].as_i64().unwrap() as i128;
        }
        accounts.push(Account {
            id: id.clone(),
            name,
            before: serde_json::to_value(ledger::cached_balances(db, budget, &id)?)
                .map_err(Error::internal)?,
            after,
        });
    }
    if calculated
        .iter()
        .any(|v| v.abs() > crate::models::MAX_MONEY as i128)
    {
        scan.issue(
            "ledger",
            "budget",
            "Recalculated budget total exceeds the exact money range. No repair applied.",
            true,
        );
    }
    let mut orphans=db.prepare("SELECT l.account_id FROM ledger_totals l LEFT JOIN accounts a ON a.budget_id=l.budget_id AND a.id=l.account_id WHERE l.budget_id=?1 AND a.id IS NULL ORDER BY l.account_id")?;
    for id in orphans.query_map([budget], |r| r.get::<_, String>(0))? {
        scan.issue("ledger",&id?,"A cached total refers to a missing account. Keep the original records and review the missing account before automatic recalculation.",true);
    }
    let original = lifecycle::archive::capture(db, budget)?;
    let backup_restorable = match lifecycle::archive::validate(&original) {
        Ok(_) => true,
        Err(error) => {
            scan.issue("archive","budget",format!("Original records cannot pass the existing restore validator: {}. Keep the original data and review these inconsistencies; automatic repair is blocked.",error.message),true);
            false
        }
    };
    let snapshot = concurrency::cursor(db, budget)?;
    let clock: String = db.query_row(
        "SELECT clock FROM budget_sync WHERE budget_id=?1",
        [budget],
        |r| r.get(0),
    )?;
    let changed = accounts.iter().any(|a| a.before != a.after) || !scan.repairs.is_empty();
    let can_apply = changed && backup_restorable && !scan.issues.iter().any(|i| i.blocking);
    let token = lifecycle::archive::hash(
        json!([
            "actualBudget.recovery.v1",
            budget,
            metadata["epoch"],
            snapshot,
            clock,
            source,
            scan.ordering_hash,
            accounts,
            scan.repairs,
            scan.issues
        ])
        .to_string()
        .as_bytes(),
    );
    Ok(Preview {
        epoch: metadata["epoch"].as_str().unwrap().into(),
        snapshot,
        preview_token: token,
        name: metadata["name"].as_str().unwrap().into(),
        accounts,
        repairs: scan.repairs,
        issues: scan.issues,
        can_apply,
        backup_restorable,
    })
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Apply {
    preview_token: String,
    snapshot: String,
    confirm_name: String,
}
fn validate_plan(db: &Connection, b: &str, input: &Apply, epoch: &str) -> Result<Preview, Error> {
    lifecycle::require_epoch(db, b, Some(epoch))?;
    let p = preview(db, b)?;
    if p.snapshot != input.snapshot || p.preview_token != input.preview_token {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "recovery_preview_changed",
            "Budget data or the recovery plan changed. Review a new complete preview before applying.",
        ));
    }
    if p.name != input.confirm_name {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "budget_confirmation_mismatch",
            "Enter the current budget name to confirm the reviewed repair.",
        ));
    }
    if !p.can_apply {
        return Err(Error::conflict(
            "No safely applicable repair. Review the listed issues; original purchases have been kept.",
        ));
    }
    Ok(p)
}
pub fn router() -> Router<Store> {
    Router::new().route("/api/v1/budgets/{budget}/recovery", get(read).post(apply))
}
async fn read(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<Value>, Error> {
    let budget = api::budget_id(path)?;
    Ok(Json(
        json!({"data":store.run(move|db|{serde_json::to_value(preview(db,&budget)?).map_err(Error::internal)}).await?}),
    ))
}
async fn apply(
    State(store): State<Store>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    input: Result<Json<Apply>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    let budget = api::budget_id(path)?;
    let Json(input) = input.map_err(api::json_error)?;
    if input.preview_token.len() != 64
        || !input
            .preview_token
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        || input.snapshot.len() != 50
        || !input.snapshot.starts_with("c1")
        || !input.snapshot[2..]
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        || input.confirm_name.chars().count() > 120
    {
        return Err(Error::invalid(
            "Provide the original complete recovery token, snapshot and budget confirmation",
        ));
    }
    let key = api::idempotency_key(&headers)?;
    let epoch = lifecycle::epoch_header(&headers)?
        .ok_or_else(|| Error::invalid("Provide the reviewed budget epoch"))?;
    if headers.contains_key("X-Actual-Edit-Clock") || headers.contains_key("X-Actual-History-Owner")
    {
        return Err(Error::invalid(
            "Recovery owns its maintenance clock; use the reviewed epoch and operation identity",
        ));
    }
    let cancel = concurrency::cancel_header(&headers)?;
    let payload = json!({"request":input,"budget_epoch":epoch}).to_string();
    let result=store.run(move|db|{
        let scope=format!("POST /budgets/{budget}/recovery");
        if cancel||store::receipt(db,&scope,&key,&payload)?.is_some(){return lifecycle::cancellation::write(db,&scope,&key,&payload,cancel,|_|Err(Error::internal("Receipt replay cannot execute repair")));}
        validate_plan(db,&budget,&input,&epoch)?;
        // Commit the restorable original BEFORE the financial receipt transaction.
        let backup=store::write(db,&format!("POST /budgets/{budget}/recovery/backup"),&key,&payload,|db|{
            validate_plan(db,&budget,&input,&epoch)?;
            lifecycle::make_point(db,&budget,"manual","Before reviewed data repair")
        })?;
        lifecycle::cancellation::write(db,&scope,&key,&payload,false,|db|{
            let p=validate_plan(db,&budget,&input,&epoch)?;
            let clock=concurrency::clock(db,&budget,None)?;
            for repair in &p.repairs {
                // fields are from the closed, source-backed registry above.
                db.execute(&format!("UPDATE transactions SET {}=?3 WHERE budget_id=?1 AND id=?2",repair.field),params![budget,repair.id,repair.after.as_str()])?;
                concurrency::seed_field_version(db,&budget,"transactions",&repair.id,repair.field,&clock)?;
            }
            let mut changed=BTreeSet::new();
            for account in &p.accounts {ledger::refresh_account(db,&budget,&account.id)?;if account.before!=account.after {changed.insert(account.id.clone());}}
            for id in changed {concurrency::mark(db,&budget,"accounts",&id)?;}
            lifecycle::archive::validate(&lifecycle::archive::capture(db,&budget)?)?;
            Ok(json!({"backup":backup,"repaired_fields":p.repairs.len(),"recalculated_accounts":p.accounts.len(),"epoch":epoch,"snapshot":concurrency::cursor(db,&budget)?,"edit_clock":clock}))
        })
    }).await?;
    Ok(Json(json!({"data":result})))
}
