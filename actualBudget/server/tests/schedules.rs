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
const P: &str = "21220000-0000-4000-8000-000000000103";
const PAYEE: &str = "21220000-0000-4000-8000-000000000104";
const CATEGORY: &str = "21220000-0000-4000-8000-000000000105";
fn mapped(value: &Value, ids: &std::collections::BTreeMap<String, String>) -> Value {
    match value {
        Value::String(v) => ids.get(v).map_or_else(|| value.clone(), |id| json!(id)),
        Value::Array(items) => Value::Array(items.iter().map(|v| mapped(v, ids)).collect()),
        Value::Object(items) => Value::Object(
            items
                .iter()
                .map(|(k, v)| (k.clone(), mapped(v, ids)))
                .collect(),
        ),
        _ => value.clone(),
    }
}
#[test]
fn linked_schedule_financial_pipeline_matches_independent_settled_source_ledger() {
    let oracle: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/schedule-financial.json")).unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let s = Server::start();
        setup(&s);
        let root = format!("/api/v1/budgets/{B}");
        assert_eq!(
            s.post(
                &format!("{root}/accounts"),
                "peer",
                json!({"id":P,"name":"Savings","opening_balance":0,"opening_date":"2026-01-01"})
            )
            .status(),
            201
        );
        assert_eq!(
            s.post(
                &format!("{root}/payees"),
                "payee",
                json!({"id":PAYEE,"name":"Bill"})
            )
            .status(),
            201
        );
        assert_eq!(
            s.post(
                &format!("{root}/categories"),
                "category",
                json!({"id":CATEGORY,"name":"Utilities","is_income":false})
            )
            .status(),
            201
        );
        let mut ids = std::collections::BTreeMap::from([
            ("ACCOUNT".into(), A.into()),
            ("PEER".into(), P.into()),
            ("PAYEE".into(), PAYEE.into()),
            ("CATEGORY".into(), CATEGORY.into()),
        ]);
        if case["transfer"] == true || case["name"].as_str().unwrap().contains("split transfer") {
            // Ordinary public transfer lifecycle establishes account-managed identities;
            // removing the zero-value pair leaves the financial fixture empty.
            let seed = "21220000-0000-4000-8000-000000000220";
            let response=s.post(&format!("{root}/transfers"),"bootstrap-transfer",json!({"id":seed,"counterpart_id":"21220000-0000-4000-8000-000000000221","account_id":A,"to_account_id":P,"amount":0,"date":"2026-11-09"}));
            assert_eq!(response.status(), 201, "{}", response.text().unwrap());
            let response = s
                .client
                .delete(format!("{}{root}/transactions/{seed}", s.url))
                .header("Idempotency-Key", "remove-bootstrap")
                .json(&json!({}))
                .send()
                .unwrap();
            assert_eq!(response.status(), 200, "{}", response.text().unwrap());
            let payees: Value = s
                .get(&format!("{root}/payees?include_transfers=true"))
                .json()
                .unwrap();
            for (name, target) in [("TRANSFER", P), ("BACK_TRANSFER", A)] {
                ids.insert(
                    name.into(),
                    payees["data"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|p| p["transfer_account_id"] == target)
                        .unwrap()["id"]
                        .as_str()
                        .unwrap()
                        .into(),
                );
            }
        }
        let response = command(
            &s,
            "schedule",
            json!({"action":"create","id":case["schedule"],"name":case["name"],"conditions":mapped(&case["conditions"],&ids),"actions":mapped(&case["actions"],&ids)}),
        );
        assert_eq!(
            response.status(),
            201,
            "{}: {}",
            case["name"],
            response.text().unwrap()
        );
        if case["ordinary"] == true {
            let response=s.post(&format!("{root}/rules"),"ordinary-rule",json!({"id":"21220000-0000-4000-8000-000000000230","stage":"post","conditionsOp":"and","conditions":[{"field":"category","op":"is","value":CATEGORY}],"actions":[{"op":"append-notes","value":" / checked"}],"snapshot":checkpoint(&s)["snapshot"]}));
            assert_eq!(response.status(), 201);
        }
        let response = command(
            &s,
            "post",
            json!({"action":"post","id":case["schedule"],"occurrence_date":case["date"].as_str().unwrap_or("2026-11-09"),"today":case["today"].as_bool().unwrap_or(false)}),
        );
        assert_eq!(
            response.status(),
            200,
            "{}: {}",
            case["name"],
            response.text().unwrap()
        );
        let mut flat = vec![];
        let mut balances = vec![];
        for (name, account) in [("ACCOUNT", A), ("PEER", P)] {
            let view: Value = s
                .get(&format!("{root}/transactions?account_id={account}"))
                .json()
                .unwrap();
            balances.push(view["balances"]["total"].clone());
            let mut parents = view["data"].as_array().unwrap().clone();
            parents.sort_by(|a, b| {
                a["date"]
                    .as_str()
                    .cmp(&b["date"].as_str())
                    .then(a["amount"].as_i64().cmp(&b["amount"].as_i64()))
                    .then(a["notes"].as_str().cmp(&b["notes"].as_str()))
            });
            for (i, parent) in parents.iter().enumerate() {
                let key = format!("{name}/{i}");
                flat.push((key.clone(), None, parent.clone()));
                for (j, child) in parent["children"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    flat.push((format!("{key}/{j}"), Some(key.clone()), child.clone()));
                }
            }
        }
        let labels = flat
            .iter()
            .map(|(key, _, row)| (row["id"].as_str().unwrap().to_owned(), key.clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let reference = |value: &Value| {
            if value.is_null() {
                Value::Null
            } else {
                json!(
                    ids.iter()
                        .find(|(_, id)| value == id.as_str())
                        .unwrap_or_else(|| panic!("Unknown reference {value}"))
                        .0
                )
            }
        };
        let actual=flat.iter().map(|(key,parent,row)|json!({"key":key,"parent":parent,"amount":row["amount"],"date":row["date"],"notes":row["notes"],"cleared":row["cleared"],"payee":reference(&row["payee_id"]),"category":reference(&row["category_id"]),"schedule":row.get("schedule_id").unwrap_or(&Value::Null),"peer":row["transfer_id"].as_str().map(|id|labels.get(id).expect("Missing reciprocal row"))})).collect::<Vec<_>>();
        let mut expected = case["ledger"].clone();
        if case["today"] == true {
            let today =
                s.get(&format!("{root}/schedules")).json::<Value>().unwrap()["data"]["today"]
                    .clone();
            for row in expected.as_array_mut().unwrap() {
                row["date"] = today.clone();
            }
        }
        assert_eq!(json!(actual), expected, "{}", case["name"]);
        assert_eq!(json!(balances), case["balances"], "{}", case["name"]);
    }
}
#[test]
fn final_and_one_time_skip_are_baseline_noops_not_unposted_suppression() {
    for recurring in [false, true] {
        let s = Server::start();
        setup(&s);
        let date = if recurring {
            json!({"start":"2026-11-08","frequency":"daily","endMode":"after_n_occurrences","endOccurrences":1})
        } else {
            json!("2026-11-08")
        };
        let response = command(
            &s,
            "schedule",
            json!({"action":"create","id":S,"posts_transaction":true,"conditions":[{"field":"account","op":"is","value":A},{"field":"amount","op":"is","value":-100},{"field":"date","op":"is","value":date}]}),
        );
        assert_eq!(response.status(), 201);
        let response = command(
            &s,
            "skip-final",
            json!({"action":"skip","id":S,"occurrence_date":"2026-11-08"}),
        );
        assert_eq!(response.status(), 200, "{}", response.text().unwrap());
        let response = command(
            &s,
            "catchup",
            json!({"action":"catch-up","through":"2026-11-09"}),
        );
        assert_eq!(response.status(), 200, "{}", response.text().unwrap());
        let result: Value = response.json().unwrap();
        assert_eq!(result["data"]["posted_count"], 1);
        let ledger: Value = s
            .get(&format!("/api/v1/budgets/{B}/transactions?account_id={A}"))
            .json()
            .unwrap();
        assert_eq!(ledger["balances"]["total"], -100);
        assert_eq!(ledger["data"].as_array().unwrap().len(), 1);
    }
}
#[test]
fn horizon_previews_match_exact_pinned_month_helper_and_never_mutate() {
    let s = Server::start();
    let oracle: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/schedule-financial.json")).unwrap();
    for case in oracle["horizons"].as_array().unwrap() {
        let response=s.post("/api/v1/schedules/preview-dates","horizon",json!({"config":case["today"],"from":case["today"],"count":1,"upcoming_length":case["value"]}));
        assert_eq!(
            response.status(),
            200,
            "{case}: {}",
            response.text().unwrap()
        );
        let result: Value = response.json().unwrap();
        assert_eq!(result["upcoming_days"], case["days"], "{case}");
    }
    assert_eq!(
        s.get("/api/v1/budgets").json::<Value>().unwrap()["data"],
        json!([])
    );
}
#[test]
fn scheduled_range_midpoints_match_pinned_javascript_at_safe_integer_boundaries() {
    let oracle: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/schedule-financial.json")).unwrap();
    for case in oracle["amounts"].as_array().unwrap() {
        let s = Server::start();
        setup(&s);
        let amount = &case["value"];
        let response = command(
            &s,
            "schedule",
            json!({"action":"create","id":S,"conditions":[{"field":"account","op":"is","value":A},{"field":"amount","op":if amount.is_object(){"isbetween"}else{"is"},"value":amount},{"field":"date","op":"is","value":"2026-11-09"}]}),
        );
        assert_eq!(
            response.status(),
            201,
            "{case}: {}",
            response.text().unwrap()
        );
        let response = command(
            &s,
            "post",
            json!({"action":"post","id":S,"occurrence_date":"2026-11-09"}),
        );
        assert_eq!(
            response.status(),
            200,
            "{case}: {}",
            response.text().unwrap()
        );
        let result: Value = s
            .get(&format!("/api/v1/budgets/{B}/transactions?account_id={A}"))
            .json()
            .unwrap();
        assert_eq!(result["data"][0]["amount"], case["amount"], "{case}");
        assert_eq!(result["balances"]["total"], case["amount"]);
    }
}
#[test]
fn foreground_due_probe_is_read_only_and_stops_after_todays_occurrence() {
    let s = Server::start();
    setup(&s);
    let root = format!("/api/v1/budgets/{B}");
    let calendar: Value = s.get(&format!("{root}/schedules")).json().unwrap();
    let today = calendar["data"]["today"].clone();
    let response = command(
        &s,
        "schedule",
        json!({"action":"create","id":S,"posts_transaction":true,"conditions":[{"field":"account","op":"is","value":A},{"field":"amount","op":"is","value":-100},{"field":"date","op":"is","value":today}]}),
    );
    assert_eq!(response.status(), 201);
    let cp = checkpoint(&s);
    let probe = s.get(&format!("{root}/schedules/due"));
    assert_eq!(probe.status(), 200, "{}", probe.text().unwrap());
    let probe: Value = probe.json().unwrap();
    assert_eq!(probe["data"]["due"], true);
    assert_eq!(checkpoint(&s), cp);
    assert_eq!(
        command(&s, "foreground", json!({"action":"catch-up"})).status(),
        200
    );
    let cp = checkpoint(&s);
    for _ in 0..2 {
        let probe: Value = s.get(&format!("{root}/schedules/due")).json().unwrap();
        assert_eq!(probe["data"]["due"], false);
        assert_eq!(checkpoint(&s), cp);
    }
    let rows: Value = s
        .get(&format!("{root}/transactions?account_id={A}"))
        .json()
        .unwrap();
    assert_eq!(rows["data"].as_array().unwrap().len(), 1);
}
#[test]
fn overdue_cap_refuses_atomically_then_explicit_through_dates_make_recoverable_progress() {
    let s = Server::start();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/schedules-overdue-26.10.json"
    ))
    .unwrap();
    let response = s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Idempotency-Key", "overdue-source")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/schedules-overdue-26.10.zip").to_vec())
        .send()
        .unwrap();
    assert_eq!(response.status(), 201);
    let stage: Value = response.json().unwrap();
    assert_eq!(stage["data"]["status"], "ready");
    let id = stage["data"]["id"].as_str().unwrap();
    let b = stage["data"]["budget_id"].as_str().unwrap();
    assert_eq!(
        s.post(
            &format!("/api/v1/imports/{id}/activate"),
            "activate-overdue",
            json!({})
        )
        .status(),
        201
    );
    let root = format!("/api/v1/budgets/{b}");
    let cp: Value = s.get(&format!("{root}/checkpoint")).json().unwrap();
    let send = |key: &str, through: &Value| {
        s.client
            .post(format!("{}{root}/schedules/commands", s.url))
            .timeout(Duration::from_secs(30))
            .header("Idempotency-Key", key)
            .header(
                "X-Actual-Budget-Epoch",
                cp["data"]["epoch"].as_str().unwrap(),
            )
            .json(&json!({"action":"catch-up","through":through}))
            .send()
            .unwrap()
    };
    let over = send("too-many", &fixture["through"]);
    assert_eq!(over.status(), 400, "{}", over.text().unwrap());
    let error: Value = over.json().unwrap();
    assert!(error["error"]["message"].as_str().unwrap().contains("500"));
    assert_eq!(
        s.get(&format!("{root}/checkpoint"))
            .json::<Value>()
            .unwrap(),
        cp
    );
    let empty: Value = s.get(&format!("{root}/transactions")).json().unwrap();
    assert_eq!(empty["data"], json!([]));
    let first = send("first-bounded-part", &fixture["first_chunk"]["through"]);
    assert_eq!(first.status(), 200, "{}", first.text().unwrap());
    let first: Value = first.json().unwrap();
    assert_eq!(
        first["data"]["posted_count"],
        fixture["first_chunk"]["transaction_count"]
    );
    assert_eq!(
        first["data"]["transaction_ids"].as_array().unwrap().len(),
        400
    );
    assert!(
        first.to_string().len() < 30000,
        "Catch-up receipts must stay bounded instead of hydrating every row"
    );
    let final_part = send("second-bounded-part", &fixture["through"]);
    assert_eq!(final_part.status(), 200, "{}", final_part.text().unwrap());
    let last: Value = final_part.json().unwrap();
    assert_eq!(last["data"]["posted_count"], 278);
    let rows: Value = s
        .get(&format!("{root}/transactions?limit=1000"))
        .json()
        .unwrap();
    assert_eq!(rows["balances"]["total"], fixture["after"]["total"]);
    let mut actual = rows["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| json!({"date":r["date"],"amount":r["amount"],"cleared":r["cleared"],"schedule_id":r["schedule_id"]}))
        .collect::<Vec<_>>();
    let mut expected = fixture["transactions"].as_array().unwrap().clone();
    actual.sort_by_key(Value::to_string);
    expected.sort_by_key(Value::to_string);
    assert_eq!(actual.len(), expected.len());
    for (i, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(actual, expected, "Source occurrence {i}");
    }
}
#[test]
fn occurrence_pages_expose_stable_ids_and_allow_review_beyond_summary_preview_limit() {
    let s = Server::start();
    setup(&s);
    let response = command(
        &s,
        "schedule",
        json!({"action":"create","id":S,"conditions":[{"field":"account","op":"is","value":A},{"field":"amount","op":"is","value":-100},{"field":"date","op":"is","value":{"start":"2026-11-09","frequency":"daily","endMode":"never"}}]}),
    );
    assert_eq!(response.status(), 201);
    let root = format!("/api/v1/budgets/{B}");
    let cp = checkpoint(&s);
    let page = s.get(&format!(
        "{root}/schedules/{S}/occurrences?from=2026-11-09&through=2026-12-31&limit=2"
    ));
    assert_eq!(page.status(), 200, "{}", page.text().unwrap());
    let first: Value = page.json().unwrap();
    assert_eq!(first["data"]["occurrences"].as_array().unwrap().len(), 2);
    assert_eq!(first["data"]["next_from"], "2026-11-11");
    let second: Value = s
        .get(&format!(
            "{root}/schedules/{S}/occurrences?from=2026-11-11&through=2026-12-31&limit=2"
        ))
        .json()
        .unwrap();
    assert_eq!(second["data"]["occurrences"][0]["date"], "2026-11-11");
    assert_ne!(
        first["data"]["occurrences"][0]["id"],
        second["data"]["occurrences"][0]["id"]
    );
    assert_eq!(checkpoint(&s), cp);
    let again: Value = s
        .get(&format!(
            "{root}/schedules/{S}/occurrences?from=2026-11-09&through=2026-12-31&limit=2"
        ))
        .json()
        .unwrap();
    assert_eq!(again, first);
}
#[test]
fn first_transfer_schedule_creates_account_managed_reference_without_posting_money() {
    let s = Server::start();
    setup(&s);
    let root = format!("/api/v1/budgets/{B}");
    assert_eq!(
        s.post(
            &format!("{root}/accounts"),
            "peer",
            json!({"id":P,"name":"Savings","opening_balance":0,"opening_date":"2026-01-01"})
        )
        .status(),
        201
    );
    let response = command(
        &s,
        "transfer-schedule",
        json!({"action":"create","id":S,"transfer_account_id":P,"conditions":[{"field":"account","op":"is","value":A},{"field":"amount","op":"is","value":-987},{"field":"date","op":"is","value":"2026-11-09"}]}),
    );
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    let saved: Value = response.json().unwrap();
    assert!(
        saved["data"]["conditions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["field"] == "payee" && c["value"].is_string())
    );
    let empty: Value = s.get(&format!("{root}/transactions")).json().unwrap();
    assert_eq!(empty["data"], json!([]));
    assert_eq!(
        command(
            &s,
            "post-transfer",
            json!({"action":"post","id":S,"occurrence_date":"2026-11-09"})
        )
        .status(),
        200
    );
    let origin: Value = s
        .get(&format!("{root}/transactions?account_id={A}"))
        .json()
        .unwrap();
    let peer: Value = s
        .get(&format!("{root}/transactions?account_id={P}"))
        .json()
        .unwrap();
    assert_eq!(origin["balances"]["total"], -987);
    assert_eq!(peer["balances"]["total"], 987);
    assert_eq!(origin["data"][0]["transfer_id"], peer["data"][0]["id"]);
    assert_eq!(peer["data"][0]["schedule_id"], S);
}
#[test]
fn already_linked_imported_manual_occurrence_is_acknowledged_without_a_second_purchase() {
    let s = Server::start();
    let stage: Value = s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Idempotency-Key", "manual-source")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/schedules-missed-26.10.zip").to_vec())
        .send()
        .unwrap()
        .json()
        .unwrap();
    let id = stage["data"]["id"].as_str().unwrap();
    let b = stage["data"]["budget_id"].as_str().unwrap();
    assert_eq!(
        s.post(
            &format!("/api/v1/imports/{id}/activate"),
            "activate",
            json!({})
        )
        .status(),
        201
    );
    let root = format!("/api/v1/budgets/{b}");
    let before: Value = s.get(&format!("{root}/transactions")).json().unwrap();
    let cp: Value = s.get(&format!("{root}/checkpoint")).json().unwrap();
    let response=s.post(&format!("{root}/schedules/commands"),"ack-source-post",json!({"action":"post","id":"21220000-0000-4000-8000-000000000002","occurrence_date":"2026-11-06","snapshot":cp["data"]["snapshot"]}));
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let after: Value = s.get(&format!("{root}/transactions")).json().unwrap();
    assert_eq!(after["data"], before["data"]);
    assert_eq!(after["balances"], before["balances"]);
}

