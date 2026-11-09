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
        json!({"id":ACCOUNT,"name":"Spending","notes":"Cash expenses","off_budget":false,"closed":false,"balance":-12345,"group_id":null,"sort_order":0,"deleted":false,"field_versions":{}})
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
    for (verb, path) in [
        (reqwest::Method::DELETE, "/api/v1/budgets"),
        (reqwest::Method::POST, "/api/v1/themes/catalog"),
        (reqwest::Method::POST, "/api/v1/imports"),
    ] {
        let method = server
            .client
            .request(verb, format!("{}{path}", server.url))
            .send()
            .unwrap();
        assert_eq!(method.status(), 405, "{path}");
        assert_eq!(
            method
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok()),
            Some("application/json"),
            "{path}"
        );
        assert_eq!(
            method.json::<Value>().unwrap()["error"]["code"],
            "method_not_allowed",
            "{path}"
        );
    }
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

#[test]
fn ordinary_income_expense_and_refund_have_exact_register_balances_after_restart() {
    let server = Server::start();
    create_budget(&server);
    let accounts = format!("/api/v1/budgets/{BUDGET}/accounts");
    assert_eq!(server.post(&accounts,"spending",json!({"id":ACCOUNT,"name":"Spending","opening_balance":10000,"opening_date":"2026-01-01"})).status(),201);
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    for (index, (amount, date, cleared, notes)) in [
        (20000, "2026-01-02", true, "Salary"),
        (-1234, "2026-01-03", false, "Lunch"),
        (234, "2026-01-04", true, "Refund"),
    ]
    .into_iter()
    .enumerate()
    {
        let response=server.post(&path,&format!("entry-{index}"),json!({"id":uuid::Uuid::new_v4().to_string(),"account_id":ACCOUNT,"amount":amount,"date":date,"cleared":cleared,"notes":notes}));
        assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    }
    let server = server.restart();
    let register: Value = server
        .get(&format!("{path}?account_id={ACCOUNT}"))
        .json()
        .unwrap();
    assert_eq!(
        register["balances"],
        json!({"total":29000,"cleared":30234,"uncleared":-1234})
    );
    assert_eq!(register["data"][0]["notes"], "Refund");
    assert_eq!(register["data"][0]["running_balance"], 29000);
    assert_eq!(register["data"][1]["running_balance"], 28766);
    assert_eq!(
        server.get(&accounts).json::<Value>().unwrap()["data"][0]["balance"],
        29000
    );
}

