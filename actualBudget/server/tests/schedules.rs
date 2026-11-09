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
fn schedule_calendar_matches_pinned_engine_without_writing() {
    let server = Server::start();
    let oracle: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/schedules.json")).unwrap();
    for case in oracle["calendar"].as_array().unwrap() {
        let response = server.post(
            "/api/v1/schedules/preview-dates",
            "calendar",
            json!({"config":case["config"],"from":case["as_of"],"count":16}),
        );
        assert_eq!(
            response.status(),
            200,
            "{}: {}",
            case["name"],
            response.text().unwrap()
        );
        let body: Value = response.json().unwrap();
        assert_eq!(body["data"], case["dates"], "{}", case["name"]);
    }
    assert_eq!(
        server.get("/api/v1/budgets").json::<Value>().unwrap()["data"],
        json!([])
    );
}
const B: &str = "21220000-0000-4000-8000-000000000100";
const A: &str = "21220000-0000-4000-8000-000000000101";
const S: &str = "21220000-0000-4000-8000-000000000102";
fn setup(server: &Server) {
    assert_eq!(
        server
            .post(
                "/api/v1/budgets",
                "budget",
                json!({"id":B,"name":"Schedules","budget_type":"tracking"})
            )
            .status(),
        201
    );
    assert_eq!(
        server
            .post(
                &format!("/api/v1/budgets/{B}/accounts"),
                "account",
                json!({"id":A,"name":"Cash","opening_balance":0,"opening_date":"2026-01-01"})
            )
            .status(),
        201
    );
}
fn checkpoint(server: &Server) -> Value {
    server
        .get(&format!("/api/v1/budgets/{B}/checkpoint"))
        .json::<Value>()
        .unwrap()["data"]
        .clone()
}
fn command(server: &Server, key: &str, mut body: Value) -> Response {
    let cp = checkpoint(server);
    body["snapshot"] = cp["snapshot"].clone();
    server
        .client
        .post(format!(
            "{}/api/v1/budgets/{B}/schedules/commands",
            server.url
        ))
        .header("Idempotency-Key", key)
        .header("X-Actual-Budget-Epoch", cp["epoch"].as_str().unwrap())
        .json(&body)
        .send()
        .unwrap()
}
#[test]
fn manual_schedule_posts_once_with_stable_occurrence_and_immutable_receipt() {
    let server = Server::start();
    setup(&server);
    let response = command(
        &server,
        "schedule",
        json!({"action":"create","id":S,"name":"Water","posts_transaction":false,"conditions":[{"field":"account","op":"is","value":A},{"field":"amount","op":"is","value":-1234},{"field":"date","op":"is","value":"2026-11-09"}]}),
    );
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    let cp = checkpoint(&server);
    let body =
        json!({"action":"post","id":S,"occurrence_date":"2026-11-09","snapshot":cp["snapshot"]});
    let send = |s: &Server| {
        s.client
            .post(format!("{}/api/v1/budgets/{B}/schedules/commands", s.url))
            .header("Idempotency-Key", "post-original")
            .header("X-Actual-Budget-Epoch", cp["epoch"].as_str().unwrap())
            .json(&body)
            .send()
            .unwrap()
    };
    let first = send(&server);
    assert_eq!(first.status(), 200, "{}", first.text().unwrap());
    let first: Value = first.json().unwrap();
    let server = server.restart();
    assert_eq!(send(&server).json::<Value>().unwrap(), first);
    let again = command(
        &server,
        "post-second-key",
        json!({"action":"post","id":S,"occurrence_date":"2026-11-09"}),
    );
    assert_eq!(again.status(), 200, "{}", again.text().unwrap());
    let rows = server
        .get(&format!("/api/v1/budgets/{B}/transactions?account_id={A}"))
        .json::<Value>()
        .unwrap();
    assert_eq!(rows["data"].as_array().unwrap().len(), 1);
    assert_eq!(rows["data"][0]["amount"], -1234);
    assert_eq!(rows["data"][0]["schedule_id"], S);
    assert_eq!(rows["balances"]["total"], -1234);
}

