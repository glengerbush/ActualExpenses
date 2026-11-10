//! Real public API against independently exported Actual26.10 planning states.
use reqwest::blocking::{Client, Response};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
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
    fn start(data: TempDir) -> Self {
        let web = data.path().join("web");
        std::fs::create_dir_all(&web).unwrap();
        std::fs::write(web.join("index.html"), "<title>Planning fixture</title>").unwrap();
        let mut process = Command::new(
            std::env::var("ACTUALBUDGET_BINARY")
                .unwrap_or_else(|_| env!("CARGO_BIN_EXE_actualbudget-server").into()),
        )
        .env("ACTUALBUDGET_DATA_DIR", data.path())
        .env("ACTUALBUDGET_WEB_DIR", web)
        .env("ACTUALBUDGET_HOST", "127.0.0.1")
        .env("ACTUALBUDGET_PORT", "0")
        .env_remove("ACTUALBUDGET_PUBLIC_ORIGIN")
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
    fn post(&self, path: &str, key: &str, body: &Value, epoch: Option<&str>) -> Response {
        let mut request = self
            .client
            .post(format!("{}{path}", self.url))
            .header("Idempotency-Key", key)
            .header(
                "X-Actual-History-Owner",
                "23240000-0000-4000-8000-000000000998",
            )
            .json(body);
        if let Some(epoch) = epoch {
            request = request.header("X-Actual-Budget-Epoch", epoch);
        }
        request.send().unwrap()
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

fn decoded(value: &Value) -> Value {
    match value {
        Value::String(s) => {
            serde_json::from_str(s).expect("Goal must retain valid JSON representation")
        }
        _ => value.clone(),
    }
}
fn assert_budget(server: &Server, root: &str, case: &Value, expected: &Value) {
    let month = case["month"].as_str().unwrap();
    let view = server.get(&format!("{root}/budget?month={month}"));
    assert_eq!(view["data"]["method"], case["method"]);
    let data = &view["data"]["months"][month];
    let rows = data["categories"].as_array().unwrap();
    // The existing public budget view puts income first. Compare the relative
    // expense-category order; source monetary results below remain authoritative
    // for allocation ordering, rather than inferring it from either UI layout.
    let expense = |v: &Value| matches!(v.as_str(), Some("A" | "B" | "C"));
    if case["category_order"].is_array() {
        let source_order = case["category_order"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| expense(v))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            rows.iter()
                .map(|r| r["name"].clone())
                .filter(expense)
                .collect::<Vec<_>>(),
            source_order,
            "source category order"
        );
    }
    for (name, source) in expected["categories"].as_object().unwrap() {
        let row = rows.iter().find(|r| r["id"] == case["ids"][name]).unwrap();
        assert_eq!(row["name"], *name);
        // Envelope income has no planning cell in Actual; the public budget view
        // represents that as zero budget/balance, as established by tickets17/18.
        let income = case["method"] == "envelope" && name == "Salary";
        assert_eq!(
            row["budgeted"],
            if income {
                json!(0)
            } else {
                source["budget"].clone()
            },
            "{} {name} budget",
            case["name"]
        );
        assert_eq!(
            row["balance"],
            if income {
                json!(0)
            } else {
                source["leftover"].clone()
            },
            "{} {name} balance",
            case["name"]
        );
        assert_eq!(
            decoded(&row["goal"]),
            source["goal"],
            "{} {name} goal",
            case["name"]
        );
        assert_eq!(
            decoded(&row["long_goal"]),
            source["long-goal"],
            "{} {name} long goal",
            case["name"]
        );
        if let Some(note) = case["configuration"]["notes"].get(name) {
            assert_eq!(row["notes"], *note, "original notes");
        }
    }
    let key = if case["method"] == "tracking" {
        "total_saved"
    } else {
        "to_budget"
    };
    assert_eq!(
        data["summary"][key], expected["available"],
        "{} available funds",
        case["name"]
    );
}
fn assert_source_ledger(server: &Server, root: &str, case: &Value) -> Value {
    let mut ledger = server.get(&format!("{root}/transactions"));
    let mut actual = vec![];
    for row in ledger["data"].as_array().unwrap() {
        assert_eq!(row["account_id"], case["ids"]["Checking"]);
        assert!(
            row["payee_id"].is_null() && row["parent_id"].is_null() && row["transfer_id"].is_null()
        );
        assert!(!row["is_parent"].as_bool().unwrap_or(false));
        let category = case["ids"]
            .as_object()
            .unwrap()
            .iter()
            .find(|(_, id)| **id == row["category_id"])
            .unwrap()
            .0;
        actual.push(json!({"id":row["id"],"date":row["date"],"amount":row["amount"],"category":category,"notes":row["notes"],"cleared":row["cleared"],"reconciled":row["reconciled"]}));
    }
    actual.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    assert_eq!(
        actual,
        *case["ledger"].as_array().unwrap(),
        "{} complete original financial rows",
        case["name"]
    );
    // Planning writes advance the budget-wide snapshot, even though every
    // transaction, its field clocks, order and register totals stay unchanged.
    ledger.as_object_mut().unwrap().remove("snapshot");
    ledger
}
fn run_case(name: &str) {
    let oracle: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/budget-template-financial.json"
    ))
    .unwrap();
    let case = oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap();
    assert!(
        case["category_order"].is_array(),
        "financial fixture requires source order"
    );
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/budget-template-financial")
        .join(case["archive"]["filename"].as_str().unwrap());
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        case["archive"]["sha256"]
    );
    let server = Server::start(tempfile::tempdir().unwrap());
    let imported: Value = server
        .client
        .post(format!("{}/api/v1/imports/actual", server.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "source")
        .body(bytes)
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(imported["data"]["status"], "ready", "{name}: {imported}");
    let response = server.post(
        &format!(
            "/api/v1/imports/{}/activate",
            imported["data"]["id"].as_str().unwrap()
        ),
        "activate",
        &json!({}),
        None,
    );
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    let root = format!(
        "/api/v1/budgets/{}",
        imported["data"]["budget_id"].as_str().unwrap()
    );
    let original = assert_source_ledger(&server, &root, case);
    assert_budget(&server, &root, case, &case["seed"]);
    // Actual's per-category projection intentionally ignores available-funds
    // clamping; compare it separately from the full apply preview below.
    let checkpoint = server.get(&format!("{root}/checkpoint"));
    for (category, expected) in case["projections"].as_object().unwrap() {
        let response = server.post(
            &format!("{root}/budget/templates/projection"),
            "projection",
            &json!({"month":case["month"],"snapshot":checkpoint["data"]["snapshot"],
                "category_id":case["ids"][category],"templates":case["templates"][category]}),
            None,
        );
        assert_eq!(
            response.status(),
            200,
            "{name} {category}: {}",
            response.text().unwrap()
        );
        let projection: Value = response.json().unwrap();
        assert_eq!(
            projection["data"]["budgeted"], expected["budgeted"],
            "{name} {category} projection total"
        );
        assert_eq!(
            projection["data"]["per_template"], expected["perTemplate"],
            "{name} {category} projection contributions"
        );
        assert_eq!(
            server.get(&format!("{root}/checkpoint")),
            checkpoint,
            "read-only projection"
        );
    }
    assert_budget(&server, &root, case, &case["seed"]);
    assert_eq!(assert_source_ledger(&server, &root, case), original);
    let mut receipts = vec![];
    for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
        let checkpoint = server.get(&format!("{root}/checkpoint"));
        let epoch = checkpoint["data"]["epoch"].as_str().unwrap().to_owned();
        let action = &step["action"];
        let mut body = json!({"month":case["month"],"snapshot":checkpoint["data"]["snapshot"],"action":if action["action"]=="apply"{"apply"}else{"overwrite"}});
        if let Some(category) = action["category"].as_str() {
            body["category_ids"] = json!([case["ids"][category]]);
        }
        if let Some(categories) = action["categories"].as_array() {
            body["category_ids"] = json!(
                categories
                    .iter()
                    .map(|v| case["ids"][v.as_str().unwrap()].clone())
                    .collect::<Vec<_>>()
            );
        }
        let mut request = body.clone();
        request.as_object_mut().unwrap().remove("action");
        request["force"] = json!(action["action"] != "apply");
        let preview = server.post(
            &format!("{root}/budget/templates/preview"),
            "preview",
            &request,
            None,
        );
        assert_eq!(
            preview.status(),
            200,
            "{name} {index}: {}",
            preview.text().unwrap()
        );
        let plan: Value = preview.json().unwrap();
        assert_eq!(
            server.get(&format!("{root}/checkpoint")),
            checkpoint,
            "read-only preview"
        );
        let failed = step["notification"]["message"] == "template-errors"
            || step["notification"].get("error").is_some();
        if failed {
            assert!(
                !plan["data"]["errors"].as_array().unwrap().is_empty(),
                "{name}: {plan}"
            );
        } else {
            assert_eq!(plan["data"]["errors"], json!([]), "{name}: {plan}");
            for (category, expected) in step["result"]["categories"].as_object().unwrap() {
                let row = plan["data"]["categories"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|r| r["id"] == case["ids"][category])
                    .unwrap();
                let income = case["method"] == "envelope" && category == "Salary";
                assert_eq!(
                    row["budgeted"],
                    if income {
                        json!(0)
                    } else {
                        expected["budget"].clone()
                    },
                    "{name} {category} preview budget"
                );
                assert_eq!(
                    row["goal"], expected["goal"],
                    "{name} {category} preview goal"
                );
                assert_eq!(
                    row["long_goal"], expected["long-goal"],
                    "{name} {category} preview long goal"
                );
            }
        }
        let key = format!("apply-{index}");
        let response = server.post(
            &format!("{root}/budget/templates/commands"),
            &key,
            &body,
            Some(&epoch),
        );
        if failed {
            assert_eq!(
                response.status(),
                400,
                "{name}: {}",
                response.text().unwrap()
            );
            assert_eq!(
                server.get(&format!("{root}/checkpoint")),
                checkpoint,
                "failed apply is atomic"
            );
        } else {
            assert_eq!(
                response.status(),
                200,
                "{name}: {}",
                response.text().unwrap()
            );
            receipts.push((key, body, epoch, response.json::<Value>().unwrap()));
        }
        assert_budget(&server, &root, case, &step["result"]);
        assert_eq!(
            assert_source_ledger(&server, &root, case),
            original,
            "allocation must not change transactions"
        );
    }
    let before_restart = server.get(&format!(
        "{root}/budget?month={}",
        case["month"].as_str().unwrap()
    ));
    let checkpoint = server.get(&format!("{root}/checkpoint"));
    let server = server.restart();
    for (key, body, epoch, receipt) in receipts {
        let response = server.post(
            &format!("{root}/budget/templates/commands"),
            &key,
            &body,
            Some(&epoch),
        );
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.json::<Value>().unwrap(),
            receipt,
            "exact original receipt after restart"
        );
    }
    assert_eq!(
        server.get(&format!(
            "{root}/budget?month={}",
            case["month"].as_str().unwrap()
        )),
        before_restart
    );
    assert_eq!(server.get(&format!("{root}/checkpoint")), checkpoint);
    assert_eq!(assert_source_ledger(&server, &root, case), original);
}

