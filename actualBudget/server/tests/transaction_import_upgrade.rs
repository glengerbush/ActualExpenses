//! Genuine old-binary state, exercised only through the running public API.
use reqwest::blocking::Client;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    time::Duration,
};
use tempfile::TempDir;

struct Server {
    process: Child,
    url: String,
    _data: TempDir,
    client: Client,
}
impl Server {
    fn start(data: TempDir) -> Self {
        std::fs::write(data.path().join("index.html"), "Upgrade fixture").unwrap();
        let mut process = Command::new(
            std::env::var("ACTUALBUDGET_BINARY")
                .unwrap_or_else(|_| env!("CARGO_BIN_EXE_actualbudget-server").into()),
        )
        .env("ACTUALBUDGET_DATA_DIR", data.path())
        .env("ACTUALBUDGET_WEB_DIR", data.path())
        .env("ACTUALBUDGET_HOST", "127.0.0.1")
        .env("ACTUALBUDGET_PORT", "0")
        .env_remove("ACTUALBUDGET_PUBLIC_ORIGIN")
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
            _data: data,
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
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures")
            .join(name),
    )
    .unwrap()
}
fn read_history(server: &Server, root: &str, f: &Value) -> Value {
    let mut request = server.client.get(format!("{}{root}/history", server.url));
    for (k, v) in f["owner_headers"].as_object().unwrap() {
        request = request.header(k, v.as_str().unwrap());
    }
    request
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap()
}

