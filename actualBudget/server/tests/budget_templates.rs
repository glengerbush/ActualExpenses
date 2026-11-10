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
        let mut command = Command::new(
            std::env::var("ACTUALBUDGET_BINARY")
                .unwrap_or_else(|_| env!("CARGO_BIN_EXE_actualbudget-server").into()),
        );
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

#[test]
fn notes_are_parsed_without_mutating_the_budget_or_erasing_invalid_directives() {
    let s = Server::start();
    let b = "23240000-0000-4000-8000-000000000001";
    assert_eq!(
        s.post(
            "/api/v1/budgets",
            "budget",
            json!({"id":b,"name":"Template parsing"})
        )
        .status(),
        201
    );
    let path = format!("/api/v1/budgets/{b}");
    let before = s
        .get(&format!("{path}/checkpoint"))
        .json::<Value>()
        .unwrap();
    let response=s.post(&format!("{path}/budget/templates/parse"),"parse",json!({"notes":"Save for repairs\n#template-2 12.34\n#goal 100\n\n#template broken syntax"}));
    assert_eq!(response.status(), 200);
    let result = response.json::<Value>().unwrap();
    assert_eq!(
        result["data"]["templates"][0],
        json!({"type":"simple","monthly":12.34,"limit":null,"priority":2,"directive":"template","description":"Save for repairs"})
    );
    assert_eq!(
        result["data"]["templates"][1],
        json!({"type":"goal","amount":100,"priority":null,"directive":"goal"})
    );
    assert_eq!(result["data"]["templates"][2]["type"], "error");
    assert_eq!(
        result["data"]["templates"][2]["line"],
        "#template broken syntax"
    );
    assert_eq!(result["data"]["errors"].as_array().unwrap().len(), 1);
    assert_eq!(
        s.get(&format!("{path}/checkpoint"))
            .json::<Value>()
            .unwrap(),
        before
    );
}
#[test]
fn directives_match_seventy_independently_executed_source_notes() {
    let s = Server::start();
    let b = "23240000-0000-4000-8000-000000000002";
    assert_eq!(
        s.post(
            "/api/v1/budgets",
            "create",
            json!({"id":b,"name":"Source grammar"})
        )
        .status(),
        201
    );
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/budget-template-grammar.json"
    ))
    .unwrap();
    let mut differences = vec![];
    for case in fixture["cases"].as_array().unwrap() {
        let response = s.post(
            &format!("/api/v1/budgets/{b}/budget/templates/parse"),
            "parse",
            json!({"notes":case["notes"]}),
        );
        assert_eq!(response.status(), 200);
        let result = response.json::<Value>().unwrap();
        let mut actual = result["data"]["templates"].clone();
        let mut expected = case["templates"].clone();
        for list in [&mut actual, &mut expected] {
            for row in list.as_array_mut().unwrap() {
                if row["type"] == "error" {
                    assert!(!row["error"].as_str().unwrap_or("").is_empty());
                    row.as_object_mut().unwrap().remove("error");
                }
            }
        }
        if actual != expected {
            differences.push(json!({"case":case["name"],"actual":actual,"expected":expected}));
        }
    }
    assert_eq!(
        differences,
        json!([]).as_array().unwrap().clone(),
        "{}",
        json!(differences)
    );
}
#[test]
fn genuine_notes_template_applies_exact_budget_and_goal_without_changing_notes() {
    let s = Server::start();
    let imported = s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "source")
        .body(
            include_bytes!("../../tests/fixtures/budget-templates-simple-base-26.10.zip").to_vec(),
        )
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap();
    assert_eq!(imported["data"]["status"], "ready", "{imported}");
    let activation = s.post(
        &format!(
            "/api/v1/imports/{}/activate",
            imported["data"]["id"].as_str().unwrap()
        ),
        "activate",
        json!({}),
    );
    assert!(
        activation.status().is_success(),
        "{}",
        activation.text().unwrap()
    );
    let root = format!(
        "/api/v1/budgets/{}",
        imported["data"]["budget_id"].as_str().unwrap()
    );
    let initial = s
        .get(&format!("{root}/budget?month=2026-11"))
        .json::<Value>()
        .unwrap();
    let fixed = initial["data"]["months"]["2026-11"]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Fixed")
        .unwrap();
    let id = fixed["id"].clone();
    let notes = fixed["notes"].clone();
    let snapshot = initial["data"]["snapshot"].clone();
    let preview = s.post(
        &format!("{root}/budget/templates/preview"),
        "preview",
        json!({"month":"2026-11","snapshot":snapshot}),
    );
    assert_eq!(preview.status(), 200);
    let preview = preview.json::<Value>().unwrap();
    let planned = preview["data"]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap();
    assert_eq!(planned["budgeted"], 7299);
    assert_eq!(planned["goal"], 7299);
    assert_eq!(planned["long_goal"], Value::Null);
    assert_eq!(
        s.get(&format!("{root}/checkpoint"))
            .json::<Value>()
            .unwrap()["data"]["snapshot"],
        snapshot
    );
    let response = s.post(
        &format!("{root}/budget/templates/commands"),
        "apply",
        json!({"action":"apply","month":"2026-11","snapshot":snapshot}),
    );
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let after = s
        .get(&format!("{root}/budget?month=2026-11"))
        .json::<Value>()
        .unwrap();
    let row = after["data"]["months"]["2026-11"]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap();
    assert_eq!(row["budgeted"], 7299);
    assert_eq!(row["goal"], "7299");
    assert_eq!(row["balance"], 7299);
    assert_eq!(row["notes"], notes);
    let replay = s.post(
        &format!("{root}/budget/templates/commands"),
        "apply",
        json!({"action":"apply","month":"2026-11","snapshot":snapshot}),
    );
    assert_eq!(replay.status(), 200);
    assert_eq!(
        s.get(&format!("{root}/budget?month=2026-11"))
            .json::<Value>()
            .unwrap(),
        after
    );
}
#[test]
fn visual_definition_preserves_notes_and_uses_original_snapshot_for_atomic_apply() {
    let s = Server::start();
    let b = "23240000-0000-4000-8000-000000000003";
    let c = "23240000-0000-4000-8000-000000000004";
    assert_eq!(
        s.post(
            "/api/v1/budgets",
            "create",
            json!({"id":b,"name":"Visual templates"})
        )
        .status(),
        201
    );
    let root = format!("/api/v1/budgets/{b}");
    assert_eq!(s.post(&format!("{root}/categories"),"category",json!({"id":c,"name":"Repairs","is_income":false,"notes":"Keep this description\n#template 10"})).status(),201);
    let cp = s
        .get(&format!("{root}/checkpoint"))
        .json::<Value>()
        .unwrap();
    let body = json!({"action":"configure","snapshot":cp["data"]["snapshot"],"category_id":c,"source":"ui","templates":[{"type":"simple","directive":"template","priority":0,"monthly":25},{"type":"goal","directive":"goal","amount":100}]});
    let response = s.post(
        &format!("{root}/budget/templates/commands"),
        "configure",
        body.clone(),
    );
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let defs = s
        .get(&format!("{root}/budget/templates?month=2026-11"))
        .json::<Value>()
        .unwrap();
    let def = defs["data"]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == c)
        .unwrap();
    assert_eq!(def["source"], "ui");
    assert_eq!(def["notes"], "Keep this description\n#template 10");
    assert_eq!(def["templates"], body["templates"]);
    let cp = s
        .get(&format!("{root}/checkpoint"))
        .json::<Value>()
        .unwrap();
    let apply = json!({"action":"apply","month":"2026-11","snapshot":cp["data"]["snapshot"]});
    let response = s.post(
        &format!("{root}/budget/templates/commands"),
        "apply",
        apply.clone(),
    );
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let view = s
        .get(&format!("{root}/budget?month=2026-11"))
        .json::<Value>()
        .unwrap();
    let row = view["data"]["months"]["2026-11"]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == c)
        .unwrap();
    assert_eq!(row["budgeted"], 2500);
    assert_eq!(row["goal"], "10000");
    assert_eq!(row["long_goal"], "1");
    assert_eq!(row["notes"], "Keep this description\n#template 10");
    let stale = s.post(
        &format!("{root}/budget/templates/commands"),
        "stale",
        json!({"action":"overwrite","month":"2026-11","snapshot":cp["data"]["snapshot"]}),
    );
    assert_eq!(stale.status(), 409);
    assert_eq!(
        s.get(&format!("{root}/budget?month=2026-11"))
            .json::<Value>()
            .unwrap(),
        view
    );
}
#[test]
fn definition_and_allocation_history_reject_newer_fields_and_keep_original_receipts() {
    let s = Server::start();
    let b = "23240000-0000-4000-8000-000000000005";
    let c = "23240000-0000-4000-8000-000000000006";
    let owner = "23240000-0000-4000-8000-000000000007";
    assert_eq!(
        s.post(
            "/api/v1/budgets",
            "create",
            json!({"id":b,"name":"Template history"})
        )
        .status(),
        201
    );
    let root = format!("/api/v1/budgets/{b}");
    assert_eq!(
        s.post(
            &format!("{root}/categories"),
            "category",
            json!({"id":c,"name":"Repairs","is_income":false,"notes":"Original note\n#template 10"})
        )
        .status(),
        201
    );
    let epoch = s
        .get(&format!("{root}/checkpoint"))
        .json::<Value>()
        .unwrap()["data"]["epoch"]
        .as_str()
        .unwrap()
        .to_owned();
    let send = |route: &str, key: &str, body: &Value| {
        s.client
            .post(format!("{}{root}/{route}", s.url))
            .header("Idempotency-Key", key)
            .header("X-Actual-History-Owner", owner)
            .header("X-Actual-Budget-Epoch", &epoch)
            .json(body)
            .send()
            .unwrap()
    };
    let cp = s
        .get(&format!("{root}/checkpoint"))
        .json::<Value>()
        .unwrap();
    let configure = json!({"action":"configure","snapshot":cp["data"]["snapshot"],"category_id":c,"source":"ui","templates":[{"type":"simple","directive":"template","priority":0,"monthly":25}]});
    let configured = send("budget/templates/commands", "configure", &configure);
    assert_eq!(configured.status(), 200);
    let receipt = configured.json::<Value>().unwrap();
    let cp = s
        .get(&format!("{root}/checkpoint"))
        .json::<Value>()
        .unwrap();
    let apply = json!({"action":"apply","month":"2026-11","snapshot":cp["data"]["snapshot"]});
    assert_eq!(
        send("budget/templates/commands", "apply", &apply).status(),
        200
    );
    let history = || {
        s.client
            .get(format!("{}{root}/history", s.url))
            .header("X-Actual-History-Owner", owner)
            .send()
            .unwrap()
            .json::<Value>()
            .unwrap()
    };
    let state = history();
    assert_eq!(
        send(
            "history/undo",
            "undo-allocation",
            &json!({"action_id":state["data"]["undo"]["id"]})
        )
        .status(),
        200
    );
    let view = s
        .get(&format!("{root}/budget?month=2026-11"))
        .json::<Value>()
        .unwrap();
    let row = view["data"]["months"]["2026-11"]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == c)
        .unwrap();
    assert_eq!(row["budgeted"], 0);
    assert_eq!(row["goal"], Value::Null);
    let state = history();
    assert_eq!(
        send(
            "history/undo",
            "undo-definition",
            &json!({"action_id":state["data"]["undo"]["id"]})
        )
        .status(),
        200
    );
    let read = || {
        s.get(&format!("{root}/budget/templates?month=2026-11"))
            .json::<Value>()
            .unwrap()
    };
    let defs = read();
    let row = defs["data"]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == c)
        .unwrap();
    assert_eq!(row["source"], "notes");
    assert_eq!(row["notes"], "Original note\n#template 10");
    let state = history();
    assert_eq!(
        send(
            "history/redo",
            "redo-definition",
            &json!({"action_id":state["data"]["redo"]["id"]})
        )
        .status(),
        200
    );
    assert_eq!(
        send("budget/templates/commands", "configure", &configure)
            .json::<Value>()
            .unwrap(),
        receipt
    );
    // A fresh snapshot cannot authorize an older or equal logical field write.
    let physical = std::time::SystemTime::now() + Duration::from_secs(2);
    let date = chrono::DateTime::<chrono::Utc>::from(physical);
    let clock = format!(
        "{}-0000-2324000000000001",
        date.format("%Y-%m-%dT%H:%M:%S%.3fZ")
    );
    let earlier = format!(
        "{}-0000-2324000000000001",
        (date - chrono::TimeDelta::seconds(1)).format("%Y-%m-%dT%H:%M:%S%.3fZ")
    );
    let cp = s
        .get(&format!("{root}/checkpoint"))
        .json::<Value>()
        .unwrap();
    let mut newer = configure.clone();
    newer["snapshot"] = cp["data"]["snapshot"].clone();
    newer["templates"][0]["monthly"] = json!(50);
    let url = format!("{}{root}/budget/templates/commands", s.url);
    assert_eq!(
        s.client
            .post(&url)
            .header("Idempotency-Key", "newer")
            .header("X-Actual-Edit-Clock", &clock)
            .json(&newer)
            .send()
            .unwrap()
            .status(),
        200
    );
    let cp = s
        .get(&format!("{root}/checkpoint"))
        .json::<Value>()
        .unwrap();
    newer["snapshot"] = cp["data"]["snapshot"].clone();
    newer["templates"][0]["monthly"] = json!(1);
    for (key, clock) in [("older", earlier), ("equal", clock)] {
        assert_eq!(
            s.client
                .post(&url)
                .header("Idempotency-Key", key)
                .header("X-Actual-Edit-Clock", clock)
                .json(&newer)
                .send()
                .unwrap()
                .status(),
            409
        );
        assert_eq!(
            s.get(&format!("{root}/checkpoint"))
                .json::<Value>()
                .unwrap(),
            cp
        );
    }
    let state = history();
    assert_eq!(
        send(
            "history/undo",
            "undo-stale-definition",
            &json!({"action_id":state["data"]["undo"]["id"]})
        )
        .status(),
        409
    );
}