#[test]
fn edit_duplicate_and_delete_use_stable_operations_and_keep_balances_exact() {
    let server = Server::start();
    create_budget(&server);
    assert_eq!(
        server
            .post(
                &format!("/api/v1/budgets/{BUDGET}/accounts"),
                "account",
                json!({"id":ACCOUNT,"name":"Cash","opening_balance":0,"opening_date":"2026-01-01"})
            )
            .status(),
        201
    );
    let id = "00000000-0000-4000-8000-000000000010";
    let duplicate = "00000000-0000-4000-8000-000000000011";
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    assert_eq!(server.post(&path,"new",json!({"id":id,"account_id":ACCOUNT,"amount":-1234,"date":"2026-01-01","cleared":true,"notes":"Lunch"})).status(),201);
    let mutation = |method: reqwest::Method, suffix: &str, key: &str, body: Value| {
        server
            .client
            .request(method, format!("{}{path}{suffix}", server.url))
            .header("Idempotency-Key", key)
            .json(&body)
            .send()
            .unwrap()
    };
    let edited = mutation(
        reqwest::Method::PATCH,
        &format!("/{id}"),
        "edit",
        json!({"amount":-1200,"date":"2024-02-29","notes":"Corrected"}),
    );
    assert_eq!(edited.status(), 200, "{}", edited.text().unwrap());
    assert_eq!(edited.json::<Value>().unwrap()["data"]["cleared"], true);
    let copied = server.post(
        &format!("{path}/{id}/duplicate"),
        "duplicate",
        json!({"id":duplicate}),
    );
    assert_eq!(copied.status(), 201);
    let copied: Value = copied.json().unwrap();
    assert_eq!(copied["data"]["cleared"], false);
    assert_eq!(copied["data"]["reconciled"], false);
    assert_eq!(copied["data"]["amount"], -1200);
    assert_eq!(
        mutation(
            reqwest::Method::DELETE,
            &format!("/{id}"),
            "delete",
            json!({})
        )
        .status(),
        200
    );
    let server = server.restart();
    let replay = server
        .client
        .delete(format!("{}{path}/{id}", server.url))
        .header("Idempotency-Key", "delete")
        .send()
        .unwrap();
    assert_eq!(replay.status(), 200);
    assert_eq!(
        server
            .post(
                &format!("{path}/{id}/duplicate"),
                "duplicate",
                json!({"id":duplicate})
            )
            .status(),
        201
    );
    let rows: Value = server
        .get(&format!("{path}?account_id={ACCOUNT}"))
        .json()
        .unwrap();
    assert_eq!(rows["data"].as_array().unwrap().len(), 1);
    assert_eq!(rows["balances"]["total"], -1200);
    assert_eq!(
        server
            .post(
                &path,
                "reuse-deleted",
                json!({"id":id,"account_id":ACCOUNT,"amount":1,"date":"2026-01-01"})
            )
            .status(),
        409
    );
}

