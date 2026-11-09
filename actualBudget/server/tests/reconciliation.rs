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
#[test]
fn statement_adjust_lock_confirm_unlock_restart_and_retry() {
    let (s, root, a) = setup();
    let path = format!("{root}/accounts/{a}/reconciliation");
    let preview = s.get(&format!("{path}?statement_balance=-12500"))["data"].clone();
    assert_eq!(preview["difference"], -2500);
    assert_eq!(preview["balances"]["cleared"], -10000);
    let body = json!({"action":"adjust","statement_balance":-12500,"snapshot":preview["snapshot"],"date":"2026-02-28","transaction_id":id(3)});
    let adjusted = ok(s.write("POST", &path, "adjust", None, body.clone()));
    assert_eq!(adjusted["data"]["difference"], 0);
    let s = s.restart();
    assert_eq!(ok(s.write("POST", &path, "adjust", None, body)), adjusted);
    let body = json!({"action":"lock","statement_balance":-12500,"snapshot":adjusted["data"]["snapshot"],"date":"2026-02-28"});
    let locked = ok(s.write("POST", &path, "lock", None, body.clone()));
    assert_eq!(locked["data"]["locked_count"], 2);
    assert!(
        s.get(&format!("{root}/accounts/{a}"))["data"]["last_reconciled"]
            .as_str()
            .unwrap()
            .parse::<i64>()
            .unwrap()
            > 0
    );
    let row = format!("{root}/transactions/{}", id(3));
    assert_eq!(
        s.write(
            "PATCH",
            &row,
            "unconfirmed",
            None,
            json!({"notes":"Changed"})
        )
        .status(),
        409
    );
    let edited = ok(s.write(
        "PATCH",
        &row,
        "confirmed",
        None,
        json!({"notes":"Confirmed","confirm_reconciled":true,"reconciled_version":s.get(&row)["data"]["field_versions"]["reconciled"]}),
    ));
    assert_eq!(edited["data"]["reconciled"], true);
    assert_eq!(
        s.write("DELETE", &row, "delete-locked", None, Value::Null)
            .status(),
        409
    );
    let current = s.get(&row)["data"].clone();
    let unlock = ok(s.write(
        "POST",
        &format!("{row}/unlock"),
        "unlock",
        None,
        json!({"reconciled_version":current["field_versions"]["reconciled"]}),
    ));
    assert_eq!(unlock["data"]["reconciled"], false);
    assert_eq!(unlock["data"]["cleared"], true);
    let s = s.restart();
    assert_eq!(ok(s.write("POST", &path, "lock", None, body)), locked);
    assert_eq!(s.get(&row)["data"]["reconciled"], false);
    assert!(
        s.get(&format!("{root}/accounts/{a}"))["data"]["last_reconciled"]
            .as_str()
            .unwrap()
            .parse::<i64>()
            .unwrap()
            > 0
    );
}
#[test]
fn stale_preview_difference_and_invalid_requests_are_atomic() {
    let (s, root, a) = setup();
    let path = format!("{root}/accounts/{a}/reconciliation");
    let preview = s.get(&format!("{path}?statement_balance=0"))["data"].clone();
    let body = json!({"action":"lock","statement_balance":0,"snapshot":preview["snapshot"],"date":"2026-02-28"});
    assert_eq!(s.write("POST", &path, "mismatch", None, body).status(), 409);
    ok(s.write(
        "POST",
        &format!("{root}/transactions"),
        "new",
        None,
        json!({"id":id(3),"account_id":a,"amount":-300,"date":"2026-02-28","cleared":false}),
    ));
    let body = json!({"action":"adjust","statement_balance":0,"snapshot":preview["snapshot"],"date":"2026-02-28","transaction_id":id(4)});
    assert_eq!(s.write("POST", &path, "stale", None, body).status(), 409);
    let fresh = s.get(&format!("{path}?statement_balance=-10000"))["data"].clone();
    assert_eq!(
        fresh["balances"],
        json!({"total":-10300,"cleared":-10000,"uncleared":-300})
    );
    let body = json!({"action":"lock","statement_balance":-10000,"snapshot":fresh["snapshot"],"date":"2026-02-28"});
    let r = ok(s.write("POST", &path, "lock", None, body));
    assert_eq!(r["data"]["locked_count"], 1);
    assert_eq!(
        s.get(&format!("{root}/transactions/{}", id(3)))["data"]["reconciled"],
        false
    );
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
    let mut rows=page["data"].as_array().unwrap().iter().map(|r|json!({"amount":r["amount"],"date":r["date"],"cleared":r["cleared"],"reconciled":r["reconciled"],"notes":r["notes"]})).collect::<Vec<_>>();
    rows.sort_by_key(|r| r["amount"].as_i64().unwrap());
    assert_eq!(json!(rows), expected["transactions"], "{step}");
    for f in ["total", "cleared", "uncleared"] {
        assert_eq!(page["balances"][f], expected[f], "{step} {f}");
    }
}
#[test]
fn public_operations_match_independent_actual_and_portable_import_restores_dates() {
    let (s, root, a) = setup();
    // Oracle ordinary opening row uses blank notes instead of the opening-balance payee.
    ok(s.write(
        "POST",
        &format!("{root}/transactions"),
        "uncleared",
        None,
        json!({"id":id(3),"account_id":a,"amount":-300,"date":"2026-02-28","cleared":false}),
    ));
    oracle(&s, &root, &a, "debt-initial");
    let path = format!("{root}/accounts/{a}/reconciliation");
    let p = s.get(&format!("{path}?statement_balance=-12500"))["data"].clone();
    let p=ok(s.write("POST",&path,"adjust",None,json!({"action":"adjust","statement_balance":-12500,"snapshot":p["snapshot"],"date":"2026-02-28","transaction_id":id(4)})))["data"].clone();
    oracle(&s, &root, &a, "debt-adjusted");
    ok(s.write("POST",&path,"lock",None,json!({"action":"lock","statement_balance":-12500,"snapshot":p["snapshot"],"date":"2026-02-28"})));
    oracle(&s, &root, &a, "debt-locked");
    ok(s.write(
        "PATCH",
        &format!("{root}/transactions/{}", id(4)),
        "edit",
        None,
        json!({"notes":"Confirmed","confirm_reconciled":true,"reconciled_version":s.get(&format!("{root}/transactions/{}",id(4)))["data"]["field_versions"]["reconciled"]}),
    ));
    oracle(&s, &root, &a, "debt-confirmed-edit");
    let row = s.get(&format!("{root}/transactions/{}", id(4)))["data"].clone();
    ok(s.write(
        "POST",
        &format!("{root}/transactions/{}/unlock", id(4)),
        "unlock",
        None,
        json!({"reconciled_version":row["field_versions"]["reconciled"]}),
    ));
    oracle(&s, &root, &a, "debt-unlocked");
    let asset = id(5);
    ok(s.write("POST",&format!("{root}/accounts"),"asset",None,json!({"id":asset,"name":"House","off_budget":true,"opening_balance":23110000,"opening_date":"2026-01-01"})));
    let path = format!("{root}/accounts/{asset}/reconciliation");
    let p = s.get(&format!("{path}?statement_balance=23460000"))["data"].clone();
    let p=ok(s.write("POST",&path,"asset-adjust",None,json!({"action":"adjust","statement_balance":23460000,"snapshot":p["snapshot"],"date":"2026-02-28","transaction_id":id(6)})))["data"].clone();
    ok(s.write("POST",&path,"asset-lock",None,json!({"action":"lock","statement_balance":23460000,"snapshot":p["snapshot"],"date":"2026-02-28"})));
    oracle(&s, &root, &asset, "asset-revalued");
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
    assert_eq!(account["last_reconciled"], source["last_reconciled"]);
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
        source["last_reconciled"]
    );
    oracle(&target, &imported_root, debt, "debt-unlocked");
}
#[test]
fn cancellation_epoch_clocks_and_money_are_guarded() {
    let (s, root, a) = setup();
    let path = format!("{root}/accounts/{a}/reconciliation");
    let p = s.get(&format!("{path}?statement_balance=-10000"))["data"].clone();
    let body = json!({"action":"lock","statement_balance":-10000,"snapshot":p["snapshot"],"date":"2026-02-28"});
    let cancel = s
        .client
        .post(format!("{}{path}", s.url))
        .header("Idempotency-Key", "cancel")
        .header("X-Actual-Cancel-If-Uncommitted", "true")
        .json(&body)
        .send()
        .unwrap();
    assert_eq!(cancel.status(), 409);
    assert_eq!(
        cancel.json::<Value>().unwrap()["error"]["code"],
        "operation_canceled"
    );
    assert_eq!(
        s.write("POST", &path, "cancel", None, body.clone())
            .json::<Value>()
            .unwrap()["error"]["code"],
        "operation_canceled"
    );
    let future = "2099-01-01T00:00:00.000Z-0000-0123456789abcdef";
    assert_eq!(
        s.write("POST", &path, "future", Some(future), body.clone())
            .json::<Value>()
            .unwrap()["error"]["code"],
        "clock_skew"
    );
    let old = "1970-01-01T00:00:00.000Z-0001-0123456789abcdef";
    assert_eq!(
        s.write("POST", &path, "older", Some(old), body.clone())
            .json::<Value>()
            .unwrap()["error"]["code"],
        "reconciliation_superseded"
    );
    let epoch = s
        .client
        .post(format!("{}{path}", s.url))
        .header("Idempotency-Key", "epoch")
        .header("X-Actual-Budget-Epoch", "00000000000000000000000000000000")
        .json(&body)
        .send()
        .unwrap();
    assert_eq!(epoch.status(), 409);
    let too_big = s
        .client
        .get(format!(
            "{}{path}?statement_balance=9007199254740991",
            s.url
        ))
        .send()
        .unwrap();
    assert_eq!(too_big.status(), 400);
    let success = ok(s.write("POST", &path, "success", None, body.clone()));
    let canceled_after = s
        .client
        .post(format!("{}{path}", s.url))
        .header("Idempotency-Key", "success")
        .header("X-Actual-Cancel-If-Uncommitted", "true")
        .json(&body)
        .send()
        .unwrap();
    assert_eq!(ok(canceled_after), success);
    let changed =
        json!({"action":"lock","statement_balance":0,"snapshot":p["snapshot"],"date":"2026-02-28"});
    assert_eq!(
        s.write("POST", &path, "success", None, changed).status(),
        409
    );
}

