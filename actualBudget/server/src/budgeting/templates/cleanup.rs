//! Read-only cleanup planning, followed by one receipt-owned atomic commit.
//! Ordering and rounding deliberately follow the pinned source, including its
//! scoped-pool penny over-allocation and tracking/global availability behavior.
use super::*;
use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

struct Work(usize);
impl Work {
    fn tick(&mut self) -> Result<(), Error> {
        self.spend(1)
    }
    fn spend(&mut self, amount: usize) -> Result<(), Error> {
        self.0 = self.0.saturating_add(amount);
        if self.0 > 100_000 {
            return Err(Error::invalid(
                "Cleanup exceeds the bounded work limit. Reduce the number of pool directives.",
            ));
        }
        Ok(())
    }
}

/// One command/read shares this index and work allowance across all categories.
pub(super) struct PoolResolver {
    names: BTreeMap<String, usize>,
    ids: BTreeSet<String>,
    work: Work,
}
impl PoolResolver {
    pub(super) fn new(pools: &[Value]) -> Result<Self, Error> {
        let mut result = Self {
            names: BTreeMap::new(),
            ids: BTreeSet::new(),
            work: Work(0),
        };
        for (i, row) in pools.iter().enumerate() {
            result.work.tick()?;
            result
                .names
                .entry(row["name"].as_str().unwrap().to_lowercase())
                .or_insert(i);
            result.ids.insert(row["id"].as_str().unwrap().to_owned());
        }
        Ok(result)
    }
    fn resolve(&mut self, b: &str, name: &str, pools: &mut Vec<Value>) -> Result<String, Error> {
        self.work.tick()?;
        let name = name.trim();
        crate::models::validate_name(name)?;
        let key = name.to_lowercase();
        if let Some(i) = self.names.get(&key) {
            pools[*i]["deleted"] = json!(false);
            return Ok(pools[*i]["id"].as_str().unwrap().to_owned());
        }
        let hash = Sha256::digest(format!("{b}\0budget-cleanup\0{key}").as_bytes());
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&hash[..16]);
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let id = uuid::Uuid::from_bytes(bytes).to_string();
        if !self.ids.insert(id.clone()) {
            return Err(Error::conflict(
                "Cleanup pool identity conflicts. Rename the pool.",
            ));
        }
        self.names.insert(key, pools.len());
        pools.push(json!({"id":id,"name":name,"deleted":false}));
        Ok(id)
    }
}
pub(super) fn resolve_pool(b: &str, name: &str, pools: &mut Vec<Value>) -> Result<String, Error> {
    PoolResolver::new(pools)?.resolve(b, name, pools)
}

fn sink_weight(text: &str) -> Option<u64> {
    if text.is_empty() {
        return Some(1);
    }
    if !text.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    text.parse::<u64>().ok().map(|n| if n == 0 { 1 } else { n })
}

pub(super) fn notes_with_resolver(
    b: &str,
    text: &str,
    pools: &mut Vec<Value>,
    resolver: &mut PoolResolver,
) -> Result<Vec<Value>, Error> {
    let mut result = vec![];
    for line in text.lines() {
        resolver.work.tick()?;
        let Some(raw) = line.trim().strip_prefix("#cleanup ") else {
            continue;
        };
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let mut role = "overspend";
        let mut group = Some(raw);
        let mut weight = None;
        if raw == "source" {
            role = "source";
            group = None;
        } else if raw == "sink" {
            role = "sink";
            group = None;
            weight = Some(1);
        } else if let Some(rest) = raw
            .strip_prefix("sink")
            .and_then(|s| sink_weight(s.trim_start_matches(' ')))
        {
            role = "sink";
            group = None;
            weight = Some(rest);
        } else if let Some(name) = raw.strip_suffix(" source") {
            if name.contains(" source") {
                continue;
            }
            role = "source";
            group = Some(name);
        } else if let Some((name, suffix)) = raw.split_once(" sink")
            && let Some(n) = sink_weight(suffix.trim_start_matches(' '))
        {
            role = "sink";
            group = Some(name);
            weight = Some(n);
        }
        // The pinned grammar rejects incomplete source suffixes; other bare
        // names represent an overspending pool, not a global sink.
        if role == "overspend" && raw.contains(" source") {
            continue;
        }
        let pool = group
            .map(|name| resolver.resolve(b, name, pools))
            .transpose()?;
        let mut row = json!({"role":role,"groupId":pool});
        if let Some(weight) = weight {
            row["weight"] = json!(weight);
        }
        result.push(row);
        if result.len() > 256 {
            return Err(Error::invalid(
                "Use at most256 cleanup directives per category",
            ));
        }
    }
    Ok(result)
}

