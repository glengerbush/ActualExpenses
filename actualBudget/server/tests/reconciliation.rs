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
    data: TempDir,
    url: String,
    client: Client,
}
impl Server {
    fn start() -> Self {
        Self::open(tempfile::tempdir().unwrap())
    }
    fn open(data: TempDir) -> Self {
        let mut process = Command::new(env!("CARGO_BIN_EXE_actualbudget-server"))
            .env("ACTUALBUDGET_DATA_DIR", data.path())
            .env("ACTUALBUDGET_PORT", "0")
            .env("ACTUALBUDGET_HOST", "127.0.0.1")
            .env_remove("ACTUALBUDGET_PUBLIC_ORIGIN")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(process.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert!(line.starts_with("LISTENING "));
        Self {
            process,
            data,
            url: format!("http://{}", line.trim().trim_start_matches("LISTENING ")),
            client: Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
        }
    }
    fn get(&self, path: &str) -> Value {
        let r = self
            .client
            .get(format!("{}{path}", self.url))
            .send()
            .unwrap();
        assert!(r.status().is_success(), "{}", r.text().unwrap());
        r.json().unwrap()
    }
    fn write(
        &self,
        method: &str,
        path: &str,
        key: &str,
        clock: Option<&str>,
        body: Value,
    ) -> Response {
        let mut r = self
            .client
            .request(method.parse().unwrap(), format!("{}{path}", self.url))
            .header("Idempotency-Key", key);
        if let Some(clock) = clock {
            r = r.header("X-Actual-Edit-Clock", clock);
        }
        if !body.is_null() {
            r = r.json(&body);
        }
        r.send().unwrap()
    }
    fn restart(mut self) -> Self {
        self.process.kill().unwrap();
        self.process.wait().unwrap();
        let data = std::mem::replace(&mut self.data, tempfile::tempdir().unwrap());
        Self::open(data)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

const B: &str = "90000000-0000-4000-8000-000000000001";
fn id(n: usize) -> String {
    format!("90000000-0000-4000-8000-{n:012}")
}
fn ok(r: Response) -> Value {
    let status = r.status();
    let v: Value = r.json().unwrap();
    assert!(status.is_success(), "{status}: {v}");
    v
}

fn setup() -> (Server, String, String) {
    let s = Server::start();
    let root = format!("/api/v1/budgets/{B}");
    let a = id(2);
    ok(s.write(
        "POST",
        "/api/v1/budgets",
        "budget",
        None,
        json!({"id":B,"name":"Reconcile"}),
    ));
    ok(s.write("POST",&format!("{root}/accounts"),"account",None,json!({"id":a,"name":"Debt","off_budget":true,"opening_balance":-10000,"opening_date":"2026-01-01"})));
    (s, root, a)
}
fn oracle(s: &Server, root: &str, a: &str, step: &str) {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/reconciliation.json")).unwrap();
    let expected = fixture["snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["step"] == step)
        .unwrap();
    let page = s.get(&format!("{root}/transactions?account_id={a}"));
    let financial = |rows: &Value| {
        let mut rows = rows
            .as_array()
            .unwrap()
            .iter()
            .map(|r| json!({"amount":r["amount"],"date":r["date"]}))
            .collect::<Vec<_>>();
        rows.sort_by_key(|r| r["amount"].as_i64().unwrap());
        rows
    };
    assert_eq!(
        financial(&page["data"]),
        financial(&expected["transactions"])
    );
    assert_eq!(page["balances"]["total"], expected["total"]);
    assert_eq!(page["balances"]["cleared"], expected["total"]);
    assert_eq!(page["balances"]["uncleared"], 0);
}
// Display colors were introduced after the genuine source fixture.
fn strip_display(value: &mut Value) {
    match value {
        Value::Object(row) => {
            for key in [
                "category_color",
                "category_group_color",
                "color_override",
                "group_sort_order",
            ] {
                row.remove(key);
            }
            for child in row.values_mut() {
                strip_display(child);
            }
        }
        Value::Array(rows) => {
            for row in rows {
                strip_display(row);
            }
        }
        _ => {}
    }
}
// Compare the complete historical row, changing only explicitly retired fields.
fn manual_rows(value: &mut Value) {
    match value {
        Value::Object(row) => {
            if row.get("amount").is_some_and(Value::is_number) && row.contains_key("date") {
                if row.contains_key("notes") {
                    row.insert("notes".into(), json!(""));
                }
                if row.contains_key("cleared") {
                    row.insert("cleared".into(), json!(true));
                }
                if row.contains_key("reconciled") {
                    row.insert("reconciled".into(), json!(false));
                }
            }
            if let Some(total) = row.get("total").cloned() {
                if row.contains_key("cleared") {
                    row.insert("cleared".into(), total);
                }
                if row.contains_key("uncleared") {
                    row.insert("uncleared".into(), json!(0));
                }
            }
            for child in row.values_mut() {
                manual_rows(child);
            }
        }
        Value::Array(rows) => {
            for row in rows {
                manual_rows(row);
            }
        }
        _ => {}
    }
}
#[test]
fn reconciliation_routes_are_retired_without_changing_transactions() {
    let (s, root, account) = setup();
    let before = s.get(&format!("{root}/transactions"));
    let row = before["data"][0]["id"].as_str().unwrap();
    for (method, path) in [
        ("GET", format!("{root}/accounts/{account}/reconciliation")),
        ("POST", format!("{root}/accounts/{account}/reconciliation")),
        ("POST", format!("{root}/transactions/{row}/unlock")),
    ] {
        assert_eq!(
            s.write(method, &path, "retired", None, json!({})).status(),
            404
        );
    }
    assert_eq!(s.get(&format!("{root}/transactions")), before);
}

#[test]
fn historical_import_and_portable_restore_preserve_financial_oracle() {
    let s = Server::start();
    let imported = ok(s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "import")
        .body(include_bytes!("../../tests/fixtures/reconciliation-import.zip").to_vec())
        .send()
        .unwrap());
    assert_eq!(imported["data"]["status"], "ready", "{imported}");
    let bid = imported["data"]["budget_id"].as_str().unwrap();
    let import_id = imported["data"]["id"].as_str().unwrap();
    ok(s.write(
        "POST",
        &format!("/api/v1/imports/{import_id}/activate"),
        "activate",
        None,
        json!({}),
    ));
    let source: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/reconciliation-import.json"
    ))
    .unwrap();
    let imported_root = format!("/api/v1/budgets/{bid}");
    let debt = source["debt"].as_str().unwrap();
    let account = s.get(&format!("{imported_root}/accounts/{debt}"))["data"].clone();
    assert_eq!(account["last_reconciled"], Value::Null);
    oracle(&s, &imported_root, debt, "debt-unlocked");
    let backup = ok(s.write(
        "POST",
        &format!("{imported_root}/backups"),
        "backup",
        None,
        json!({"label":"Locked history"}),
    ));
    let bytes = s
        .client
        .get(format!(
            "{}{imported_root}/backups/{}/download",
            s.url,
            backup["data"]["id"].as_str().unwrap()
        ))
        .send()
        .unwrap()
        .bytes()
        .unwrap();
    let target = Server::start();
    let staged = ok(target
        .client
        .post(format!("{}/api/v1/restores", target.url))
        .header("Idempotency-Key", "portable")
        .header("Content-Type", "application/zip")
        .body(bytes.to_vec())
        .send()
        .unwrap());
    ok(target.write(
        "POST",
        &format!(
            "{imported_root}/restores/{}/activate",
            staged["data"]["id"].as_str().unwrap()
        ),
        "restore",
        None,
        json!({"confirm_name":"Reconciliation oracle"}),
    ));
    let target = target.restart();
    assert_eq!(
        target.get(&format!("{imported_root}/accounts/{debt}"))["data"]["last_reconciled"],
        Value::Null
    );
    oracle(&target, &imported_root, debt, "debt-unlocked");
}
#[test]
fn genuine_schema11_upgrade_preserves_split_receipts_settings_and_retained_source() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/reconciliation-schema11.json"
    ))
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("actualbudget.sqlite3"),
        include_bytes!("../../tests/fixtures/reconciliation-schema11.sqlite3"),
    )
    .unwrap();
    let s = Server::open(directory);
    let root = format!("/api/v1/budgets/{}", fixture["budget"].as_str().unwrap());
    let account = fixture["account"].as_str().unwrap();
    let parent = fixture["parent"].as_str().unwrap();
    let split = format!("{root}/transactions/{parent}/splits");
    assert_eq!(
        s.get(&format!("{root}/accounts/{account}"))["data"]["last_reconciled"],
        Value::Null
    );
    assert_eq!(
        ok(s.write(
            "POST",
            &format!("{root}/accounts"),
            "account",
            None,
            fixture["account_body"].clone()
        )),
        fixture["account_receipt"]
    );
    assert_eq!(
        ok(s.write("POST", &split, "split", None, fixture["split_body"].clone())),
        fixture["split_receipt"]
    );
    assert_eq!(
        s.get(&format!("{root}/register-settings"))["data"]["settings"],
        fixture["settings"]
    );
    let active = format!(
        "/api/v1/budgets/{}",
        fixture["active_budget"].as_str().unwrap()
    );
    let mut current = s.get(&format!("{active}/transactions"));
    let mut original = fixture["active_register"].clone();
    let old = original["snapshot"].as_str().unwrap();
    let fresh = current["snapshot"].as_str().unwrap();
    assert_eq!(
        &old[..34],
        &fresh[..34],
        "Tracking budget retains checkpoint epoch"
    );
    assert!(
        u64::from_str_radix(&fresh[34..], 16).unwrap()
            > u64::from_str_radix(&old[34..], 16).unwrap()
    );
    let changes = s.get(&format!("{active}/changes?cursor={old}&limit=1000"));
    assert_eq!(changes["has_more"], false);
    assert_eq!(changes["next_cursor"], fresh);
    manual_rows(&mut original);
    strip_display(&mut original);
    strip_display(&mut current);
    current.as_object_mut().unwrap().remove("snapshot");
    original.as_object_mut().unwrap().remove("snapshot");
    assert_eq!(
        current, original,
        "Rules upgrade preserves every financial row, balance and ordering"
    );
    let upload = |key: &str| {
        ok(s.client
            .post(format!("{}/api/v1/imports/actual", s.url))
            .header("Idempotency-Key", key)
            .header("Content-Type", "application/zip")
            .body(include_bytes!("../../tests/fixtures/reconciliation-import.zip").to_vec())
            .send()
            .unwrap())
    };
    assert_eq!(upload("blocked-source"), fixture["blocked_receipt"]);
    let reviewed = upload("review-retained-date");
    assert_eq!(reviewed["data"]["status"], "ready");
    ok(s.write(
        "POST",
        &format!(
            "/api/v1/imports/{}/activate",
            reviewed["data"]["id"].as_str().unwrap()
        ),
        "activate-retained-date",
        None,
        json!({}),
    ));
    let source: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/reconciliation-import.json"
    ))
    .unwrap();
    assert_eq!(
        s.get(&format!(
            "/api/v1/budgets/{}/accounts/{}",
            reviewed["data"]["budget_id"].as_str().unwrap(),
            source["debt"].as_str().unwrap()
        ))["data"]["last_reconciled"],
        Value::Null
    );
    let target = Server::start();
    let staged = ok(target
        .client
        .post(format!("{}/api/v1/restores", target.url))
        .header("Idempotency-Key", "old-schema11-archive")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/reconciliation-schema11.zip").to_vec())
        .send()
        .unwrap());
    assert_eq!(staged["data"]["report"]["schema_version"], 36);
    ok(target.write(
        "POST",
        &format!(
            "{root}/restores/{}/activate",
            staged["data"]["id"].as_str().unwrap()
        ),
        "restore-schema11",
        None,
        json!({"confirm_name":"Schema11 reconciliation"}),
    ));
    let target = target.restart();
    assert_eq!(
        target.get(&format!("{root}/accounts/{account}"))["data"]["last_reconciled"],
        Value::Null
    );
    assert_eq!(
        target.get(&format!("{root}/register-settings"))["data"]["settings"],
        fixture["settings"]
    );
    let mut expected = fixture["split_receipt"]["data"].clone();
    manual_rows(&mut expected);
    let mut actual = target.get(&split)["data"].clone();
    strip_display(&mut expected);
    strip_display(&mut actual);
    assert_eq!(actual, expected);
    assert_eq!(
        ok(target.write("POST", &split, "split", None, fixture["split_body"].clone())),
        fixture["split_receipt"]
    );
}