#[test]
fn confirmation_cannot_cross_a_new_lock_but_committed_receipts_still_replay() {
    let (s, root, account) = setup();
    let path = format!("{root}/accounts/{account}/reconciliation");
    let preview = s.get(&path)["data"].clone();
    let base = chrono::DateTime::parse_from_rfc3339(&preview["edit_clock"].as_str().unwrap()[..24])
        .unwrap()
        .timestamp_millis();
    let clock = |offset: i64| {
        format!(
            "{}-0000-0123456789abcdef",
            chrono::DateTime::<chrono::Utc>::from_timestamp_millis(base + offset)
                .unwrap()
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        )
    };
    let lock1 = clock(1000);
    let intent2 = clock(2000);
    let unlock3 = clock(3000);
    let lock4 = clock(4000);
    let intent5 = clock(5000);
    let lock = |preview: &Value| json!({"action":"lock","statement_balance":-10000,"snapshot":preview["snapshot"],"date":"2026-02-28"});
    ok(s.write("POST", &path, "first-lock", Some(&lock1), lock(&preview)));
    let entry = s.get(&format!("{root}/transactions?account_id={account}"))["data"][0].clone();
    let row_path = format!("{root}/transactions/{}", entry["id"].as_str().unwrap());
    let version1 = entry["field_versions"]["reconciled"].clone();
    let committed_body = json!({"notes":"Confirmed before relock","confirm_reconciled":true,"reconciled_version":version1});
    let committed = ok(s.write(
        "PATCH",
        &row_path,
        "committed",
        Some(&intent2),
        committed_body.clone(),
    ));
    let delayed = json!({"amount":-12000,"confirm_reconciled":true,"reconciled_version":version1});
    ok(s.write(
        "POST",
        &format!("{row_path}/unlock"),
        "unlock",
        Some(&unlock3),
        json!({"reconciled_version":version1}),
    ));
    ok(s.write(
        "POST",
        &path,
        "second-lock",
        Some(&lock4),
        lock(&s.get(&path)["data"]),
    ));
    let s = s.restart();
    let before = s.get(&row_path);
    for (key, order, body) in [
        ("delayed", intent2.as_str(), delayed.clone()),
        (
            "stale-version-newer-clock",
            intent5.as_str(),
            delayed.clone(),
        ),
        (
            "missing-version",
            intent5.as_str(),
            json!({"amount":-12000,"confirm_reconciled":true}),
        ),
        (
            "old-clock-current-version",
            intent2.as_str(),
            json!({"amount":-12000,"confirm_reconciled":true,"reconciled_version":lock4}),
        ),
    ] {
        let r = s.write("PATCH", &row_path, key, Some(order), body);
        assert_eq!(r.status(), 409, "{key}: {}", r.text().unwrap());
        assert_eq!(s.get(&row_path), before, "{key} changed settled data");
    }
    assert_eq!(
        ok(s.write(
            "PATCH",
            &row_path,
            "committed",
            Some(&intent2),
            committed_body
        )),
        committed
    );
    assert_eq!(
        s.get(&row_path),
        before,
        "Replay must not reapply an old edit"
    );
    let fresh = ok(s.write(
        "PATCH",
        &row_path,
        "fresh-confirmation",
        Some(&intent5),
        json!({"amount":-12000,"confirm_reconciled":true,"reconciled_version":lock4}),
    ));
    assert_eq!(fresh["data"]["amount"], -12000);
    assert_eq!(fresh["data"]["reconciled"], true);
}

