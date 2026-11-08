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
fn budget_and_exact_opening_balance_survive_process_restart_without_login() {
    let server = Server::start();
    create_budget(&server);
    let accounts = format!("/api/v1/budgets/{BUDGET}/accounts");
    let response=server.post(&accounts,"account-one",json!({"id":ACCOUNT,"name":"Spending","notes":"Cash expenses","off_budget":false,"opening_balance":-12345,"opening_date":"2026-01-31"}));
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    let server = server.restart();
    let budgets: Value = server.get("/api/v1/budgets").json().unwrap();
    assert_eq!(budgets["data"][0]["name"], "Home");
    let accounts: Value = server.get(&accounts).json().unwrap();
    assert_eq!(
        accounts["data"][0],
        json!({"id":ACCOUNT,"name":"Spending","notes":"Cash expenses","off_budget":false,"closed":false,"balance":-12345})
    );
}
#[test]
fn opening_balances_match_pinned_actual_public_api_fixture() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/opening-balances.json")).unwrap();
    let server = Server::start();
    create_budget(&server);
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let id = uuid::Uuid::new_v4().to_string();
        let mut input = case["input"].clone();
        input["id"] = json!(id);
        let response = server.post(
            &format!("/api/v1/budgets/{BUDGET}/accounts"),
            &format!("account-{index}"),
            input,
        );
        assert_eq!(response.status(), 201, "{}", response.text().unwrap());
        let created: Value = response.json().unwrap();
        assert_eq!(created["data"]["balance"], case["expected"]["balance"]);
        let response = server.get(&format!(
            "/api/v1/budgets/{BUDGET}/transactions?account_id={id}"
        ));
        assert_eq!(response.status(), 200);
        let rows: Value = response.json().unwrap();
        let normalized:Vec<Value>=rows["data"].as_array().unwrap().iter().map(|row|json!({
            "amount":row["amount"],"date":row["date"],"cleared":row["cleared"],"reconciled":row["reconciled"],
            "starting_balance":row["starting_balance"],"payee_name":row["payee_name"],"category_name":row["category_name"],"category_is_income":row["category_is_income"]
        })).collect();
        assert_eq!(
            json!(normalized),
            case["expected"]["transactions"],
            "{}",
            case["input"]["name"]
        );
    }
}
#[test]
fn retries_are_atomic_and_durable_and_reject_reused_keys_with_changed_payloads() {
    let server = Server::start();
    create_budget(&server);
    let path = format!("/api/v1/budgets/{BUDGET}/accounts");
    let input =
        json!({"id":ACCOUNT,"name":"Cash","opening_balance":123,"opening_date":"2026-10-15"});
    std::thread::scope(|scope| {
        let threads: Vec<_> = (0..6)
            .map(|_| scope.spawn(|| server.post(&path, "same-operation", input.clone())))
            .collect();
        let mut first = None;
        for thread in threads {
            let response = thread.join().unwrap();
            assert_eq!(response.status(), 201, "{}", response.text().unwrap());
            let body: Value = response.json().unwrap();
            if let Some(expected) = &first {
                assert_eq!(&body, expected);
            } else {
                first = Some(body);
            }
        }
    });
    let server = server.restart();
    assert_eq!(
        server.post(&path, "same-operation", input.clone()).status(),
        201
    );
    let mut changed = input.clone();
    changed["opening_balance"] = json!(999);
    let response = server.post(&path, "same-operation", changed);
    assert_eq!(response.status(), 409);
    assert_eq!(
        response.json::<Value>().unwrap()["error"]["code"],
        "idempotency_conflict"
    );
    assert_eq!(
        server.post(&path, "different-operation", input).status(),
        409
    );
    let accounts: Value = server.get(&path).json().unwrap();
    assert_eq!(accounts["data"].as_array().unwrap().len(), 1);
    let tx: Value = server
        .get(&format!(
            "/api/v1/budgets/{BUDGET}/transactions?account_id={ACCOUNT}"
        ))
        .json()
        .unwrap();
    assert_eq!(tx["data"].as_array().unwrap().len(), 1);
    assert_eq!(tx["data"][0]["amount"], 123);
}
#[test]
fn malformed_writes_are_structured_and_leave_no_partial_records() {
    let server = Server::start();
    create_budget(&server);
    let path = format!("/api/v1/budgets/{BUDGET}/accounts");
    let valid =
        json!({"id":ACCOUNT,"name":"Cash","opening_balance":123,"opening_date":"2026-10-15"});
    for (field, value) in [
        ("id", json!("not-a-uuid")),
        ("name", json!("  ")),
        ("opening_balance", json!(9007199254740992_i64)),
        ("opening_balance", json!(1.5)),
        ("opening_date", json!("2026-02-30")),
        ("opening_date", json!("2026-1-1")),
        ("off_budget", json!("yes")),
        ("surprise", json!(true)),
    ] {
        let mut input = valid.clone();
        input[field] = value;
        let response = server.post(&path, "invalid", input);
        assert_eq!(response.status(), 400, "invalid {field}");
        assert_eq!(
            response.json::<Value>().unwrap()["error"]["code"],
            "invalid_request"
        );
    }
    let response = server
        .client
        .post(format!("{}{path}", server.url))
        .json(&valid)
        .send()
        .unwrap();
    assert_eq!(response.status(), 400);
    let response = server
        .client
        .post(format!("{}{path}", server.url))
        .header("Content-Type", "application/json")
        .body("{broken")
        .send()
        .unwrap();
    assert_eq!(response.status(), 400);
    assert!(response.json::<Value>().unwrap()["error"]["message"].is_string());
    assert_eq!(
        server.get(&path).json::<Value>().unwrap()["data"],
        json!([])
    );
    assert_eq!(
        server.post(&path, "invalid", valid).status(),
        201,
        "validation must not consume operation key"
    );
    assert_eq!(
        server
            .get("/api/v1/budgets/00000000-0000-4000-8000-000000000001/accounts")
            .status(),
        404
    );
    assert_eq!(
        server
            .get(&format!(
                "/api/v1/budgets/{BUDGET}/transactions?account_id={ACCOUNT}&limit=1001"
            ))
            .status(),
        400
    );
}
#[test]
fn browser_writes_require_same_origin_but_native_clients_need_no_credentials() {
    let server = Server::start();
    let input = json!({"id":BUDGET,"name":"Home","budget_type":"tracking"});
    for origin in ["https://hostile.example", "null", "http://127.0.0.1:1"] {
        let response = server
            .client
            .post(format!("{}/api/v1/budgets", server.url))
            .header("Origin", origin)
            .header("Idempotency-Key", "origin")
            .json(&input)
            .send()
            .unwrap();
        assert_eq!(response.status(), 403);
        assert_eq!(
            response.json::<Value>().unwrap()["error"]["code"],
            "origin_rejected"
        );
    }
    let response = server
        .client
        .post(format!("{}/api/v1/budgets", server.url))
        .header("Sec-Fetch-Site", "cross-site")
        .header("Idempotency-Key", "origin")
        .json(&input)
        .send()
        .unwrap();
    assert_eq!(response.status(), 403);
    assert_eq!(
        server.get("/api/v1/budgets").json::<Value>().unwrap()["data"],
        json!([])
    );
    let response = server
        .client
        .post(format!("{}/api/v1/budgets", server.url))
        .header("Origin", &server.url)
        .header("Idempotency-Key", "origin")
        .json(&input)
        .send()
        .unwrap();
    assert_eq!(response.status(), 201);
    assert_eq!(server.get("/api/v1/budgets").status(), 200);
}