#[test]
fn genuine_source_schedules_activate_with_relationships_and_read_only_previews() {
    let server = Server::start();
    let response = server
        .client
        .post(format!("{}/api/v1/imports/actual", server.url))
        .header("Idempotency-Key", "source-schedules")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/schedules-26.10.zip").to_vec())
        .send()
        .unwrap();
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    let stage: Value = response.json().unwrap();
    let stage = &stage["data"];
    assert_eq!(stage["status"], "ready", "{stage}");
    let id = stage["id"].as_str().unwrap();
    let budget = stage["budget_id"].as_str().unwrap();
    let response = server.post(
        &format!("/api/v1/imports/{id}/activate"),
        "activate",
        json!({}),
    );
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    let path = format!("/api/v1/budgets/{budget}/schedules");
    let first: Value = server.get(&path).json().unwrap();
    let second: Value = server.get(&path).json().unwrap();
    assert_eq!(first, second);
    assert_eq!(first["data"]["schedules"].as_array().unwrap().len(), 4);
    assert_eq!(
        first["data"]["schedules"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["completed"] == true)
            .count(),
        3
    );
    let register: Value = server
        .get(&format!("/api/v1/budgets/{budget}/transactions"))
        .json()
        .unwrap();
    assert_eq!(register["data"].as_array().unwrap().len(), 7);
    for row in register["data"].as_array().unwrap() {
        assert!(row["schedule_id"].is_string(), "{row}");
    }
}