#[test]
fn imported_schedule_generation_selector_clocks_guard_effective_next_date() {
    let oracle: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/schedules-generation.json"
    ))
    .unwrap();
    for stage in oracle["stages"].as_array().unwrap() {
        let server = Server::start();
        let archive = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../tests/fixtures")
                .join(stage["archive"].as_str().unwrap()),
        )
        .unwrap();
        let response = server
            .client
            .post(format!("{}/api/v1/imports/actual", server.url))
            .header("Idempotency-Key", "generation-import")
            .header("Content-Type", "application/zip")
            .body(archive)
            .send()
            .unwrap();
        assert_eq!(response.status(), 201);
        let import: Value = response.json().unwrap();
        let id = import["data"]["id"].as_str().unwrap();
        let b = import["data"]["budget_id"].as_str().unwrap();
        let activated = server.post(
            &format!("/api/v1/imports/{id}/activate"),
            "activate",
            json!({}),
        );
        assert_eq!(activated.status(), 201, "{}", activated.text().unwrap());
        let path = format!("/api/v1/budgets/{b}/schedules");
        let before: Value = server.get(&path).json().unwrap();
        assert_eq!(
            before["data"]["schedules"][0]["next_date"],
            stage["next_date"]
        );
        let response=server.client.post(format!("{}/api/v1/budgets/{b}/schedules/commands",server.url)).header("Idempotency-Key","equal-selector-generation").header("X-Actual-Edit-Clock",stage["guard_clock"].as_str().unwrap()).header("X-Actual-Budget-Epoch",before["data"]["epoch"].as_str().unwrap()).json(&json!({"action":"skip","id":oracle["schedule_id"],"occurrence_date":stage["next_date"],"snapshot":before["data"]["snapshot"]})).send().unwrap();
        assert_eq!(
            response.status(),
            409,
            "{}: {}",
            stage["name"],
            response.text().unwrap()
        );
        assert_eq!(
            server.get(&path).json::<Value>().unwrap(),
            before,
            "Refused generation must change neither schedule nor cursor"
        );
    }
}

