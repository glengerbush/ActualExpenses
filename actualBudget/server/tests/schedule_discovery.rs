//! Independent source archives exercise discovery only through the public API.
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
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

fn import(server: &Server, archive: &Value) -> String {
    use sha2::{Digest, Sha256};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/schedule-discovery")
        .join(archive["filename"].as_str().unwrap());
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        archive["sha256"].as_str().unwrap()
    );
    let result: Value = server
        .client
        .post(format!("{}/api/v1/imports/actual", server.url))
        .header("Idempotency-Key", "discovery-source")
        .header("Content-Type", "application/zip")
        .body(bytes)
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
        .header("Idempotency-Key", "activate-source")
        .json(&json!({}))
        .send()
        .unwrap();
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    format!(
        "/api/v1/budgets/{}",
        result["data"]["budget_id"].as_str().unwrap()
    )
}

fn normalized(value: &Value, ids: &BTreeMap<String, String>) -> Value {
    match value {
        Value::String(s) if uuid::Uuid::parse_str(s).is_ok() => json!(
            ids.iter()
                .find(|(_, id)| *id == s)
                .unwrap_or_else(|| panic!("Unmapped original ID {s}"))
                .0
        ),
        Value::Array(a) => Value::Array(a.iter().map(|v| normalized(v, ids)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), normalized(v, ids)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

#[test]
fn discovery_matches_all_pinned_histories_and_preserves_imported_ledgers() {
    let oracle: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/schedule-matching.json")).unwrap();
    let manifest: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/schedule-discovery/manifest.json"
    ))
    .unwrap();
    assert_eq!(
        manifest["archives"].as_array().unwrap().len(),
        oracle["discovery"].as_array().unwrap().len()
    );
    for case in oracle["discovery"].as_array().unwrap() {
        let archive = manifest["archives"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == case["name"])
            .unwrap();
        let server = Server::start(tempfile::tempdir().unwrap());
        let root = import(&server, archive);
        let ids: BTreeMap<String, String> = serde_json::from_value(archive["ids"].clone()).unwrap();
        let ledger = server.get(&format!("{root}/transactions"));
        let mut seed = vec![];
        for row in ledger["data"].as_array().unwrap() {
            seed.push(projection(row, &ids));
            for child in row["children"].as_array().into_iter().flatten() {
                seed.push(projection(child, &ids));
            }
        }
        let mut expected_seed = case["seed"]["rows"].as_array().unwrap().clone();
        seed.sort_by_key(Value::to_string);
        expected_seed.sort_by_key(Value::to_string);
        assert_eq!(seed, expected_seed, "{} imported seed", case["name"]);
        let cp = server.get(&format!("{root}/checkpoint"));
        let path = format!("{}{root}/schedules/discovery", server.url);
        let response = server
            .client
            .get(&path)
            .query(&[
                ("snapshot", cp["data"]["snapshot"].as_str().unwrap()),
                ("as_of", "2026-11-09"),
            ])
            .send()
            .unwrap();
        assert_eq!(
            response.status(),
            200,
            "{}: {}",
            case["name"],
            response.text().unwrap()
        );
        let body: Value = response.json().unwrap();
        let mut actual=body["data"]["suggestions"].as_array().unwrap().iter().map(|v|{
            assert!(uuid::Uuid::parse_str(v["id"].as_str().unwrap()).is_ok());
            normalized(&json!({"account":v["account_id"],"payee":v["payee_id"],"date":v["date"],"amount":v["amount"],"_conditions":v["conditions"]}),&ids)
        }).collect::<Vec<_>>();
        let mut expected = case["suggestions"].as_array().unwrap().clone();
        actual.sort_by_key(Value::to_string);
        expected.sort_by_key(Value::to_string);
        assert_eq!(actual, expected, "{} suggestions", case["name"]);
        assert_eq!(body["data"]["total_count"], expected.len());
        assert!(body["data"]["next_offset"].is_null());
        assert_eq!(body["data"]["snapshot"], cp["data"]["snapshot"]);
        assert_eq!(body["data"]["epoch"], cp["data"]["epoch"]);
        assert_eq!(body["data"]["as_of"], "2026-11-09");
        assert_eq!(server.get(&format!("{root}/checkpoint")), cp);
        assert_eq!(server.get(&format!("{root}/transactions")), ledger);
        // Process restart must not randomize suggestion identity or page results.
        let server = server.restart();
        let again: Value = server
            .client
            .get(format!("{}{root}/schedules/discovery", server.url))
            .query(&[
                ("snapshot", cp["data"]["snapshot"].as_str().unwrap()),
                ("as_of", "2026-11-09"),
            ])
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .unwrap();
        assert_eq!(again, body, "{} stable suggestions", case["name"]);
    }
}

#[test]
fn discovery_pages_are_complete_and_invalid_or_stale_reads_have_no_effects() {
    let manifest: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/schedule-discovery/manifest.json"
    ))
    .unwrap();
    let archive = &manifest["archives"][0];
    let server = Server::start(tempfile::tempdir().unwrap());
    let root = import(&server, archive);
    let payee = uuid::Uuid::new_v4().to_string();
    let response = server
        .client
        .post(format!("{}{root}/payees", server.url))
        .header("Idempotency-Key", "extra-payee")
        .json(&json!({"id":payee,"name":"Another bill"}))
        .send()
        .unwrap();
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    for date in ["2026-10-19", "2026-10-26", "2026-11-02"] {
        let response=server.client.post(format!("{}{root}/transactions",server.url)).header("Idempotency-Key",date).json(&json!({"id":uuid::Uuid::new_v4().to_string(),"account_id":archive["ids"]["ACCOUNT"],"payee_id":payee,"amount":-7777,"date":date})).send().unwrap();
        assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    }
    let cp = server.get(&format!("{root}/checkpoint"));
    let ledger = server.get(&format!("{root}/transactions"));
    let query = json!({"snapshot":cp["data"]["snapshot"],"as_of":"2026-11-09"});
    let read = |body: &Value| {
        server
            .client
            .get(format!("{}{root}/schedules/discovery", server.url))
            .query(body)
            .send()
            .unwrap()
    };
    let full: Value = read(&query).error_for_status().unwrap().json().unwrap();
    assert_eq!(full["data"]["total_count"], 2);
    let mut pages = vec![];
    for offset in 0..2 {
        let mut body = query.clone();
        body["limit"] = json!(1);
        body["offset"] = json!(offset);
        let page: Value = read(&body).error_for_status().unwrap().json().unwrap();
        assert_eq!(page["data"]["suggestions"].as_array().unwrap().len(), 1);
        assert_eq!(page["data"]["total_count"], 2);
        assert_eq!(
            page["data"]["next_offset"],
            if offset == 0 { json!(1) } else { Value::Null }
        );
        pages.push(page["data"]["suggestions"][0].clone());
    }
    assert_eq!(json!(pages), full["data"]["suggestions"]);
    let mut filtered = query.clone();
    filtered["account_id"] = archive["ids"]["PEER"].clone();
    let none: Value = read(&filtered).error_for_status().unwrap().json().unwrap();
    assert_eq!(none["data"]["suggestions"], json!([]));
    for patch in [
        json!({"limit":0}),
        json!({"limit":1001}),
        json!({"offset":-1}),
        json!({"as_of":"2026-02-30"}),
        json!({"account_id":"invalid"}),
    ] {
        let mut invalid = query.clone();
        invalid
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert_eq!(read(&invalid).status(), 400, "{invalid}");
    }
    assert_eq!(read(&json!({"as_of":"2026-11-09"})).status(), 400);
    assert_eq!(server.get(&format!("{root}/checkpoint")), cp);
    assert_eq!(server.get(&format!("{root}/transactions")), ledger);
    let response=server.client.post(format!("{}{root}/accounts",server.url)).header("Idempotency-Key","advance-discovery").json(&json!({"id":uuid::Uuid::new_v4().to_string(),"name":"New account","opening_balance":0,"opening_date":"2026-11-09"})).send().unwrap();
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    let advanced = server.get(&format!("{root}/checkpoint"));
    assert_ne!(advanced, cp);
    assert_eq!(read(&query).status(), 409);
    assert_eq!(server.get(&format!("{root}/checkpoint")), advanced);
}

