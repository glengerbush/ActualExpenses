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
    url: String,
    data: TempDir,
    client: Client,
}
impl Server {
    fn start() -> Self {
        Self::with_data(tempfile::tempdir().unwrap())
    }
    fn with_data(data: TempDir) -> Self {
        Self::with_origin(data, None)
    }
    fn with_origin(data: TempDir, origin: Option<&str>) -> Self {
        let web = data.path().join("web");
        std::fs::create_dir_all(&web).unwrap();
        std::fs::write(
            web.join("index.html"),
            "<!doctype html><title>Budget</title>",
        )
        .unwrap();
        std::fs::write(web.join("app.js"), "console.log('budget');").unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_actualbudget-server"));
        command
            .env("ACTUALBUDGET_WEB_DIR", web)
            .env_remove("ACTUALBUDGET_PUBLIC_ORIGIN")
            .env("ACTUALBUDGET_DATA_DIR", data.path())
            .env("ACTUALBUDGET_PORT", "0")
            .env("ACTUALBUDGET_HOST", "127.0.0.1")
            .stdout(Stdio::piped());
        if let Some(origin) = origin {
            command.env("ACTUALBUDGET_PUBLIC_ORIGIN", origin);
        }
        let mut process = command.spawn().unwrap();
        let mut line = String::new();
        BufReader::new(process.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert!(
            line.starts_with("LISTENING "),
            "server failed to start: {line}"
        );
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
    fn get(&self, path: &str) -> Response {
        self.client
            .get(format!("{}{path}", self.url))
            .send()
            .unwrap()
    }
    fn post(&self, path: &str, key: &str, body: Value) -> Response {
        self.client
            .post(format!("{}{path}", self.url))
            .header("Idempotency-Key", key)
            .json(&body)
            .send()
            .unwrap()
    }
    fn restart(mut self) -> Self {
        self.process.kill().unwrap();
        self.process.wait().unwrap();
        let data = std::mem::replace(&mut self.data, tempfile::tempdir().unwrap());
        Self::with_data(data)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

#[test]
fn companion_returns_ten_expenses_and_requested_older_entries_at_one_checkpoint() {
    let server = Server::start();
    let budget = uuid::Uuid::new_v4().to_string();
    let account = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        server
            .post(
                "/api/v1/budgets",
                "budget",
                json!({"id":budget,"name":"Companion"})
            )
            .status(),
        201
    );
    assert_eq!(
        server
            .post(
                &format!("/api/v1/budgets/{budget}/accounts"),
                "account",
                json!({"id":account,"name":"Spending","opening_balance":0,"opening_date":"2026-01-01"})
            )
            .status(),
        201
    );
    let mut ids = vec![];
    for number in 0..15 {
        let id = uuid::Uuid::new_v4().to_string();
        assert_eq!(server.post(&format!("/api/v1/budgets/{budget}/transactions"),&format!("expense-{number}"),json!({"id":id,"account_id":account,"amount":-100-number,"date":"2026-01-01","cleared":true})).status(),201);
        ids.push(id);
    }
    assert_eq!(server.post(&format!("/api/v1/budgets/{budget}/transactions"),"income",json!({"id":uuid::Uuid::new_v4(),"account_id":account,"amount":500,"date":"2027-01-01"})).status(),201);
    let path = format!(
        "/api/v1/budgets/{budget}/companion?account_id={account}&retain={}",
        ids[0]
    );
    let response = server.get(&path);
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let view = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(
        view["recent"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ids.iter()
            .rev()
            .take(10)
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    assert_eq!(view["retained"][0]["id"], ids[0]);
    assert_eq!(view["epoch"].as_str().unwrap().len(), 32);
    assert_eq!(
        server
            .get(&format!(
                "{path}&snapshot={}",
                view["snapshot"].as_str().unwrap()
            ))
            .status(),
        200
    );
    assert_eq!(server.post(&format!("/api/v1/budgets/{budget}/transactions"),"newer",json!({"id":uuid::Uuid::new_v4(),"account_id":account,"amount":-123,"date":"2026-01-02"})).status(),201);
    let server = server.restart();
    let changed = server.get(&format!(
        "{path}&snapshot={}",
        view["snapshot"].as_str().unwrap()
    ));
    assert_eq!(changed.status(), 409);
    assert_eq!(
        changed.json::<Value>().unwrap()["error"]["code"],
        "snapshot_changed"
    );
    assert_eq!(
        server
            .get(&format!(
                "/api/v1/budgets/{budget}/companion?account_id={account}&retain=invalid"
            ))
            .status(),
        400
    );
}
