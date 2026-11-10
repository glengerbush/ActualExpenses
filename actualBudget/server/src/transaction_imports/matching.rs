//! Source matching has three global passes; a low-fidelity early row cannot
//! consume a later row's payee match. Every pass shares a bounded work allowance.
use super::*;
use chrono::{Days, NaiveDate};
#[derive(Clone)]
struct Candidate {
    id: String,
    date: NaiveDate,
    payee: Option<String>,
    imported_id: Option<String>,
    rank: usize,
}
fn spend(work: &mut usize, n: usize) -> Result<(), Error> {
    *work=work.checked_sub(n).ok_or_else(||Error::invalid("Matching exceeds the bounded work limit. Import a narrower date range, or explicitly import without matching."))?;
    Ok(())
}
pub(super) struct Mode<'a> {
    pub options: &'a parse::Options,
    pub strict_ids: bool,
    pub bank_remote: Option<&'a str>,
}
pub(super) fn apply(
    db: &Connection,
    b: &str,
    account: &str,
    rows: &mut [Value],
    mode: Mode<'_>,
    work: &mut usize,
) -> Result<(), Error> {
    let Mode {
        options,
        strict_ids: strict_id_checking,
        bank_remote,
    } = mode;
    if !options.reconcile || rows.is_empty() {
        return Ok(());
    }
    let day = |r: &Value| {
        NaiveDate::parse_from_str(r["transaction"]["date"].as_str().unwrap(), "%Y-%m-%d")
            .map_err(Error::internal)
    };
    let dates = rows.iter().map(day).collect::<Result<Vec<_>, _>>()?;
    let start = dates
        .iter()
        .min()
        .unwrap()
        .checked_sub_days(Days::new(7))
        .ok_or_else(|| Error::invalid("Date outside matching range"))?
        .to_string();
    let end = dates
        .iter()
        .max()
        .unwrap()
        .checked_add_days(Days::new(7))
        .ok_or_else(|| Error::invalid("Date outside matching range"))?
        .to_string();
    let amounts = rows
        .iter()
        .map(|r| r["transaction"]["amount"].as_i64().unwrap())
        .collect::<BTreeSet<_>>();
    let mut indexed: BTreeMap<i64, Vec<Candidate>> = BTreeMap::new();
    let mut query=db.prepare(&format!("SELECT t.id,t.date,t.payee_id,t.amount,CASE WHEN m.tombstone=0 THEN m.imported_id END FROM transactions t LEFT JOIN transaction_import_metadata m ON m.budget_id=t.budget_id AND m.id=t.id LEFT JOIN transactions parent ON parent.budget_id=t.budget_id AND parent.id=t.parent_id WHERE t.budget_id=?1 AND t.account_id=?2 AND t.tombstone=0 AND (t.parent_id IS NULL OR parent.tombstone=0) AND t.date>=?3 AND t.date<=?4 ORDER BY t.date DESC,t.starting_balance,{} DESC,t.id",crate::android_upgrade::order::SQL))?;
    for (rank, result) in query
        .query_map(params![b, account, start, end], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?
        .enumerate()
    {
        spend(work, 1)?;
        let (id, date, payee, amount, imported_id) = result?;
        if amounts.contains(&amount) {
            indexed.entry(amount).or_default().push(Candidate {
                id,
                date: NaiveDate::parse_from_str(&date, "%Y-%m-%d").map_err(Error::internal)?,
                payee,
                imported_id,
                rank,
            });
        }
    }
    let mut used = BTreeSet::new();
    let mut primary_cache: BTreeMap<String, Option<String>> = BTreeMap::new();
    // Exact imported IDs are intentionally not consumed like fuzzy matches: the
    // source can encounter the same bank ID more than once in a single file.
    for row in rows.iter_mut() {
        spend(work, 1)?;
        let Some(bank_id) = row["transaction"]["imported_id"]
            .as_str()
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let found = if let Some(found) = primary_cache.get(bank_id) {
            found.clone()
        } else {
            let found=db.query_row(&format!("SELECT t.id FROM transactions t JOIN transaction_import_metadata m ON m.budget_id=t.budget_id AND m.id=t.id WHERE t.budget_id=?1 AND (t.account_id=?2 OR (?5 IS NOT NULL AND EXISTS(SELECT 1 FROM bank_transaction_origins moved WHERE moved.budget_id=t.budget_id AND moved.id=t.id AND moved.remote_account_id=?5))) AND m.imported_id=?3 AND (m.tombstone=0 OR (?5 IS NOT NULL AND t.tombstone=1)) AND (?4=0 OR t.tombstone=0) AND (?5 IS NULL OR NOT EXISTS(SELECT 1 FROM bank_transaction_origins origin WHERE origin.budget_id=t.budget_id AND origin.id=t.id AND origin.remote_account_id<>?5)) ORDER BY t.date DESC,t.starting_balance,{} DESC,t.id LIMIT 1",crate::android_upgrade::order::SQL),params![b,account,bank_id,options.reimport_deleted,bank_remote],|r|r.get::<_,String>(0)).optional()?;
            primary_cache.insert(bank_id.into(), found.clone());
            found
        };
        if let Some(id) = found {
            used.insert(id.clone());
            row["match_id"] = json!(id);
        }
    }
    let mut candidates = Vec::with_capacity(rows.len());
    for (row, date) in rows.iter().zip(dates) {
        let mut selected = vec![];
        if row["match_id"].is_null()
            && let Some(group) = indexed.get(&row["transaction"]["amount"].as_i64().unwrap())
        {
            for candidate in group {
                spend(work, 1)?;
                if strict_id_checking
                    && row["transaction"]["imported_id"]
                        .as_str()
                        .is_some_and(|s| !s.is_empty())
                    && candidate
                        .imported_id
                        .as_deref()
                        .is_some_and(|s| !s.is_empty())
                {
                    continue;
                }
                let distance = (candidate.date - date).num_days().unsigned_abs();
                if distance <= 7 {
                    selected.push((distance, candidate));
                }
            }
        }
        // Actual's comparator returns -1 even for equal distances. Equal-distance
        // source-view order consequently reverses; the independent tie fixture pins it.
        selected.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.rank.cmp(&a.1.rank)));
        candidates.push(selected.into_iter().map(|(_, c)| c).collect::<Vec<_>>());
    }
    for same_payee in [true, false] {
        for (row, candidates) in rows.iter_mut().zip(&candidates) {
            if !row["match_id"].is_null() {
                continue;
            }
            for candidate in candidates {
                spend(work, 1)?;
                if !used.contains(&candidate.id)
                    && (!same_payee
                        || candidate.payee.as_deref() == row["transaction"]["payee_id"].as_str())
                {
                    used.insert(candidate.id.clone());
                    row["match_id"] = json!(candidate.id);
                    break;
                }
            }
        }
    }
    for row in rows {
        row["action"] = json!(if row["match_id"].is_null() {
            "add"
        } else {
            "merge"
        });
    }
    Ok(())
}