#[test]
fn payees_and_categories_can_be_created_selected_and_cleared_without_partial_writes() {
    let server = Server::start();
    create_budget(&server);
    server.post(
        &format!("/api/v1/budgets/{BUDGET}/accounts"),
        "account",
        json!({"id":ACCOUNT,"name":"Cash","opening_balance":0,"opening_date":"2026-01-01"}),
    );
    let payee = "00000000-0000-4000-8000-000000000020";
    let category = "00000000-0000-4000-8000-000000000021";
    let entry = "00000000-0000-4000-8000-000000000022";
    for (resource, id, name) in [("payees", payee, "Café"), ("categories", category, "Food")] {
        let path = format!("/api/v1/budgets/{BUDGET}/{resource}");
        let response = server.post(&path, resource, json!({"id":id,"name":name}));
        assert_eq!(response.status(), 201, "{}", response.text().unwrap());
        assert!(
            server.get(&path).json::<Value>().unwrap()["data"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["id"] == id)
        );
    }
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    let mut input = json!({"id":entry,"account_id":ACCOUNT,"amount":-123,"date":"2026-01-01","payee_id":payee,"category_id":"00000000-0000-4000-8000-000000000099"});
    assert_eq!(server.post(&path, "entry", input.clone()).status(), 400);
    assert!(
        server
            .get(&format!("{path}?account_id={ACCOUNT}"))
            .json::<Value>()
            .unwrap()["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    input["category_id"] = json!(category);
    assert_eq!(server.post(&path, "entry", input).status(), 201);
    let updated = server
        .client
        .patch(format!("{}{path}/{entry}", server.url))
        .header("Idempotency-Key", "uncategorize")
        .json(&json!({"category_id":null,"payee_id":null}))
        .send()
        .unwrap();
    assert_eq!(updated.status(), 200);
    let updated: Value = updated.json().unwrap();
    assert_eq!(updated["data"]["category_id"], Value::Null);
    assert_eq!(updated["data"]["payee_id"], Value::Null);
}

#[test]
fn calculator_matches_pinned_actual_rounding_precedence_and_prefix_behavior() {
    let server = Server::start();
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/calculator.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let response = server.post(
            "/api/v1/amounts/evaluate",
            "unused",
            json!({"expression":case["canonical"],"decimal_places":case["decimal_places"]}),
        );
        assert_eq!(response.status(), 200, "{}", response.text().unwrap());
        assert_eq!(
            response.json::<Value>().unwrap()["data"]["amount"],
            case["expected"],
            "{}",
            case["expression"]
        );
    }
    for expression in ["1/0", "0/0", "999999999999999999999999", "(", "no amount"] {
        assert_eq!(
            server
                .post(
                    "/api/v1/amounts/evaluate",
                    "unused",
                    json!({"expression":expression})
                )
                .status(),
            400,
            "{expression}"
        );
    }
}

#[test]
fn register_mutations_and_date_cutoffs_match_independent_actual_fixture() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/register.json")).unwrap();
    let mut server = Server::start();
    create_budget(&server);
    let mut account = fixture["input"]["account"].clone();
    account["id"] = json!(ACCOUNT);
    assert_eq!(
        server
            .post(
                &format!("/api/v1/budgets/{BUDGET}/accounts"),
                "account",
                account
            )
            .status(),
        201
    );
    let payee = "00000000-0000-4000-8000-000000000070";
    let category = "00000000-0000-4000-8000-000000000071";
    server.post(
        &format!("/api/v1/budgets/{BUDGET}/payees"),
        "payee",
        json!({"id":payee,"name":"Cafe"}),
    );
    server.post(
        &format!("/api/v1/budgets/{BUDGET}/categories"),
        "category",
        json!({"id":category,"name":"Food"}),
    );
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    for (index, operation) in fixture["input"]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let mut requests = Vec::new();
        match operation["action"].as_str().unwrap() {
            "create" => {
                for entry in fixture["input"]["transactions"].as_array().unwrap() {
                    let mut entry = entry.clone();
                    entry["account_id"] = json!(ACCOUNT);
                    let object = entry.as_object_mut().unwrap();
                    if object.remove("payee_name").is_some() {
                        object.insert("payee_id".into(), json!(payee));
                    }
                    if object.remove("category_name").is_some() {
                        object.insert("category_id".into(), json!(category));
                    }
                    requests.push((reqwest::Method::POST, path.clone(), entry, 201));
                }
            }
            "update" => requests.push((
                reqwest::Method::PATCH,
                format!("{path}/{}", operation["id"].as_str().unwrap()),
                operation["patch"].clone(),
                200,
            )),
            "duplicate" => requests.push((
                reqwest::Method::POST,
                format!(
                    "{path}/{}/duplicate",
                    operation["source_id"].as_str().unwrap()
                ),
                json!({"id":operation["id"]}),
                201,
            )),
            "delete" => requests.push((
                reqwest::Method::DELETE,
                format!("{path}/{}", operation["id"].as_str().unwrap()),
                json!({}),
                200,
            )),
            other => panic!("Unknown fixture action {other}"),
        }
        for round in 0..2 {
            if round == 1 {
                server = server.restart();
            }
            for (n, (method, path, body, status)) in requests.iter().enumerate() {
                let response = server
                    .client
                    .request(method.clone(), format!("{}{path}", server.url))
                    .header("Idempotency-Key", format!("fixture-{index}-{n}"))
                    .json(body)
                    .send()
                    .unwrap();
                assert_eq!(
                    response.status().as_u16(),
                    *status,
                    "{}",
                    response.text().unwrap()
                );
            }
        }
        let snapshot = &fixture["snapshots"][index];
        let register: Value = server
            .get(&format!("{path}?account_id={ACCOUNT}"))
            .json()
            .unwrap();
        assert_eq!(
            register["balances"], snapshot["balances"],
            "{}",
            snapshot["name"]
        );
        let mut normalized:Vec<Value>=register["data"].as_array().unwrap().iter().map(|entry|json!({"id":if entry["starting_balance"]==true {json!("opening")}else{entry["id"].clone()},"date":entry["date"],"amount":entry["amount"],"notes":entry["notes"],"cleared":entry["cleared"],"reconciled":entry["reconciled"],"starting_balance":entry["starting_balance"],"payee_name":entry["payee_name"],"category_name":entry["category_name"]})).collect();
        normalized.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        assert_eq!(json!(normalized), snapshot["transactions"]);
        for cutoff in snapshot["through_dates"].as_array().unwrap() {
            let rows: Value = server
                .get(&format!(
                    "{path}?account_id={ACCOUNT}&as_of={}",
                    cutoff["date"].as_str().unwrap()
                ))
                .json()
                .unwrap();
            assert_eq!(rows["balances"]["total"], cutoff["balance"]);
        }
    }
}

