use crate::{
    concurrency as sync,
    error::Error,
    ledger,
    register_regex::{self, Pattern},
    splits,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Notes {
    pub mode: String,
    #[serde(default)]
    pub find: String,
    #[serde(default)]
    pub use_regex: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Bulk {
    pub snapshot: String,
    pub action: String,
    pub ids: Vec<String>,
    #[serde(default)]
    pub fields: Option<Value>,
    #[serde(default)]
    pub duplicate_ids: Vec<String>,
    #[serde(default)]
    pub notes: Option<Notes>,
}
impl Bulk {
    pub fn validate(&self) -> Result<(), Error> {
        if self.ids.is_empty()
            || self.ids.len() > 100
            || self.ids.iter().collect::<BTreeSet<_>>().len() != self.ids.len()
        {
            return Err(Error::invalid("Select 1–100 distinct entries"));
        }
        for id in self.ids.iter().chain(&self.duplicate_ids) {
            crate::models::validate_id(id)?;
        }
        if !["update", "duplicate", "delete"].contains(&self.action.as_str())
            || self.action == "update"
                && self
                    .fields
                    .as_ref()
                    .is_none_or(|v| v.as_object().is_none_or(|v| v.is_empty()))
            || self.action != "update" && self.fields.is_some()
            || self.action == "duplicate" && self.duplicate_ids.len() != self.ids.len()
            || self.action != "duplicate" && !self.duplicate_ids.is_empty()
        {
            return Err(Error::invalid(
                "Invalid bulk action fields or duplicate identifiers",
            ));
        }
        if self
            .fields
            .as_ref()
            .is_some_and(|v| v.get("new_payee").is_some() || v.get("new_category").is_some())
        {
            return Err(Error::invalid(
                "Create the payee or category before applying it to selected entries",
            ));
        }
        if let Some(notes) = &self.notes
            && (self.action != "update"
                || self.fields.as_ref().is_none_or(|v| !v["notes"].is_string())
                || !["replace", "prepend", "append", "find_replace"].contains(&notes.mode.as_str()))
        {
            return Err(Error::invalid("Invalid bulk notes mode"));
        }
        Ok(())
    }
}
pub fn apply(
    db: &Connection,
    budget: &str,
    input: &Bulk,
    clock: &str,
    epoch: Option<&str>,
) -> Result<Value, Error> {
    let selected: BTreeSet<&str> = input.ids.iter().map(String::as_str).collect();
    let mut groups: BTreeMap<String, (Value, usize)> = BTreeMap::new();
    for (index, id) in input.ids.iter().enumerate() {
        let row = ledger::get(db, budget, id)?;
        let root = row["parent_id"].as_str().unwrap_or(id).to_string();
        if let std::collections::btree_map::Entry::Vacant(entry) = groups.entry(root) {
            let parent = if entry.key() == id {
                row
            } else {
                ledger::get(db, budget, entry.key())?
            };
            entry.insert((parent, index));
        }
    }
    let regex_budget = register_regex::replacement_budget();
    let pattern = input
        .notes
        .as_ref()
        .filter(|n| n.mode == "find_replace" && !n.find.is_empty())
        .map(|n| {
            Pattern::compile(
                &if n.use_regex {
                    n.find.clone()
                } else {
                    register_regex::escape(&n.find)
                },
                false,
            )
            .map_err(|e| e.api_error())
        })
        .transpose()?;
    let patch = |row: &Value| -> Result<Value, Error> {
        let mut fields = input.fields.clone().unwrap_or(json!({}));
        if let Some(notes) = &input.notes {
            let old = row["notes"].as_str().unwrap_or("");
            let value = fields["notes"].as_str().unwrap();
            fields["notes"] = json!(match notes.mode.as_str() {
                "prepend" => format!("{value}{old}"),
                "append" => format!("{old}{value}"),
                "find_replace" =>
                    if let Some(pattern) = &pattern {
                        pattern.replace_all(old, value, &regex_budget)?
                    } else {
                        old.to_string()
                    },
                _ => value.to_string(),
            });
        }
        Ok(fields)
    };
    let mut outcomes = vec![];
    for (root, (parent, index)) in groups {
        // Every selected group was read live before writes. A now-deleted root
        // therefore belongs to the earlier leg of this same atomic deletion.
        if input.action == "delete" {
            let active: bool = db.query_row(
                "SELECT tombstone=0 FROM transactions WHERE budget_id=?1 AND id=?2",
                rusqlite::params![budget, root],
                |r| r.get(0),
            )?;
            if !active {
                continue;
            }
        }
        let mutation = if input.action == "duplicate" {
            sync::Mutation::Duplicate(root.clone(), input.duplicate_ids[index].clone())
        } else if input.action == "delete" && selected.contains(root.as_str()) {
            sync::Mutation::Delete(root.clone())
        } else if parent["is_parent"] != true {
            sync::Mutation::Patch(root.clone(), patch(&parent)?)
        } else {
            let parent_selected = selected.contains(root.as_str());
            let mut fields = if parent_selected && input.action == "update" {
                patch(&parent)?
            } else {
                json!({})
            };
            let category = fields.as_object_mut().unwrap().remove("category_id");
            let mut children = vec![];
            for child in parent["children"].as_array().unwrap() {
                let id = child["id"].as_str().unwrap();
                let chosen = selected.contains(id);
                if input.action == "delete" && chosen {
                    continue;
                }
                let mut data = json!({"id":id,"amount":child["amount"],"payee_id":child.get("retained_payee_id").unwrap_or(&child["payee_id"]),"category_id":category.as_ref().unwrap_or(&child["category_id"]),"notes":child["notes"]});
                if let Some(peer) = child["transfer_id"].as_str() {
                    data["transfer_account_id"] = child["transfer_account_id"].clone();
                    data["counterpart_id"] = json!(peer);
                }
                if chosen && input.action == "update" {
                    let child_patch = patch(child)?;
                    for (field, value) in child_patch.as_object().unwrap() {
                        if !["amount", "notes", "payee_id", "category_id"].contains(&field.as_str())
                        {
                            return Err(Error::invalid(
                                "Account, date and status belong to the whole split. Select its parent to change them.",
                            ));
                        }
                        data[field] = value.clone();
                    }
                }
                children.push(
                    serde_json::from_value::<splits::Child>(data)
                        .map_err(|_| Error::invalid("Invalid child changes"))?,
                );
            }
            let action = if children.is_empty() {
                splits::Action::Recombine
            } else {
                splits::Action::Replace
            };
            sync::Mutation::Split(
                root.clone(),
                splits::Change {
                    snapshot: sync::cursor(db, budget)?,
                    action,
                    children,
                    allocation: splits::Allocation::Exact,
                    detach: vec![],
                    confirm_reconciled: false,
                    reconciled_versions: BTreeMap::new(),
                    reconciliation_token: None,
                    transfer_protection_token: None,
                    changes: None,
                    fields: if fields.as_object().unwrap().is_empty() {
                        None
                    } else {
                        Some(fields)
                    },
                },
            )
        };
        let result = sync::execute(db, budget, Some(clock), false, epoch, mutation)?;
        if result["response_status"].as_u64().unwrap_or(500) >= 400
            || result["response_body"]["conflicts"]
                .as_array()
                .is_some_and(|v| !v.is_empty())
        {
            return Err(Error::new(
                axum::http::StatusCode::CONFLICT,
                "bulk_conflict",
                "One or more selected entries changed. Nothing was saved. Refresh and review your selection.",
            ));
        }
        outcomes.push(result["response_body"]["data"].clone());
    }
    Ok(
        json!({"affected_count":input.ids.len(),"aggregate_count":outcomes.len(),"ids":input.ids,"entries":outcomes}),
    )
}
