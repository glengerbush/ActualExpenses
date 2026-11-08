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
use tokio::sync::Semaphore;
use uuid::Uuid;

#[derive(Clone)]
pub struct Store {
    db: Arc<Mutex<Connection>>,
    worker: Arc<Semaphore>,
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
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            worker: Arc::new(Semaphore::new(1)),
        })
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
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            operation(&mut *db.lock().map_err(Error::internal)?)
        })
        .await
        .map_err(Error::internal)?
    }
}
pub fn budgets(db: &Connection) -> Result<Value, Error> {
    let mut stmt = db.prepare("SELECT id,name,budget_type FROM budgets ORDER BY name,id")?;
    let rows=stmt.query_map([],|row|Ok(json!({"id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,"budget_type":row.get::<_,String>(2)?})))?;
    Ok(json!(rows.collect::<Result<Vec<_>, _>>()?))
}
pub fn require_budget(db: &Connection, id: &str) -> Result<(), Error> {
    if !db.query_row(
        "SELECT EXISTS(SELECT 1 FROM budgets WHERE id=?1)",
        [id],
        |row| row.get::<_, bool>(0),
    )? {
        return Err(Error::missing());
    }
    Ok(())
}
pub fn accounts(db: &Connection, budget: &str) -> Result<Value, Error> {
    require_budget(db, budget)?;
    let mut stmt=db.prepare("SELECT a.id,a.name,a.notes,a.off_budget,a.closed,COALESCE(t.total,0) FROM accounts a LEFT JOIN ledger_totals t ON t.budget_id=a.budget_id AND t.account_id=a.id WHERE a.budget_id=?1 ORDER BY a.name,a.id")?;
    let rows=stmt.query_map([budget],|row|Ok(json!({"id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,"notes":row.get::<_,String>(2)?,"off_budget":row.get::<_,bool>(3)?,"closed":row.get::<_,bool>(4)?,"balance":row.get::<_,i64>(5)?})))?;
    Ok(json!(rows.collect::<Result<Vec<_>, _>>()?))
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
    tx.execute(
        "INSERT INTO categories(budget_id,id,name,is_income) VALUES(?1,?2,'Starting Balances',1)",
        params![input.id, Uuid::new_v4().to_string()],
    )?;
    Ok(json!({"id":input.id,"name":input.name,"budget_type":input.budget_type}))
}
pub fn create_account(db: &Connection, budget: &str, input: NewAccount) -> Result<Value, Error> {
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
        "INSERT INTO accounts(budget_id,id,name,notes,off_budget) VALUES(?1,?2,?3,?4,?5)",
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
            tx.query_row("SELECT id FROM categories WHERE budget_id=?1 AND is_income=1 ORDER BY name='Starting Balances' DESC,id LIMIT 1",[budget],|r|r.get(0)).optional()?
        };
        tx.execute("INSERT INTO transactions(budget_id,id,account_id,amount,date,cleared,starting_balance,payee_id,category_id) VALUES(?1,?2,?3,?4,?5,1,1,?6,?7)",params![budget,Uuid::new_v4().to_string(),input.id,input.opening_balance,input.opening_date,payee,category])?;
    }
    crate::ledger::refresh_account(db, budget, &input.id)?;
    crate::ledger::validate_budget_total(db, budget)?;
    Ok(
        json!({"id":input.id,"name":input.name,"notes":input.notes,"off_budget":input.off_budget,"closed":false,"balance":input.opening_balance}),
    )
}

pub fn write(
    db: &mut Connection,
    scope: &str,
    key: &str,
    payload: &str,
    operation: impl FnOnce(&Connection) -> Result<Value, Error>,
) -> Result<Value, Error> {
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let existing: Option<(String, String)> = tx
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
        return serde_json::from_str(&response).map_err(Error::internal);
    }
    let result = operation(&tx)?;
    tx.execute(
        "INSERT INTO operations(scope,operation_id,payload,response) VALUES(?1,?2,?3,?4)",
        params![scope, key, payload, result.to_string()],
    )?;
    tx.commit()?;
    Ok(result)
}