pub(super) fn validate_directives(rows: &[Value], pools: &[Value]) -> Result<(), Error> {
    for row in rows {
        if !matches!(row["role"].as_str(), Some("source" | "sink" | "overspend")) {
            return Err(Error::invalid("Unknown cleanup role"));
        }
        if let Some(id) = row["groupId"].as_str() {
            if !pools.iter().any(|p| p["id"] == id && p["deleted"] != true) {
                return Err(Error::invalid(
                    "Cleanup refers to a missing pool. Choose an existing pool.",
                ));
            }
        } else if !row["groupId"].is_null() || row["role"] == "overspend" {
            return Err(Error::invalid("Overspending cleanup requires a pool"));
        }
        if row["role"] == "sink"
            && row["weight"]
                .as_u64()
                .is_none_or(|n| n == 0 || n > crate::models::MAX_MONEY as u64)
        {
            return Err(Error::invalid(
                "Cleanup sink weight must be a positive exact integer",
            ));
        }
    }
    Ok(())
}

struct Plan {
    rows: Vec<Value>,
    available: i64,
    tracking: bool,
    work: Work,
}
impl Plan {
    fn amount(&self, i: usize) -> i64 {
        self.rows[i]["budgeted"].as_i64().unwrap()
    }
    fn balance(&self, i: usize) -> i64 {
        self.rows[i]["balance"].as_i64().unwrap()
    }
    fn set(&mut self, i: usize, value: i128) -> Result<(), Error> {
        self.work.tick()?;
        let amount = super::super::money(value)?;
        let delta = amount as i128 - self.amount(i) as i128;
        let balance = super::super::money(self.balance(i) as i128 + delta)?;
        if self.rows[i]["is_income"] != true {
            self.available = super::super::money(self.available as i128 - delta)?;
        } else if self.tracking {
            self.available = super::super::money(self.available as i128 + delta)?;
        }
        self.rows[i]["budgeted"] = json!(amount);
        self.rows[i]["balance"] = json!(balance);
        Ok(())
    }
    fn global_available(&self) -> i64 {
        if self.tracking { 0 } else { self.available }
    }
}

