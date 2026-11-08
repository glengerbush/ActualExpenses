use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NewBudget {
    pub id: String,
    pub name: String,
    #[serde(default = "tracking")]
    pub budget_type: String,
}
fn tracking() -> String {
    "tracking".into()
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NewAccount {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub off_budget: bool,
    pub opening_balance: i64,
    pub opening_date: String,
}

use crate::error::Error;
pub const MAX_MONEY: i64 = 9_007_199_254_740_991;
pub fn validate_id(id: &str) -> Result<(), Error> {
    let parsed =
        uuid::Uuid::parse_str(id).map_err(|_| Error::invalid("id must be a canonical UUID"))?;
    if parsed.to_string() != id {
        return Err(Error::invalid("id must be a canonical lowercase UUID"));
    }
    Ok(())
}
fn validate_name(name: &str) -> Result<(), Error> {
    if name.trim().is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
        return Err(Error::invalid(
            "name must contain 1–120 characters and no control characters",
        ));
    }
    Ok(())
}
impl NewBudget {
    pub fn validate(&self) -> Result<(), Error> {
        validate_id(&self.id)?;
        validate_name(&self.name)?;
        if !["tracking", "envelope"].contains(&self.budget_type.as_str()) {
            return Err(Error::invalid("budget_type must be tracking or envelope"));
        }
        Ok(())
    }
}
impl NewAccount {
    pub fn validate(&self) -> Result<(), Error> {
        validate_id(&self.id)?;
        validate_name(&self.name)?;
        if self.notes.chars().count() > 10000 {
            return Err(Error::invalid(
                "notes must contain at most 10000 characters",
            ));
        }
        if !(-MAX_MONEY..=MAX_MONEY).contains(&self.opening_balance) {
            return Err(Error::invalid(
                "opening_balance must be an integer within the safe range ±9007199254740991",
            ));
        }
        let date = chrono::NaiveDate::parse_from_str(&self.opening_date, "%Y-%m-%d")
            .map_err(|_| Error::invalid("opening_date must be a valid YYYY-MM-DD calendar date"))?;
        if self.opening_date.len() != 10
            || self.opening_date.starts_with("0000")
            || date.format("%Y-%m-%d").to_string() != self.opening_date
        {
            return Err(Error::invalid(
                "opening_date must use YYYY-MM-DD, years 0001–9999",
            ));
        }
        Ok(())
    }
}