#[test]
fn missed_occurrences_catch_up_atomically_and_repeat_without_duplicate_purchases() {
    let server = Server::start();
    let stage: Value = server
        .client
        .post(format!("{}/api/v1/imports/actual", server.url))
        .header("Idempotency-Key", "missed-source")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/schedules-missed-26.10.zip").to_vec())
        .send()
        .unwrap()
        .json()
        .unwrap();
    let id = stage["data"]["id"].as_str().unwrap();
    let b = stage["data"]["budget_id"].as_str().unwrap();
    assert_eq!(stage["data"]["status"], "ready");
    assert_eq!(
        server
            .post(
                &format!("/api/v1/imports/{id}/activate"),
                "activate-missed",
                json!({})
            )
            .status(),
        201
    );
    let root = format!("/api/v1/budgets/{b}");
    let before: Value = server.get(&format!("{root}/transactions")).json().unwrap();
    let cp: Value = server.get(&format!("{root}/checkpoint")).json().unwrap();
    let body = json!({"action":"catch-up","through":"2026-11-09"});
    let send = |key: &str| {
        server
            .client
            .post(format!("{}{root}/schedules/commands", server.url))
            .header("Idempotency-Key", key)
            .header(
                "X-Actual-Budget-Epoch",
                cp["data"]["epoch"].as_str().unwrap(),
            )
            .json(&body)
            .send()
            .unwrap()
    };
    let first = send("catch-up");
    assert_eq!(first.status(), 200, "{}", first.text().unwrap());
    let first: Value = first.json().unwrap();
    assert_eq!(first["data"]["posted_count"], 5);
    assert_eq!(send("catch-up").json::<Value>().unwrap(), first);
    let second = send("catch-up-again");
    assert_eq!(second.status(), 200, "{}", second.text().unwrap());
    let second: Value = second.json().unwrap();
    assert_eq!(second["data"]["posted_count"], 0);
    let after: Value = server.get(&format!("{root}/transactions")).json().unwrap();
    let oracle: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/schedules.json")).unwrap();
    let expected = oracle["snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["step"] == "catch up")
        .unwrap();
    let project = |rows: &Value, schedule: &str, payee: &str| {
        let mut rows=rows.as_array().unwrap().iter().map(|r|json!({"schedule":r[schedule],"date":r["date"],"amount":r["amount"],"cleared":r["cleared"],"notes":r["notes"].as_str().unwrap_or(""),"has_payee":!r[payee].is_null()})).collect::<Vec<_>>();
        rows.sort_by_key(Value::to_string);
        rows
    };
    assert_eq!(
        project(&after["data"], "schedule_id", "payee_id"),
        project(&expected["transactions"], "schedule", "payee")
    );
    assert!(
        after["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == before["data"][0]["id"])
    );
    assert_eq!(after["balances"]["total"], -600);
    assert_eq!(after["balances"]["cleared"], 0);
    let still: Value = server.get(&format!("{root}/transactions")).json().unwrap();
    assert_eq!(still, after);
}

#[test]
fn genuine_schema18_upgrade_keeps_ledger_history_receipts_and_refreshes_blocked_schedule_stage() {
    let f: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/schedules-schema18.json")).unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::write(
        data.path().join("actualbudget.sqlite3"),
        include_bytes!("../../tests/fixtures/schedules-schema18.sqlite3"),
    )
    .unwrap();
    let server = Server::with_data(data);
    let b = f["metadata"]["id"].as_str().unwrap();
    let a = f["account"]["id"].as_str().unwrap();
    let root = format!("/api/v1/budgets/{b}");
    assert_eq!(
        server
            .get(&format!("{root}/transactions?account_id={a}&limit=1000"))
            .json::<Value>()
            .unwrap(),
        f["register"]
    );
    assert_eq!(
        server
            .get(&format!("{root}/checkpoint"))
            .json::<Value>()
            .unwrap(),
        f["checkpoint"]
    );
    let mut history = server.client.get(format!("{}{root}/history", server.url));
    for (k, v) in f["owner_headers"].as_object().unwrap() {
        history = history.header(k, v.as_str().unwrap());
    }
    assert_eq!(
        history.send().unwrap().json::<Value>().unwrap(),
        f["history"]
    );
    for i in 0..2 {
        let mut request = server
            .client
            .post(format!("{}{root}/transactions", server.url))
            .header("Idempotency-Key", f["creation_keys"][i].as_str().unwrap())
            .json(&f["creations"][i]);
        for (k, v) in f["owner_headers"].as_object().unwrap() {
            request = request.header(k, v.as_str().unwrap());
        }
        let response = request.send().unwrap();
        assert_eq!(response.status(), 201);
        assert_eq!(response.json::<Value>().unwrap(), f["created"][i]);
    }
    let stage = f["blocked_stage"]["data"]["id"].as_str().unwrap();
    let latest: Value = server
        .get(&format!("/api/v1/imports/{stage}"))
        .json()
        .unwrap();
    assert_eq!(latest["data"]["status"], "ready", "{latest}");
    let replay = server
        .client
        .post(format!("{}/api/v1/imports/actual", server.url))
        .header("Idempotency-Key", f["blocked_stage_key"].as_str().unwrap())
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/schedules-schema18-source.zip").to_vec())
        .send()
        .unwrap();
    assert_eq!(replay.json::<Value>().unwrap(), f["blocked_stage"]);
    assert_eq!(
        server
            .post(
                &format!("/api/v1/imports/{stage}/activate"),
                "activate-previously-blocked",
                json!({})
            )
            .status(),
        201
    );
    let target = Server::start();
    let upload = target
        .client
        .post(format!("{}/api/v1/restores", target.url))
        .header("Idempotency-Key", "native18")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/schedules-schema18.zip").to_vec())
        .send()
        .unwrap();
    assert_eq!(upload.status(), 201, "{}", upload.text().unwrap());
    let stage: Value = upload.json().unwrap();
    assert_eq!(stage["data"]["report"]["schema_version"], 19);
    let response = target.post(
        &format!(
            "{root}/restores/{}/activate",
            stage["data"]["id"].as_str().unwrap()
        ),
        "activate-native18",
        json!({"confirm_name":f["metadata"]["name"]}),
    );
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let restored: Value = target
        .get(&format!("{root}/transactions?account_id={a}&limit=1000"))
        .json()
        .unwrap();
    let mut expected = f["register"].clone();
    expected["snapshot"] = restored["snapshot"].clone();
    assert_eq!(restored, expected);
    assert_ne!(restored["snapshot"], f["register"]["snapshot"]);
}

#[test]
fn owned_post_undo_redo_and_same_budget_native_restore_keep_occurrence_identity() {
    let server = Server::start();
    setup(&server);
    let response = command(
        &server,
        "schedule",
        json!({"action":"create","id":S,"name":"Water","conditions":[{"field":"account","op":"is","value":A},{"field":"amount","op":"is","value":-1234},{"field":"date","op":"is","value":"2026-11-09"}]}),
    );
    assert_eq!(response.status(), 201);
    let cp = checkpoint(&server);
    let epoch = cp["epoch"].as_str().unwrap();
    let owner = "21220000-0000-4000-8000-000000000199";
    let owned = |method: reqwest::Method, path: &str, key: Option<&str>, body: Option<Value>| {
        let mut request = server
            .client
            .request(method, format!("{}{path}", server.url))
            .header("X-Actual-History-Owner", owner)
            .header("X-Actual-Budget-Epoch", epoch);
        if let Some(key) = key {
            request = request.header("Idempotency-Key", key);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        request.send().unwrap()
    };
    let root = format!("/api/v1/budgets/{B}");
    let response = owned(
        reqwest::Method::POST,
        &format!("{root}/schedules/commands"),
        Some("owned-post"),
        Some(
            json!({"action":"post","id":S,"occurrence_date":"2026-11-09","snapshot":cp["snapshot"]}),
        ),
    );
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let saved: Value = server
        .get(&format!("{root}/transactions?account_id={A}"))
        .json()
        .unwrap();
    let transaction = saved["data"][0]["id"].clone();
    let history: Value = owned(reqwest::Method::GET, &format!("{root}/history"), None, None)
        .json()
        .unwrap();
    let action = history["data"]["undo"]["id"].clone();
    let undo = owned(
        reqwest::Method::POST,
        &format!("{root}/history/undo"),
        Some("undo-post"),
        Some(json!({"action_id":action})),
    );
    assert_eq!(undo.status(), 200, "{}", undo.text().unwrap());
    let empty: Value = server
        .get(&format!("{root}/transactions?account_id={A}"))
        .json()
        .unwrap();
    assert_eq!(empty["data"], json!([]));
    assert_eq!(empty["balances"]["total"], 0);
    let redo = owned(
        reqwest::Method::POST,
        &format!("{root}/history/redo"),
        Some("redo-post"),
        Some(json!({"action_id":action})),
    );
    assert_eq!(redo.status(), 200, "{}", redo.text().unwrap());
    let rows: Value = server
        .get(&format!("{root}/transactions?account_id={A}"))
        .json()
        .unwrap();
    assert_eq!(rows["data"][0]["id"], transaction);
    assert_eq!(rows["data"][0]["schedule_id"], S);
    assert_eq!(rows["balances"], saved["balances"]);
    let response = server.post(
        &format!("{root}/backups"),
        "with-occurrence",
        json!({"label":"Schedule occurrence"}),
    );
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    let backup: Value = response.json().unwrap();
    let id = backup["data"]["id"].as_str().unwrap();
    let restore = server.post(
        &format!("{root}/backups/{id}/restore"),
        "same-budget-restore",
        json!({"confirm_name":"Schedules"}),
    );
    assert_eq!(restore.status(), 200, "{}", restore.text().unwrap());
    let restored: Value = server
        .get(&format!("{root}/transactions?account_id={A}"))
        .json()
        .unwrap();
    assert_eq!(restored["data"], rows["data"]);
    assert_eq!(restored["balances"], rows["balances"]);
    assert_ne!(restored["snapshot"], rows["snapshot"]);
    let response = command(
        &server,
        "post-after-restore",
        json!({"action":"post","id":S,"occurrence_date":"2026-11-09"}),
    );
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let unchanged: Value = server
        .get(&format!("{root}/transactions?account_id={A}"))
        .json()
        .unwrap();
    assert_eq!(unchanged["data"], restored["data"]);
    assert_eq!(unchanged["balances"]["total"], -1234);
}
