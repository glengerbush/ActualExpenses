use super::*;
fn read(bytes: &[u8]) -> Result<Value, Error> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| invalid("Choose a valid unencrypted YNAB4 ZIP export"))?;
    if archive.len() > 10000 {
        return Err(invalid("YNAB4 archive contains more than10000 entries"));
    }
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    let mut expanded = 0u64;
    let mut actual_size = 0usize;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| invalid("Encrypted or corrupt YNAB4 entries are not supported"))?;
        let name = entry.name().to_owned();
        if name.is_empty()
            || name.starts_with('/')
            || name.contains(['\\', '\0', ':'])
            || name.split('/').any(|part| part == ".." || part == ".")
            || !seen.insert(name.to_lowercase())
        {
            return Err(invalid("Unsafe or duplicate YNAB4 archive entry"));
        }
        expanded = expanded
            .checked_add(entry.size())
            .ok_or_else(|| invalid("YNAB4 archive is too large"))?;
        if expanded > 64 * 1024 * 1024 {
            return Err(invalid("Expanded YNAB4 export exceeds64 MiB"));
        }
        let mut data = Vec::new();
        entry
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut data)
            .map_err(|_| invalid("Could not read YNAB4 archive"))?;
        if data.len() > 64 * 1024 * 1024 {
            return Err(invalid("Expanded YNAB4 file exceeds64 MiB"));
        }
        actual_size = actual_size
            .checked_add(data.len())
            .ok_or_else(|| invalid("Expanded YNAB4 export exceeds64 MiB"))?;
        if actual_size > 64 * 1024 * 1024 {
            return Err(invalid("Expanded YNAB4 export exceeds64 MiB"));
        }
        entries.push((name, data));
    }
    let root = entries
        .first()
        .and_then(|(name, _)| name.split('/').find(|part| part.ends_with(".ynab4")))
        .map(|name| format!("{name}/"))
        .unwrap_or_default();
    let get = |name: &str| {
        entries
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, bytes)| bytes.as_slice())
            .ok_or_else(|| invalid(format!("YNAB4 export is missing {name}")))
    };
    let metadata: Value = serde_json::from_slice(get(&format!("{root}Budget.ymeta"))?)
        .map_err(|_| invalid("Invalid Budget.ymeta JSON"))?;
    let directory = text(&metadata, "relativeDataFolderName")?.trim_matches('/');
    if directory.is_empty() || directory.split('/').any(|part| part == ".." || part == ".") {
        return Err(invalid("Invalid YNAB4 data folder"));
    }
    let base = format!("{root}{directory}/");
    let prefix = format!("{base}devices");
    let mut devices = vec![];
    for (name, bytes) in &entries {
        if !name.starts_with(&prefix) {
            continue;
        }
        let Ok(device) = serde_json::from_slice::<Value>(bytes) else {
            continue;
        };
        if device["hasFullKnowledge"] != true {
            continue;
        }
        let knowledge = text(&device, "knowledge")?;
        let mut score = 0u64;
        for version in knowledge.split(',') {
            let (_, number) = version
                .split_once('-')
                .ok_or_else(|| invalid("Invalid YNAB4 device knowledge"))?;
            score = score
                .checked_add(
                    number
                        .parse::<u64>()
                        .map_err(|_| invalid("Invalid YNAB4 device knowledge"))?,
                )
                .ok_or_else(|| invalid("Invalid YNAB4 device knowledge"))?;
        }
        let guid = text(&device, "deviceGUID")?;
        if guid.contains(['/', '\\']) {
            return Err(invalid("Invalid YNAB4 device identity"));
        }
        devices.push((score, guid.to_owned()));
    }
    devices.sort_by_key(|(score, _)| *score);
    let (_, guid) = devices
        .last()
        .ok_or_else(|| invalid("No complete YNAB4 device snapshot was found"))?;
    serde_json::from_slice(get(&format!("{base}{guid}/Budget.yfull"))?)
        .map_err(|_| invalid("Invalid Budget.yfull JSON"))
}
fn ordered<'a>(rows: &'a [Value], field: &str) -> Result<Vec<&'a Value>, Error> {
    let mut result = rows
        .iter()
        .filter(|r| r["isTombstone"] != true)
        .collect::<Vec<_>>();
    if result
        .iter()
        .any(|row| row[field].as_f64().is_none_or(|n| !n.is_finite()))
    {
        return Err(invalid(format!("Invalid YNAB4 {field}")));
    }
    result.sort_by(|a, b| a[field].as_f64().partial_cmp(&b[field].as_f64()).unwrap());
    Ok(result)
}
fn boolean(row: &Value, key: &str) -> Result<bool, Error> {
    match &row[key] {
        Value::Null => Ok(false),
        Value::Bool(value) => Ok(*value),
        _ => Err(invalid(format!("Invalid {key} flag"))),
    }
}
fn optional_text(row: &Value, key: &str) -> Result<Value, Error> {
    match &row[key] {
        Value::Null => Ok(Value::Null),
        Value::String(value) => Ok(json!(value)),
        _ => Err(invalid(format!("Invalid {key} text"))),
    }
}
fn category(source: &Source, value: &Value) -> Value {
    match value.as_str() {
        Some("Category/__ImmediateIncome__" | "Category/__DeferredIncome__") => {
            json!(identity("builtin", "income"))
        }
        Some("Category/__Split__") | None => Value::Null,
        Some(raw) => source
            .mappings
            .get(&format!("category:{raw}"))
            .map(|id| json!(id))
            .unwrap_or(Value::Null),
    }
}
fn properties(source: &Source, row: &Value) -> Result<(Value, Value, Value), Error> {
    let transfer = row["transferTransactionId"]
        .as_str()
        .and_then(|raw| source.mappings.get(&format!("transaction:{raw}")))
        .map(|id| json!(id))
        .unwrap_or(Value::Null);
    if transfer.is_string() {
        let target = source.find("account", &row["targetAccountId"])?;
        let account = target
            .as_str()
            .ok_or_else(|| invalid("Missing YNAB4 transfer target"))?;
        return Ok((
            json!(identity("transfer-payee", account)),
            Value::Null,
            transfer,
        ));
    }
    let raw = row["payeeId"].as_str();
    let payee = raw
        .and_then(|id| source.mappings.get(&format!("payee:{id}")))
        .map(|id| json!(id))
        .unwrap_or(Value::Null);
    let imported = raw
        .and_then(|id| source.payee_names.get(id))
        .map(|name| json!(name))
        .unwrap_or(Value::Null);
    Ok((payee, imported, Value::Null))
}
pub(super) fn convert(filename: &str, bytes: &[u8]) -> Result<(String, Source, Vec<Value>), Error> {
    let data = read(bytes)?;
    for field in [
        "accounts",
        "payees",
        "masterCategories",
        "transactions",
        "monthlyBudgets",
    ] {
        rows(&data, field)?;
    }
    let filename = filename.replace('\\', "/");
    if !filename.ends_with(".zip") {
        return Err(invalid("Choose a YNAB4 ZIP filename"));
    }
    let name = filename
        .rsplit('/')
        .next()
        .unwrap()
        .trim_end_matches(".zip")
        .trim_end_matches(".ynab4")
        .split('~')
        .next()
        .unwrap()
        .to_owned();
    crate::models::validate_name(&name)?;
    let mut source = Source::new()?;
    source.income();
    for payee in rows(&data, "payees")? {
        if let (Some(id), Some(name)) = (payee["entityId"].as_str(), payee["name"].as_str()) {
            source.payee_names.insert(id.to_owned(), name.to_owned());
        }
    }
    for account in ordered(rows(&data, "accounts")?, "sortableIndex")? {
        if boolean(account, "isTombstone")? {
            continue;
        }
        let id = source.map("account", text(account, "entityId")?)?;
        let title = text(account, "accountName")?;
        source
            .off_budget
            .insert(id.clone(), !boolean(account, "onBudget")?);
        source.push("accounts",json!({"id":id,"name":title,"offbudget":i32::from(!boolean(account,"onBudget")?),"closed":i32::from(boolean(account,"hidden")?),"sort_order":source.data["accounts"].as_array().unwrap().len(),"tombstone":0}));
        source.push("payees",json!({"id":identity("transfer-payee",&id),"name":title,"transfer_acct":id,"tombstone":0}));
    }
    let mut category_count = 0;
    for group in ordered(rows(&data, "masterCategories")?, "sortableIndex")? {
        if boolean(group, "isTombstone")? || group["type"] != "OUTFLOW" {
            continue;
        }
        let Some(children) = group["subCategories"].as_array() else {
            continue;
        };
        if !children.iter().any(|c| c["isTombstone"] != true) {
            continue;
        }
        let id = source.map("group", text(group, "entityId")?)?;
        let title = text(group, "name")?;
        source.push("category_groups",json!({"id":id,"name":title,"is_income":0,"hidden":0,"sort_order":source.data["category_groups"].as_array().unwrap().len()-1,"tombstone":0}));
        source.note(&id, &group["note"])?;
        for child in ordered(children, "sortableIndex")? {
            if boolean(child, "isTombstone")? {
                continue;
            }
            let category_id = source.map("category", text(child, "entityId")?)?;
            let title = text(child, "name")?;
            let title = if title == "" {
                return Err(invalid("Empty YNAB4 category name"));
            } else if group["name"] == "Hidden Categories" {
                let mut parts = title.split(" ` ").collect::<Vec<_>>();
                parts.pop();
                parts.join("/").trim().to_owned()
            } else {
                title.to_owned()
            };
            let parent = source.find("group", &child["masterCategoryId"])?;
            source.push("categories",json!({"id":category_id,"name":title,"is_income":0,"cat_group":parent,"hidden":0,"sort_order":category_count,"tombstone":0}));
            source.note(&category_id, &child["note"])?;
            category_count += 1;
        }
    }
    let account_transfer = source.data["payees"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["transfer_acct"].as_str().unwrap().to_owned(),
                p["id"].as_str().unwrap().to_owned(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for payee in rows(&data, "payees")? {
        if boolean(payee, "isTombstone")? {
            continue;
        }
        let raw = text(payee, "entityId")?;
        if !payee["targetAccountId"].is_null() {
            let account = source.find("account", &payee["targetAccountId"])?;
            let id = account_transfer
                .get(
                    account
                        .as_str()
                        .ok_or_else(|| invalid("Missing transfer payee account"))?,
                )
                .ok_or_else(|| invalid("Missing transfer payee"))?;
            source.mappings.insert(format!("payee:{raw}"), id.clone());
        } else {
            let id = source.map("payee", raw)?;
            source.push(
                "payees",
                json!({"id":id,"name":text(payee,"name")?,"transfer_acct":null,"tombstone":0}),
            );
        }
    }
    for transaction in rows(&data, "transactions")? {
        source.map("transaction", text(transaction, "entityId")?)?;
        if let Some(children) = transaction["subTransactions"].as_array() {
            for child in children {
                source.map("transaction", text(child, "entityId")?)?;
            }
        }
    }
    for (index, transaction) in rows(&data, "transactions")?.iter().enumerate() {
        if boolean(transaction, "isTombstone")? {
            continue;
        }
        let id = source.find("transaction", &transaction["entityId"])?;
        let account = source.find("account", &transaction["accountId"])?;
        let off_budget = *source
            .off_budget
            .get(
                account
                    .as_str()
                    .ok_or_else(|| invalid("Missing source account"))?,
            )
            .ok_or_else(|| invalid("Missing source account"))?;
        let children = transaction["subTransactions"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|c| c["isTombstone"] != true)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let cleared = matches!(
            transaction["cleared"].as_str(),
            Some("Cleared" | "Reconciled")
        );
        let reconciled = transaction["cleared"] == "Reconciled";
        let date = text(transaction, "date")?;
        crate::models::validate_date(date)?;
        let date = date
            .replace('-', "")
            .parse::<i64>()
            .map_err(|_| invalid("Invalid source date"))?;
        let rank = (100001 - index as i64) * 100001;
        let (payee, imported, transfer) = properties(&source, transaction)?;
        source.push("transactions",json!({"id":id,"acct":account,"amount":money(&transaction["amount"],100.0)?,"date":date,"category":if off_budget{Value::Null}else{category(&source,&transaction["categoryId"])},"description":payee,"imported_description":imported,"transferred_id":transfer,"notes":optional_text(transaction,"memo")?,"cleared":i32::from(cleared),"reconciled":i32::from(reconciled),"isParent":i32::from(!children.is_empty()),"isChild":0,"parent_id":null,"sort_order":rank,"tombstone":0,"starting_balance_flag":0}));
        for (child_index, child) in children.into_iter().enumerate() {
            let child_id = source.find("transaction", &child["entityId"])?;
            let (payee, imported, transfer) = properties(&source, child)?;
            source.push("transactions",json!({"id":child_id,"acct":account,"amount":money(&child["amount"],100.0)?,"date":date,"category":category(&source,&child["categoryId"]),"description":payee,"imported_description":imported,"transferred_id":transfer,"notes":optional_text(child,"memo")?,"cleared":i32::from(cleared),"reconciled":i32::from(reconciled),"isParent":0,"isChild":1,"parent_id":id,"sort_order":rank-child_index as i64-1,"tombstone":0,"starting_balance_flag":0}));
        }
    }
    let mut months = rows(&data, "monthlyBudgets")?.iter().collect::<Vec<_>>();
    months.sort_by_key(|m| m["month"].as_str().unwrap_or(""));
    let categories = source
        .mappings
        .iter()
        .filter(|(key, _)| key.starts_with("category:"))
        .map(|(key, id)| (key[9..].to_owned(), id.clone()))
        .collect::<BTreeMap<_, _>>();
    if months
        .len()
        .checked_mul(categories.len())
        .is_none_or(|n| n > 100000)
    {
        return Err(invalid(
            "Allocation history exceeds100000 category-month cells",
        ));
    }
    let mut carryovers = BTreeMap::<String, bool>::new();
    for month in months {
        let date = text(month, "month")?;
        crate::models::validate_date(date)?;
        let month_id = &date[..7];
        let month_number = month_id
            .replace('-', "")
            .parse::<i64>()
            .map_err(|_| invalid("Invalid allocation month"))?;
        let mut budgets = BTreeMap::new();
        for cell in rows(month, "monthlySubCategoryBudgets")? {
            if boolean(cell, "isTombstone")? {
                continue;
            }
            let category = text(cell, "categoryId")?;
            if budgets.insert(category, cell).is_some() {
                return Err(invalid("Duplicate category allocation in one month"));
            }
        }
        for (original, id) in &categories {
            let cell = budgets.get(original.as_str()).copied();
            let amount = cell
                .map(|c| money(&c["budgeted"], 100.0))
                .transpose()?
                .unwrap_or(0);
            if let Some(value) = cell.map(|c| &c["overspendingHandling"]) {
                if value == "Confined" {
                    carryovers.insert(id.clone(), true);
                } else if value == "AffectsBuffer" {
                    carryovers.insert(id.clone(), false);
                }
            }
            let carry = *carryovers.get(id).unwrap_or(&false);
            source.push("zero_budgets",json!({"id":identity("budget-cell",&format!("{month_id}:{id}")),"month":month_number,"category":id,"amount":amount,"carryover":i32::from(carry),"goal":null,"long_goal":null}));
        }
    }
    let requirements = vec![
        json!({"feature":"ynab4_source_model","count":1,"blocking":false,"message":"YNAB4 scheduled transactions, payee autofill/rename configuration and edit logs are not migrated by the baseline importer. Original export remains available. Review split payees and historical overspending rollover."}),
    ];
    Ok((name, source, requirements))
}
