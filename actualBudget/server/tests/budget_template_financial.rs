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
