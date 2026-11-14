//! Source-backed conversion graphs and horizon warnings through public HTTP.
#[path = "support/retired_rows.rs"]
mod retirement;
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

fn import(server: &Server, archive: &Value) -> String {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/schedule-conversion")
            .join(archive["filename"].as_str().unwrap()),
    )
    .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        archive["sha256"].as_str().unwrap()
    );
    let result: Value = server
        .client
        .post(format!("{}/api/v1/imports/actual", server.url))
        .header("Idempotency-Key", "conversion-source")
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
fn post(server: &Server, path: &str, key: &str, body: &Value) -> reqwest::blocking::Response {
    server
        .client
        .post(format!("{}{path}", server.url))
        .header("Idempotency-Key", key)
        .header(
            "X-Actual-History-Owner",
            "22220000-0000-4000-8000-000000000777",
        )
        .json(body)
        .send()
        .unwrap()
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
fn draft(case: &Value, ids: &BTreeMap<String, String>) -> Value {
    let input = &case["input"];
    resolve(
        &json!({"id":input["id"],"account_id":input["account"],"amount":input["amount"],"date":input["date"],"payee_id":input["payee"],"category_id":input["category"],"notes":input["notes"]}),
        ids,
    )
}
fn flat(ledger: &Value) -> Vec<Value> {
    let mut result = vec![];
    for parent in ledger["data"].as_array().unwrap() {
        result.push(parent.clone());
        result.extend(parent["children"].as_array().into_iter().flatten().cloned());
    }
    result
}
fn sorted(mut rows: Vec<Value>) -> Vec<Value> {
    rows.sort_by_key(Value::to_string);
    rows
}
fn plain_rule(values: &Value) -> Vec<Value> {
    sorted(
        values
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["op"] != "link-schedule")
            .map(|v| {
                let mut v = v.clone();
                let o = v.as_object_mut().unwrap();
                o.remove("type");
                if o.get("field") == Some(&Value::Null) {
                    o.remove("field");
                }
                v
            })
            .collect(),
    )
}
// Normalize only generated transaction IDs; preserve child ordering, financial
// multiplicity, allocation references and both reciprocal edges.
fn graph(rows: Vec<Value>) -> Vec<Value> {
    let mut parents = rows
        .iter()
        .filter(|r| r["parent_id"].is_null())
        .collect::<Vec<_>>();
    parents.sort_by_key(|r| {
        (
            r["account"].to_string(),
            r["date"].to_string(),
            r["amount"].as_i64(),
            r["notes"].to_string(),
        )
    });
    let mut labels = BTreeMap::new();
    for (i, parent) in parents.iter().enumerate() {
        let key = format!("parent-{i}");
        labels.insert(parent["id"].as_str().unwrap().to_owned(), key.clone());
        for (j, child) in rows
            .iter()
            .filter(|r| r["parent_id"] == parent["id"])
            .enumerate()
        {
            labels.insert(
                child["id"].as_str().unwrap().to_owned(),
                format!("{key}/child-{j}"),
            );
        }
    }
    assert_eq!(
        labels.len(),
        rows.len(),
        "Every row has one valid parent position"
    );
    sorted(
        rows.into_iter()
            .map(|mut r| {
                for field in ["id", "parent_id", "transfer_id"] {
                    if let Some(id) = r[field].as_str() {
                        r[field] = json!(
                            labels
                                .get(id)
                                .unwrap_or_else(|| panic!("Missing graph edge {id}"))
                        );
                    }
                }
                r
            })
            .collect(),
    )
}
fn new_projection(row: &Value, ids: &BTreeMap<String, String>) -> Value {
    // Reference labels come from the source archive; generated financial IDs are
    // left intact until graph() checks the full counterpart/parent relation.
    let label = |field: &str| {
        row[field].as_str().map(|id| {
            ids.iter()
                .find(|(_, v)| v.as_str() == id)
                .unwrap_or_else(|| panic!("Unmapped {field}: {id}"))
                .0
                .clone()
        })
    };
    json!({"id":row["id"],"account":label("account_id"),"payee":label("payee_id"),"category":label("category_id"),"schedule":label("schedule_id"),"parent_id":row["parent_id"],"transfer_id":row["transfer_id"],"is_parent":row["is_parent"].as_bool().unwrap_or(false),"date":row["date"],"amount":row["amount"],"notes":row["notes"],"cleared":row["cleared"],"reconciled":row["reconciled"]})
}

