//! Genuine old-binary state, exercised only through the running public API.
use reqwest::blocking::Client;
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
    _data: TempDir,
    client: Client,
}
impl Server {
    fn start(data: TempDir) -> Self {
        std::fs::write(data.path().join("index.html"), "Upgrade fixture").unwrap();
        let mut process = Command::new(env!("CARGO_BIN_EXE_actualbudget-server"))
            .env("ACTUALBUDGET_DATA_DIR", data.path())
            .env("ACTUALBUDGET_WEB_DIR", data.path())
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
            _data: data,
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
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures")
            .join(name),
    )
    .unwrap()
}
fn read_history(server: &Server, root: &str, f: &Value) -> Value {
    let mut request = server.client.get(format!("{}{root}/history", server.url));
    for (k, v) in f["owner_headers"].as_object().unwrap() {
        request = request.header(k, v.as_str().unwrap());
    }
    request
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap()
}

fn persisted_schedules(mut view: Value, restored: bool) -> Value {
    // These projections use the real current UTC date, not persisted state.
    // Keep every schedule definition, next date and page/horizon setting.
    for field in ["today", "previews", "preview_continuations"] {
        view["data"].as_object_mut().unwrap().remove(field);
    }
    for row in view["data"]["schedules"].as_array_mut().unwrap() {
        row.as_object_mut().unwrap().remove("status");
    }
    if restored {
        for field in ["snapshot", "epoch"] {
            view["data"].as_object_mut().unwrap().remove(field);
        }
    }
    view
}

#[test]
fn genuine_schema19_keeps_goal_cells_notes_schedules_history_and_original_receipts() {
    let f: Value = serde_json::from_slice(&fixture("budget-automation-schema19.json")).unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::write(
        data.path().join("actualbudget.sqlite3"),
        fixture("budget-automation-schema19.sqlite3"),
    )
    .unwrap();
    let server = Server::start(data);
    let root = format!("/api/v1/budgets/{}", f["metadata"]["id"].as_str().unwrap());
    let register = format!(
        "{root}/transactions?account_id={}",
        f["account_id"].as_str().unwrap()
    );
    assert_eq!(server.get(&register), f["register"]);
    assert_eq!(
        server.get(&format!("{root}/budget?month=2026-11")),
        f["plan"]
    );
    assert_eq!(server.get(&format!("{root}/accounts")), f["accounts"]);
    assert_eq!(
        persisted_schedules(server.get(&format!("{root}/schedules")), false),
        persisted_schedules(f["schedules"].clone(), false)
    );
    assert_eq!(server.get(&format!("{root}/checkpoint")), f["checkpoint"]);
    assert_eq!(read_history(&server, &root, &f), f["history"]);
    for command in f["commands"].as_array().unwrap() {
        let mut request = server
            .client
            .request(
                command["method"].as_str().unwrap().parse().unwrap(),
                format!(
                    "{}{}{}",
                    server.url,
                    root,
                    command["path"].as_str().unwrap()
                ),
            )
            .header("Idempotency-Key", command["key"].as_str().unwrap())
            .json(&command["body"]);
        for (k, v) in f["owner_headers"].as_object().unwrap() {
            request = request.header(k, v.as_str().unwrap());
        }
        let result: Value = request
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .unwrap();
        assert_eq!(result, command["result"], "old receipt {}", command["key"]);
    }
    assert_eq!(server.get(&register), f["register"]);
    assert_eq!(
        server.get(&format!("{root}/budget?month=2026-11")),
        f["plan"]
    );
    assert_eq!(read_history(&server, &root, &f), f["history"]);
    for blocked in f["blocked"].as_array().unwrap() {
        let id = blocked["result"]["data"]["id"].as_str().unwrap();
        let latest = server.get(&format!("/api/v1/imports/{id}"));
        assert_eq!(
            latest["data"]["status"], "ready",
            "current converter must refresh old blocked stage: {latest}"
        );
        let bytes = fixture(blocked["archive"].as_str().unwrap());
        assert_eq!(format!("{:x}", Sha256::digest(&bytes)), blocked["sha256"]);
        let original = server
            .client
            .get(format!("{}/api/v1/imports/{id}/original.zip", server.url))
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .bytes()
            .unwrap();
        assert_eq!(original.as_ref(), bytes);
        let replay: Value = server
            .client
            .post(format!("{}/api/v1/imports/actual", server.url))
            .header("Content-Type", "application/zip")
            .header("Idempotency-Key", blocked["key"].as_str().unwrap())
            .body(bytes)
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .unwrap();
        assert_eq!(
            replay, blocked["result"],
            "original blocked response receipt is immutable"
        );
        let activation = server
            .client
            .post(format!("{}/api/v1/imports/{id}/activate", server.url))
            .header("Idempotency-Key", format!("activate-{id}"))
            .json(&json!({}))
            .send()
            .unwrap();
        assert_eq!(activation.status(), 201, "{}", activation.text().unwrap());
    }
    assert_eq!(server.get(&register), f["register"]);
    assert_eq!(server.get(&format!("{root}/checkpoint")), f["checkpoint"]);
}