fn run_cleanup_case(name: &str) {
    let oracle: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/budget-cleanup.json")).unwrap();
    let case = oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap();
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/budget-cleanup")
            .join(case["archive"]["filename"].as_str().unwrap()),
    )
    .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        case["archive"]["sha256"]
    );
    let server = Server::start(tempfile::tempdir().unwrap());
    let stage: Value = server
        .client
        .post(format!("{}/api/v1/imports/actual", server.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "cleanup-source")
        .body(bytes)
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(stage["data"]["status"], "ready", "{stage}");
    let response = server.post(
        &format!(
            "/api/v1/imports/{}/activate",
            stage["data"]["id"].as_str().unwrap()
        ),
        "activate-cleanup",
        &json!({}),
        None,
    );
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    let root = format!(
        "/api/v1/budgets/{}",
        stage["data"]["budget_id"].as_str().unwrap()
    );
    assert_budget(&server, &root, case, &case["seed"]);
    let original = assert_source_ledger(&server, &root, case);
    let definitions = server.get(&format!("{root}/budget/templates?month=2026-11"));
    for (category, expected) in case["definitions"].as_object().unwrap() {
        let row = definitions["data"]["categories"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == case["ids"][category])
            .unwrap();
        let mut cleanup = expected["cleanup"].as_array().cloned().unwrap_or_default();
        for directive in &mut cleanup {
            if let Some(label) = directive["groupId"].as_str().map(str::to_owned) {
                directive["groupId"] = case["ids"][label].clone();
            }
        }
        assert_eq!(
            row["cleanup"],
            json!(cleanup),
            "{name} imported cleanup definition"
        );
        assert_eq!(
            row["template_settings"], expected["template_settings"],
            "{name} effective source precedence"
        );
    }
    let mut receipts = vec![];
    for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
        let checkpoint = server.get(&format!("{root}/checkpoint"));
        let epoch = checkpoint["data"]["epoch"].as_str().unwrap().to_owned();
        let body = json!({"action":"cleanup","month":case["month"],"snapshot":checkpoint["data"]["snapshot"]});
        let preview = server.post(
            &format!("{root}/budget/templates/preview"),
            "cleanup-preview",
            &body,
            None,
        );
        assert_eq!(preview.status(), 200, "{name}: {}", preview.text().unwrap());
        let preview: Value = preview.json().unwrap();
        assert_eq!(
            preview["data"]["notification"], step["notification"],
            "{name} preview warnings"
        );
        for (category, expected) in step["result"]["categories"].as_object().unwrap() {
            let row = preview["data"]["categories"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == case["ids"][category])
                .unwrap();
            let income = case["method"] == "envelope" && category == "Salary";
            assert_eq!(
                row["budgeted"],
                if income {
                    json!(0)
                } else {
                    expected["budget"].clone()
                },
                "{name} {category} cleanup preview"
            );
            assert_eq!(
                row["goal"], expected["goal"],
                "{name} {category} cleanup goal"
            );
            assert_eq!(
                row["long_goal"], expected["long-goal"],
                "{name} {category} cleanup long goal"
            );
        }
        assert_eq!(
            server.get(&format!("{root}/checkpoint")),
            checkpoint,
            "read-only cleanup preview"
        );
        assert_eq!(assert_source_ledger(&server, &root, case), original);
        let key = format!("cleanup-{index}");
        let response = server.post(
            &format!("{root}/budget/templates/commands"),
            &key,
            &body,
            Some(&epoch),
        );
        assert_eq!(
            response.status(),
            200,
            "{name}: {}",
            response.text().unwrap()
        );
        let receipt: Value = response.json().unwrap();
        assert_eq!(
            receipt["notification"], step["notification"],
            "{name} source warning/count result"
        );
        receipts.push((key, body, epoch, receipt));
        assert_budget(&server, &root, case, &step["result"]);
        assert_eq!(assert_source_ledger(&server, &root, case), original);
    }
    let checkpoint = server.get(&format!("{root}/checkpoint"));
    let server = server.restart();
    for (key, body, epoch, expected) in receipts {
        let replay = server.post(
            &format!("{root}/budget/templates/commands"),
            &key,
            &body,
            Some(&epoch),
        );
        assert_eq!(replay.status(), 200);
        assert_eq!(replay.json::<Value>().unwrap(), expected);
    }
    assert_eq!(
        server.get(&format!("{root}/checkpoint")),
        checkpoint,
        "replay never reallocates"
    );
    assert_budget(
        &server,
        &root,
        case,
        &case["steps"].as_array().unwrap().last().unwrap()["result"],
    );
    assert_eq!(assert_source_ledger(&server, &root, case), original);
}

