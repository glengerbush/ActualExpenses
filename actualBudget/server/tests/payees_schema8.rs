use reqwest::blocking::{Client, Response};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    time::Duration,
};
use tempfile::TempDir;
struct Server {
    process: Child,
    _data: TempDir,
    url: String,
    client: Client,
}
impl Server {
    fn start() -> Self {
        Self::open(tempfile::tempdir().unwrap())
    }
    fn open(data: TempDir) -> Self {
        let mut process = Command::new(env!("CARGO_BIN_EXE_actualbudget-server"))
            .env("ACTUALBUDGET_DATA_DIR", data.path())
            .env("ACTUALBUDGET_PORT", "0")
            .env("ACTUALBUDGET_HOST", "127.0.0.1")
            .env("ACTUALBUDGET_BACKUP_INTERVAL_SECONDS", "1")
            .env_remove("ACTUALBUDGET_PUBLIC_ORIGIN")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(process.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert!(line.starts_with("LISTENING "));
        Self {
            process,
            _data: data,
            url: format!("http://{}", line.trim().trim_start_matches("LISTENING ")),
            client: Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
        }
    }
    fn get(&self, path: &str) -> Value {
        let r = self
            .client
            .get(format!("{}{path}", self.url))
            .send()
            .unwrap();
        assert!(r.status().is_success(), "{}", r.text().unwrap());
        r.json().unwrap()
    }
    fn write(
        &self,
        method: &str,
        path: &str,
        key: &str,
        clock: Option<&str>,
        body: Value,
    ) -> Response {
        let mut r = self
            .client
            .request(method.parse().unwrap(), format!("{}{path}", self.url))
            .header("Idempotency-Key", key);
        if let Some(clock) = clock {
            r = r.header("X-Actual-Edit-Clock", clock);
        }
        if !body.is_null() {
            r = r.json(&body);
        }
        r.send().unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

fn ok(response: Response) -> Value {
    let status = response.status();
    let body: Value = response.json().unwrap();
    assert!(status.is_success(), "{status}: {body}");
    body
}
fn qualify(server: &Server, expected: &Value) {
    let budget = expected["budget"]["id"].as_str().unwrap();
    let base = format!("/api/v1/budgets/{budget}");
    for domain in [
        "accounts",
        "account-groups",
        "categories",
        "category-groups",
        "transactions",
    ] {
        let actual = server.get(&format!("{base}/{domain}"));
        let actual = actual["data"].as_array().unwrap();
        let old = expected[domain.replace('-', "_")].as_array().unwrap();
        assert_eq!(actual.len(), old.len(), "{domain}");
        let mut running: i64 = if domain == "transactions" {
            old.iter().map(|row| row["amount"].as_i64().unwrap()).sum()
        } else {
            0
        };
        for (row, saved) in actual.iter().zip(old) {
            // Persisted values, ordering and HLCs survive. All-account running
            // balances are newly derived from the original snapshot amounts.
            for (field, value) in saved.as_object().unwrap() {
                if domain == "transactions" && field == "running_balance" {
                    assert_eq!(row[field], running);
                    continue;
                }
                if domain == "transactions" {
                    match field.as_str() {
                        "notes" => {
                            assert_eq!(row[field], "");
                            continue;
                        }
                        "cleared" => {
                            assert_eq!(row[field], true);
                            continue;
                        }
                        "reconciled" => {
                            assert_eq!(row[field], false);
                            continue;
                        }
                        _ => {}
                    }
                }
                if domain == "categories" || domain == "category-groups" {
                    if field == "notes" {
                        assert!(row.get(field).is_none());
                        continue;
                    }
                    if field == "hidden" {
                        assert_eq!(row[field], false);
                        continue;
                    }
                    if field == "field_versions" {
                        let mut expected = value.clone();
                        expected.as_object_mut().unwrap().remove("notes");
                        assert_eq!(row[field], expected);
                        continue;
                    }
                }
                assert_eq!(&row[field], value, "{domain}/{} {field}", saved["id"]);
            }
            if domain == "transactions" {
                running -= saved["amount"].as_i64().unwrap();
            }
        }
    }
    for receipt in expected["receipts"].as_array().unwrap() {
        let response = server.write(
            receipt["method"].as_str().unwrap(),
            &format!("/api/v1{}", receipt["path"].as_str().unwrap()),
            receipt["key"].as_str().unwrap(),
            None,
            receipt["body"].clone(),
        );
        assert_eq!(
            response.status().as_u16(),
            receipt["status"].as_u64().unwrap() as u16
        );
        assert_eq!(response.json::<Value>().unwrap(), receipt["response"]);
    }
}
#[test]
fn genuine_schema8_upgrade_and_restore_preserve_account_category_metadata_and_receipts() {
    for (metadata, database, archive) in [
        (
            include_str!("../../tests/fixtures/payees-native-schema8.json"),
            include_bytes!("../../tests/fixtures/payees-native-schema8.sqlite3").as_slice(),
            include_bytes!("../../tests/fixtures/payees-native-schema8.zip").as_slice(),
        ),
        (
            include_str!("../../tests/fixtures/payees-import-schema8.json"),
            include_bytes!("../../tests/fixtures/payees-import-schema8.sqlite3").as_slice(),
            include_bytes!("../../tests/fixtures/payees-import-schema8.zip").as_slice(),
        ),
    ] {
        let expected: Value = serde_json::from_str(metadata).unwrap();
        let budget = expected["budget"]["id"].as_str().unwrap();
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("actualbudget.sqlite3"), database).unwrap();
        let upgraded = Server::open(directory);
        qualify(&upgraded, &expected);
        assert_eq!(
            upgraded.get(&format!("/api/v1/budgets/{budget}/checkpoint"))["data"]["epoch"],
            expected["checkpoint"]["epoch"]
        );
        let restored = Server::start();
        let stage = ok(restored
            .client
            .post(format!("{}/api/v1/restores", restored.url))
            .header("Content-Type", "application/zip")
            .header("Idempotency-Key", "schema8-stage")
            .body(archive)
            .send()
            .unwrap());
        ok(restored.write(
            "POST",
            &format!(
                "/api/v1/budgets/{budget}/restores/{}/activate",
                stage["data"]["id"].as_str().unwrap()
            ),
            "schema8-restore",
            None,
            json!({"confirm_name":expected["budget"]["name"]}),
        ));
        qualify(&restored, &expected);
        assert_ne!(
            restored.get(&format!("/api/v1/budgets/{budget}/checkpoint"))["data"]["epoch"],
            expected["checkpoint"]["epoch"]
        );
    }
}
