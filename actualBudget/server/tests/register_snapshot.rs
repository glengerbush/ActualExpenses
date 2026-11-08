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
    _data: TempDir,
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
            _data: data,
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
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
const BUDGET: &str = "7b7e5bcc-9b19-418d-8bc2-20661dd6cf09";
const ACCOUNT: &str = "b4cdd385-e9e6-4dca-827a-c4cf9a153eaa";
fn create_budget(server: &Server) {
    let response = server.post(
        "/api/v1/budgets",
        "budget-one",
        json!({"id":BUDGET,"name":"Home","budget_type":"tracking"}),
    );
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
}
#[test]
fn stable_pages_remain_coherent_and_insert_edit_delete_invalidate_continuations() {
    let server = Server::start();
    create_budget(&server);
    assert_eq!(server.post(&format!("/api/v1/budgets/{BUDGET}/accounts"), "account", json!({"id":ACCOUNT,"name":"Spending","opening_balance":0,"opening_date":"2026-01-01"})).status(),201);
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    for index in 1..=3 {
        assert_eq!(server.post(&path,&format!("entry-{index}"),json!({"id":format!("10000000-0000-4000-8000-{index:012}"),"account_id":ACCOUNT,"amount":-100,"date":format!("2026-01-0{index}")})).status(),201);
    }
    let first: Value = server
        .get(&format!("{path}?account_id={ACCOUNT}&limit=2"))
        .json()
        .unwrap();
    let snapshot = first["snapshot"]
        .as_str()
        .expect("every page returns an opaque snapshot");
    let continuation = format!("{path}?account_id={ACCOUNT}&limit=2&offset=2&snapshot={snapshot}");
    let next = server.get(&continuation);
    assert_eq!(next.status(), 200);
    let next: Value = next.json().unwrap();
    assert_eq!(next["snapshot"], snapshot);
    assert_eq!(next["data"].as_array().unwrap().len(), 1);
    assert_eq!(
        next["data"][0]["id"],
        "10000000-0000-4000-8000-000000000001"
    );
    assert_eq!(next["balances"]["total"], -300);
    assert_eq!(server.post(&path,"newest",json!({"id":"10000000-0000-4000-8000-000000000004","account_id":ACCOUNT,"amount":-200,"date":"2026-01-04"})).status(),201);
    let changed = server.get(&continuation);
    assert_eq!(changed.status(), 409);
    assert_eq!(
        changed.json::<Value>().unwrap()["error"]["code"],
        "register_changed"
    );
    let refreshed: Value = server
        .get(&format!("{path}?account_id={ACCOUNT}&limit=2"))
        .json()
        .unwrap();
    assert_ne!(refreshed["snapshot"], snapshot);
    assert_eq!(refreshed["balances"]["total"], -500);
    for (method, key, body, expected_total) in [
        (
            reqwest::Method::PATCH,
            "edit",
            json!({"amount":-300,"notes":"Changed on another device"}),
            -700,
        ),
        (reqwest::Method::DELETE, "delete", json!({}), -400),
    ] {
        let page: Value = server
            .get(&format!("{path}?account_id={ACCOUNT}&limit=2"))
            .json()
            .unwrap();
        let old_snapshot = page["snapshot"].as_str().unwrap();
        let continuation =
            format!("{path}?account_id={ACCOUNT}&limit=2&offset=2&snapshot={old_snapshot}");
        let response = server
            .client
            .request(
                method.clone(),
                format!("{}{path}/10000000-0000-4000-8000-000000000001", server.url),
            )
            .header("Idempotency-Key", key)
            .json(&body)
            .send()
            .unwrap();
        assert_eq!(response.status(), 200);
        let changed = server.get(&continuation);
        assert_eq!(changed.status(), 409);
        assert_eq!(
            changed.json::<Value>().unwrap()["error"]["code"],
            "register_changed"
        );
        let fresh: Value = server
            .get(&format!("{path}?account_id={ACCOUNT}&limit=2"))
            .json()
            .unwrap();
        assert_eq!(fresh["balances"]["total"], expected_total);
        // A replay has no new effect and must not invalidate an otherwise stable page.
        assert_eq!(
            server
                .client
                .request(
                    method,
                    format!("{}{path}/10000000-0000-4000-8000-000000000001", server.url)
                )
                .header("Idempotency-Key", key)
                .json(&body)
                .send()
                .unwrap()
                .status(),
            200
        );
        let again: Value = server
            .get(&format!("{path}?account_id={ACCOUNT}&limit=2"))
            .json()
            .unwrap();
        assert_eq!(fresh, again);
    }
}
#[test]
fn malformed_snapshot_tokens_are_structured_invalid_requests() {
    let server = Server::start();
    create_budget(&server);
    for token in [
        "",
        "r1000000000000000",
        "r100000000000000000",
        "r1000000000000000g",
        "r1000000000000000A",
        "%C3%A9",
        "r1%F0%9F%98%80",
    ] {
        let response = server.get(&format!(
            "/api/v1/budgets/{BUDGET}/transactions?snapshot={token}"
        ));
        assert_eq!(response.status(), 400, "{token}");
        assert_eq!(
            response.json::<Value>().unwrap()["error"]["code"],
            "invalid_request"
        );
    }
}