#[test]
fn automatic_entry_retains_linked_schedule_and_suppresses_duplicate_posting() {
    let server = Server::start();
    setup(&server);
    let response = command(
        &server,
        "create-linked",
        json!({"action":"create","id":S,"name":"Bill","posts_transaction":true,"conditions":[{"field":"account","op":"is","value":A},{"field":"amount","op":"is","value":-1234},{"field":"date","op":"is","value":"2026-11-09"}]}),
    );
    assert_eq!(response.status(), 201);
    let cp = checkpoint(&server);
    let id = "21220000-0000-4000-8000-000000000199";
    let body = json!({"snapshot":cp["snapshot"],"id":id,"account_id":A,"date":"2026-11-09","amount":-1234,"notes":"Actual payment"});
    let response = server
        .client
        .post(format!(
            "{}/api/v1/budgets/{B}/rules/transactions",
            server.url
        ))
        .header("Idempotency-Key", "automatic-payment")
        .header("X-Actual-Budget-Epoch", cp["epoch"].as_str().unwrap())
        .json(&body)
        .send()
        .unwrap();
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    let saved: Value = response.json().unwrap();
    assert_eq!(saved["data"]["schedule_id"], S);
    let response = command(
        &server,
        "catchup-linked",
        json!({"action":"catch-up","through":"2026-11-09"}),
    );
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    assert_eq!(response.json::<Value>().unwrap()["data"]["posted_count"], 0);
    let rows: Value = server
        .get(&format!("/api/v1/budgets/{B}/transactions"))
        .json()
        .unwrap();
    assert_eq!(rows["data"].as_array().unwrap().len(), 1);
    assert_eq!(rows["data"][0]["id"], id);
    assert_eq!(rows["balances"]["total"], -1234);
}