fn data() -> Value {
    serde_json::from_slice(&fixture("transaction-import-schema20.json")).unwrap()
}
fn root(f: &Value) -> String {
    format!("/api/v1/budgets/{}", f["metadata"]["id"].as_str().unwrap())
}
fn receipts(s: &Server, f: &Value) {
    for command in f["commands"].as_array().unwrap() {
        let mut request = s
            .client
            .request(
                command["method"].as_str().unwrap().parse().unwrap(),
                format!("{}{}{}", s.url, root(f), command["path"].as_str().unwrap()),
            )
            .header("Idempotency-Key", command["key"].as_str().unwrap())
            .json(&command["body"]);
        for (k, v) in f["owner_headers"].as_object().unwrap() {
            request = request.header(k, v.as_str().unwrap());
        }
        let response = request.send().unwrap();
        assert_eq!(
            response.status(),
            if command["method"] == "POST" {
                201
            } else {
                200
            },
            "{}",
            response.text().unwrap()
        );
        assert_eq!(response.json::<Value>().unwrap(), command["result"]);
    }
}
fn financial(s: &Server, f: &Value, _restored: bool) {
    let root = root(f);
    let path = format!(
        "{root}/transactions?account_id={}",
        f["account_id"].as_str().unwrap()
    );
    let mut actual = s.get(&path);
    let mut expected = f["register"].clone();
    assert_ne!(actual["snapshot"], expected["snapshot"]);
    expected["snapshot"] = actual["snapshot"].clone();
    for row in expected["data"].as_array_mut().unwrap() {
        row["notes"] = json!("");
        row["cleared"] = json!(true);
        row["reconciled"] = json!(false);
    }
    for key in ["balances", "filtered_balances"] {
        expected[key]["cleared"] = expected[key]["total"].clone();
        expected[key]["uncleared"] = json!(0);
    }
    for row in actual["data"].as_array_mut().unwrap() {
        for key in [
            "category_color",
            "category_group_color",
            "color_override",
            "group_sort_order",
        ] {
            row.as_object_mut().unwrap().remove(key);
        }
    }
    assert_eq!(actual, expected);
    for (key, path) in [
        ("accounts", "accounts"),
        ("categories", "categories"),
        ("payees", "payees"),
    ] {
        let mut actual = s.get(&format!("{root}/{path}"));
        let mut expected = f[key].clone();
        if key == "categories" {
            for row in expected["data"].as_array_mut().unwrap() {
                row.as_object_mut().unwrap().remove("notes");
                row["field_versions"]
                    .as_object_mut()
                    .unwrap()
                    .remove("notes");
                row["hidden"] = json!(false);
                row["group_hidden"] = json!(false);
            }
            for row in actual["data"].as_array_mut().unwrap() {
                for key in [
                    "category_color",
                    "category_group_color",
                    "color_override",
                    "group_sort_order",
                    "resolved_color",
                    "color",
                ] {
                    row.as_object_mut().unwrap().remove(key);
                }
            }
        }
        assert_eq!(actual, expected);
    }
}
#[test]
fn genuine20_database_preserves_finances_history_receipts_and_refreshes_blocked_stages() {
    let f = data();
    let bytes = fixture("transaction-import-schema20.sqlite3");
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        f["database_sha256"]
    );
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("actualbudget.sqlite3"), bytes).unwrap();
    let s = Server::start(dir);
    let root = root(&f);
    financial(&s, &f, false);
    assert_eq!(read_history(&s, &root, &f), f["history"]);
    let checkpoint = s.get(&format!("{root}/checkpoint"));
    let mut expected_checkpoint = f["checkpoint"].clone();
    assert_ne!(
        checkpoint["data"]["snapshot"],
        expected_checkpoint["data"]["snapshot"]
    );
    expected_checkpoint["data"]["snapshot"] = checkpoint["data"]["snapshot"].clone();
    assert_eq!(checkpoint, expected_checkpoint);
    receipts(&s, &f);
    for stage in f["stages"].as_array().unwrap() {
        let id = stage["result"]["data"]["id"].as_str().unwrap();
        let current = s.get(&format!("/api/v1/imports/{id}"));
        assert_eq!(current["data"]["status"], "ready", "{current}");
        let bytes = fixture(stage["archive"].as_str().unwrap());
        assert_eq!(format!("{:x}", Sha256::digest(&bytes)), stage["sha256"]);
        let original = s
            .client
            .get(format!("{}/api/v1/imports/{id}/original.zip", s.url))
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .bytes()
            .unwrap();
        assert_eq!(original.as_ref(), bytes);
        let replay = s
            .client
            .post(format!("{}/api/v1/imports/actual", s.url))
            .header("Content-Type", "application/zip")
            .header("Idempotency-Key", stage["key"].as_str().unwrap())
            .body(bytes)
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .json::<Value>()
            .unwrap();
        assert_eq!(replay, stage["result"]);
        let activated = s
            .client
            .post(format!("{}/api/v1/imports/{id}/activate", s.url))
            .header("Idempotency-Key", format!("activate21-{id}"))
            .json(&json!({}))
            .send()
            .unwrap();
        assert_eq!(activated.status(), 201, "{}", activated.text().unwrap());
        if stage["archive"] == "transaction-import-rules-before-match-26.10.zip" {
            let source: Value =
                serde_json::from_slice(&fixture("transaction-import-rules-source.json")).unwrap();
            let case = source["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == "rules-before-match")
                .unwrap();
            let settings = s.get(&format!(
                "/api/v1/budgets/{}/accounts/{}/transaction-import-settings?format=csv",
                current["data"]["budget_id"].as_str().unwrap(),
                case["ids"]["account"].as_str().unwrap()
            ));
            assert_eq!(settings["data"]["raw_preferences"], case["settings"]);
            assert_eq!(
                settings["data"]["field_versions"],
                json!({}),
                "Official source ZIPs erase CRDT clocks; migration must not mint them"
            );
        }
    }
    financial(&s, &f, false);
    assert_eq!(read_history(&s, &root, &f), f["history"]);
    let checkpoint = s.get(&format!("{root}/checkpoint"));
    let mut expected_checkpoint = f["checkpoint"].clone();
    assert_ne!(
        checkpoint["data"]["snapshot"],
        expected_checkpoint["data"]["snapshot"]
    );
    expected_checkpoint["data"]["snapshot"] = checkpoint["data"]["snapshot"].clone();
    assert_eq!(checkpoint, expected_checkpoint);
}
#[test]
fn genuine20_native_backup_preserves_receipts_finances_and_rotates_history_epoch() {
    let f = data();
    let bytes = fixture("transaction-import-schema20.zip");
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), f["archive_sha256"]);
    let s = Server::start(tempfile::tempdir().unwrap());
    let root = root(&f);
    let upload = s
        .client
        .post(format!("{}/api/v1/restores", s.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "native20")
        .body(bytes)
        .send()
        .unwrap();
    assert_eq!(upload.status(), 201, "{}", upload.text().unwrap());
    let stage: Value = upload.json().unwrap();
    assert_eq!(stage["data"]["report"]["schema_version"], 36);
    let response = s
        .client
        .post(format!(
            "{}{root}/restores/{}/activate",
            s.url,
            stage["data"]["id"].as_str().unwrap()
        ))
        .header("Idempotency-Key", "restore20")
        .json(&json!({"confirm_name":f["metadata"]["name"]}))
        .send()
        .unwrap();
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let current = s.get(&format!("{root}/checkpoint"));
    assert_ne!(current["data"]["epoch"], f["checkpoint"]["data"]["epoch"]);
    financial(&s, &f, true);
    receipts(&s, &f);
    let history = s
        .client
        .get(format!("{}{root}/history", s.url))
        .header(
            "X-Actual-History-Owner",
            f["owner_headers"]["X-Actual-History-Owner"]
                .as_str()
                .unwrap(),
        )
        .header(
            "X-Actual-Budget-Epoch",
            current["data"]["epoch"].as_str().unwrap(),
        )
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json::<Value>()
        .unwrap();
    assert_eq!(history["data"]["undo"], Value::Null);
    assert_eq!(history["data"]["redo"], Value::Null);
}