#[test]
fn imported_tied_account_ranks_preserve_source_name_order_and_discovery_winner() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/discovery-account-order.json"
    ))
    .unwrap();
    let server = Server::start(tempfile::tempdir().unwrap());
    let root = import(
        &server,
        &json!({"filename":"../discovery-account-order-26.10.zip","sha256":fixture["archive_sha256"]}),
    );
    let ids: BTreeMap<String, String> = serde_json::from_value(fixture["ids"].clone()).unwrap();
    assert!(ids["ZULU"] < ids["ALPHA"]);
    let cp = server.get(&format!("{root}/checkpoint"));
    let ledger = server.get(&format!("{root}/transactions"));
    assert_eq!(ledger["data"].as_array().unwrap().len(), 8);
    let response = server
        .client
        .get(format!("{}{root}/schedules/discovery", server.url))
        .query(&[
            ("snapshot", cp["data"]["snapshot"].as_str().unwrap()),
            ("as_of", "2026-11-09"),
        ])
        .send()
        .unwrap();
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let body: Value = response.json().unwrap();
    let actual=body["data"]["suggestions"].as_array().unwrap().iter().map(|v|normalized(&json!({"account":v["account_id"],"payee":v["payee_id"],"date":v["date"],"amount":v["amount"],"_conditions":v["conditions"]}),&ids)).collect::<Vec<_>>();
    assert_eq!(json!(actual), fixture["observed"]["suggestions"]);
    let accounts = server.get(&format!("{root}/accounts"));
    assert_eq!(
        accounts["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["Alpha", "Zulu"]
    );
    assert_eq!(server.get(&format!("{root}/checkpoint")), cp);
    assert_eq!(server.get(&format!("{root}/transactions")), ledger);
}