#[test]
fn conversion_preserves_source_financial_graphs_and_corrects_split_transfer_loss_atomically() {
    let oracle: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/schedule-matching.json"))
            .map(retirement::current)
            .unwrap();
    let manifest: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/schedule-conversion/manifest.json"
    ))
    .map(retirement::current)
    .unwrap();
    assert_eq!(manifest["archives"].as_array().unwrap().len(), 5);
    for case in oracle["conversion"].as_array().unwrap() {
        let archive = manifest["archives"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == case["name"])
            .unwrap();
        let mut ids: BTreeMap<String, String> =
            serde_json::from_value(archive["ids"].clone()).unwrap();
        let mut server = Server::start(tempfile::tempdir().unwrap());
        let root = import(&server, archive);
        let ledger = server.get(&format!("{root}/transactions"));
        let originals = flat(&ledger);
        assert_eq!(
            sorted(originals.iter().map(|r| projection(r, &ids)).collect()),
            sorted(case["snapshots"][0]["rows"].as_array().unwrap().clone()),
            "{} seed",
            case["name"]
        );
        let cp = server.get(&format!("{root}/checkpoint"));
        let mut preview = json!({"snapshot":cp["data"]["snapshot"],"as_of":"2026-11-09"});
        if case["persisted"] == true {
            preview["transaction_id"] = json!(ids["ORIGINAL"]);
        } else {
            preview["transaction"] = draft(case, &ids);
        }
        let response = post(
            &server,
            &format!("{root}/schedules/convert-preview"),
            "read-only",
            &preview,
        );
        assert_eq!(
            response.status(),
            200,
            "{}: {}",
            case["name"],
            response.text().unwrap()
        );
        let previewed: Value = response.json().unwrap();
        assert_eq!(
            plain_rule(&normalized(&previewed["data"]["conditions"], &ids)),
            plain_rule(&case["rule"]["conditions"])
        );
        let correction = case["split"] == true && case["transfer"] == true;
        let mut expected_actions = plain_rule(&case["rule"]["actions"]);
        if correction {
            expected_actions.push(
                json!({"op":"set","field":"payee","value":"TRANSFER","options":{"splitIndex":1}}),
            );
            expected_actions = sorted(expected_actions);
        }
        assert_eq!(
            plain_rule(&normalized(&previewed["data"]["actions"], &ids)),
            expected_actions,
            "{} actions",
            case["name"]
        );
        assert_eq!(previewed["data"]["protected_count"], 0);
        assert_eq!(
            sorted(
                previewed["data"]["affected_transactions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| projection(r, &ids))
                    .collect()
            ),
            sorted(case["snapshots"][0]["rows"].as_array().unwrap().clone())
        );
        assert_eq!(server.get(&format!("{root}/checkpoint")), cp);
        assert_eq!(server.get(&format!("{root}/transactions")), ledger);

        let today = server.get(&format!("{root}/schedules/due"))["data"]["today"]
            .as_str()
            .unwrap()
            .to_owned();
        let future = (chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d").unwrap()
            + chrono::TimeDelta::days(8))
        .to_string();
        // Public date-only edit keeps fixtures valid after their original2026 date.
        if !originals.is_empty() {
            let parent_ids = ledger["data"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["id"].clone())
                .collect::<Vec<_>>();
            let response = post(
                &server,
                &format!("{root}/transactions/bulk"),
                "shift-source-date",
                &json!({"action":"update","ids":parent_ids,"fields":{"date":future},"snapshot":cp["data"]["snapshot"]}),
            );
            assert_eq!(
                response.status(),
                200,
                "{} date-shift: {}",
                case["name"],
                response.text().unwrap()
            );
            assert!(
                flat(&server.get(&format!("{root}/transactions")))
                    .iter()
                    .all(|r| r["date"] == future)
            );
        }
        let cp = server.get(&format!("{root}/checkpoint"));
        let schedule = uuid::Uuid::new_v4().to_string();
        ids.insert("SCHEDULE".into(), schedule.clone());
        let mut command =
            json!({"action":"convert","id":schedule,"snapshot":cp["data"]["snapshot"]});
        if case["persisted"] == true {
            command["transaction_id"] = json!(ids["ORIGINAL"]);
        } else {
            let mut input = draft(case, &ids);
            input["date"] = json!(future);
            command["transaction"] = input;
        }
        let response = post(
            &server,
            &format!("{root}/schedules/commands"),
            "convert-source",
            &command,
        );
        assert_eq!(
            response.status(),
            201,
            "{} convert: {}",
            case["name"],
            response.text().unwrap()
        );
        let receipt: Value = response.json().unwrap();
        assert_eq!(receipt["data"]["days_until_transaction"], 8);
        assert_eq!(
            server.get(&format!("{root}/transactions"))["data"],
            json!([])
        );
        server = server.restart();
        let replay = post(
            &server,
            &format!("{root}/schedules/commands"),
            "convert-source",
            &command,
        );
        assert_eq!(replay.status(), 201, "{}", replay.text().unwrap());
        assert_eq!(replay.json::<Value>().unwrap(), receipt);
        if case["persisted"] != true {
            let before = server.get(&format!("{root}/checkpoint"));
            assert_eq!(
                post(
                    &server,
                    &format!("{root}/transactions"),
                    "late-draft-save",
                    &command["transaction"]
                )
                .status(),
                409
            );
            assert_eq!(server.get(&format!("{root}/checkpoint")), before);
        }
        let cp = server.get(&format!("{root}/checkpoint"));
        let body = json!({"action":"post","id":schedule,"snapshot":cp["data"]["snapshot"],"occurrence_date":future});
        let response = post(
            &server,
            &format!("{root}/schedules/commands"),
            "post-converted",
            &body,
        );
        assert_eq!(
            response.status(),
            200,
            "{} post: {}",
            case["name"],
            response.text().unwrap()
        );
        let posted: Value = response.json().unwrap();
        let actual_ledger = server.get(&format!("{root}/transactions"));
        let actual = flat(&actual_ledger);
        assert!(actual.iter().all(
            |r| !originals.iter().any(|old| old["id"] == r["id"]) && r["id"] != ids["ORIGINAL"]
        ));
        let mut expected = case["snapshots"].as_array().unwrap().last().unwrap()["rows"]
            .as_array()
            .unwrap()
            .clone();
        if correction {
            // Explicit correction to the source bug: retain the original transfer
            // child's payee and peer; leave the committed source loss unchanged.
            let child = expected
                .iter_mut()
                .find(|r| r["parent_id"].is_string() && r["amount"] == -40)
                .unwrap();
            let child_id = child["id"].clone();
            child["payee"] = json!("TRANSFER");
            child["transfer_id"] = json!("corrected-peer");
            let mut peer = case["snapshots"][0]["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["account"] == "PEER")
                .unwrap()
                .clone();
            peer["id"] = json!("corrected-peer");
            peer["transfer_id"] = child_id;
            peer["cleared"] = json!(true);
            peer["reconciled"] = json!(false);
            peer["schedule"] = Value::Null;
            expected.push(peer);
        }
        for row in &mut expected {
            row["date"] = json!(future);
        }
        assert_eq!(
            graph(actual.iter().map(|r| new_projection(r, &ids)).collect()),
            graph(expected),
            "{} posted graph",
            case["name"]
        );
        let replay = post(
            &server,
            &format!("{root}/schedules/commands"),
            "post-converted",
            &body,
        );
        assert_eq!(replay.status(), 200);
        assert_eq!(replay.json::<Value>().unwrap(), posted);
        assert_eq!(server.get(&format!("{root}/transactions")), actual_ledger);
    }
}

#[test]
fn conversion_window_preview_matches_twenty_frozen_source_boundaries_without_writes() {
    let oracle: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/schedule-matching.json"))
            .map(retirement::current)
            .unwrap();
    let manifest: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/schedule-conversion/manifest.json"
    ))
    .map(retirement::current)
    .unwrap();
    let archive = &manifest["archives"][0];
    let ids: BTreeMap<String, String> = serde_json::from_value(archive["ids"].clone()).unwrap();
    let case = &oracle["conversion"][0];
    let server = Server::start(tempfile::tempdir().unwrap());
    let root = import(&server, archive);
    let cp = server.get(&format!("{root}/checkpoint"));
    let ledger = server.get(&format!("{root}/transactions"));
    for boundary in oracle["warnings"].as_array().unwrap() {
        let mut transaction = draft(case, &ids);
        transaction["date"] = boundary["date"].clone();
        let body = json!({"snapshot":cp["data"]["snapshot"],"transaction":transaction,"as_of":"2026-11-09","upcoming_length":boundary["horizon"]});
        let response = post(
            &server,
            &format!("{root}/schedules/convert-preview"),
            "readonly-window",
            &body,
        );
        assert_eq!(response.status(), 200, "{}", response.text().unwrap());
        let preview: Value = response.json().unwrap();
        for (actual, expected) in [
            ("is_future", "future"),
            ("is_beyond_window", "isBeyondWindow"),
            ("days_until_transaction", "daysUntilTransaction"),
            ("upcoming_days", "upcomingDays"),
        ] {
            assert_eq!(
                preview["data"][actual], boundary[expected],
                "{boundary} {actual}"
            );
        }
        assert_eq!(server.get(&format!("{root}/checkpoint")), cp);
    }
    assert_eq!(server.get(&format!("{root}/transactions")), ledger);
}