#[test]
fn explicit_rule_application_persists_schedule_only_changes_and_keeps_linked_rules_coherent() {
    let server = Server::start();
    setup(&server);
    let created = command(
        &server,
        "create-link-rule",
        json!({"action":"create","id":S,"name":"Bill","conditions":[{"field":"account","op":"is","value":A},{"field":"amount","op":"is","value":-1234},{"field":"date","op":"is","value":"2026-11-09"}]}),
    );
    assert_eq!(created.status(), 201);
    let schedule: Value = created.json().unwrap();
    let rule = schedule["data"]["rule_id"].as_str().unwrap();
    let id = "21220000-0000-4000-8000-000000000198";
    assert_eq!(
        server
            .post(
                &format!("/api/v1/budgets/{B}/transactions"),
                "payment",
                json!({"id":id,"account_id":A,"date":"2026-11-09","amount":-1234})
            )
            .status(),
        201
    );
    let cp = checkpoint(&server);
    let response = server.post(
        &format!("/api/v1/budgets/{B}/rules/apply"),
        "link-payment",
        json!({"snapshot":cp["snapshot"],"ids":[id],"rule_ids":[rule]}),
    );
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    assert_eq!(
        response.json::<Value>().unwrap()["data"]["affected_count"],
        1
    );
    let rows: Value = server
        .get(&format!("/api/v1/budgets/{B}/transactions"))
        .json()
        .unwrap();
    assert_eq!(rows["data"][0]["schedule_id"], S);
    assert_eq!(rows["balances"]["total"], -1234);
    let cp = checkpoint(&server);
    let response = server
        .client
        .delete(format!("{}/api/v1/budgets/{B}/rules/{rule}", server.url))
        .header("Idempotency-Key", "delete-linked-rule")
        .json(&json!({"snapshot":cp["snapshot"]}))
        .send()
        .unwrap();
    assert_eq!(
        response.status(),
        409,
        "Deleting a linked rule must retain a usable schedule"
    );
    assert_eq!(
        server
            .get(&format!("/api/v1/budgets/{B}/schedules"))
            .status(),
        200
    );
}