#[test]
fn rejected_financial_change_rolls_back_inline_references_and_operation_outcome() {
    let server = Server::start();
    create_budget(&server);
    server.post(&format!("/api/v1/budgets/{BUDGET}/accounts"),"account",json!({"id":ACCOUNT,"name":"Maximum","opening_balance":9007199254740991_i64,"opening_date":"2026-01-01"}));
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    let id = "00000000-0000-4000-8000-000000000080";
    let payee = "00000000-0000-4000-8000-000000000081";
    let mut input = json!({"id":id,"account_id":ACCOUNT,"amount":1,"date":"2026-01-02","new_payee":{"id":payee,"name":"Never orphaned"}});
    assert_eq!(server.post(&path, "entry", input.clone()).status(), 400);
    let payees: Value = server
        .get(&format!("/api/v1/budgets/{BUDGET}/payees"))
        .json()
        .unwrap();
    assert!(
        !payees["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value["id"] == payee)
    );
    input["amount"] = json!(-1);
    assert_eq!(server.post(&path, "entry", input).status(), 201);
    let page: Value = server
        .get(&format!("{path}?account_id={ACCOUNT}&limit=1&offset=1"))
        .json()
        .unwrap();
    assert_eq!(page["data"][0]["running_balance"], 9007199254740991_i64);
}

#[test]
fn schema_one_upgrade_preserves_data_replay_and_restorable_backup() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("actualbudget.sqlite3"),
        include_bytes!("../../tests/fixtures/server-v1.sqlite3"),
    )
    .unwrap();
    let server = Server::with_data(directory);
    let budget = "20000000-0000-4000-8000-000000000001";
    let account = "20000000-0000-4000-8000-000000000002";
    let path = format!("/api/v1/budgets/{budget}/accounts");
    let original: Value = server.get(&path).json().unwrap();
    assert_eq!(original["data"][0]["balance"], -12345);
    assert_eq!(original["data"][0]["notes"], "Schema one data");
    let replay=server.post(&path,"account",json!({"id":account,"name":"Preserved account","opening_balance":-12345,"opening_date":"2024-02-29","notes":"Schema one data"}));
    assert_eq!(replay.status(), 201);
    // Historical receipts stay byte-equivalent; fresh reads include schema 7 metadata.
    assert_eq!(
        replay.json::<Value>().unwrap()["data"],
        json!({"id":account,"name":"Preserved account","notes":"Schema one data","off_budget":false,"closed":false,"balance":-12345})
    );
    assert_eq!(original["data"][0]["group_id"], Value::Null);
    assert_eq!(original["data"][0]["deleted"], false);
    let backups = std::fs::read_dir(server.data.path().join("backups"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(backups.len(), 1);
    let restored = tempfile::tempdir().unwrap();
    std::fs::copy(
        backups[0].path(),
        restored.path().join("actualbudget.sqlite3"),
    )
    .unwrap();
    assert_eq!(server.post(&format!("/api/v1/budgets/{budget}/transactions"),"after-upgrade",json!({"id":"20000000-0000-4000-8000-000000000003","account_id":account,"amount":345,"date":"2026-01-01"})).status(),201);
    let server = server.restart();
    assert_eq!(
        server.get(&path).json::<Value>().unwrap()["data"][0]["balance"],
        -12000
    );
    let backup = Server::with_data(restored);
    assert_eq!(
        backup.get(&path).json::<Value>().unwrap()["data"],
        original["data"]
    );
}
