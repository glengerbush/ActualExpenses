//! Public matching boundary checked against the independent Actual26.10 fixture.
#[path = "support/retired_rows.rs"]
mod retirement;
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
    io::{Cursor, Read},
    process::{Child, Command, Stdio},
    time::Duration,
};
use tempfile::TempDir;

struct Server {
    process: Child,
    url: String,
    data: TempDir,
    client: Client,
}
impl Server {
    fn start(data: TempDir) -> Self {
        let web = data.path().join("web");
        std::fs::create_dir_all(&web).unwrap();
        std::fs::write(
            web.join("index.html"),
            "<!doctype html><title>Budget</title>",
        )
        .unwrap();
        let mut process = Command::new(env!("CARGO_BIN_EXE_actualbudget-server"))
            .env("ACTUALBUDGET_WEB_DIR", web)
            .env_remove("ACTUALBUDGET_PUBLIC_ORIGIN")
            .env("ACTUALBUDGET_DATA_DIR", data.path())
            .env("ACTUALBUDGET_PORT", "0")
            .env("ACTUALBUDGET_HOST", "127.0.0.1")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(process.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert!(line.starts_with("LISTENING "), "{line}");
        Self {
            process,
            url: format!("http://{}", line.trim().trim_start_matches("LISTENING ")),
            data,
            client: Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
        }
    }
    fn get(&self, path: &str) -> Value {
        self.client
            .get(format!("{}{path}", self.url))
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .unwrap()
    }
    fn restart(mut self) -> Self {
        self.process.kill().unwrap();
        self.process.wait().unwrap();
        let data = std::mem::replace(&mut self.data, tempfile::tempdir().unwrap());
        Self::start(data)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

fn oracle() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/schedule-matching.json"))
        .map(retirement::current)
        .unwrap()
}

// Source labels resolve only from the committed archive, never replacement results.
fn source_ids() -> BTreeMap<String, String> {
    let data = tempfile::tempdir().unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(include_bytes!(
        "../../tests/fixtures/schedule-matching-26.10.zip"
    )))
    .unwrap();
    let mut bytes = Vec::new();
    archive
        .by_name("db.sqlite")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    let path = data.path().join("source.sqlite3");
    std::fs::write(&path, bytes).unwrap();
    let db = rusqlite::Connection::open_with_flags(
        format!("file:{}?immutable=1", path.display()),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap();
    let mut ids = BTreeMap::new();
    for (index, label) in [
        "exact",
        "amount-low-edge",
        "amount-low-outside",
        "amount-high-edge",
        "amount-high-outside",
        "date-low-edge",
        "date-low-outside",
        "date-high-edge",
        "date-high-outside",
        "income",
        "zero",
        "older-recurrence",
        "wrong-payee",
        "reconciled",
        "transfer",
        "split",
        "split-child",
        "split-transfer",
        "OTHER_SCHEDULE",
    ]
    .iter()
    .enumerate()
    {
        ids.insert(
            (*label).to_owned(),
            format!("22220000-0000-4000-8000-{:012}", index + 1),
        );
    }
    for (label, sql, key) in [
        (
            "ACCOUNT",
            "SELECT id FROM accounts WHERE name=?1",
            "Spending",
        ),
        ("PEER", "SELECT id FROM accounts WHERE name=?1", "Savings"),
        ("PAYEE", "SELECT id FROM payees WHERE name=?1", "Bill"),
        (
            "OTHER_PAYEE",
            "SELECT id FROM payees WHERE name=?1",
            "Other",
        ),
        (
            "CATEGORY",
            "SELECT id FROM categories WHERE name=?1",
            "Utilities",
        ),
    ] {
        ids.insert(
            label.into(),
            db.query_row(sql, [key], |r| r.get(0)).unwrap(),
        );
    }
    for (label, sql, key) in [
        (
            "TRANSFER",
            "SELECT id FROM payees WHERE transfer_acct=?1",
            "PEER",
        ),
        (
            "BACK_TRANSFER",
            "SELECT id FROM payees WHERE transfer_acct=?1",
            "ACCOUNT",
        ),
        (
            "generated-1",
            "SELECT transferred_id FROM transactions WHERE id=?1",
            "split-transfer",
        ),
        (
            "generated-2",
            "SELECT transferred_id FROM transactions WHERE id=?1",
            "transfer",
        ),
    ] {
        let value: String = db.query_row(sql, [&ids[key]], |r| r.get(0)).unwrap();
        ids.insert(label.into(), value);
    }
    ids
}

fn resolve(value: &Value, ids: &BTreeMap<String, String>) -> Value {
    match value {
        Value::String(s) if ids.contains_key(s) => json!(ids[s]),
        Value::Array(a) => Value::Array(a.iter().map(|v| resolve(v, ids)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), resolve(v, ids)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

fn projection(row: &Value, ids: &BTreeMap<String, String>) -> Value {
    let label = |field: &str| {
        row[field].as_str().map(|id| {
            ids.iter()
                .find(|(_, v)| v.as_str() == id)
                .unwrap_or_else(|| panic!("Unknown {field}: {id}"))
                .0
                .clone()
        })
    };
    json!({"id":label("id"),"account":label("account_id"),"payee":label("payee_id"),
        "category":label("category_id"),"parent_id":label("parent_id"),"transfer_id":label("transfer_id"),
        "schedule":label("schedule_id"),"date":row["date"],"amount":row["amount"],
        "notes":row["notes"],"cleared":row["cleared"],"reconciled":row["reconciled"],
        "is_parent":row["is_parent"].as_bool().unwrap_or(false)})
}

fn import(server: &Server) -> String {
    let result: Value = server
        .client
        .post(format!("{}/api/v1/imports/actual", server.url))
        .header("Idempotency-Key", "matching-source")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/schedule-matching-26.10.zip").to_vec())
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(result["data"]["status"], "ready", "{result}");
    let stage = result["data"]["id"].as_str().unwrap();
    let response = server
        .client
        .post(format!("{}/api/v1/imports/{stage}/activate", server.url))
        .header("Idempotency-Key", "matching-activate")
        .json(&json!({}))
        .send()
        .unwrap();
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    format!(
        "/api/v1/budgets/{}",
        result["data"]["budget_id"].as_str().unwrap()
    )
}

fn matches(server: &Server, root: &str, body: Value) -> reqwest::blocking::Response {
    // Read-only query intentionally has no mutation identity or edit clock.
    server
        .client
        .post(format!("{}{root}/schedules/matches", server.url))
        .json(&body)
        .send()
        .unwrap()
}

#[test]
fn matching_boundaries_and_exact_financial_rows_match_pinned_source_without_writes() {
    let server = Server::start(tempfile::tempdir().unwrap());
    let root = import(&server);
    let oracle = oracle();
    let ids = source_ids();
    let before = server.get(&format!("{root}/checkpoint"));
    let ledger = server.get(&format!("{root}/transactions"));
    let mut seed = Vec::new();
    for parent in ledger["data"].as_array().unwrap() {
        seed.push(projection(parent, &ids));
        for child in parent["children"].as_array().into_iter().flatten() {
            seed.push(projection(child, &ids));
        }
    }
    let mut expected_seed = oracle["matching_seed"]["rows"].as_array().unwrap().clone();
    seed.sort_by_key(|r| r["id"].as_str().unwrap().to_owned());
    expected_seed.sort_by_key(|r| r["id"].as_str().unwrap().to_owned());
    assert_eq!(
        seed, expected_seed,
        "Original imported seed must match before querying"
    );
    for case in oracle["matching"].as_array().unwrap() {
        let response = matches(
            &server,
            &root,
            json!({"snapshot":before["data"]["snapshot"],"conditions":resolve(&case["conditions"],&ids)}),
        );
        assert_eq!(
            response.status(),
            200,
            "{}: {}",
            case["name"],
            response.text().unwrap()
        );
        let body: Value = response.json().unwrap();
        assert_eq!(body["data"]["snapshot"], before["data"]["snapshot"]);
        assert_eq!(body["data"]["epoch"], before["data"]["epoch"]);
        assert!(body["data"]["next_offset"].is_null());
        let mut actual = body["data"]["transactions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| projection(r, &ids))
            .collect::<Vec<_>>();
        let expected_ids = case["matched_ids"].as_array().unwrap();
        let mut expected = oracle["matching_seed"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| expected_ids.contains(&r["id"]))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(expected.len(), expected_ids.len());
        assert_eq!(body["data"]["total_count"], expected.len());
        actual.sort_by_key(|r| r["id"].as_str().unwrap().to_owned());
        expected.sort_by_key(|r| r["id"].as_str().unwrap().to_owned());
        assert_eq!(actual, expected, "{}", case["name"]);
        assert_eq!(server.get(&format!("{root}/checkpoint")), before);
    }
    assert_eq!(server.get(&format!("{root}/transactions")), ledger);
}

#[test]
fn matching_pages_resume_after_restart_and_invalid_or_stale_queries_are_read_only() {
    let server = Server::start(tempfile::tempdir().unwrap());
    let root = import(&server);
    let conditions = resolve(&oracle()["matching"][1]["conditions"], &source_ids());
    let cp = server.get(&format!("{root}/checkpoint"));
    let query = json!({"snapshot":cp["data"]["snapshot"],"conditions":conditions});
    let response = matches(&server, &root, query.clone());
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let full: Value = response.json().unwrap();
    let mut collected = Vec::new();
    let mut offset = json!(0);
    let mut server = server;
    loop {
        let mut page_query = query.clone();
        page_query["limit"] = json!(2);
        page_query["offset"] = offset.clone();
        let response = matches(&server, &root, page_query);
        assert_eq!(response.status(), 200, "{}", response.text().unwrap());
        let page: Value = response.json().unwrap();
        let rows = page["data"]["transactions"].as_array().unwrap();
        assert!(!rows.is_empty() && rows.len() <= 2);
        assert_eq!(page["data"]["total_count"], full["data"]["total_count"]);
        assert_eq!(page["data"]["snapshot"], cp["data"]["snapshot"]);
        assert_eq!(page["data"]["epoch"], cp["data"]["epoch"]);
        collected.extend(rows.iter().cloned());
        let total = full["data"]["transactions"].as_array().unwrap().len();
        assert!(
            collected.len() <= total,
            "Pagination repeated or invented rows"
        );
        offset = page["data"]["next_offset"].clone();
        if offset.is_null() {
            break;
        }
        assert!(
            collected.len() < total,
            "Continuation exceeds complete result"
        );
        assert_eq!(offset, collected.len());
        if collected.len() == 2 {
            server = server.restart();
        }
    }
    assert_eq!(json!(collected), full["data"]["transactions"]);
    for invalid in [
        json!({"limit":0}),
        json!({"limit":1001}),
        json!({"offset":-1}),
        json!({"snapshot":null}),
    ] {
        let mut body = query.clone();
        body.as_object_mut()
            .unwrap()
            .extend(invalid.as_object().unwrap().clone());
        let response = matches(&server, &root, body);
        assert!(response.status().is_client_error(), "{}", response.status());
        assert_eq!(server.get(&format!("{root}/checkpoint")), cp);
    }
    let response=server.client.post(format!("{}{root}/accounts",server.url))
        .header("Idempotency-Key","advance-matching-snapshot")
        .json(&json!({"id":"22220000-0000-4000-8000-000000000999","name":"New account","opening_balance":0,"opening_date":"2026-11-09"})).send().unwrap();
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    let advanced = server.get(&format!("{root}/checkpoint"));
    assert_ne!(advanced["data"]["snapshot"], cp["data"]["snapshot"]);
    let response = matches(&server, &root, query);
    assert_eq!(response.status(), 409, "{}", response.text().unwrap());
    assert_eq!(server.get(&format!("{root}/checkpoint")), advanced);
}