fn split_proofs(row: &Value) -> Value {
    std::iter::once(row)
        .chain(row["children"].as_array().unwrap().iter())
        .map(|r| {
            (
                r["id"].as_str().unwrap().to_owned(),
                r["field_versions"]["reconciled"].clone(),
            )
        })
        .collect::<serde_json::Map<_, _>>()
        .into()
}
fn split_oracle(s: &Server, root: &str, account: &str, parent: &str, step: &str) {
    let row = s.get(&format!("{root}/transactions/{parent}"))["data"].clone();
    let mut entries:Vec<Value>=std::iter::once(&row).chain(row["children"].as_array().unwrap().iter()).map(|r|json!({"amount":r["amount"],"notes":r["notes"],"cleared":r["cleared"],"reconciled":r["reconciled"],"parent":r["is_parent"]==true,"child":!r["parent_id"].is_null()})).collect();
    entries.sort_by_key(|r| r["amount"].as_i64().unwrap());
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/reconciliation-splits.json"
    ))
    .unwrap();
    let expected = fixture["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["step"] == step)
        .unwrap();
    assert_eq!(json!(entries), expected["entries"], "{step}");
    let preview = s.get(&format!("{root}/accounts/{account}/reconciliation"));
    assert_eq!(preview["data"]["balances"]["cleared"], expected["cleared"]);
}
#[test]
fn split_reconciliation_proofs_filters_restart_and_portable_archive() {
    let (s, root, _) = setup();
    let a = id(60);
    let parent = id(61);
    let c1 = id(62);
    let c2 = id(63);
    ok(s.write(
        "POST",
        &format!("{root}/accounts"),
        "split-account",
        None,
        json!({"id":a,"name":"Split account","opening_balance":0,"opening_date":"2026-01-01"}),
    ));
    ok(s.write("POST",&format!("{root}/transactions"),"split-parent",None,json!({"id":parent,"account_id":a,"amount":-300,"date":"2026-02-28","cleared":true,"notes":"Settled split"})));
    let split = format!("{root}/transactions/{parent}/splits");
    let mut children = json!([{"id":c1,"amount":-100,"notes":"First allocation"},{"id":c2,"amount":-200,"notes":"Second allocation"}]);
    let current = s.get(&split);
    ok(s.write(
        "POST",
        &split,
        "split",
        None,
        json!({"snapshot":current["snapshot"],"children":children}),
    ));
    split_oracle(&s, &root, &a, &parent, "initial");
    let path = format!("{root}/accounts/{a}/reconciliation");
    let p = s.get(&path)["data"].clone();
    assert_eq!(p["ready_count"], 1);
    assert_eq!(p["locked_count"], 0);
    let lock = json!({"action":"lock","statement_balance":-300,"snapshot":p["snapshot"],"date":"2026-02-28"});
    let locked = ok(s.write("POST", &path, "split-lock", None, lock));
    assert_eq!(locked["data"]["locked_count"], 1);
    split_oracle(&s, &root, &a, &parent, "locked");
    let current = s.get(&split);
    let row = &current["data"];
    let proofs = split_proofs(row);
    for child in row["children"].as_array().unwrap() {
        assert_eq!(
            child["field_versions"]["reconciled"],
            row["field_versions"]["reconciled"]
        );
    }
    children[0]["notes"] = json!("Confirmed allocation");
    let mut change = json!({"snapshot":current["snapshot"],"children":children,"confirm_reconciled":true,"reconciled_versions":proofs});
    let mut missing = change.clone();
    missing["reconciled_versions"]
        .as_object_mut()
        .unwrap()
        .remove(&c2);
    assert_eq!(
        s.write("POST", &split, "missing-child-proof", None, missing)
            .status(),
        409
    );
    let mut stale = change.clone();
    stale["reconciled_versions"][&c2] = json!("0000000000000-000000-0000000000000000");
    assert_eq!(
        s.write("POST", &split, "wrong-child-proof", None, stale)
            .status(),
        409
    );
    assert_eq!(s.get(&split), current);
    assert_eq!(
        s.write(
            "PATCH",
            &format!("{root}/transactions/{c1}"),
            "direct-child",
            None,
            json!({"notes":"Unsafe","confirm_reconciled":true,"reconciled_version":proofs[&c1]})
        )
        .status(),
        409
    );
    assert_eq!(
        s.write(
            "DELETE",
            &format!("{root}/transactions/{parent}"),
            "delete-settled",
            None,
            Value::Null
        )
        .status(),
        409
    );
    let confirmed_input = change.clone();
    let confirmed = ok(s.write(
        "POST",
        &split,
        "confirmed-split",
        None,
        confirmed_input.clone(),
    ));
    split_oracle(&s, &root, &a, &parent, "confirmed");
    let current = s.get(&split);
    let new_proofs = split_proofs(&current["data"]);
    let unlock = format!("{root}/transactions/{c1}/unlock");
    assert_eq!(
        s.write(
            "POST",
            &unlock,
            "single-proof-child",
            None,
            json!({"reconciled_version":new_proofs[&c1]})
        )
        .status(),
        400
    );
    ok(s.write(
        "POST",
        &unlock,
        "unlock-aggregate",
        None,
        json!({"reconciled_versions":new_proofs}),
    ));
    split_oracle(&s, &root, &a, &parent, "unlocked");
    let p = s.get(&path)["data"].clone();
    ok(s.write("POST",&path,"relock-aggregate",None,json!({"action":"lock","statement_balance":-300,"snapshot":p["snapshot"],"date":"2026-02-28"})));
    let p = s.get(&split);
    change["snapshot"] = p["snapshot"].clone();
    assert_eq!(
        s.write("POST", &split, "old-proof-new-snapshot", None, change)
            .status(),
        409
    );
    let s = s.restart();
    // A committed split receipt must replay despite current protection/snapshot changes.
    assert_eq!(
        ok(s.write("POST", &split, "confirmed-split", None, confirmed_input)),
        confirmed
    );
    for flag in ["hide_cleared", "hide_reconciled"] {
        let query = json!({flag:true});
        let response = s
            .client
            .get(format!("{}{root}/transactions", s.url))
            .query(&[("account_id", a.as_str()), ("query", &query.to_string())])
            .send()
            .unwrap();
        let filtered = ok(response);
        assert_eq!(filtered["data"], json!([]));
        assert_eq!(filtered["balances"]["total"], -300);
    }
    let settings = format!("{root}/register-settings");
    let state = s.get(&settings)["data"].clone();
    let settings_body = json!({"revision":state["revision"],"settings":{"views":{"all":{"query":{"hide_reconciled":true}}},"saved_filters":[]}});
    ok(s.client
        .patch(format!("{}{}", s.url, settings))
        .header("Idempotency-Key", "settings")
        .header("X-Actual-Budget-Epoch", state["epoch"].as_str().unwrap())
        .json(&settings_body)
        .send()
        .unwrap());
    let backup = ok(s.write(
        "POST",
        &format!("{root}/backups"),
        "split-backup",
        None,
        json!({"label":"Reconciled split"}),
    ));
    let bytes = s
        .client
        .get(format!(
            "{}{root}/backups/{}/download",
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
        .header("Idempotency-Key", "split-portable")
        .header("Content-Type", "application/zip")
        .body(bytes.to_vec())
        .send()
        .unwrap());
    assert_eq!(staged["data"]["report"]["schema_version"], 12);
    ok(target.write(
        "POST",
        &format!(
            "{root}/restores/{}/activate",
            staged["data"]["id"].as_str().unwrap()
        ),
        "split-restore",
        None,
        json!({"confirm_name":"Reconcile"}),
    ));
    let target = target.restart();
    assert_eq!(
        target.get(&format!("{root}/transactions/{parent}"))["data"],
        s.get(&format!("{root}/transactions/{parent}"))["data"]
    );
    assert_eq!(
        target.get(&settings)["data"]["settings"],
        s.get(&settings)["data"]["settings"]
    );
    assert_eq!(
        target.get(&format!("{root}/accounts/{a}"))["data"]["last_reconciled"],
        s.get(&format!("{root}/accounts/{a}"))["data"]["last_reconciled"]
    );
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
    assert_eq!(
        s.get(&format!(
            "/api/v1/budgets/{}/transactions",
            fixture["active_budget"].as_str().unwrap()
        )),
        fixture["active_register"]
    );
    let path = format!("{root}/accounts/{account}/reconciliation");
    let progress = s.get(&path)["data"].clone();
    assert_eq!(progress["ready_count"], 1);
    assert_eq!(progress["balances"]["cleared"], -300);
    ok(s.write("POST",&path,"upgrade-lock",None,json!({"action":"lock","statement_balance":-300,"snapshot":progress["snapshot"],"date":"2026-02-28"})));
    let locked = s.get(&split)["data"].clone();
    assert!(
        locked["children"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["reconciled"] == true)
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
        source["last_reconciled"]
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
    assert_eq!(staged["data"]["report"]["schema_version"], 12);
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
    assert_eq!(target.get(&split)["data"], fixture["split_receipt"]["data"]);
    assert_eq!(
        ok(target.write("POST", &split, "split", None, fixture["split_body"].clone())),
        fixture["split_receipt"]
    );
}
