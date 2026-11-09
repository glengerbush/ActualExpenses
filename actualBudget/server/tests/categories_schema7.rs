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
    // Exact schema7 public snapshots retain account order, membership, notes and HLCs.
    for domain in ["accounts", "account-groups", "transactions"] {
        let mut current = server.get(&format!("{base}/{domain}"))["data"].clone();
        if domain == "accounts" {
            for row in current.as_array_mut().unwrap() {
                assert!(row["last_reconciled"].is_null());
                row.as_object_mut().unwrap().remove("last_reconciled");
            }
        }
        if domain == "transactions" {
            for row in current.as_array_mut().unwrap() {
                assert!(row["imported_payee"].is_null());
                assert_eq!(row["retained_payee_id"], row["payee_id"]);
                row.as_object_mut().unwrap().remove("imported_payee");
                row.as_object_mut().unwrap().remove("retained_payee_id");
            }
        }
        assert_eq!(current, expected[domain.replace('-', "_")], "{domain}");
    }
    let native = server.get(&format!(
        "{base}/categories/{}",
        expected["native_category"].as_str().unwrap()
    ));
    let native = &native["data"];
    assert_eq!(native["name"], "Native category after import");
    assert_eq!(native["hidden"], false);
    assert_eq!(native["deleted"], false);
    assert_eq!(native["notes"], "");
    assert_eq!(native["sort_order"], 0.0);
    assert!(native["group_id"].is_null());
    if expected["mode"] == "import" {
        let pinned: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/import-26.10.0.json")).unwrap();
        for source in pinned["public"]["categories"].as_array().unwrap() {
            let category = server.get(&format!(
                "{base}/categories/{}",
                source["id"].as_str().unwrap()
            ));
            for field in ["name", "is_income", "hidden", "group_id"] {
                assert_eq!(category["data"][field], source[field], "{field}");
            }
        }
        assert_eq!(
            server.get(&format!("{base}/category-groups"))["data"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    } else {
        assert_eq!(
            server.get(&format!("{base}/category-groups"))["data"],
            json!([])
        );
    }
    // Replays return the old public response exactly, even after epoch replacement.
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
fn genuine_accounts_schema7_upgrades_and_restores_both_domains_without_replaying_imports() {
    for (metadata, database, archive) in [
        (
            include_str!("../../tests/fixtures/category-native-schema7.json"),
            include_bytes!("../../tests/fixtures/category-native-schema7.sqlite3").as_slice(),
            include_bytes!("../../tests/fixtures/category-native-schema7.zip").as_slice(),
        ),
        (
            include_str!("../../tests/fixtures/category-import-schema7.json"),
            include_bytes!("../../tests/fixtures/category-import-schema7.sqlite3").as_slice(),
            include_bytes!("../../tests/fixtures/category-import-schema7.zip").as_slice(),
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
            .header("Idempotency-Key", "schema7-stage")
            .body(archive)
            .send()
            .unwrap());
        ok(restored.write(
            "POST",
            &format!(
                "/api/v1/budgets/{budget}/restores/{}/activate",
                stage["data"]["id"].as_str().unwrap()
            ),
            "schema7-restore",
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