fn run_visual_case(name: &str) {
    let oracle: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/budget-visual-automations.json"
    ))
    .unwrap();
    let case = oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap();
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/budget-visual-automations")
            .join(case["archive"]["filename"].as_str().unwrap()),
    )
    .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        case["archive"]["sha256"]
    );
    let server = Server::start(tempfile::tempdir().unwrap());
    let stage: Value = server
        .client
        .post(format!("{}/api/v1/imports/actual", server.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "visual-source")
        .body(bytes)
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(stage["data"]["status"], "ready", "{stage}");
    let response = server.post(
        &format!(
            "/api/v1/imports/{}/activate",
            stage["data"]["id"].as_str().unwrap()
        ),
        "activate-visual",
        &json!({}),
        None,
    );
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    let root = format!(
        "/api/v1/budgets/{}",
        stage["data"]["budget_id"].as_str().unwrap()
    );
    assert_budget(&server, &root, case, &case["seed"]);
    let original = assert_source_ledger(&server, &root, case);
    let definitions = server.get(&format!("{root}/budget/templates?month=2026-11"));
    let def = definitions["data"]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == case["ids"]["A"])
        .unwrap();
    assert_eq!(
        def["source"],
        if case["mode"] == "ui" { "ui" } else { "notes" }
    );
    let mut effective = def["templates"].clone();
    for t in effective.as_array_mut().unwrap() {
        for field in ["category", "scheduleId"] {
            if let Some((label, _)) = case["ids"]
                .as_object()
                .unwrap()
                .iter()
                .find(|(_, id)| **id == t[field])
            {
                t[field] = json!(label);
            }
        }
    }
    assert_eq!(
        effective, case["effective"],
        "{name} effective imported definition"
    );
    let checkpoint = server.get(&format!("{root}/checkpoint"));
    let epoch = checkpoint["data"]["epoch"].as_str().unwrap();
    let request =
        json!({"month":case["month"],"snapshot":checkpoint["data"]["snapshot"],"force":true});
    let preview = server.post(
        &format!("{root}/budget/templates/preview"),
        "visual-preview",
        &request,
        None,
    );
    assert_eq!(preview.status(), 200, "{}", preview.text().unwrap());
    let preview: Value = preview.json().unwrap();
    assert_eq!(preview["data"]["errors"], json!([]), "{name}: {preview}");
    for (category, expected) in case["result"]["categories"].as_object().unwrap() {
        let row = preview["data"]["categories"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == case["ids"][category])
            .unwrap();
        assert_eq!(
            row["budgeted"],
            if case["method"] == "envelope" && category == "Salary" {
                json!(0)
            } else {
                expected["budget"].clone()
            },
            "{name} {category} visual preview"
        );
        assert_eq!(row["goal"], expected["goal"]);
        assert_eq!(row["long_goal"], expected["long-goal"]);
    }
    assert_eq!(server.get(&format!("{root}/checkpoint")), checkpoint);
    let body = json!({"action":"overwrite","month":case["month"],"snapshot":checkpoint["data"]["snapshot"]});
    let response = server.post(
        &format!("{root}/budget/templates/commands"),
        "visual-apply",
        &body,
        Some(epoch),
    );
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let receipt: Value = response.json().unwrap();
    assert_budget(&server, &root, case, &case["result"]);
    assert_eq!(assert_source_ledger(&server, &root, case), original);
    let committed = server.get(&format!("{root}/checkpoint"));
    let server = server.restart();
    let replay = server.post(
        &format!("{root}/budget/templates/commands"),
        "visual-apply",
        &body,
        Some(epoch),
    );
    assert_eq!(replay.status(), 200);
    assert_eq!(replay.json::<Value>().unwrap(), receipt);
    assert_eq!(server.get(&format!("{root}/checkpoint")), committed);
    assert_budget(&server, &root, case, &case["result"]);
    assert_eq!(assert_source_ledger(&server, &root, case), original);
}