pub(super) fn compute(db: &Connection, b: &str, month: &str) -> Result<Value, Error> {
    super::super::month(month)?;
    let before = super::super::view(db, b, month, 1)?;
    let tracking = before["method"] == "tracking";
    let data = &before["months"][month];
    let categories = data["categories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| (row["id"].as_str().unwrap().to_owned(), row.clone()))
        .collect::<BTreeMap<_, _>>();
    // Source cleanup scans insertion order, unlike the budget's display order.
    // Imports and native archives already retain category row order.
    let mut query =
        db.prepare("SELECT id FROM categories WHERE budget_id=?1 AND tombstone=0 ORDER BY rowid")?;
    let order = query
        .query_map([b], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut query = db.prepare(
        "SELECT id,name,tombstone FROM budget_cleanup_groups WHERE budget_id=?1 ORDER BY rowid",
    )?;
    let mut pools=query.query_map([b],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"deleted":r.get::<_,bool>(2)?})))?.collect::<Result<Vec<_>,_>>()?;
    let mut resolver = PoolResolver::new(&pools)?;
    let mut rows = vec![];
    let mut definitions = vec![];
    let mut work = Work(0);
    work.spend(pools.len())?;
    for id in order {
        work.tick()?;
        let Some(category) = categories.get(&id) else {
            continue;
        };
        let config = storage::config(db, b, &id)?;
        let cleanup = if config["template_settings"]["source"] == "ui" {
            config["cleanup_def"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        } else {
            notes_with_resolver(
                b,
                category["notes"].as_str().unwrap_or(""),
                &mut pools,
                &mut resolver,
            )?
        };
        definitions.push(json!({"id":id,"cleanup":cleanup}));
        // Source checks zero_budgets carryover even when tracking is selected.
        let rollover:bool=db.query_row("SELECT carryover FROM budget_cells WHERE budget_id=?1 AND category_id=?2 AND month=?3 AND method='envelope' AND tombstone=0",params![b,id,month],|r|r.get(0)).optional()?.unwrap_or(false);
        rows.push(json!({"id":id,"name":category["name"],"is_income":category["is_income"],"budgeted":category["budgeted"],"before_budgeted":category["budgeted"],"balance":category["balance"],"goal":category["goal"].as_str().and_then(|s|serde_json::from_str::<Value>(s).ok()),"long_goal":category["long_goal"].as_str().and_then(|s|serde_json::from_str::<Value>(s).ok()),"notes":category["notes"],"cleanup":cleanup,"rollover":rollover,"templates":config["goal_def"].as_array().cloned().unwrap_or_default(),"applies":false,"clear_goal":false,"per_template":[]}));
    }
    work.spend(resolver.work.0)?;
    for row in &rows {
        work.spend(
            row["cleanup"]
                .as_array()
                .unwrap()
                .len()
                .saturating_mul(pools.len().saturating_add(1)),
        )?;
        validate_directives(row["cleanup"].as_array().unwrap(), &pools)?;
    }
    let mut plan = Plan {
        rows,
        available: data["summary"][if tracking { "total_saved" } else { "to_budget" }]
            .as_i64()
            .unwrap(),
        tracking,
        work,
    };
    let mut group_order = vec![];
    let mut seen = BTreeSet::new();
    for row in &plan.rows {
        for directive in row["cleanup"].as_array().unwrap() {
            if directive["role"] == "source"
                && let Some(id) = directive["groupId"].as_str()
                && seen.insert(id.to_owned())
            {
                group_order.push(id.to_owned());
            }
        }
    }
    let mut warnings = vec![];
    for group in group_order {
        let mut sources = vec![];
        let mut sinks = vec![];
        let mut overspends = vec![];
        for (i, row) in plan.rows.iter().enumerate() {
            for directive in row["cleanup"].as_array().unwrap() {
                plan.work.tick()?;
                if directive["groupId"] == group {
                    match directive["role"].as_str().unwrap() {
                        "source" => sources.push(i),
                        "sink" => sinks.push((i, directive["weight"].as_u64().unwrap())),
                        "overspend" => overspends.push(i),
                        _ => unreachable!(),
                    }
                }
            }
        }
        if sinks.is_empty() && overspends.is_empty() {
            let name = pools.iter().find(|p| p["id"] == group).unwrap()["name"]
                .as_str()
                .unwrap();
            warnings.push(format!(
                "Cleanup group \"{name}\" has no matching sink categories."
            ));
            continue;
        }
        let mut available = 0i64;
        for i in sources {
            let balance = plan.balance(i);
            if balance < 0 {
                warnings.push(format!(
                    "{} does not have available funds.",
                    plan.rows[i]["name"].as_str().unwrap()
                ));
                continue;
            }
            plan.set(i, plan.amount(i) as i128 - balance as i128)?;
            available = super::super::money(available as i128 + balance as i128)?;
        }
        for i in overspends {
            plan.work.tick()?;
            if available <= 0 {
                break;
            }
            let balance = plan.balance(i);
            if balance < 0 && plan.rows[i]["rollover"] != true {
                let allocation = (-balance).min(available);
                plan.set(i, plan.amount(i) as i128 + allocation as i128)?;
                available -= allocation;
            }
        }
        let total = sinks.iter().map(|(_, weight)| *weight as f64).sum::<f64>();
        for (i, weight) in sinks {
            plan.work.tick()?;
            if available <= 0 {
                break;
            }
            let allocation = engine::round(weight as f64 / total * available as f64)?;
            plan.set(i, plan.amount(i) as i128 + allocation as i128)?;
        }
    }
    let mut source_count = 0;
    let mut sinks = vec![];
    for i in 0..plan.rows.len() {
        plan.work.tick()?;
        let defs = plan.rows[i]["cleanup"].as_array().unwrap();
        let source = defs
            .iter()
            .any(|d| d["role"] == "source" && d["groupId"].is_null());
        let weight = defs
            .iter()
            .find(|d| d["role"] == "sink" && d["groupId"].is_null())
            .map(|d| d["weight"].as_u64().unwrap());
        if source {
            let balance = plan.balance(i);
            if balance >= 0 {
                let value = plan.amount(i) as i128 - balance as i128;
                plan.set(i, value)?;
                plan.rows[i]["goal"] = json!(plan.amount(i));
                plan.rows[i]["long_goal"] = json!(0);
                source_count += 1;
            } else {
                warnings.push(format!(
                    "{} does not have available funds.",
                    plan.rows[i]["name"].as_str().unwrap()
                ));
            }
        }
        if let Some(weight) = weight {
            sinks.push((i, weight));
        }
    }
    for i in 0..plan.rows.len() {
        plan.work.tick()?;
        let balance = plan.balance(i);
        if balance < 0 && plan.rows[i]["is_income"] != true && plan.rows[i]["rollover"] != true {
            let allocation = (-balance).min(plan.global_available());
            plan.set(i, plan.amount(i) as i128 + allocation as i128)?;
        }
    }
    let available = plan.global_available();
    if available < 0 {
        warnings.push("Global: No funds are available to reallocate.".to_owned());
    }
    let total = sinks.iter().map(|(_, weight)| *weight as f64).sum::<f64>();
    let sink_count = sinks.len();
    for (index, (i, weight)) in sinks.into_iter().enumerate() {
        plan.work.tick()?;
        let mut value = plan.amount(i) as i128
            + engine::round(weight as f64 / total * available as f64)? as i128;
        // Source compares the complete new allocation against remaining funds.
        if index == sink_count - 1 && value > plan.global_available() as i128 {
            value = plan.amount(i) as i128 + plan.global_available() as i128;
        }
        plan.set(i, value)?;
    }
    let notification = if !warnings.is_empty() {
        json!({"type":"warning","message":"cleanup-no-funds","pre":warnings.join("\n\n")})
    } else if source_count == 0 || available == 0 {
        json!({"type":"message","message":"cleanup-up-to-date"})
    } else {
        json!({"type":"message","message":"cleanup-applied","sourceCount":source_count,"sinkCount":sink_count})
    };
    for row in &mut plan.rows {
        let original = &categories[row["id"].as_str().unwrap()];
        let goal = original["goal"]
            .as_str()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .unwrap_or(Value::Null);
        let long_goal = original["long_goal"]
            .as_str()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .unwrap_or(Value::Null);
        row["applies"] = json!(
            row["budgeted"] != original["budgeted"]
                || row["goal"] != goal
                || row["long_goal"] != long_goal
        );
    }
    let referenced = plan
        .rows
        .iter()
        .flat_map(|r| r["cleanup"].as_array().unwrap())
        .filter_map(|d| d["groupId"].as_str())
        .collect::<BTreeSet<_>>();
    for pool in &mut pools {
        pool["deleted"] = json!(!referenced.contains(pool["id"].as_str().unwrap()));
    }
    Ok(
        json!({"month":month,"method":before["method"],"categories":plan.rows,"errors":[],"notification":notification,"available_after":plan.available,"snapshot":before["snapshot"],"epoch":before["epoch"],"cleanup_definitions":definitions,"pools":pools}),
    )
}

