use crate::{
    error::Error,
    models::{NewAccount, NewBudget},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{Notify, Semaphore};
use uuid::Uuid;

#[derive(Clone)]
pub struct Store {
    db: Arc<Mutex<Connection>>,
    worker: Arc<Semaphore>,
    changed: Arc<Notify>,
}
impl Store {
    pub fn open(directory: &Path) -> Result<Self, Error> {
        std::fs::create_dir_all(directory).map_err(Error::internal)?;
        let mut db = Connection::open(directory.join("actualbudget.sqlite3"))?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )?;
        crate::migrations::apply(&mut db, directory)?;
        crate::lifecycle::automatic::configure(&db)?;
        crate::history::configure(&db)?;
        let store = Self {
            db: Arc::new(Mutex::new(db)),
            worker: Arc::new(Semaphore::new(1)),
            changed: Arc::new(Notify::new()),
        };
        crate::lifecycle::automatic::start(store.clone(), store.changed.clone());
        Ok(store)
    }
    pub async fn run<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Connection) -> Result<T, Error> + Send + 'static,
    ) -> Result<T, Error> {
        // Wait asynchronously: only one blocking worker ever waits on this connection.
        let permit = self
            .worker
            .clone()
            .acquire_owned()
            .await
            .map_err(Error::internal)?;
        let db = self.db.clone();
        let changed = self.changed.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut db = db.lock().map_err(Error::internal)?;
            let before = db.total_changes();
            let result = operation(&mut db);
            if db.total_changes() != before {
                changed.notify_one();
            }
            result
        })
        .await
        .map_err(Error::internal)?
    }
}
pub fn require_budget(db: &Connection, id: &str) -> Result<(), Error> {
    crate::lifecycle::metadata(db, id, false).map(|_| ())
}
pub fn accounts(db: &Connection, budget: &str) -> Result<Value, Error> {
    require_budget(db, budget)?;
    let mut stmt=db.prepare("SELECT id FROM accounts WHERE budget_id=?1 AND tombstone=0 ORDER BY off_budget,sort_order,id")?;
    let ids = stmt
        .query_map([budget], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!(
        ids.iter()
            .map(|id| crate::accounts::get(db, budget, id))
            .collect::<Result<Vec<_>, _>>()?
    ))
}
pub fn create_budget(db: &Connection, input: NewBudget) -> Result<Value, Error> {
    let tx = db;
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM budgets WHERE id=?1)",
        [&input.id],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Error::conflict("Budget ID already exists"));
    }
    tx.execute(
        "INSERT INTO budgets(id,name,budget_type) VALUES(?1,?2,?3)",
        params![input.id, input.name, input.budget_type],
    )?;
    let category_id = Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO categories(budget_id,id,name,is_income) VALUES(?1,?2,'Starting Balances',1)",
        params![input.id, category_id],
    )?;
    crate::categories::stamp_new(db, &input.id, &category_id)?;
    crate::lifecycle::metadata(db, &input.id, false)
}
pub fn create_account(db: &Connection, budget: &str, input: NewAccount) -> Result<Value, Error> {
    crate::notes::guard_domain_id(db,budget,&input.id)?;
    let tx = db;
    require_budget(tx, budget)?;
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE budget_id=?1 AND id=?2)",
        params![budget, input.id],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Error::conflict("Account ID already exists"));
    }
    tx.execute(
        "INSERT INTO accounts(budget_id,id,name,notes,off_budget,sort_order) VALUES(?1,?2,?3,?4,?5,(SELECT COALESCE(MAX(sort_order),-1)+1 FROM accounts WHERE budget_id=?1))",
        params![budget, input.id, input.name, input.notes, input.off_budget],
    )?;
    if input.opening_balance != 0 {
        let payee = tx
            .query_row(
                "SELECT id FROM payees WHERE budget_id=?1 AND name='Starting Balance'",
                [budget],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        tx.execute(
            "INSERT OR IGNORE INTO payees(budget_id,id,name) VALUES(?1,?2,'Starting Balance')",
            params![budget, payee],
        )?;
        let category: Option<String> = if input.off_budget {
            None
        } else {
            tx.query_row("SELECT id FROM categories WHERE budget_id=?1 AND is_income=1 AND tombstone=0 ORDER BY name='Starting Balances' DESC,id LIMIT 1",[budget],|r|r.get(0)).optional()?
        };
        let transaction_id = Uuid::new_v4().to_string();
        tx.execute("INSERT INTO transactions(budget_id,id,account_id,amount,date,cleared,starting_balance,payee_id,category_id) VALUES(?1,?2,?3,?4,?5,1,1,?6,?7)",params![budget,transaction_id,input.id,input.opening_balance,input.opening_date,payee,category])?;
        crate::concurrency::stamp_new_transaction(db, budget, &transaction_id)?;
    }
    crate::ledger::refresh_account(db, budget, &input.id)?;
    crate::ledger::validate_budget_total(db, budget)?;
    crate::accounts::get(db, budget, &input.id)
}

pub fn receipt(
    db: &Connection,
    scope: &str,
    key: &str,
    payload: &str,
) -> Result<Option<Value>, Error> {
    let existing: Option<(String, String)> = db
        .query_row(
            "SELECT payload,response FROM operations WHERE scope=?1 AND operation_id=?2",
            params![scope, key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((previous, response)) = existing {
        if previous != payload {
            return Err(Error::new(
                axum::http::StatusCode::CONFLICT,
                "idempotency_conflict",
                "Idempotency-Key was already used with a different payload",
            ));
        }
        return serde_json::from_str(&response)
            .map(Some)
            .map_err(Error::internal);
    }
    Ok(None)
}
pub fn write(
    db: &mut Connection,
    scope: &str,
    key: &str,
    payload: &str,
    operation: impl FnOnce(&Connection) -> Result<Value, Error>,
) -> Result<Value, Error> {
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if let Some(result) = receipt(&tx, scope, key, payload)? {
        return Ok(result);
    }
    let result = operation(&tx)?;
    tx.execute(
        "INSERT INTO operations(scope,operation_id,payload,response) VALUES(?1,?2,?3,?4)",
        params![scope, key, payload, result.to_string()],
    )?;
    crate::lifecycle::link_operation(&tx, scope, key, payload, &result.to_string())?;
    tx.commit()?;
    Ok(result)
}