#[test]
fn cleanup_new_pool_preview_cancel_undo_and_redo_keep_one_financial_action() {
    let oracle: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/budget-cleanup.json")).unwrap();
    let case = oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "envelope-global-weighted")
        .unwrap();
    let server = Server::start(tempfile::tempdir().unwrap());
    let stage: Value = server
        .client
        .post(format!("{}/api/v1/imports/actual", server.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "pool-source")
        .body(
            include_bytes!("../../tests/fixtures/budget-cleanup/envelope-global-weighted.zip")
                .to_vec(),
        )
        .send()
        .unwrap()
        .json()
        .unwrap();
    let response = server.post(
        &format!(
            "/api/v1/imports/{}/activate",
            stage["data"]["id"].as_str().unwrap()
        ),
        "activate-pool",
        &json!({}),
        None,
    );
    assert_eq!(response.status(), 201);
    let root = format!(
        "/api/v1/budgets/{}",
        stage["data"]["budget_id"].as_str().unwrap()
    );
    let epoch = server.get(&format!("{root}/checkpoint"))["data"]["epoch"]
        .as_str()
        .unwrap()
        .to_owned();
    for (category, directive) in [("A", "source"), ("B", "sink1"), ("C", "sink 2")] {
        let cp = server.get(&format!("{root}/checkpoint"));
        let response=server.post(&format!("{root}/budget"),&format!("pool-note-{category}"),&json!({"action":"category_notes","month":"2026-11","category":case["ids"][category],"notes":format!("Original {category} description\n#cleanup Reserve {directive}"),"snapshot":cp["data"]["snapshot"]}),Some(&epoch));
        assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    }
    let checkpoint = server.get(&format!("{root}/checkpoint"));
    let original = assert_source_ledger(&server, &root, case);
    let definitions = server.get(&format!("{root}/budget/templates?month=2026-11"));
    assert_eq!(definitions["data"]["pools"], json!([]));
    let body =
        json!({"action":"cleanup","month":"2026-11","snapshot":checkpoint["data"]["snapshot"]});
    let preview = || {
        let response = server.post(
            &format!("{root}/budget/templates/preview"),
            "new-pool-preview",
            &body,
            None,
        );
        assert_eq!(response.status(), 200, "{}", response.text().unwrap());
        response.json::<Value>().unwrap()
    };
    let first = preview();
    assert_eq!(
        preview(),
        first,
        "pool identities are deterministic during preview"
    );
    assert_eq!(first["data"]["pools"].as_array().unwrap().len(), 1);
    let pool = first["data"]["pools"][0]["id"].clone();
    assert_eq!(server.get(&format!("{root}/checkpoint")), checkpoint);
    assert_eq!(
        server.get(&format!("{root}/budget/templates?month=2026-11")),
        definitions
    );
    let canceled = server
        .client
        .post(format!("{}{root}/budget/templates/commands", server.url))
        .header("Idempotency-Key", "cancel-new-pool")
        .header(
            "X-Actual-History-Owner",
            "23240000-0000-4000-8000-000000000998",
        )
        .header("X-Actual-Budget-Epoch", &epoch)
        .header("X-Actual-Cancel-If-Uncommitted", "true")
        .json(&body)
        .send()
        .unwrap();
    assert_eq!(canceled.status(), 409);
    assert_eq!(
        canceled.json::<Value>().unwrap()["error"]["code"],
        "operation_canceled"
    );
    let blocked = server.post(
        &format!("{root}/budget/templates/commands"),
        "cancel-new-pool",
        &body,
        Some(&epoch),
    );
    assert_eq!(blocked.status(), 409);
    assert_eq!(server.get(&format!("{root}/checkpoint")), checkpoint);
    let applied = server.post(
        &format!("{root}/budget/templates/commands"),
        "apply-new-pool",
        &body,
        Some(&epoch),
    );
    assert_eq!(applied.status(), 200, "{}", applied.text().unwrap());
    let receipt: Value = applied.json().unwrap();
    let definitions = server.get(&format!("{root}/budget/templates?month=2026-11"));
    assert_eq!(definitions["data"]["pools"][0]["id"], pool);
    assert_eq!(definitions["data"]["pools"][0]["name"], "Reserve");
    let committed = server.get(&format!("{root}/budget?month=2026-11"));
    for (category, amount) in [("A", 4000), ("B", 2000), ("C", 4000)] {
        let row = committed["data"]["months"]["2026-11"]["categories"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == case["ids"][category])
            .unwrap();
        assert_eq!(row["budgeted"], amount);
        assert_eq!(row["goal"], Value::Null);
    }
    let history = || {
        server
            .client
            .get(format!("{}{root}/history", server.url))
            .header(
                "X-Actual-History-Owner",
                "23240000-0000-4000-8000-000000000998",
            )
            .send()
            .unwrap()
            .json::<Value>()
            .unwrap()
    };
    let undone = server.post(
        &format!("{root}/history/undo"),
        "undo-pool",
        &json!({"action_id":history()["data"]["undo"]["id"]}),
        Some(&epoch),
    );
    assert_eq!(undone.status(), 200, "{}", undone.text().unwrap());
    assert_eq!(
        server.get(&format!("{root}/budget/templates?month=2026-11"))["data"]["pools"],
        json!([])
    );
    let before = server.get(&format!("{root}/budget?month=2026-11"));
    for (category, amount) in [("A", 10000), ("B", 0), ("C", 0)] {
        let row = before["data"]["months"]["2026-11"]["categories"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == case["ids"][category])
            .unwrap();
        assert_eq!(row["budgeted"], amount);
    }
    let redo = server.post(
        &format!("{root}/history/redo"),
        "redo-pool",
        &json!({"action_id":history()["data"]["redo"]["id"]}),
        Some(&epoch),
    );
    assert_eq!(redo.status(), 200, "{}", redo.text().unwrap());
    let after = server.get(&format!("{root}/budget?month=2026-11"));
    assert_eq!(after["data"]["months"], committed["data"]["months"]);
    assert_eq!(
        server.get(&format!("{root}/budget/templates?month=2026-11"))["data"]["pools"][0]["id"],
        pool
    );
    assert_eq!(assert_source_ledger(&server, &root, case), original);
    let checkpoint = server.get(&format!("{root}/checkpoint"));
    let replay = server.post(
        &format!("{root}/budget/templates/commands"),
        "apply-new-pool",
        &body,
        Some(&epoch),
    );
    assert_eq!(replay.status(), 200);
    assert_eq!(replay.json::<Value>().unwrap(), receipt);
    assert_eq!(server.get(&format!("{root}/checkpoint")), checkpoint);
}

