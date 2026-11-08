use crate::{
    error::Error,
    models::{MAX_MONEY, validate_date, validate_id, validate_name},
    store,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NewTransaction {
    pub id: String,
    pub account_id: String,
    pub amount: i64,
    pub date: String,
    #[serde(default)]
    pub cleared: bool,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub payee_id: Option<String>,
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub new_payee: Option<NewReference>,
    #[serde(default)]
    pub new_category: Option<NewReference>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NewReference {
    id: String,
    name: String,
    #[serde(default)]
    is_income: Option<bool>,
}
impl NewTransaction {
    pub fn validate(&self) -> Result<(), Error> {
        validate_id(&self.id)?;
        validate_id(&self.account_id)?;
        validate_date(&self.date)?;
        if !(-MAX_MONEY..=MAX_MONEY).contains(&self.amount) {
            return Err(Error::invalid("amount exceeds the safe integer range"));
        }
        if self.notes.chars().count() > 10000 {
            return Err(Error::invalid(
                "notes must contain at most 10000 characters",
            ));
        }
        for id in [&self.payee_id, &self.category_id].into_iter().flatten() {
            validate_id(id)?;
        }
        for reference in [&self.new_payee, &self.new_category].into_iter().flatten() {
            validate_id(&reference.id)?;
            validate_name(&reference.name)?;
        }
        if self
            .new_payee
            .as_ref()
            .is_some_and(|reference| reference.is_income.is_some())
        {
            return Err(Error::invalid("Payees cannot specify is_income"));
        }
        if self.payee_id.is_some() && self.new_payee.is_some()
            || self.category_id.is_some() && self.new_category.is_some()
        {
            return Err(Error::invalid(
                "Select an existing reference or create a new one, not both",
            ));
        }
        Ok(())
    }
}
#[derive(Default, Clone, Serialize)]
pub struct Balances {
    pub total: i64,
    pub cleared: i64,
    pub uncleared: i64,
}
impl Balances {
    fn add(&mut self, amount: i64, cleared: bool) -> Result<(), Error> {
        self.total = checked(self.total as i128 + amount as i128)?;
        if cleared {
            self.cleared = checked(self.cleared as i128 + amount as i128)?;
        } else {
            self.uncleared = checked(self.uncleared as i128 + amount as i128)?;
        }
        Ok(())
    }
}
fn checked(value: i128) -> Result<i64, Error> {
    if value < -(MAX_MONEY as i128) || value > MAX_MONEY as i128 {
        return Err(Error::invalid(
            "This change would put a ledger balance outside the exact integer range",
        ));
    }
    Ok(value as i64)
}
pub fn account_exists(db: &Connection, budget: &str, account: &str) -> Result<(), Error> {
    store::require_budget(db, budget)?;
    if !db.query_row(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE budget_id=?1 AND id=?2)",
        params![budget, account],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Error::missing());
    }
    Ok(())
}
fn validate_references(db: &Connection, budget: &str, input: &NewTransaction) -> Result<(), Error> {
    account_exists(db, budget, &input.account_id)?;
    for (table, id) in [
        ("payees", &input.payee_id),
        ("categories", &input.category_id),
    ] {
        if let Some(id) = id {
            if !db.query_row(
                &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE budget_id=?1 AND id=?2)"),
                params![budget, id],
                |r| r.get::<_, bool>(0),
            )? {
                return Err(Error::invalid(format!(
                    "Unknown {table} reference in this budget"
                )));
            }
        }
    }
    if input.category_id.is_some()
        && db.query_row(
            "SELECT off_budget FROM accounts WHERE budget_id=?1 AND id=?2",
            params![budget, input.account_id],
            |r| r.get::<_, bool>(0),
        )?
    {
        return Err(Error::invalid(
            "Off-budget transactions cannot have a category",
        ));
    }
    Ok(())
}
fn cached_balances(db: &Connection, budget: &str, account: &str) -> Result<Balances, Error> {
    Ok(db.query_row("SELECT total,cleared,uncleared FROM ledger_totals WHERE budget_id=?1 AND account_id=?2",params![budget,account],|r|Ok(Balances{total:r.get(0)?,cleared:r.get(1)?,uncleared:r.get(2)?})).optional()?.unwrap_or_default())
}
fn store_balances(
    db: &Connection,
    budget: &str,
    account: &str,
    balances: &Balances,
) -> Result<(), Error> {
    db.execute("INSERT INTO ledger_totals(budget_id,account_id,total,cleared,uncleared) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(budget_id,account_id) DO UPDATE SET total=excluded.total,cleared=excluded.cleared,uncleared=excluded.uncleared",params![budget,account,balances.total,balances.cleared,balances.uncleared])?;
    Ok(())
}
fn budget_balances(db: &Connection, budget: &str) -> Result<Balances, Error> {
    let mut values = [0_i128; 3];
    let mut query =
        db.prepare("SELECT total,cleared,uncleared FROM ledger_totals WHERE budget_id=?1")?;
    for row in query.query_map([budget], |r| {
        Ok([
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
        ])
    })? {
        for (sum, value) in values.iter_mut().zip(row?) {
            *sum += value as i128;
        }
    }
    Ok(Balances {
        total: checked(values[0])?,
        cleared: checked(values[1])?,
        uncleared: checked(values[2])?,
    })
}
pub fn refresh_account(db: &Connection, budget: &str, account: &str) -> Result<(), Error> {
    let mut balances = Balances::default();
    let mut query=db.prepare("SELECT amount,cleared FROM transactions WHERE budget_id=?1 AND account_id=?2 AND tombstone=0 ORDER BY date,sort_order,id")?;
    for row in query.query_map(params![budget, account], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, bool>(1)?))
    })? {
        let (amount, cleared) = row?;
        balances.add(amount, cleared)?;
    }
    store_balances(db, budget, account, &balances)
}
pub fn validate_budget_total(db: &Connection, budget: &str) -> Result<(), Error> {
    budget_balances(db, budget)?;
    Ok(())
}
fn apply_addition(db: &Connection, budget: &str, input: &NewTransaction) -> Result<(), Error> {
    let latest:Option<String>=db.query_row("SELECT date FROM transactions WHERE budget_id=?1 AND account_id=?2 AND tombstone=0 AND id<>?3 ORDER BY date DESC,sort_order DESC,id DESC LIMIT 1",params![budget,input.account_id,input.id],|r|r.get(0)).optional()?;
    if latest.as_ref().is_none_or(|date| date <= &input.date) {
        let mut balances = cached_balances(db, budget, &input.account_id)?;
        balances.add(input.amount, input.cleared)?;
        store_balances(db, budget, &input.account_id, &balances)?;
    } else {
        refresh_account(db, budget, &input.account_id)?;
    }
    validate_budget_total(db, budget)
}
pub fn create(db: &Connection, budget: &str, mut input: NewTransaction) -> Result<Value, Error> {
    resolve_new_references(db, budget, &mut input)?;
    validate_references(db, budget, &input)?;
    if db.query_row(
        "SELECT EXISTS(SELECT 1 FROM transactions WHERE budget_id=?1 AND id=?2)",
        params![budget, input.id],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Error::conflict(
            "Transaction ID already exists, including deleted entries",
        ));
    }
    db.execute("INSERT INTO transactions(budget_id,id,account_id,amount,date,cleared,notes,payee_id,category_id,sort_order) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,(SELECT COALESCE(MAX(sort_order),0)+1 FROM transactions WHERE budget_id=?1))",params![budget,input.id,input.account_id,input.amount,input.date,input.cleared,input.notes,input.payee_id,input.category_id])?;
    apply_addition(db, budget, &input)?;
    get(db, budget, &input.id)
}
const SELECT: &str = "SELECT t.id,t.account_id,t.amount,t.date,t.cleared,t.reconciled,t.starting_balance,t.payee_id,p.name,t.category_id,c.name,c.is_income,t.notes,a.name FROM transactions t JOIN accounts a ON a.budget_id=t.budget_id AND a.id=t.account_id LEFT JOIN payees p ON p.budget_id=t.budget_id AND p.id=t.payee_id LEFT JOIN categories c ON c.budget_id=t.budget_id AND c.id=t.category_id";
fn row(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    Ok(
        json!({"id":r.get::<_,String>(0)?,"account_id":r.get::<_,String>(1)?,"amount":r.get::<_,i64>(2)?,"date":r.get::<_,String>(3)?,"cleared":r.get::<_,bool>(4)?,"reconciled":r.get::<_,bool>(5)?,"starting_balance":r.get::<_,bool>(6)?,"payee_id":r.get::<_,Option<String>>(7)?,"payee_name":r.get::<_,Option<String>>(8)?,"category_id":r.get::<_,Option<String>>(9)?,"category_name":r.get::<_,Option<String>>(10)?,"category_is_income":r.get::<_,Option<bool>>(11)?,"notes":r.get::<_,String>(12)?,"account_name":r.get::<_,String>(13)?}),
    )
}
pub fn get(db: &Connection, budget: &str, id: &str) -> Result<Value, Error> {
    db.query_row(
        &format!("{SELECT} WHERE t.budget_id=?1 AND t.id=?2 AND t.tombstone=0"),
        params![budget, id],
        row,
    )
    .optional()?
    .ok_or_else(Error::missing)
}
pub fn list(
    db: &Connection,
    budget: &str,
    account: Option<&str>,
    as_of: Option<&str>,
    limit: u32,
    offset: u32,
) -> Result<Value, Error> {
    store::require_budget(db, budget)?;
    if let Some(account) = account {
        account_exists(db, budget, account)?;
    }
    let summary = if as_of.is_none() {
        match account {
            Some(account) => cached_balances(db, budget, account)?,
            None => budget_balances(db, budget)?,
        }
    } else {
        let mut sums = [0_i128; 3];
        let mut statement=db.prepare("SELECT amount,cleared FROM transactions WHERE budget_id=?1 AND tombstone=0 AND (?2 IS NULL OR account_id=?2) AND date<=?3")?;
        for row in statement.query_map(params![budget, account, as_of], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, bool>(1)?))
        })? {
            let (amount, cleared) = row?;
            sums[0] += amount as i128;
            sums[if cleared { 1 } else { 2 }] += amount as i128;
        }
        Balances {
            total: checked(sums[0])?,
            cleared: checked(sums[1])?,
            uncleared: checked(sums[2])?,
        }
    };
    let account_filter = if account.is_some() {
        " AND t.account_id=?2"
    } else {
        ""
    };
    let date_filter = if as_of.is_some() {
        " AND t.date<=?3"
    } else {
        ""
    };
    let filter = format!("WHERE t.budget_id=?1 AND t.tombstone=0{account_filter}{date_filter}");
    // Keep parameter slots stable while allowing SQLite to use the account/date indexes.
    let filter = format!("{filter} AND (?2 IS NULL OR 1=1) AND (?3 IS NULL OR 1=1)");
    let sql = format!(
        "{SELECT} {filter} ORDER BY t.date DESC,t.sort_order DESC,t.id DESC LIMIT ?4 OFFSET ?5"
    );
    let mut statement = db.prepare(&sql)?;
    let rows = statement.query_map(params![budget, account, as_of, limit + 1, offset], row)?;
    let mut rows = rows.collect::<Result<Vec<_>, _>>()?;
    let more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let mut running = summary.total as i128;
    if account.is_some() && offset > 0 {
        let mut preceding=db.prepare(&format!("SELECT t.amount FROM transactions t {filter} ORDER BY t.date DESC,t.sort_order DESC,t.id DESC LIMIT ?4"))?;
        for amount in preceding.query_map(params![budget, account, as_of, offset], |r| {
            r.get::<_, i64>(0)
        })? {
            running -= amount? as i128;
        }
    }
    for entry in &mut rows {
        entry["running_balance"] = if account.is_some() {
            json!(checked(running)?)
        } else {
            Value::Null
        };
        if account.is_some() {
            running -= entry["amount"].as_i64().unwrap() as i128;
        }
    }

    Ok(
        json!({"data":rows,"balances":summary,"as_of":as_of,"next_offset":if more {Some(offset+limit)} else {None}}),
    )
}

