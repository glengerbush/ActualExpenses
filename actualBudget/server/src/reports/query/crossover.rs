//! Bounded source-compatible financial-independence estimates; never financial writes.
use super::*;
use axum::Extension;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
type Cache = Arc<Mutex<super::revision_cache::Cache>>;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u8,
    as_of: String,
    options: Options,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Options {
    start: String,
    end: String,
    expense_category_ids: Vec<String>,
    income_account_ids: Vec<String>,
    safe_withdrawal_rate: f64,
    estimated_return: Option<f64>,
    expected_contribution: Option<i64>,
    projection_type: String,
    expense_adjustment_factor: Option<f64>,
}
pub(crate) fn router() -> Router<Store> {
    Router::new()
        .route("/api/v1/budgets/{budget}/reports/crossover", post(read))
        .layer(Extension(Cache::default()))
}
fn exact(value: i128) -> Result<i64, Error> {
    if value.abs() > crate::models::MAX_MONEY as i128 {
        Err(Error::invalid("Crossover exceeds exact monetary range"))
    } else {
        Ok(value as i64)
    }
}
fn rounded(value: f64) -> Result<i64, Error> {
    if !value.is_finite() || value.abs() > crate::models::MAX_MONEY as f64 {
        return Err(Error::invalid(
            "Crossover projection exceeds finite exact monetary range; reduce growth, contribution or history",
        ));
    }
    // JavaScript Math.round, including negative half ties toward positive infinity.
    let floor = value.floor();
    Ok((floor + if value - floor >= 0.5 { 1.0 } else { 0.0 }) as i64)
}
fn validate(q: &Request) -> Result<(i32, i32), Error> {
    if q.version != 1 {
        return Err(Error::invalid("Unsupported Crossover version"));
    }
    day(&q.as_of)?;
    let a = crate::budgeting::month(&q.options.start)?;
    let z = crate::budgeting::month(&q.options.end)?;
    let o = &q.options;
    if a > z || (!q.options.income_account_ids.is_empty() && z - a + 1 > 5000) {
        return Err(Error::invalid(
            "Crossover requires an ordered range up to5000 historical points",
        ));
    }
    if o.income_account_ids.len() > 1000
        || o.expense_category_ids.len() > 5000
        || (z - a + 1) as usize * o.income_account_ids.len() > 20000
    {
        return Err(Error::invalid(
            "Crossover permits1000 accounts,5000 categories and20000 account/month cells",
        ));
    }
    for id in o.income_account_ids.iter().chain(&o.expense_category_ids) {
        crate::models::validate_id(id)?;
    }
    if !["hampel", "median", "mean"].contains(&o.projection_type.as_str())
        || !o.safe_withdrawal_rate.is_finite()
        || o.safe_withdrawal_rate < 0.0
        || o.estimated_return
            .is_some_and(|v| !v.is_finite() || v < -1.0)
        || o.expense_adjustment_factor
            .is_some_and(|v| !v.is_finite() || v < 0.0)
        || o.expected_contribution
            .is_some_and(|v| (v as i128).abs() > crate::models::MAX_MONEY as i128)
    {
        return Err(Error::invalid(
            "Crossover requires a supported expense method, finite nonnegative withdrawal/expense factors, return at least-100% and exact contribution",
        ));
    }
    if serde_json::to_vec(q).map_err(Error::internal)?.len() > 65536 {
        return Err(Error::invalid("Crossover query exceeds64KiB"));
    }
    Ok((a, z))
}
#[derive(Serialize, Deserialize)]
struct Inputs {
    expenses: Vec<i64>,
    balances: Vec<i64>,
}
fn inputs(db: &Connection, b: &str, o: &Options, a: i32, z: i32) -> Result<Inputs, Error> {
    let count = (z - a + 1) as usize;
    if o.income_account_ids.is_empty() {
        return Ok(Inputs {
            expenses: vec![],
            balances: vec![],
        });
    }
    let start = format!("{}-01", o.start);
    let end = month_end(first(&o.end)?).to_string();
    let accounts = o
        .income_account_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let categories = o
        .expense_category_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut values = vec![
        SqlValue::Text(b.into()),
        SqlValue::Text(end),
        SqlValue::Text(start),
    ];
    let mut ids = |set: &BTreeSet<String>| {
        set.iter()
            .map(|id| {
                values.push(SqlValue::Text(id.clone()));
                format!("?{}", values.len())
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    let account_ids = ids(&accounts);
    let category_ids = ids(&categories);
    let category_clause = if categories.is_empty() {
        "0".into()
    } else {
        format!("(t.is_parent=0 AND t.date>=?3 AND c.id IN ({category_ids}))")
    };
    // Source AQL validates references and aggregates split leaves, including eligible
    // transfer legs. Category selection does not constrain selected-account balances.
    let joins = "FROM transactions t LEFT JOIN accounts a ON a.budget_id=t.budget_id AND a.id=t.account_id AND a.tombstone=0 LEFT JOIN categories c ON c.budget_id=t.budget_id AND c.id=t.category_id AND c.tombstone=0";
    let filter = format!(
        "t.budget_id=?1 AND t.tombstone=0 AND t.is_parent=0 AND t.date<=?2 AND (a.id IN ({account_ids}) OR {category_clause})"
    );
    let n: i64 = db.query_row(
        &format!("SELECT count(*) FROM (SELECT t.id {joins} WHERE {filter} LIMIT 100001)"),
        params_from_iter(values.iter()),
        |r| r.get(0),
    )?;
    if n > 100000 {
        return Err(Error::invalid(
            "Crossover exceeds100000 matching entries; narrow selections or history",
        ));
    }
    let mut expenses = vec![0i128; count];
    let mut account_months = BTreeMap::<String, Vec<i128>>::new();
    for id in &accounts {
        account_months.insert(id.clone(), vec![0; count]);
    }
    let mut statement = db.prepare(&format!(
        "SELECT t.date,t.amount,a.id,c.id {joins} WHERE {filter} ORDER BY t.date,t.id"
    ))?;
    let mut rows = statement.query(params_from_iter(values.iter()))?;
    while let Some(row) = rows.next()? {
        let date: String = row.get(0)?;
        let month = crate::budgeting::month(&date[..7])?;
        let amount = row.get::<_, i64>(1)? as i128;
        let account: Option<String> = row.get(2)?;
        let category: Option<String> = row.get(3)?;
        if month >= a && category.as_ref().is_some_and(|id| categories.contains(id)) {
            expenses[(month - a) as usize] -= amount;
        }
        if let Some(balance) = account.and_then(|id| account_months.get_mut(&id)) {
            balance[(month - a).max(0) as usize] += amount;
        }
    }
    let mut balances = vec![0i128; count];
    // Iterate the source selection, including a repeated account if the saved
    // source array contains one; SQL reads each underlying record only once.
    for id in &o.income_account_ids {
        let mut running = 0;
        for (index, delta) in account_months[id].iter().enumerate() {
            running += delta;
            balances[index] += running;
        }
    }
    Ok(Inputs {
        expenses: expenses.into_iter().map(exact).collect::<Result<_, _>>()?,
        balances: balances.into_iter().map(exact).collect::<Result<_, _>>()?,
    })
}
fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}
fn label(month: i32) -> Result<String, Error> {
    let date = first(&crate::budgeting::month_name(month)?)?;
    Ok(date.format("%b %Y").to_string())
}
fn calculate(q: &Request, input: &Inputs, a: i32, z: i32) -> Result<(Value, Vec<String>), Error> {
    let o = &q.options;
    if input.balances.is_empty() {
        return Ok((
            json!({"graphData":{"data":[],"start":o.start,"end":o.end,"crossoverXLabel":null},"lastKnownBalance":0,"lastKnownMonthlyIncome":0,"lastKnownMonthlyExpenses":0,"historicalReturn":null,"yearsToRetire":null,"targetMonthlyIncome":null,"targetNestEgg":null}),
            vec![],
        ));
    }
    let mut data = vec![];
    let mut months = vec![];
    let monthly_swr = o.safe_withdrawal_rate / 12.0;
    for (index, balance) in input.balances.iter().enumerate() {
        let month = a + index as i32;
        months.push(crate::budgeting::month_name(month)?);
        data.push(json!({"x":label(month)?,"investmentIncome":rounded(*balance as f64*monthly_swr)?,"expenses":input.expenses[index],"nestEgg":balance}));
    }
    let last = *input.balances.last().unwrap();
    let historical = if input.balances.len() >= 2 {
        let start = input
            .balances
            .iter()
            .find(|v| **v != 0)
            .copied()
            .unwrap_or(0);
        if start > 0 && last > 0 {
            let rate =
                (last as f64 / start as f64).powf(1.0 / (input.balances.len() - 1) as f64) - 1.0;
            Some(if rate.is_finite() { rate } else { 0.0 })
        } else {
            Some(0.0)
        }
    } else {
        None
    };
    let monthly_return = o
        .estimated_return
        .map(|rate| (1.0 + rate).powf(1.0 / 12.0) - 1.0)
        .or(historical);
    let expenses = input.expenses.iter().map(|v| *v as f64).collect::<Vec<_>>();
    let flat = match o.projection_type.as_str() {
        "mean" => expenses.iter().sum::<f64>() / expenses.len() as f64,
        "median" => median(&expenses),
        _ => {
            let m = median(&expenses);
            let mad = median(&expenses.iter().map(|v| (v - m).abs()).collect::<Vec<_>>());
            median(
                &expenses
                    .iter()
                    .copied()
                    .filter(|v| *v >= m - 1.4826 * mad * 3.0 && *v <= m + 1.4826 * mad * 3.0)
                    .collect::<Vec<_>>(),
            )
        }
    };
    let projected_expenses = flat.max(0.0);
    let adjusted = projected_expenses * o.expense_adjustment_factor.unwrap_or(1.0);
    let mut balance = last as f64;
    let mut crossover = None;
    let mut target = None;
    for index in 1..=600 {
        let month = z + index;
        balance += o.expected_contribution.unwrap_or(0) as f64;
        if let Some(rate) = monthly_return {
            balance *= 1.0 + rate;
        }
        let income = rounded(balance * monthly_swr)?;
        let expense = rounded(projected_expenses)?;
        let adjusted = rounded(adjusted)?;
        months.push(crate::budgeting::month_name(month)?);
        data.push(json!({"x":label(month)?,"investmentIncome":income,"expenses":expense,"nestEgg":rounded(balance)?,"adjustedExpenses":adjusted,"isProjection":true}));
        if income >= adjusted {
            crossover = Some(month);
            target = Some(adjusted);
            break;
        }
    }
    if target.is_some() && monthly_swr == 0.0 {
        return Err(Error::invalid(
            "A zero withdrawal rate cannot produce a finite target nest egg. Choose a positive withdrawal rate.",
        ));
    }
    let target_egg = target
        .map(|target| rounded(target as f64 / monthly_swr))
        .transpose()?;
    let today = crate::budgeting::month(&q.as_of[..7])?;
    // Source differenceInMonths compares projected month-start with today's
    // date/time. as_of is evaluated at noon UTC for a deterministic date snapshot.
    let years = crossover.map(|month| ((month - today - 1).max(0) as f64) / 12.0);
    let annual = historical.map(|rate| (1.0 + rate).powf(12.0) - 1.0);
    if annual.is_some_and(|v| !v.is_finite()) {
        return Err(Error::invalid("Crossover historical return is nonfinite"));
    }
    Ok((
        json!({"graphData":{"data":data,"start":o.start,"end":o.end,"crossoverXLabel":crossover.map(label).transpose()?},"lastKnownBalance":last,"lastKnownMonthlyIncome":rounded(last as f64*monthly_swr)?,"lastKnownMonthlyExpenses":input.expenses.last(),"historicalReturn":annual,"yearsToRetire":years,"targetMonthlyIncome":target,"targetNestEgg":target_egg}),
        months,
    ))
}
async fn read(
    State(store): State<Store>,
    Path(b): Path<String>,
    Extension(cache): Extension<Cache>,
    input: Result<Json<Request>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    let Json(query) = input.map_err(crate::api::json_error)?;
    let (a, z) = validate(&query)?;
    let value=store.run(move|db|{let tx=db.transaction()?;super::sql_work::run(&tx,||{
        store::require_budget(&tx,&b)?;let epoch=crate::lifecycle::epoch(&tx,&b)?;
        let sequence:i64=tx.query_row("SELECT COALESCE(max(sequence),0) FROM budget_changes WHERE budget_id=?1 AND entity IN('transactions','accounts','categories')",[&b],|r|r.get(0))?;
        let revision=format!("c1:{epoch}:{sequence:016x}");let key=format!("{b}:{revision}:{}",serde_json::to_string(&query).map_err(Error::internal)?);
        let prior=cache.lock().map_err(Error::internal)?.get(&key);let hit=prior.is_some();
        let (result,months)=if let Some(prior)=prior{(prior["result"].clone(),prior["months"].clone())}else{let(result,months)=calculate(&query,&inputs(&tx,&b,&query.options,a,z)?,a,z)?;let months=json!(months);cache.lock().map_err(Error::internal)?.put(key,json!({"result":result,"months":months}));(result,months)};
        Ok(json!({"result":result,"months":months,"epoch":epoch,"snapshot":sync::cursor(&tx,&b)?,"as_of":query.as_of,"recorded":false,"basis":"withdrawal_based_financial_independence_projection","cache":{"hit":hit,"input_revision":revision},"assumptions":[{"code":"estimated_income","message":"Investment income is a withdrawal-rate estimate, not recorded earnings or a guaranteed return. Historical growth includes contributions and balance changes."},{"code":"projection","message":"Monthly contributions precede growth. Expenses use the selected historical method and adjustment. Temporary historical crossings are ignored; projections stop at crossover or600 months."},{"code":"clock","message":"Years to crossover use the as-of date evaluated at noon UTC and complete months, following the source calculation."}]}))
    })}).await?;
    Ok(Json(json!({"data":value})))
}