macro_rules! cases { ($($test:ident => $name:literal),* $(,)?) => { $(#[test] fn $test(){run_case($name);})* }; }

cases! {
    envelope_apply_skip_overwrite_selected => "envelope-apply-skip-overwrite-selected",
    tracking_apply_skip_overwrite_selected => "tracking-apply-skip-overwrite-selected",
    envelope_multiline_long_goal => "envelope-multiline-long-goal",
    tracking_multiline_long_goal => "tracking-multiline-long-goal",
    envelope_goal_only => "envelope-goal-only",
    tracking_goal_only => "tracking-goal-only",
    envelope_monthly_cap => "envelope-monthly-cap",
    tracking_monthly_cap => "tracking-monthly-cap",
    envelope_refill_and_held_refill => "envelope-refill-and-held-refill",
    tracking_refill_and_held_refill => "tracking-refill-and-held-refill",
    envelope_daily_weekly_caps => "envelope-daily-weekly-caps",
    tracking_daily_weekly_caps => "tracking-daily-weekly-caps",
    envelope_save_by_date => "envelope-save-by-date",
    tracking_save_by_date => "tracking-save-by-date",
    envelope_repeating_target => "envelope-repeating-target",
    tracking_repeating_target => "tracking-repeating-target",
    envelope_spend_from_history => "envelope-spend-from-history",
    tracking_spend_from_history => "tracking-spend-from-history",
    envelope_periodic_day_week_month => "envelope-periodic-day-week-month",
    tracking_periodic_day_week_month => "tracking-periodic-day-week-month",
    envelope_average_and_adjustments => "envelope-average-and-adjustments",
    tracking_average_and_adjustments => "tracking-average-and-adjustments",
    envelope_copy_history => "envelope-copy-history",
    tracking_copy_history => "tracking-copy-history",
    envelope_income_percentages => "envelope-income-percentages",
    tracking_income_percentages => "tracking-income-percentages",
    envelope_available_per_priority => "envelope-available-per-priority",
    tracking_available_per_priority => "tracking-available-per-priority",
    envelope_priority_demand_and_clamp => "envelope-priority-demand-and-clamp",
    tracking_priority_demand_and_clamp => "tracking-priority-demand-and-clamp",
    envelope_weighted_remainder_pennies => "envelope-weighted-remainder-pennies",
    tracking_weighted_remainder_pennies => "tracking-weighted-remainder-pennies",
    envelope_capped_remainder_redistribution => "envelope-capped-remainder-redistribution",
    tracking_capped_remainder_redistribution => "tracking-capped-remainder-redistribution",
    envelope_schedule_demand_adjustments => "envelope-schedule-demand-adjustments",
    tracking_schedule_demand_adjustments => "tracking-schedule-demand-adjustments",
    envelope_hidden_and_selected => "envelope-hidden-and-selected",
    tracking_hidden_and_selected => "tracking-hidden-and-selected",
    envelope_invalid_goals_atomic => "envelope-invalid-goals-atomic",
    tracking_invalid_goals_atomic => "tracking-invalid-goals-atomic",
    envelope_invalid_schedule_atomic => "envelope-invalid-schedule-atomic",
    tracking_invalid_schedule_atomic => "tracking-invalid-schedule-atomic",
    envelope_invalid_income_category => "envelope-invalid-income-category",
    tracking_invalid_income_category => "tracking-invalid-income-category",
    envelope_fraction_hidden => "envelope-fraction-hidden",
    tracking_fraction_hidden => "tracking-fraction-hidden",
    envelope_jpy_minor_units => "envelope-jpy-minor-units",
    tracking_jpy_minor_units => "tracking-jpy-minor-units",
    envelope_kwd_minor_units => "envelope-kwd-minor-units",
    tracking_kwd_minor_units => "tracking-kwd-minor-units",
    tracking_tracking_income_category => "tracking-tracking-income-category",
}

mod cleanup {
    use super::run_cleanup_case;
    #[test]
    fn envelope_global_weighted() {
        run_cleanup_case("envelope-global-weighted");
    }
    #[test]
    fn tracking_global_weighted() {
        run_cleanup_case("tracking-global-weighted");
    }
    #[test]
    fn envelope_global_unallocated_and_pennies() {
        run_cleanup_case("envelope-global-unallocated-and-pennies");
    }
    #[test]
    fn tracking_global_unallocated_and_pennies() {
        run_cleanup_case("tracking-global-unallocated-and-pennies");
    }
    #[test]
    fn envelope_source_without_sink() {
        run_cleanup_case("envelope-source-without-sink");
    }
    #[test]
    fn tracking_source_without_sink() {
        run_cleanup_case("tracking-source-without-sink");
    }
    #[test]
    fn envelope_sink_without_source() {
        run_cleanup_case("envelope-sink-without-source");
    }
    #[test]
    fn tracking_sink_without_source() {
        run_cleanup_case("tracking-sink-without-source");
    }
    #[test]
    fn envelope_overspending_no_directives() {
        run_cleanup_case("envelope-overspending-no-directives");
    }
    #[test]
    fn tracking_overspending_no_directives() {
        run_cleanup_case("tracking-overspending-no-directives");
    }
    #[test]
    fn envelope_global_overspending_first() {
        run_cleanup_case("envelope-global-overspending-first");
    }
    #[test]
    fn tracking_global_overspending_first() {
        run_cleanup_case("tracking-global-overspending-first");
    }
    #[test]
    fn envelope_rollover_overspending() {
        run_cleanup_case("envelope-rollover-overspending");
    }
    #[test]
    fn tracking_rollover_overspending() {
        run_cleanup_case("tracking-rollover-overspending");
    }
    #[test]
    fn envelope_negative_source() {
        run_cleanup_case("envelope-negative-source");
    }
    #[test]
    fn tracking_negative_source() {
        run_cleanup_case("tracking-negative-source");
    }
    #[test]
    fn envelope_negative_global_available() {
        run_cleanup_case("envelope-negative-global-available");
    }
    #[test]
    fn tracking_negative_global_available() {
        run_cleanup_case("tracking-negative-global-available");
    }
    #[test]
    fn envelope_buffer_source_and_sink() {
        run_cleanup_case("envelope-buffer-source-and-sink");
    }
    #[test]
    fn tracking_buffer_source_and_sink() {
        run_cleanup_case("tracking-buffer-source-and-sink");
    }
    #[test]
    fn envelope_pool_weighted_isolated() {
        run_cleanup_case("envelope-pool-weighted-isolated");
    }
    #[test]
    fn tracking_pool_weighted_isolated() {
        run_cleanup_case("tracking-pool-weighted-isolated");
    }
    #[test]
    fn envelope_pool_rounding_pennies() {
        run_cleanup_case("envelope-pool-rounding-pennies");
    }
    #[test]
    fn tracking_pool_rounding_pennies() {
        run_cleanup_case("tracking-pool-rounding-pennies");
    }
    #[test]
    fn envelope_pool_overspend_before_sink() {
        run_cleanup_case("envelope-pool-overspend-before-sink");
    }
    #[test]
    fn tracking_pool_overspend_before_sink() {
        run_cleanup_case("tracking-pool-overspend-before-sink");
    }
    #[test]
    fn envelope_pool_partial_overspend() {
        run_cleanup_case("envelope-pool-partial-overspend");
    }
    #[test]
    fn tracking_pool_partial_overspend() {
        run_cleanup_case("tracking-pool-partial-overspend");
    }
    #[test]
    fn envelope_pool_rollover() {
        run_cleanup_case("envelope-pool-rollover");
    }
    #[test]
    fn tracking_pool_rollover() {
        run_cleanup_case("tracking-pool-rollover");
    }
    #[test]
    fn envelope_pool_missing_sink() {
        run_cleanup_case("envelope-pool-missing-sink");
    }
    #[test]
    fn tracking_pool_missing_sink() {
        run_cleanup_case("tracking-pool-missing-sink");
    }
    #[test]
    fn envelope_pool_negative_source() {
        run_cleanup_case("envelope-pool-negative-source");
    }
    #[test]
    fn tracking_pool_negative_source() {
        run_cleanup_case("tracking-pool-negative-source");
    }
    #[test]
    fn envelope_pool_case_insensitive() {
        run_cleanup_case("envelope-pool-case-insensitive");
    }
    #[test]
    fn tracking_pool_case_insensitive() {
        run_cleanup_case("tracking-pool-case-insensitive");
    }
    #[test]
    fn envelope_pool_then_global() {
        run_cleanup_case("envelope-pool-then-global");
    }
    #[test]
    fn tracking_pool_then_global() {
        run_cleanup_case("tracking-pool-then-global");
    }
    #[test]
    fn envelope_grammar_and_original_notes() {
        run_cleanup_case("envelope-grammar-and-original-notes");
    }
    #[test]
    fn tracking_grammar_and_original_notes() {
        run_cleanup_case("tracking-grammar-and-original-notes");
    }
    #[test]
    fn envelope_ui_source_ignores_stale_notes() {
        run_cleanup_case("envelope-ui-source-ignores-stale-notes");
    }
    #[test]
    fn tracking_ui_source_ignores_stale_notes() {
        run_cleanup_case("tracking-ui-source-ignores-stale-notes");
    }
    #[test]
    fn envelope_ui_pool_ignores_stale_notes() {
        run_cleanup_case("envelope-ui-pool-ignores-stale-notes");
    }
    #[test]
    fn tracking_ui_pool_ignores_stale_notes() {
        run_cleanup_case("tracking-ui-pool-ignores-stale-notes");
    }
}

mod visual {
    use super::run_visual_case;
    #[test]
    fn envelope_fixed_with_standalone_cap_ui() {
        run_visual_case("envelope-fixed-with-standalone-cap-ui");
    }
    #[test]
    fn envelope_fixed_with_standalone_cap_rendered_notes() {
        run_visual_case("envelope-fixed-with-standalone-cap-rendered-notes");
    }
    #[test]
    fn tracking_fixed_with_standalone_cap_ui() {
        run_visual_case("tracking-fixed-with-standalone-cap-ui");
    }
    #[test]
    fn tracking_fixed_with_standalone_cap_rendered_notes() {
        run_visual_case("tracking-fixed-with-standalone-cap-rendered-notes");
    }
    #[test]
    fn envelope_refill_with_standalone_cap_ui() {
        run_visual_case("envelope-refill-with-standalone-cap-ui");
    }
    #[test]
    fn envelope_refill_with_standalone_cap_rendered_notes() {
        run_visual_case("envelope-refill-with-standalone-cap-rendered-notes");
    }
    #[test]
    fn tracking_refill_with_standalone_cap_ui() {
        run_visual_case("tracking-refill-with-standalone-cap-ui");
    }
    #[test]
    fn tracking_refill_with_standalone_cap_rendered_notes() {
        run_visual_case("tracking-refill-with-standalone-cap-rendered-notes");
    }
    #[test]
    fn envelope_held_weekly_cap_ui() {
        run_visual_case("envelope-held-weekly-cap-ui");
    }
    #[test]
    fn envelope_held_weekly_cap_rendered_notes() {
        run_visual_case("envelope-held-weekly-cap-rendered-notes");
    }
    #[test]
    fn tracking_held_weekly_cap_ui() {
        run_visual_case("tracking-held-weekly-cap-ui");
    }
    #[test]
    fn tracking_held_weekly_cap_rendered_notes() {
        run_visual_case("tracking-held-weekly-cap-rendered-notes");
    }
    #[test]
    fn envelope_fixed_plus_long_goal_ui() {
        run_visual_case("envelope-fixed-plus-long-goal-ui");
    }
    #[test]
    fn envelope_fixed_plus_long_goal_rendered_notes() {
        run_visual_case("envelope-fixed-plus-long-goal-rendered-notes");
    }
    #[test]
    fn tracking_fixed_plus_long_goal_ui() {
        run_visual_case("tracking-fixed-plus-long-goal-ui");
    }
    #[test]
    fn tracking_fixed_plus_long_goal_rendered_notes() {
        run_visual_case("tracking-fixed-plus-long-goal-rendered-notes");
    }
    #[test]
    fn envelope_save_target_ui() {
        run_visual_case("envelope-save-target-ui");
    }
    #[test]
    fn envelope_save_target_rendered_notes() {
        run_visual_case("envelope-save-target-rendered-notes");
    }
    #[test]
    fn tracking_save_target_ui() {
        run_visual_case("tracking-save-target-ui");
    }
    #[test]
    fn tracking_save_target_rendered_notes() {
        run_visual_case("tracking-save-target-rendered-notes");
    }
    #[test]
    fn envelope_spend_target_ui() {
        run_visual_case("envelope-spend-target-ui");
    }
    #[test]
    fn envelope_spend_target_rendered_notes() {
        run_visual_case("envelope-spend-target-rendered-notes");
    }
    #[test]
    fn tracking_spend_target_ui() {
        run_visual_case("tracking-spend-target-ui");
    }
    #[test]
    fn tracking_spend_target_rendered_notes() {
        run_visual_case("tracking-spend-target-rendered-notes");
    }
    #[test]
    fn envelope_history_adjusted_ui() {
        run_visual_case("envelope-history-adjusted-ui");
    }
    #[test]
    fn envelope_history_adjusted_rendered_notes() {
        run_visual_case("envelope-history-adjusted-rendered-notes");
    }
    #[test]
    fn tracking_history_adjusted_ui() {
        run_visual_case("tracking-history-adjusted-ui");
    }
    #[test]
    fn tracking_history_adjusted_rendered_notes() {
        run_visual_case("tracking-history-adjusted-rendered-notes");
    }
    #[test]
    fn envelope_income_category_id_ui() {
        run_visual_case("envelope-income-category-id-ui");
    }
    #[test]
    fn envelope_income_category_id_rendered_notes() {
        run_visual_case("envelope-income-category-id-rendered-notes");
    }
    #[test]
    fn tracking_income_category_id_ui() {
        run_visual_case("tracking-income-category-id-ui");
    }
    #[test]
    fn tracking_income_category_id_rendered_notes() {
        run_visual_case("tracking-income-category-id-rendered-notes");
    }
    #[test]
    fn envelope_schedule_id_and_adjustment_ui() {
        run_visual_case("envelope-schedule-id-and-adjustment-ui");
    }
    #[test]
    fn envelope_schedule_id_and_adjustment_rendered_notes() {
        run_visual_case("envelope-schedule-id-and-adjustment-rendered-notes");
    }
    #[test]
    fn tracking_schedule_id_and_adjustment_ui() {
        run_visual_case("tracking-schedule-id-and-adjustment-ui");
    }
    #[test]
    fn tracking_schedule_id_and_adjustment_rendered_notes() {
        run_visual_case("tracking-schedule-id-and-adjustment-rendered-notes");
    }
    #[test]
    fn envelope_weighted_remainder_cap_ui() {
        run_visual_case("envelope-weighted-remainder-cap-ui");
    }
    #[test]
    fn envelope_weighted_remainder_cap_rendered_notes() {
        run_visual_case("envelope-weighted-remainder-cap-rendered-notes");
    }
    #[test]
    fn tracking_weighted_remainder_cap_ui() {
        run_visual_case("tracking-weighted-remainder-cap-ui");
    }
    #[test]
    fn tracking_weighted_remainder_cap_rendered_notes() {
        run_visual_case("tracking-weighted-remainder-cap-rendered-notes");
    }
}