pub(super) fn persist_definitions(
    db: &Connection,
    b: &str,
    plan: &Value,
    clock: &str,
) -> Result<(), Error> {
    for row in plan["cleanup_definitions"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        let old = storage::config(db, b, id)?;
        let cleanup = if row["cleanup"].as_array().unwrap().is_empty() {
            Value::Null
        } else {
            row["cleanup"].clone()
        };
        if old["cleanup_def"] == cleanup {
            continue;
        }
        for field in ["cleanup_def", "tombstone"] {
            super::super::actions::guard_field(db, b, "budget_automations", id, field, clock)?;
        }
        db.execute("INSERT INTO budget_automations(budget_id,id,cleanup_def) VALUES(?1,?2,?3) ON CONFLICT(budget_id,id) DO UPDATE SET cleanup_def=excluded.cleanup_def,tombstone=0",params![b,id,if cleanup.is_null(){None}else{Some(cleanup.to_string())}])?;
        for field in ["cleanup_def", "tombstone"] {
            crate::concurrency::seed_field_version(db, b, "budget_automations", id, field, clock)?;
        }
        crate::concurrency::mark(db, b, "budget_automations", id)?;
    }
    for pool in plan["pools"].as_array().unwrap() {
        let id = pool["id"].as_str().unwrap();
        let old: Option<(String, bool)> = db
            .query_row(
                "SELECT name,tombstone FROM budget_cleanup_groups WHERE budget_id=?1 AND id=?2",
                params![b, id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if old
            .as_ref()
            .is_some_and(|(name, deleted)| *name == pool["name"] && *deleted == pool["deleted"])
        {
            continue;
        }
        for field in ["name", "tombstone"] {
            super::super::actions::guard_field(db, b, "budget_cleanup_groups", id, field, clock)?;
        }
        db.execute("INSERT INTO budget_cleanup_groups(budget_id,id,name,tombstone) VALUES(?1,?2,?3,?4) ON CONFLICT(budget_id,id) DO UPDATE SET name=excluded.name,tombstone=excluded.tombstone",params![b,id,pool["name"].as_str(),pool["deleted"].as_bool()])?;
        for field in ["name", "tombstone"] {
            crate::concurrency::seed_field_version(
                db,
                b,
                "budget_cleanup_groups",
                id,
                field,
                clock,
            )?;
        }
        crate::concurrency::mark(db, b, "budget_cleanup_groups", id)?;
    }
    Ok(())
}
