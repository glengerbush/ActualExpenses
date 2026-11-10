use super::calculators::{self, Environment};
use super::*;
use std::collections::BTreeSet;
pub(super) struct Context {
    pub row: Value,
    pub templates: Vec<Value>,
    pub from: f64,
    pub old: f64,
    pub amount: f64,
    pub full: Option<f64>,
    pub goal: Option<f64>,
    pub long: bool,
    pub priorities: BTreeSet<i64>,
    pub weight: f64,
    pub limit: Option<f64>,
    pub met: bool,
    pub excess: f64,
    pub contributions: Vec<f64>,
}
impl Context {
    pub fn init(row: Value, templates: Vec<Value>, env: &mut Environment) -> Result<Self, Error> {
        let id = row["id"].as_str().unwrap();
        let previous = env.category(env.number - 1, id)?;
        let mut from = previous["balance"].as_f64().unwrap();
        let carry = previous["carryover"] == true;
        if from < 0.0 && !carry || row["is_income"] == true || env.method == "tracking" && !carry {
            from = 0.0;
        }
        let mut priorities = BTreeSet::new();
        let mut weight = 0.0;
        let mut goals = 0;
        let mut spends = 0;
        let mut joint_priority = None;
        for t in &templates {
            env.tick()?;
            let kind = t["type"]
                .as_str()
                .ok_or_else(|| Error::invalid("Template type is required"))?;
            if kind == "error" {
                return Err(Error::invalid(
                    t["error"].as_str().unwrap_or("Invalid template directive"),
                ));
            }
            if ![
                "simple",
                "refill",
                "limit",
                "by",
                "spend",
                "periodic",
                "percentage",
                "schedule",
                "average",
                "copy",
                "remainder",
                "goal",
            ]
            .contains(&kind)
            {
                return Err(Error::invalid(
                    "Unrecognized template type; retain this configuration and review it",
                ));
            }
            if kind == "goal" {
                goals += 1;
            }
            if kind == "spend" {
                spends += 1;
            }
            if matches!(kind, "by" | "spend") {
                calculators::target_month(t, env.number)?;
            }
            if matches!(kind, "by" | "schedule") {
                let p = t["priority"]
                    .as_i64()
                    .ok_or_else(|| Error::invalid("Template priority is required"))?;
                if joint_priority.is_some_and(|previous| previous != p) {
                    return Err(Error::invalid(
                        "Schedule and By templates must be the same priority level",
                    ));
                }
                joint_priority = Some(p);
            }
            if kind == "percentage" {
                let source = t["category"]
                    .as_str()
                    .ok_or_else(|| Error::invalid("Percentage source is required"))?;
                let known = matches!(
                    source.to_lowercase().as_str(),
                    "all income" | "available funds"
                ) || env.data(env.number)?["categories"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| {
                        c["is_income"] == true
                            && (c["id"] == source
                                || c["name"].as_str().unwrap_or("").to_lowercase()
                                    == source.to_lowercase())
                    });
                if !known {
                    return Err(Error::invalid(format!(
                        "Category \"{source}\" is not found in available income categories"
                    )));
                }
            }
            if kind == "schedule" {
                schedule::validate(env, t)?;
            }
            if kind == "remainder" {
                let w = Environment::value(t, "weight")?;
                if w <= 0.0 {
                    return Err(Error::invalid("Remainder weight must be positive"));
                }
                weight += w;
            } else if t["directive"] == "template" && kind != "limit" {
                let p = t["priority"]
                    .as_i64()
                    .filter(|n| *n >= 0 && *n <= i32::MAX as i64)
                    .ok_or_else(|| Error::invalid("Priority must be a nonnegative integer"))?;
                priorities.insert(p);
            }
        }
        if goals > 1 {
            return Err(Error::invalid("Only one #goal is allowed per category"));
        }
        if spends > 1 {
            return Err(Error::invalid(
                "Only one spend template is allowed per category",
            ));
        }
        let mut limit = None;
        let mut hold = false;
        for t in &templates {
            let cap = if t["type"] == "limit" {
                t
            } else if matches!(
                t["type"].as_str(),
                Some("simple" | "periodic" | "remainder")
            ) {
                &t["limit"]
            } else {
                continue;
            };
            if cap.is_null() {
                continue;
            }
            if limit.is_some() {
                return Err(Error::invalid("Only one up to is allowed per category"));
            }
            let amount = env.money(Environment::value(cap, "amount")?)?;
            let amount = match cap["period"].as_str() {
                Some("monthly") => amount,
                Some("daily") => {
                    let a =
                        chrono::NaiveDate::parse_from_str(&format!("{}-01", env.month), "%Y-%m-%d")
                            .map_err(Error::internal)?;
                    let end = chrono::NaiveDate::parse_from_str(
                        &format!("{}-01", super::super::month_name(env.number + 1)?),
                        "%Y-%m-%d",
                    )
                    .map_err(Error::internal)?;
                    amount * (end - a).num_days() as f64
                }
                Some("weekly") => {
                    let start = cap["start"]
                        .as_str()
                        .ok_or_else(|| Error::invalid("Weekly limit requires a start date"))?;
                    crate::models::validate_date(start)?;
                    let start = chrono::NaiveDate::parse_from_str(start, "%Y-%m-%d")
                        .map_err(Error::internal)?;
                    let first =
                        chrono::NaiveDate::parse_from_str(&format!("{}-01", env.month), "%Y-%m-%d")
                            .map_err(Error::internal)?;
                    let end = chrono::NaiveDate::parse_from_str(
                        &format!("{}-01", super::super::month_name(env.number + 1)?),
                        "%Y-%m-%d",
                    )
                    .map_err(Error::internal)?;
                    let offset = (first - start).num_days().max(0);
                    let first_occurrence = start + chrono::TimeDelta::days(((offset + 6) / 7) * 7);
                    let count = if first_occurrence >= end {
                        0
                    } else {
                        ((end - first_occurrence).num_days() - 1) / 7 + 1
                    };
                    amount * count as f64
                }
                _ => {
                    return Err(Error::invalid(
                        "Invalid limit period. Check template syntax",
                    ));
                }
            };
            limit = Some(amount);
            hold = cap["hold"] == true;
        }
        let old = row["budgeted"].as_f64().unwrap();
        let met = limit.is_some_and(|cap| from >= cap);
        let excess = if met && !hold {
            from - limit.unwrap()
        } else {
            0.0
        };
        let full = if met { Some(-excess) } else { None };
        let contributions = vec![0.0; templates.len()];
        Ok(Self {
            row,
            templates,
            from,
            old,
            amount: -excess,
            full,
            goal: None,
            long: false,
            priorities,
            weight,
            limit,
            met,
            excess,
            contributions,
        })
    }
    pub fn goal_only(&self) -> bool {
        self.templates.iter().any(|t| t["type"] == "goal")
            && self.templates.iter().all(|t| t["type"] == "goal")
    }
    pub fn priority(
        &mut self,
        p: i64,
        available: f64,
        start: f64,
        projection: bool,
        env: &mut Environment,
    ) -> Result<f64, Error> {
        if self.met || !self.priorities.contains(&p) {
            return Ok(0.0);
        }
        let id = self.row["id"].as_str().unwrap();
        let mut amount = 0.0;
        let mut rest = available;
        let mut shares = vec![0.0; self.templates.len()];
        let mut ran_by = false;
        let mut ran_schedule = false;
        for (index, t) in self.templates.iter().enumerate() {
            if t["directive"] != "template"
                || t["priority"].as_i64() != Some(p)
                || t["type"] == "remainder"
            {
                continue;
            }
            env.tick()?;
            let request = match t["type"].as_str().unwrap() {
                "simple" => {
                    if t["monthly"].is_null() {
                        self.limit
                            .ok_or_else(|| Error::invalid("Refill requires a limit"))?
                            - self.from
                    } else {
                        env.money(Environment::value(t, "monthly")?)?
                    }
                }
                "refill" => {
                    self.limit
                        .ok_or_else(|| Error::invalid("Refill requires a limit"))?
                        - self.from
                }
                "copy" => {
                    let n = t["lookBack"]
                        .as_i64()
                        .filter(|n| *n > 0 && *n <= 1200)
                        .ok_or_else(|| Error::invalid("Copy history must be1–1200 months"))?;
                    env.category(env.number - n as i32, id)?["budgeted"]
                        .as_f64()
                        .unwrap()
                }
                "periodic" => calculators::period(t, env)?,
                "spend" => calculators::spend(t, env, id, self.from)?,
                "average" => {
                    let months = t["numMonths"]
                        .as_i64()
                        .and_then(|n| i32::try_from(n).ok())
                        .ok_or_else(|| Error::invalid("Average history must be an integer"))?;
                    let average = env.average(id, months)?.abs();
                    engine::round(calculators::adjust(average, t, env)?)? as f64
                }
                "percentage" => {
                    let source = t["category"].as_str().unwrap();
                    let month = env.number - if t["previous"] == true { 1 } else { 0 };
                    let income = if source.eq_ignore_ascii_case("available funds") {
                        start
                    } else if source.eq_ignore_ascii_case("all income") {
                        env.data(month)?["summary"]["total_income"]
                            .as_f64()
                            .unwrap()
                    } else {
                        let category = env.data(month)?["categories"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|c| {
                                c["is_income"] == true
                                    && (c["id"] == source
                                        || c["name"].as_str().unwrap_or("").to_lowercase()
                                            == source.to_lowercase())
                            })
                            .ok_or_else(|| Error::invalid("Income category not found"))?;
                        category["actual"].as_f64().unwrap()
                    };
                    (engine::round(income * (Environment::value(t, "percent")? / 100.0))? as f64)
                        .max(0.0)
                }
                "by" => {
                    if ran_by {
                        0.0
                    } else {
                        ran_by = true;
                        let siblings = self
                            .templates
                            .iter()
                            .enumerate()
                            .filter(|(_, t)| t["type"] == "by")
                            .map(|(i, t)| (i, t.clone()))
                            .collect::<Vec<_>>();
                        let (sum, weights) = calculators::by(
                            &siblings.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>(),
                            env,
                            self.from,
                        )?;
                        distribute(
                            &mut shares,
                            &siblings.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
                            &weights,
                            sum,
                        )?;
                        sum
                    }
                }
                "schedule" => {
                    if ran_schedule {
                        0.0
                    } else {
                        ran_schedule = true;
                        let siblings = self
                            .templates
                            .iter()
                            .enumerate()
                            .filter(|(_, t)| {
                                t["type"] == "schedule" && t["priority"].as_i64() == Some(p)
                            })
                            .map(|(i, t)| (i, t.clone()))
                            .collect::<Vec<_>>();
                        let (sum, weights) = schedule::run(
                            env,
                            &siblings.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>(),
                            &self.row,
                            self.from,
                            amount,
                        )?;
                        let sum = sum - amount;
                        distribute(
                            &mut shares,
                            &siblings.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
                            &weights,
                            sum,
                        )?;
                        sum
                    }
                }
                "limit" => 0.0,
                _ => return Err(Error::invalid("Invalid allocation template")),
            };
            rest -= request;
            amount += request;
            if !matches!(t["type"].as_str(), Some("by" | "schedule")) {
                shares[index] += request;
            }
        }
        let original = amount;
        let mut scale = 1.0;
        if let Some(limit) = self.limit
            && amount + self.amount + self.from >= limit {
                amount = limit - self.amount - self.from;
                self.met = true;
                rest += original - amount;
                if original > 0.0 {
                    scale *= amount / original;
                }
            }
        if env.hide {
            let before = amount;
            amount = env.fraction(amount)?;
            if before != 0.0 {
                scale *= amount / before;
            }
        }
        self.full = Some(self.full.unwrap_or(0.0) + amount);
        if p > 0 && rest < 0.0 && self.row["is_income"] != true && !projection {
            let clamped = (amount + rest).max(0.0);
            if amount > 0.0 {
                scale *= clamped / amount;
            }
            amount = clamped;
        }
        self.amount += amount;
        let indices = self
            .templates
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                t["directive"] == "template"
                    && t["priority"].as_i64() == Some(p)
                    && t["type"] != "remainder"
                    && t["type"] != "limit"
            })
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let mut remaining = amount.max(0.0);
        for (index, i) in indices.iter().enumerate() {
            let share = if index + 1 == indices.len() {
                remaining
            } else {
                (engine::round(shares[*i] * scale.max(0.0))? as f64)
                    .max(0.0)
                    .min(remaining)
            };
            self.contributions[*i] += share;
            remaining -= share;
        }
        Ok(if self.row["is_income"] == true {
            -amount
        } else {
            amount
        })
    }
    pub fn remainder(
        &mut self,
        available: f64,
        unit: f64,
        env: &Environment,
    ) -> Result<f64, Error> {
        let mut amount = engine::round(self.weight * unit)? as f64;
        let smallest = if env.hide {
            amount = env.fraction(amount)?;
            100.0
        } else {
            1.0
        };
        if amount > available || available - amount <= smallest {
            amount = available;
        }
        if let Some(limit) = self.limit
            && amount + self.amount + self.from >= limit {
                amount = limit - self.amount - self.from;
                self.met = true;
            }
        let indices = self
            .templates
            .iter()
            .enumerate()
            .filter(|(_, t)| t["type"] == "remainder")
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if amount > 0.0 && self.weight > 0.0 {
            let weights = indices
                .iter()
                .map(|i| self.templates[*i]["weight"].as_f64().unwrap())
                .collect::<Vec<_>>();
            let mut shares = vec![0.0; self.templates.len()];
            distribute(&mut shares, &indices, &weights, amount)?;
            for (i, share) in shares.into_iter().enumerate() {
                self.contributions[i] += share;
            }
        }
        self.amount += amount;
        Ok(amount)
    }
    pub fn finish(&mut self, env: &Environment) -> Result<(), Error> {
        if let Some(goal) = self.templates.iter().find(|t| t["type"] == "goal") {
            if self.goal_only() {
                self.amount = self.old;
            }
            self.long = true;
            self.goal = Some(env.money(Environment::value(goal, "amount")?)?);
        } else {
            self.goal = self.full;
        }
        engine::round(self.amount)?;
        if let Some(n) = self.goal {
            engine::round(n)?;
        }
        Ok(())
    }
}
fn distribute(
    shares: &mut [f64],
    indices: &[usize],
    weights: &[f64],
    total: f64,
) -> Result<(), Error> {
    if indices.is_empty() {
        return Ok(());
    }
    let weight = weights.iter().map(|n| n.max(0.0)).sum::<f64>();
    let mut remaining = total;
    for (index, i) in indices.iter().enumerate() {
        let share = if index + 1 == indices.len() {
            remaining
        } else if weight > 0.0 {
            engine::round(total * weights[index].max(0.0) / weight)? as f64
        } else {
            engine::round(total / indices.len() as f64)? as f64
        };
        let allocated = share.min(remaining).max(0.0);
        shares[*i] = allocated;
        remaining -= allocated;
    }
    Ok(())
}
