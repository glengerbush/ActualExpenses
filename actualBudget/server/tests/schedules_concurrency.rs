//! Public HTTP qualification: independent clients must not duplicate occurrences.
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Arc, Barrier},
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

#[test]
fn concurrent_catch_up_clients_and_restart_keep_one_pinned_ledger_and_original_receipts() {
    let server = Server::start(tempfile::tempdir().unwrap());
    let imported: Value = server
        .client
        .post(format!("{}/api/v1/imports/actual", server.url))
        .header("Idempotency-Key", "race-source")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/schedules-missed-26.10.zip").to_vec())
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(imported["data"]["status"], "ready");
    let stage = imported["data"]["id"].as_str().unwrap();
    let budget = imported["data"]["budget_id"].as_str().unwrap();
    let activated = server
        .client
        .post(format!("{}/api/v1/imports/{stage}/activate", server.url))
        .header("Idempotency-Key", "race-activate")
        .json(&json!({}))
        .send()
        .unwrap();
    assert_eq!(activated.status(), 201, "{}", activated.text().unwrap());
    let root = format!("/api/v1/budgets/{budget}");
    let cp = server.get(&format!("{root}/checkpoint"));
    let epoch = cp["data"]["epoch"].as_str().unwrap().to_owned();
    let body = json!({"action":"catch-up","through":"2026-11-09"});
    let url = format!("{}{root}/schedules/commands", server.url);
    let barrier = Arc::new(Barrier::new(8));
    let receipts = std::thread::scope(|scope| {
        let handles = (0..8)
            .map(|index| {
                let client = server.client.clone();
                let barrier = Arc::clone(&barrier);
                let url = &url;
                let epoch = &epoch;
                let body = &body;
                scope.spawn(move || {
                    barrier.wait();
                    let response = client
                        .post(url)
                        .header("Idempotency-Key", format!("race-client-{index}"))
                        .header("X-Actual-Budget-Epoch", epoch)
                        .json(body)
                        .send()
                        .unwrap();
                    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
                    response.json::<Value>().unwrap()
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        receipts
            .iter()
            .map(|r| r["data"]["posted_count"].as_u64().unwrap())
            .sum::<u64>(),
        5,
        "Eight independent clients together post the five missed source occurrences"
    );
    let ledger = server.get(&format!("{root}/transactions"));
    let oracle: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/schedules.json")).unwrap();
    let expected = oracle["snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["step"] == "catch up")
        .unwrap();
    let project = |rows: &Value, schedule: &str, payee: &str| {
        let mut result = rows
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                json!({
                    "schedule": row[schedule], "date": row["date"], "amount": row["amount"],
                    "cleared": row["cleared"], "notes": row["notes"].as_str().unwrap_or(""),
                    "has_payee": !row[payee].is_null()
                })
            })
            .collect::<Vec<_>>();
        result.sort_by_key(Value::to_string);
        result
    };
    assert_eq!(
        project(&ledger["data"], "schedule_id", "payee_id"),
        project(&expected["transactions"], "schedule", "payee")
    );
    assert_eq!(ledger["balances"]["total"], -600);
    assert_eq!(ledger["balances"]["cleared"], 0);
    let checkpoint = server.get(&format!("{root}/checkpoint"));
    let server = server.restart();
    for (index, original) in receipts.iter().enumerate() {
        let replay = server
            .client
            .post(format!("{}{root}/schedules/commands", server.url))
            .header("Idempotency-Key", format!("race-client-{index}"))
            .header("X-Actual-Budget-Epoch", &epoch)
            .json(&body)
            .send()
            .unwrap();
        assert_eq!(replay.status(), 200, "{}", replay.text().unwrap());
        assert_eq!(&replay.json::<Value>().unwrap(), original);
    }
    assert_eq!(server.get(&format!("{root}/transactions")), ledger);
    assert_eq!(server.get(&format!("{root}/checkpoint")), checkpoint);
}