#[test]
fn genuine_schema19_native_backup_restores_goals_notes_and_schedule_with_new_epoch() {
    let f: Value = serde_json::from_slice(&fixture("budget-automation-schema19.json")).unwrap();
    let bytes = fixture("budget-automation-schema19.zip");
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), f["archive_sha256"]);
    let server = Server::start(tempfile::tempdir().unwrap());
    let root = format!("/api/v1/budgets/{}", f["metadata"]["id"].as_str().unwrap());
    let uploaded = server
        .client
        .post(format!("{}/api/v1/restores", server.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "native19")
        .body(bytes)
        .send()
        .unwrap();
    assert_eq!(uploaded.status(), 201, "{}", uploaded.text().unwrap());
    let stage: Value = uploaded.json().unwrap();
    assert_eq!(stage["data"]["report"]["schema_version"], 20);
    let activated = server
        .client
        .post(format!(
            "{}{root}/restores/{}/activate",
            server.url,
            stage["data"]["id"].as_str().unwrap()
        ))
        .header("Idempotency-Key", "activate-native19")
        .json(&json!({"confirm_name":f["metadata"]["name"]}))
        .send()
        .unwrap();
    assert_eq!(activated.status(), 200, "{}", activated.text().unwrap());
    assert_ne!(server.get(&root)["data"]["epoch"], f["metadata"]["epoch"]);
    let register = server.get(&format!(
        "{root}/transactions?account_id={}",
        f["account_id"].as_str().unwrap()
    ));
    let mut expected = f["register"].clone();
    assert_ne!(register["snapshot"], expected["snapshot"]);
    expected["snapshot"] = register["snapshot"].clone();
    assert_eq!(register, expected);
    let mut plan = server.get(&format!("{root}/budget?month=2026-11"));
    let mut expected = f["plan"].clone();
    // Restore changes only operational epoch/snapshot; every financial cell,
    // original note and category ordering remains exactly the old public view.
    for field in ["snapshot", "epoch"] {
        plan["data"].as_object_mut().unwrap().remove(field);
        expected["data"].as_object_mut().unwrap().remove(field);
    }
    assert_eq!(plan, expected);
    assert_eq!(
        persisted_schedules(server.get(&format!("{root}/schedules")), true),
        persisted_schedules(f["schedules"].clone(), true)
    );
}

fn currency_upgrade_case(name: &str, native: bool) {
    let stem = format!("budget-currency-schema19-{name}");
    let f: Value = serde_json::from_slice(&fixture(&format!("{stem}.json"))).unwrap();
    let data = tempfile::tempdir().unwrap();
    if !native {
        std::fs::write(
            data.path().join("actualbudget.sqlite3"),
            fixture(&format!("{stem}.sqlite3")),
        )
        .unwrap();
    }
    let server = Server::start(data);
    let root = format!("/api/v1/budgets/{}", f["metadata"]["id"].as_str().unwrap());
    if native {
        let bytes = fixture(&format!("{stem}.zip"));
        assert_eq!(format!("{:x}", Sha256::digest(&bytes)), f["archive_sha256"]);
        let response = server
            .client
            .post(format!("{}/api/v1/restores", server.url))
            .header("Content-Type", "application/zip")
            .header("Idempotency-Key", "native-currency")
            .body(bytes)
            .send()
            .unwrap();
        assert_eq!(response.status(), 201, "{}", response.text().unwrap());
        let stage: Value = response.json().unwrap();
        let response = server
            .client
            .post(format!(
                "{}{root}/restores/{}/activate",
                server.url,
                stage["data"]["id"].as_str().unwrap()
            ))
            .header("Idempotency-Key", "activate-currency")
            .json(&json!({"confirm_name":f["metadata"]["name"]}))
            .send()
            .unwrap();
        assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    }
    assert_eq!(
        server.get(&format!("{root}/preferences")),
        f["preferences"],
        "legacy current preferences survive"
    );
    if !native {
        for receipt in f["receipts"].as_array().unwrap() {
            let replay = server
                .client
                .patch(format!("{}{root}/preferences", server.url))
                .header("Idempotency-Key", receipt["key"].as_str().unwrap())
                .json(&receipt["body"])
                .send()
                .unwrap();
            assert_eq!(replay.status(), 200);
            assert_eq!(replay.json::<Value>().unwrap(), receipt["result"]);
        }
        assert_eq!(server.get(&format!("{root}/checkpoint")), f["checkpoint"]);
    }
    let register = server.get(&format!("{root}/transactions"));
    if name == "ambiguous" {
        let before = server.get(&format!("{root}/checkpoint"));
        let response = server
            .client
            .post(format!("{}{root}/budget/templates/preview", server.url))
            .json(&json!({"month":"2026-11","snapshot":before["data"]["snapshot"],"force":true}))
            .send()
            .unwrap();
        assert_eq!(response.status(), 400);
        let error = response.text().unwrap();
        assert!(error.contains("currency needs review"), "{error}");
        assert!(error.contains("Settings"), "{error}");
        assert_eq!(server.get(&format!("{root}/checkpoint")), before);
        assert_eq!(server.get(&format!("{root}/transactions")), register);
        let response = server
            .client
            .patch(format!("{}{root}/preferences", server.url))
            .header("Idempotency-Key", "resolve-ambiguous-currency")
            .header(
                "X-Actual-Budget-Epoch",
                before["data"]["epoch"].as_str().unwrap(),
            )
            .json(&json!({"experimental":null}))
            .send()
            .unwrap();
        assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    }
    let checkpoint = server.get(&format!("{root}/checkpoint"));
    let body =
        json!({"action":"overwrite","month":"2026-11","snapshot":checkpoint["data"]["snapshot"]});
    let response = server
        .client
        .post(format!("{}{root}/budget/templates/commands", server.url))
        .header("Idempotency-Key", "apply-current-currency")
        .header(
            "X-Actual-Budget-Epoch",
            checkpoint["data"]["epoch"].as_str().unwrap(),
        )
        .json(&body)
        .send()
        .unwrap();
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let view = server.get(&format!("{root}/budget?month=2026-11"));
    let row = view["data"]["months"]["2026-11"]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == f["category_id"])
        .unwrap();
    assert_eq!(
        row["budgeted"], f["expected_template_allocation"],
        "{name} native={native}: newer preference must outrank retained source currency"
    );
    assert_eq!(
        row["goal"],
        if f["expected_template_allocation"] == 73 {
            "1000"
        } else {
            "100000"
        }
    );
    let mut after = server.get(&format!("{root}/transactions"));
    let mut before = register;
    after.as_object_mut().unwrap().remove("snapshot");
    before.as_object_mut().unwrap().remove("snapshot");
    assert_eq!(after, before);
}
#[test]
fn schema19_untouched_jpy_remains_calculation_currency() {
    currency_upgrade_case("untouched", false)
}
#[test]
fn schema19_later_usd_selection_overrides_original_jpy() {
    currency_upgrade_case("usd", false)
}
#[test]
fn schema19_later_currency_disable_overrides_original_jpy() {
    currency_upgrade_case("disabled", false)
}
#[test]
fn native19_untouched_jpy_remains_calculation_currency() {
    currency_upgrade_case("untouched", true)
}
#[test]
fn native19_later_usd_selection_overrides_original_jpy() {
    currency_upgrade_case("usd", true)
}
#[test]
fn native19_later_currency_disable_overrides_original_jpy() {
    currency_upgrade_case("disabled", true)
}