#[test]
fn contradictory_rule_schedule_links_on_a_pair_refuse_atomically() {
    let s = Server::start();
    setup(&s);
    let root = format!("/api/v1/budgets/{B}");
    assert_eq!(
        s.post(
            &format!("{root}/accounts"),
            "peer-account",
            json!({"id":P,"name":"Savings","opening_balance":0,"opening_date":"2026-01-01"})
        )
        .status(),
        201
    );
    let peer = "21220000-0000-4000-8000-000000000991";
    let id = "21220000-0000-4000-8000-000000000990";
    assert_eq!(s.post(&format!("{root}/transfers"),"paired-payment",json!({"id":id,"counterpart_id":peer,"account_id":A,"to_account_id":P,"amount":-100,"date":"2026-11-09"})).status(),201);
    for (schedule, account, amount) in [
        (S, A, -100),
        ("21220000-0000-4000-8000-000000000992", P, 100),
    ] {
        let response = command(
            &s,
            &format!("create-{schedule}"),
            json!({"action":"create","id":schedule,"conditions":[{"field":"account","op":"is","value":account},{"field":"amount","op":"is","value":amount},{"field":"date","op":"is","value":"2026-11-09"}]}),
        );
        assert_eq!(response.status(), 201);
    }
    let cp = checkpoint(&s);
    let before: Value = s.get(&format!("{root}/transactions")).json().unwrap();
    let response = s.post(
        &format!("{root}/rules/apply"),
        "contradictory-pair-links",
        json!({"snapshot":cp["snapshot"],"ids":[id,peer]}),
    );
    assert_eq!(response.status(), 409, "{}", response.text().unwrap());
    assert_eq!(
        s.get(&format!("{root}/transactions"))
            .json::<Value>()
            .unwrap(),
        before
    );
    assert_eq!(checkpoint(&s), cp);
}