#[test]
fn imported_large_splits_keep_bounded_structural_recovery_and_exact_totals() {
    let s = Server::start();
    let uploaded = ok(s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Idempotency-Key", "large-import")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/reconciliation-large.zip").to_vec())
        .send()
        .unwrap());
    assert_eq!(uploaded["data"]["status"], "ready");
    ok(s.write(
        "POST",
        &format!(
            "/api/v1/imports/{}/activate",
            uploaded["data"]["id"].as_str().unwrap()
        ),
        "activate-large",
        None,
        json!({}),
    ));
    let root = format!(
        "/api/v1/budgets/{}",
        uploaded["data"]["budget_id"].as_str().unwrap()
    );
    let metadata: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/reconciliation-large.json"
    ))
    .unwrap();
    for case in metadata["accounts"].as_array().unwrap() {
        let count = case["count"].as_i64().unwrap();
        let account = case["account"].as_str().unwrap();
        let parent = case["parent"].as_str().unwrap();
        let split = format!("{root}/transactions/{parent}/splits");
        let view = s.get(&split);
        assert_eq!(
            view["data"]["children"].as_array().unwrap().len(),
            count as usize
        );
        assert!(
            view["data"]["children"]
                .as_array()
                .unwrap()
                .iter()
                .all(|c| c["cleared"] == true && c["reconciled"] == false)
        );
        let action = if count == 101 { "unsplit" } else { "recombine" };
        let change = json!({"snapshot":view["snapshot"],"action":action});
        assert!(change.to_string().len() < 300);
        let key = format!("recover-{count}");
        let result = ok(s.write("POST", &split, &key, None, change.clone()));
        assert_eq!(ok(s.write("POST", &split, &key, None, change)), result);
        let page = s.get(&format!("{root}/transactions?account_id={account}"));
        assert_eq!(page["balances"]["total"], -count);
        assert_eq!(page["balances"]["cleared"], -count);
        assert_eq!(page["balances"]["uncleared"], 0);
    }
}