#[test]
fn static_app_and_versioned_machine_contract_are_available_on_plain_http() {
    let server = Server::start();
    let index = server.get("/");
    assert_eq!(index.status(), 200);
    assert!(index.text().unwrap().contains("<title>Budget</title>"));
    assert_eq!(server.get("/budgets/chosen").status(), 200);
    let script = server.get("/app.js");
    assert_eq!(script.status(), 200);
    assert!(
        script.headers()["content-type"]
            .to_str()
            .unwrap()
            .contains("javascript")
    );
    let info: Value = server.get("/api/v1/info").json().unwrap();
    assert_eq!(info["data"]["api_version"], 1);
    assert_eq!(info["data"]["authentication"], "none");
    assert!(
        info["data"]["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("opening_balances"))
    );
    let contract: Value = server
        .get(info["data"]["openapi"].as_str().unwrap())
        .json()
        .unwrap();
    assert_eq!(contract["openapi"], "3.1.0");
    assert!(
        contract["paths"]["/api/v1/budgets/{budget}/accounts"]["post"]["responses"]["201"]
            .is_object()
    );
    assert_eq!(
        contract["components"]["schemas"]["NewAccount"]["properties"]["opening_balance"]["maximum"],
        9007199254740991_i64
    );
    let missing = server.get("/api/v2/not-supported");
    assert_eq!(missing.status(), 404);
    assert_eq!(
        missing.json::<Value>().unwrap()["error"]["code"],
        "not_found"
    );
    let method = server
        .client
        .delete(format!("{}/api/v1/budgets", server.url))
        .send()
        .unwrap();
    assert_eq!(method.status(), 405);
    assert_eq!(
        method.json::<Value>().unwrap()["error"]["code"],
        "method_not_allowed"
    );
}

#[test]
fn budget_scoping_preserves_safe_integer_extremes_and_failed_writes_are_retryable() {
    let server = Server::start();
    create_budget(&server);
    let second = "00000000-0000-4000-8000-000000000002";
    let input = json!({"id":ACCOUNT,"name":"Exact","off_budget":true,"opening_balance":9007199254740991_i64,"opening_date":"2024-02-29"});
    let first_path = format!("/api/v1/budgets/{BUDGET}/accounts");
    let second_path = format!("/api/v1/budgets/{second}/accounts");
    assert_eq!(
        server.post(&second_path, "account", input.clone()).status(),
        404
    );
    let response = server.post(
        "/api/v1/budgets",
        "second-budget",
        json!({"id":second,"name":"Other","budget_type":"envelope"}),
    );
    assert_eq!(response.status(), 201);
    assert_eq!(
        server.post(&second_path, "account", input.clone()).status(),
        201
    );
    let mut negative = input;
    negative["opening_balance"] = json!(-9007199254740991_i64);
    assert_eq!(server.post(&first_path, "account", negative).status(), 201);
    let server = server.restart();
    assert_eq!(
        server.get(&first_path).json::<Value>().unwrap()["data"][0]["balance"],
        -9007199254740991_i64
    );
    assert_eq!(
        server.get(&second_path).json::<Value>().unwrap()["data"][0]["balance"],
        9007199254740991_i64
    );
    let invalid = server.get("/api/v1/budgets/%FF/accounts");
    assert_eq!(invalid.status(), 400);
    assert_eq!(
        invalid.json::<Value>().unwrap()["error"]["code"],
        "invalid_request"
    );
}

#[test]
fn unsupported_and_oversized_bodies_fail_before_creating_a_budget() {
    let server = Server::start();
    for input in [
        json!({"id":BUDGET,"name":"Home","budget_type":"unknown"}),
        json!({"id":BUDGET,"name":""}),
        json!({"id":"unsafe","name":"Home"}),
    ] {
        let response = server.post("/api/v1/budgets", "budget", input);
        assert_eq!(response.status(), 400);
        assert_eq!(
            response.json::<Value>().unwrap()["error"]["code"],
            "invalid_request"
        );
    }
    let response = server.post(
        "/api/v1/budgets",
        "budget",
        json!({"id":BUDGET,"name":"x".repeat(70000)}),
    );
    assert_eq!(response.status(), 413);
    assert_eq!(
        response.json::<Value>().unwrap()["error"]["code"],
        "invalid_request"
    );
    let response = server
        .client
        .post(format!("{}/api/v1/budgets", server.url))
        .header("Content-Type", "text/plain")
        .body("{}")
        .send()
        .unwrap();
    assert_eq!(response.status(), 415);
    assert!(response.json::<Value>().unwrap()["error"].is_object());
    assert_eq!(
        server.get("/api/v1/budgets").json::<Value>().unwrap()["data"],
        json!([])
    );
    create_budget(&server);
}

#[test]
fn configured_proxy_origin_is_explicit_and_forwarded_headers_cannot_override_it() {
    let server = Server::with_origin(tempfile::tempdir().unwrap(), Some("https://budget.lan"));
    let input = json!({"id":BUDGET,"name":"Home"});
    let response = server
        .client
        .post(format!("{}/api/v1/budgets", server.url))
        .header("Origin", "https://hostile.example")
        .header("X-Forwarded-Host", "hostile.example")
        .header("X-Forwarded-Proto", "https")
        .header("Idempotency-Key", "proxy")
        .json(&input)
        .send()
        .unwrap();
    assert_eq!(response.status(), 403);
    let response = server
        .client
        .post(format!("{}/api/v1/budgets", server.url))
        .header("Origin", "https://budget.lan")
        .header("Idempotency-Key", "proxy")
        .json(&input)
        .send()
        .unwrap();
    assert_eq!(response.status(), 201);
    assert_eq!(server.get("/api/v1/budgets").status(), 200);
}