#[test]
fn explicit_unlink_releases_occurrence_without_erasing_payment_and_new_post_replays_once() {
    let mut s = Server::start();
    setup(&s);
    let root = format!("/api/v1/budgets/{B}");
    let response = command(
        &s,
        "release-schedule",
        json!({"action":"create","id":S,"posts_transaction":true,"conditions":[{"field":"account","op":"is","value":A},{"field":"amount","op":"is","value":-100},{"field":"date","op":"is","value":"2026-11-09"}]}),
    );
    assert_eq!(response.status(), 201);
    let cp = checkpoint(&s);
    let original = json!({"action":"catch-up","through":"2026-11-09"});
    let send_original = |s: &Server| {
        s.client
            .post(format!("{}{root}/schedules/commands", s.url))
            .header("Idempotency-Key", "original-auto")
            .header("X-Actual-Budget-Epoch", cp["epoch"].as_str().unwrap())
            .json(&original)
            .send()
            .unwrap()
    };
    let first = send_original(&s);
    assert_eq!(first.status(), 200);
    let receipt: Value = first.json().unwrap();
    let old = receipt["data"]["transaction_ids"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let cp_unlink = checkpoint(&s);
    let owner = "21220000-0000-4000-8000-000000000197";
    let response=s.client.post(format!("{}{root}/schedules/commands",s.url)).header("Idempotency-Key","owned-unlink").header("X-Actual-History-Owner",owner).header("X-Actual-Budget-Epoch",cp_unlink["epoch"].as_str().unwrap()).json(&json!({"action":"unlink","id":S,"transaction_ids":[old],"snapshot":cp_unlink["snapshot"]})).send().unwrap();
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let rows: Value = s.get(&format!("{root}/transactions")).json().unwrap();
    assert_eq!(rows["data"].as_array().unwrap().len(), 1);
    assert_eq!(rows["data"][0]["amount"], -100);
    assert!(rows["data"][0]["schedule_id"].is_null());
    for action in ["undo", "redo"] {
        let history: Value = s
            .client
            .get(format!("{}{root}/history", s.url))
            .header("X-Actual-History-Owner", owner)
            .header(
                "X-Actual-Budget-Epoch",
                cp_unlink["epoch"].as_str().unwrap(),
            )
            .send()
            .unwrap()
            .json()
            .unwrap();
        let available = &history["data"][action];
        let response = s
            .client
            .post(format!("{}{root}/history/{action}", s.url))
            .header("Idempotency-Key", format!("release-{action}"))
            .header("X-Actual-History-Owner", owner)
            .header(
                "X-Actual-Budget-Epoch",
                cp_unlink["epoch"].as_str().unwrap(),
            )
            .json(&json!({"action_id":available["id"]}))
            .send()
            .unwrap();
        assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    }
    s = s.restart();
    let response = command(
        &s,
        "after-explicit-release",
        json!({"action":"catch-up","through":"2026-11-09"}),
    );
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let current: Value = response.json().unwrap();
    let new = current["data"]["transaction_ids"][0].as_str().unwrap();
    assert_ne!(new, old);
    let before: Value = s.get(&format!("{root}/transactions")).json().unwrap();
    assert_eq!(before["data"].as_array().unwrap().len(), 2);
    assert_eq!(before["balances"]["total"], -200);
    assert!(
        before["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == old && r["schedule_id"].is_null())
    );
    let cp_final = checkpoint(&s);
    s = s.restart();
    assert_eq!(send_original(&s).json::<Value>().unwrap(), receipt);
    assert_eq!(
        s.get(&format!("{root}/transactions"))
            .json::<Value>()
            .unwrap(),
        before
    );
    assert_eq!(checkpoint(&s), cp_final);
    let again = command(
        &s,
        "after-release-repeat",
        json!({"action":"catch-up","through":"2026-11-09"}),
    );
    assert_eq!(again.status(), 200);
    assert_eq!(again.json::<Value>().unwrap()["data"]["posted_count"], 0);
    assert_eq!(
        s.get(&format!("{root}/transactions"))
            .json::<Value>()
            .unwrap(),
        before
    );
}

#[test]
fn schedule_link_history_merges_unrelated_notes_but_refuses_a_new_reconciliation_generation() {
    let s = Server::start();
    setup(&s);
    let root = format!("/api/v1/budgets/{B}");
    let id = "21220000-0000-4000-8000-000000000196";
    let owner = "21220000-0000-4000-8000-000000000195";
    assert_eq!(command(&s,"history-schedule",json!({"action":"create","id":S,"conditions":[{"field":"account","op":"is","value":A},{"field":"amount","op":"is","value":-100},{"field":"date","op":"is","value":"2026-11-09"}]})).status(),201);
    assert_eq!(
        s.post(
            &format!("{root}/transactions"),
            "history-payment",
            json!({"id":id,"account_id":A,"amount":-100,"date":"2026-11-09","cleared":true})
        )
        .status(),
        201
    );
    let cp = checkpoint(&s);
    let epoch = cp["epoch"].as_str().unwrap();
    let response = s
        .client
        .post(format!("{}{root}/schedules/commands", s.url))
        .header("Idempotency-Key", "owned-link-only")
        .header("X-Actual-History-Owner", owner)
        .header("X-Actual-Budget-Epoch", epoch)
        .json(&json!({"action":"link","id":S,"transaction_ids":[id],"snapshot":cp["snapshot"]}))
        .send()
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        s.client
            .patch(format!("{}{root}/transactions/{id}", s.url))
            .header("Idempotency-Key", "unrelated-note")
            .json(&json!({"notes":"Other device note"}))
            .send()
            .unwrap()
            .status(),
        200
    );
    let inverse = |action: &str, key: &str| {
        let history: Value = s
            .client
            .get(format!("{}{root}/history", s.url))
            .header("X-Actual-History-Owner", owner)
            .header("X-Actual-Budget-Epoch", epoch)
            .send()
            .unwrap()
            .json()
            .unwrap();
        s.client
            .post(format!("{}{root}/history/{action}", s.url))
            .header("Idempotency-Key", key)
            .header("X-Actual-History-Owner", owner)
            .header("X-Actual-Budget-Epoch", epoch)
            .json(&json!({"action_id":history["data"][action]["id"]}))
            .send()
            .unwrap()
    };
    for action in ["undo", "redo"] {
        let response = inverse(action, &format!("notes-{action}"));
        assert_eq!(response.status(), 200, "{}", response.text().unwrap());
        assert_eq!(
            s.get(&format!("{root}/transactions/{id}"))
                .json::<Value>()
                .unwrap()["data"]["notes"],
            "Other device note"
        );
    }
    let cp = checkpoint(&s);
    let response=s.post(&format!("{root}/accounts/{A}/reconciliation"),"new-lock",json!({"action":"lock","statement_balance":-100,"date":"2026-11-09","snapshot":cp["snapshot"]}));
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let before: Value = s.get(&format!("{root}/transactions")).json().unwrap();
    assert_eq!(before["data"][0]["reconciled"], true);
    let response = inverse("undo", "refuse-new-lock");
    assert_eq!(
        response.status(),
        409,
        "A newer reconciliation generation must not be authorized by old link history"
    );
    assert_eq!(
        s.get(&format!("{root}/transactions"))
            .json::<Value>()
            .unwrap(),
        before
    );
}

#[test]
fn reassignment_releases_original_auto_claim_and_matches_all_four_pinned_financial_graphs() {
    let oracle: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/schedule-reassignment.json"
    ))
    .unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let s = Server::start();
        setup(&s);
        let root = format!("/api/v1/budgets/{B}");
        let target = "21220000-0000-4000-8000-000000000194";
        assert_eq!(
            s.post(
                &format!("{root}/accounts"),
                "other-account",
                json!({"id":P,"name":"Savings","opening_balance":0,"opening_date":"2026-01-01"})
            )
            .status(),
            201
        );
        assert_eq!(
            s.post(
                &format!("{root}/payees"),
                "ordinary-payee",
                json!({"id":PAYEE,"name":"Bill"})
            )
            .status(),
            201
        );
        let schedule = |id: &str, automatic: bool| {
            let mut c = json!({"action":"create","id":id,"posts_transaction":automatic,"conditions":[{"field":"account","op":"is","value":A},{"field":"amount","op":"is","value":-100},{"field":"date","op":"is","value":"2026-11-09"}]});
            if case["transfer"] == true {
                c["transfer_account_id"] = json!(P);
            } else {
                c["conditions"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"field":"payee","op":"is","value":PAYEE}));
            }
            c
        };
        assert_eq!(command(&s, "schedule-A", schedule(S, true)).status(), 201);
        let cp = checkpoint(&s);
        let original_body =
            json!({"action":"catch-up","through":"2026-11-09","snapshot":cp["snapshot"]});
        let send_original = || {
            s.client
                .post(format!("{}{root}/schedules/commands", s.url))
                .header("Idempotency-Key", "A-posted-original")
                .header("X-Actual-Budget-Epoch", cp["epoch"].as_str().unwrap())
                .json(&original_body)
                .send()
                .unwrap()
        };
        let first = send_original();
        assert_eq!(first.status(), 200);
        let receipt: Value = first.json().unwrap();
        let old = receipt["data"]["transaction_ids"][0]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            command(
                &s,
                "schedule-B",
                schedule(target, case["target_automatic"].as_bool().unwrap())
            )
            .status(),
            201
        );
        let response = command(
            &s,
            "move-to-B",
            json!({"action":"link","id":target,"transaction_ids":[old]}),
        );
        assert_eq!(response.status(), 200, "{}", response.text().unwrap());
        let point = checkpoint(&s);
        let linked = s.get(&format!(
            "{root}/schedules/{target}/transactions?snapshot={}&limit=1",
            point["snapshot"].as_str().unwrap()
        ));
        assert_eq!(linked.status(), 200, "{}", linked.text().unwrap());
        let linked: Value = linked.json().unwrap();
        assert_eq!(
            linked["data"]["total_count"],
            if case["transfer"] == true { 2 } else { 1 }
        );
        assert_eq!(linked["data"]["transactions"].as_array().unwrap().len(), 1);
        assert_eq!(linked["data"]["transactions"][0]["schedule_id"], target);
        assert_eq!(checkpoint(&s), point);
        let response = command(
            &s,
            "service-after-move",
            json!({"action":"catch-up","through":"2026-11-09"}),
        );
        assert_eq!(response.status(), 200, "{}", response.text().unwrap());
        let posted: Value = response.json().unwrap();
        assert_eq!(posted["data"]["posted_count"], 1);
        let new = posted["data"]["transaction_ids"][0]
            .as_str()
            .unwrap()
            .to_owned();
        assert_ne!(old, new);
        let view: Value = s.get(&format!("{root}/transactions")).json().unwrap();
        let rows = view["data"].as_array().unwrap();
        let mut labels = std::collections::BTreeMap::from([
            (old.clone(), "payment-1".to_owned()),
            (new.clone(), "payment-2".to_owned()),
        ]);
        for (id, label) in [(&old, "payment-1-peer"), (&new, "payment-2-peer")] {
            if let Some(peer) =
                rows.iter().find(|r| r["id"] == *id).unwrap()["transfer_id"].as_str()
            {
                labels.insert(peer.into(), label.into());
            }
        }
        let payees: Value = s
            .get(&format!("{root}/payees?include_transfers=true"))
            .json()
            .unwrap();
        let mut refs = std::collections::BTreeMap::from([
            (A.to_owned(), "ACCOUNT"),
            (P.to_owned(), "PEER"),
            (PAYEE.to_owned(), "PAYEE"),
            (S.to_owned(), "A"),
            (target.to_owned(), "B"),
        ]);
        for p in payees["data"].as_array().unwrap() {
            if p["transfer_account_id"] == P {
                refs.insert(p["id"].as_str().unwrap().into(), "TRANSFER");
            } else if p["transfer_account_id"] == A {
                refs.insert(p["id"].as_str().unwrap().into(), "BACK_TRANSFER");
            }
        }
        let reference = |v: &Value| {
            v.as_str()
                .map(|id| {
                    json!(
                        refs.get(id)
                            .unwrap_or_else(|| panic!("Unknown reference {id}"))
                    )
                })
                .unwrap_or(Value::Null)
        };
        let identity = |v: &Value| {
            v.as_str()
                .map(|id| json!(labels.get(id).expect("Missing reciprocal payment")))
                .unwrap_or(Value::Null)
        };
        let mut actual=rows.iter().map(|r|json!({"id":identity(&r["id"]),"account":reference(&r["account_id"]),"amount":r["amount"],"date":r["date"],"payee":reference(&r["payee_id"]),"category":r["category_id"],"notes":r["notes"],"cleared":r["cleared"],"reconciled":r["reconciled"],"transfer_id":identity(&r["transfer_id"]),"parent_id":identity(&r["parent_id"]),"is_parent":r["is_parent"].as_bool().unwrap_or(false),"schedule":reference(&r["schedule_id"])})).collect::<Vec<_>>();
        actual.sort_by_key(|r| r["id"].as_str().unwrap().to_owned());
        let expected = case["snapshots"].as_array().unwrap().last().unwrap();
        let mut ledger = expected["ledger"].as_array().unwrap().clone();
        ledger.sort_by_key(|r| r["id"].as_str().unwrap().to_owned());
        assert_eq!(
            actual, ledger,
            "transfer={} target_auto={}",
            case["transfer"], case["target_automatic"]
        );
        let balances = [A, P]
            .iter()
            .map(|a| {
                s.get(&format!("{root}/transactions?account_id={a}"))
                    .json::<Value>()
                    .unwrap()["balances"]["total"]
                    .clone()
            })
            .collect::<Vec<_>>();
        assert_eq!(json!(balances), expected["balances"]);
        let before = checkpoint(&s);
        assert_eq!(send_original().json::<Value>().unwrap(), receipt);
        assert_eq!(checkpoint(&s), before);
        assert_eq!(
            s.get(&format!("{root}/transactions"))
                .json::<Value>()
                .unwrap(),
            view
        );
        assert_eq!(
            command(
                &s,
                "repeat-service",
                json!({"action":"catch-up","through":"2026-11-09"})
            )
            .json::<Value>()
            .unwrap()["data"]["posted_count"],
            0
        );
    }
}