#[test]
fn schema19_unrelated_preference_keeps_original_jpy() {
    currency_upgrade_case("unrelated", false)
}

#[test]
fn native19_unrelated_preference_keeps_original_jpy() {
    currency_upgrade_case("unrelated", true)
}

#[test]
fn schema19_experimental_reset_overrides_original_jpy() {
    currency_upgrade_case("reset", false)
}

#[test]
fn native19_experimental_reset_overrides_original_jpy() {
    currency_upgrade_case("reset", true)
}

#[test]
fn schema19_reset_only_overrides_original_jpy() {
    currency_upgrade_case("reset_only", false)
}

#[test]
fn native19_reset_only_overrides_original_jpy() {
    currency_upgrade_case("reset_only", true)
}

#[test]
fn schema19_ambiguous_currency_requires_explicit_review() {
    currency_upgrade_case("ambiguous", false)
}

#[test]
fn native19_ambiguous_currency_requires_explicit_review() {
    currency_upgrade_case("ambiguous", true)
}

#[test]
fn current_experimental_reset_replaces_raw_imported_calculation_currency() {
    let f: Value =
        serde_json::from_slice(&fixture("budget-currency-schema19-untouched.json")).unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::write(
        data.path().join("actualbudget.sqlite3"),
        fixture("budget-currency-schema19-untouched.sqlite3"),
    )
    .unwrap();
    let server = Server::start(data);
    let root = format!("/api/v1/budgets/{}", f["metadata"]["id"].as_str().unwrap());
    let response = server
        .client
        .patch(format!("{}{root}/preferences", server.url))
        .header("Idempotency-Key", "current-explicit-reset")
        .json(&json!({"experimental":null}))
        .send()
        .unwrap();
    assert_eq!(response.status(), 200);
    let checkpoint = server.get(&format!("{root}/checkpoint"));
    let before = server.get(&format!("{root}/transactions"));
    let response = server
        .client
        .post(format!("{}{root}/budget/templates/preview", server.url))
        .json(&json!({"month":"2026-11","snapshot":checkpoint["data"]["snapshot"],"force":true}))
        .send()
        .unwrap();
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let preview: Value = response.json().unwrap();
    assert_eq!(preview["data"]["errors"], json!([]));
    let row = preview["data"]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == f["category_id"])
        .unwrap();
    assert_eq!(row["budgeted"], 7299);
    assert_eq!(row["goal"], 100000);
    assert_eq!(server.get(&format!("{root}/transactions")), before);
    assert_eq!(server.get(&format!("{root}/checkpoint")), checkpoint);
}