pub fn update(db: &Connection, budget: &str, id: &str, patch: Value) -> Result<Value, Error> {
    let previous = get(db, budget, id)?;
    if previous["reconciled"] == true {
        return Err(Error::conflict(
            "Unlock the reconciled transaction before editing",
        ));
    }
    let fields = patch
        .as_object()
        .ok_or_else(|| Error::invalid("Expected a transaction field object"))?;
    if fields.is_empty() {
        return Err(Error::invalid("Provide at least one field to edit"));
    }
    let mut input = json!({"id":id,"account_id":previous["account_id"],"amount":previous["amount"],"date":previous["date"],"cleared":previous["cleared"],"notes":previous["notes"],"payee_id":previous["payee_id"],"category_id":previous["category_id"]});
    for (field, value) in fields {
        if ![
            "account_id",
            "amount",
            "date",
            "cleared",
            "notes",
            "payee_id",
            "category_id",
            "new_payee",
            "new_category",
        ]
        .contains(&field.as_str())
        {
            return Err(Error::invalid(format!("Cannot edit field {field}")));
        }
        input[field] = value.clone();
        if field == "new_payee" {
            input["payee_id"] = Value::Null;
        }
        if field == "new_category" {
            input["category_id"] = Value::Null;
        }
    }
    let mut input: NewTransaction = serde_json::from_value(input)
        .map_err(|_| Error::invalid("Invalid transaction field value"))?;
    input.validate()?;
    resolve_new_references(db, budget, &mut input)?;
    validate_references(db, budget, &input)?;
    db.execute("UPDATE transactions SET account_id=?3,amount=?4,date=?5,cleared=?6,notes=?7,payee_id=?8,category_id=?9 WHERE budget_id=?1 AND id=?2",params![budget,id,input.account_id,input.amount,input.date,input.cleared,input.notes,input.payee_id,input.category_id])?;
    let financial_changed = previous["account_id"] != input.account_id
        || previous["amount"] != input.amount
        || previous["date"] != input.date
        || previous["cleared"] != input.cleared;
    if financial_changed {
        refresh_account(db, budget, previous["account_id"].as_str().unwrap())?;
        if previous["account_id"] != input.account_id {
            refresh_account(db, budget, &input.account_id)?;
        }
        validate_budget_total(db, budget)?;
    }
    get(db, budget, id)
}
pub fn delete(db: &Connection, budget: &str, id: &str) -> Result<Value, Error> {
    let previous = get(db, budget, id)?;
    if previous["reconciled"] == true {
        return Err(Error::conflict(
            "Unlock the reconciled transaction before deleting",
        ));
    }
    db.execute(
        "UPDATE transactions SET tombstone=1 WHERE budget_id=?1 AND id=?2",
        params![budget, id],
    )?;
    refresh_account(db, budget, previous["account_id"].as_str().unwrap())?;
    validate_budget_total(db, budget)?;
    Ok(json!({"id":id,"deleted":true}))
}
pub fn duplicate(db: &Connection, budget: &str, source: &str, id: &str) -> Result<Value, Error> {
    let previous = get(db, budget, source)?;
    let input:NewTransaction=serde_json::from_value(json!({"id":id,"account_id":previous["account_id"],"amount":previous["amount"],"date":previous["date"],"cleared":false,"notes":previous["notes"],"payee_id":previous["payee_id"],"category_id":previous["category_id"]})).map_err(Error::internal)?;
    input.validate()?;
    create(db, budget, input)
}

fn resolve_new_references(
    db: &Connection,
    budget: &str,
    input: &mut NewTransaction,
) -> Result<(), Error> {
    store::require_budget(db, budget)?;
    for (table, new, selected) in [
        ("payees", &input.new_payee, &mut input.payee_id),
        ("categories", &input.new_category, &mut input.category_id),
    ] {
        if let Some(reference) = new {
            if db.query_row(
                &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE budget_id=?1 AND id=?2)"),
                params![budget, reference.id],
                |r| r.get::<_, bool>(0),
            )? {
                return Err(Error::conflict("The new reference ID already exists"));
            }
            if table == "payees" {
                db.execute(
                    "INSERT INTO payees(budget_id,id,name) VALUES(?1,?2,?3)",
                    params![budget, reference.id, reference.name],
                )?;
            } else {
                db.execute(
                    "INSERT INTO categories(budget_id,id,name,is_income) VALUES(?1,?2,?3,?4)",
                    params![
                        budget,
                        reference.id,
                        reference.name,
                        reference.is_income.unwrap_or(false)
                    ],
                )?;
            }
            *selected = Some(reference.id.clone());
        }
    }
    Ok(())
}
