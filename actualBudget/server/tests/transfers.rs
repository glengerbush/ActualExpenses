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

const B: &str = "15000000-0000-4000-8000-000000000001";
fn id(n: usize) -> String {
    format!("15000000-0000-4000-8000-{n:012}")
}
fn ok(r: Response) -> Value {
    let status = r.status();
    let v: Value = r.json().unwrap();
    assert!(status.is_success(), "{status}: {v}");
    v
}
fn manual_field(expected: &Value, field: &str) -> Value {
    match field {
        "notes" => json!(""),
        "cleared" => json!(true),
        "reconciled" => json!(false),
        _ => expected[field].clone(),
    }
}
fn current_row(mut row: Value) -> Value {
    if let Some(obj) = row.as_object_mut() {
        for field in ["notes", "cleared", "reconciled"] {
            if obj.contains_key(field) {
                obj.insert(
                    field.into(),
                    match field {
                        "notes" => json!(""),
                        "cleared" => json!(true),
                        _ => json!(false),
                    },
                );
            }
        }
        if obj.contains_key("last_reconciled") {
            obj.insert("last_reconciled".into(), Value::Null);
        }
        if let Some(children) = obj.get_mut("children").and_then(Value::as_array_mut) {
            for child in children {
                *child = current_row(child.clone());
            }
        }
    }
    row
}
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
#[test]
fn paired_transfer_matches_pinned_actual_and_companion_excludes_both_legs() {
    let s = Server::start();
    let root = format!("/api/v1/budgets/{B}");
    ok(s.write(
        "POST",
        "/api/v1/budgets",
        "budget",
        None,
        json!({"id":B,"name":"Transfers"}),
    ));
    for (n, name, off) in [
        (2, "Cash", false),
        (3, "Credit card", false),
        (4, "Off budget", true),
    ] {
        ok(s.write("POST",&format!("{root}/accounts"),&format!("account-{n}"),None,json!({"id":id(n),"name":name,"off_budget":off,"opening_balance":0,"opening_date":"2026-01-01"})));
    }
    let request = json!({"id":id(11),"counterpart_id":id(12),"account_id":id(2),"to_account_id":id(3),"amount":-2500,"date":"2026-01-01","cleared":true,"notes":"Card payment"});
    let created = ok(s.write(
        "POST",
        &format!("{root}/transfers"),
        "payment",
        None,
        request.clone(),
    ));
    assert_eq!(
        created,
        ok(s.write(
            "POST",
            &format!("{root}/transfers"),
            "payment",
            None,
            request
        ))
    );
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/transfers.json")).unwrap();
    for (index, n) in [(0, 11), (1, 12)] {
        let row = s.get(&format!("{root}/transactions/{}", id(n)))["data"].clone();
        let expected = &fixture["snapshots"][0]["transactions"][index];
        for field in ["amount", "date", "cleared", "category_id", "notes"] {
            assert_eq!(row[field], manual_field(expected, field), "{field}");
        }
        assert_eq!(row["transfer_id"], id(if n == 11 { 12 } else { 11 }));
    }
    let companion = s.get(&format!(
        "{root}/companion?account_id={}&retain={}",
        id(2),
        id(11)
    ));
    assert_eq!(companion["data"]["recent"], json!([]));
    assert_eq!(companion["data"]["unavailable"], json!([id(11)]));
    ok(s.write(
        "PATCH",
        &format!("{root}/transactions/{}", id(11)),
        "edit",
        None,
        json!({"amount":-3100,"notes":"Corrected payment","date":"2026-01-02"}),
    ));
    let other = s.get(&format!("{root}/transactions/{}", id(12)));
    assert_eq!(other["data"]["amount"], 3100);
    assert_eq!(other["data"]["notes"], "");
    assert_eq!(other["data"]["date"], "2026-01-01");
    ok(s.write(
        "DELETE",
        &format!("{root}/transactions/{}", id(12)),
        "delete",
        None,
        Value::Null,
    ));
    assert_eq!(s.get(&format!("{root}/transactions"))["data"], json!([]));
    let s = s.restart();
    assert_eq!(
        s.get(&format!("{root}/transactions"))["balances"]["total"],
        0
    );
}

#[test]
fn genuine_transfer_archive_and_native_round_trip_keep_pairs() {
    let s = Server::start();
    let staged = ok(s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Idempotency-Key", "import")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/transfers-26.10.0.zip").as_slice())
        .send()
        .unwrap());
    assert_eq!(staged["data"]["status"], "ready", "{staged}");
    let import = staged["data"]["id"].as_str().unwrap();
    let b = staged["data"]["budget_id"].as_str().unwrap();
    let root = format!("/api/v1/budgets/{b}");
    ok(s.write(
        "POST",
        &format!("/api/v1/imports/{import}/activate"),
        "activate",
        None,
        json!({}),
    ));
    let rows = s.get(&format!("{root}/transactions"));
    assert_eq!(rows["data"].as_array().unwrap().len(), 2);
    assert_eq!(rows["balances"]["total"], 0);
    for row in rows["data"].as_array().unwrap() {
        let other = s.get(&format!(
            "{root}/transactions/{}",
            row["transfer_id"].as_str().unwrap()
        ));
        assert_eq!(
            row["amount"].as_i64().unwrap(),
            -other["data"]["amount"].as_i64().unwrap()
        );
    }
    assert_eq!(
        s.get(&format!("{root}/preferences"))["data"]["sync_transfer_date"],
        true
    );
    let accounts = s.get(&format!("{root}/accounts"));
    let cash = accounts["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "Cash")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let card = accounts["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "Credit card")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    ok(s.write("POST",&format!("{root}/accounts/{cash}/close"),"close-imported",None,json!({"id":id(31),"counterpart_id":id(32),"to_account_id":card,"date":"2026-03-01","expected_balance":-700})));
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/transfers.json")).unwrap();
    for (alias, account, tx) in [("cash", cash, id(31)), ("card", card, id(32))] {
        let expected = fixture["closing"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["account_id"] == alias)
            .unwrap();
        let actual = s.get(&format!("{root}/transactions/{tx}"));
        for field in ["amount", "category_id", "notes"] {
            assert_eq!(actual["data"][field], manual_field(expected, field));
        }
        let expected = fixture["closing"]["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == alias)
            .unwrap();
        assert_eq!(
            s.get(&format!("{root}/transactions?account_id={account}"))["balances"]["total"],
            expected["balance"]
        );
    }
    let rows = s.get(&format!("{root}/transactions"));
    let backup = ok(s.write(
        "POST",
        &format!("{root}/backups"),
        "backup",
        None,
        json!({"label":"Transfer state"}),
    ));
    let archive = s
        .client
        .get(format!(
            "{}{root}/backups/{}/download",
            s.url,
            backup["data"]["id"].as_str().unwrap()
        ))
        .send()
        .unwrap();
    assert!(archive.status().is_success());
    let archive = archive.bytes().unwrap();
    let stage = ok(s
        .client
        .post(format!("{}/api/v1/restores", s.url))
        .header("Idempotency-Key", "restore-upload")
        .header("Content-Type", "application/zip")
        .body(archive.to_vec())
        .send()
        .unwrap());
    let rid = stage["data"]["id"].as_str().unwrap();
    let name = s.get(&root)["data"]["name"].clone();
    ok(s.write(
        "POST",
        &format!("{root}/restores/{rid}/activate"),
        "restore",
        None,
        json!({"confirm_name":name}),
    ));
    assert_eq!(s.get(&format!("{root}/transactions"))["data"], rows["data"]);
}

fn seed(s: &Server) -> String {
    let root = format!("/api/v1/budgets/{B}");
    ok(s.write(
        "POST",
        "/api/v1/budgets",
        "budget",
        None,
        json!({"id":B,"name":"Transfers"}),
    ));
    for (n, off) in [(2, false), (3, false), (4, true)] {
        ok(s.write("POST",&format!("{root}/accounts"),&format!("account{n}"),None,json!({"id":id(n),"name":format!("Account {n}"),"off_budget":off,"opening_balance":0,"opening_date":"2026-01-01"})));
    }
    root
}
fn clock(year: i32) -> String {
    format!("{year}-01-01T00:00:00.000Z-0000-0000000000000015")
}
fn create_body() -> Value {
    json!({"id":id(11),"counterpart_id":id(12),"account_id":id(2),"to_account_id":id(3),"amount":-2500,"date":"2026-01-01","notes":"Original"})
}
#[test]
fn transfer_field_ordering_deletion_fence_and_failures_never_publish_half_pair() {
    let s = Server::start();
    let root = seed(&s);
    let path = format!("{root}/transfers");
    ok(s.write("POST", &path, "pair", Some(&clock(2020)), create_body()));
    let before = s.get(&format!("{root}/transactions"));
    let invalid = s.write(
        "PATCH",
        &format!("{root}/transactions/{}", id(11)),
        "bad",
        Some(&clock(2021)),
        json!({"amount":-4000,"date":"not-a-date"}),
    );
    assert_eq!(invalid.status(), 400);
    assert_eq!(
        s.get(&format!("{root}/transactions")),
        before,
        "failed edits cannot move balances/cursor"
    );
    ok(s.write(
        "PATCH",
        &format!("{root}/transactions/{}", id(12)),
        "newer",
        Some(&clock(2023)),
        json!({"amount":4200,"notes":"Newer"}),
    ));
    let stale = ok(s.write(
        "PATCH",
        &format!("{root}/transactions/{}", id(11)),
        "older",
        Some(&clock(2022)),
        json!({"amount":-3000,"date":"2026-02-01"}),
    ));
    assert_eq!(stale["conflicts"][0]["field"], "amount");
    assert_eq!(stale["data"]["amount"], -4200);
    assert_eq!(stale["data"]["date"], "2026-02-01");
    assert_eq!(
        s.get(&format!("{root}/transactions/{}", id(12)))["data"]["date"],
        "2026-01-01"
    );
    let delete_path = format!("{root}/transactions/{}", id(11));
    let refused = s.write(
        "DELETE",
        &delete_path,
        "old-delete",
        Some(&clock(2022)),
        Value::Null,
    );
    assert_eq!(refused.status(), 409);
    let refused: Value = refused.json().unwrap();
    assert_eq!(refused["error"]["code"], "deletion_superseded");
    ok(s.write(
        "PATCH",
        &format!("{root}/transactions/{}", id(11)),
        "date",
        Some(&clock(2024)),
        json!({"date":"2026-03-01","sync_transfer_date":true}),
    ));
    assert_eq!(
        s.get(&format!("{root}/transactions/{}", id(12)))["data"]["date"],
        "2026-03-01"
    );
    assert_eq!(
        s.get(&format!("{root}/preferences"))["data"]["sync_transfer_date"],
        true
    );
    let deleted = ok(s.write(
        "DELETE",
        &delete_path,
        "delete",
        Some(&clock(2025)),
        Value::Null,
    ));
    assert_eq!(deleted["data"]["deleted"], true);
    assert_eq!(s.get(&format!("{root}/transactions"))["data"], json!([]));
    let s = s.restart();
    assert_eq!(
        s.write(
            "DELETE",
            &delete_path,
            "old-delete",
            Some(&clock(2022)),
            Value::Null
        )
        .json::<Value>()
        .unwrap(),
        refused
    );
    assert_eq!(
        ok(s.write(
            "DELETE",
            &delete_path,
            "delete",
            Some(&clock(2025)),
            Value::Null
        )),
        deleted
    );
}
#[test]
fn canceled_transfer_fences_late_arrival_and_incremental_payees_are_transfer_identities() {
    let s = Server::start();
    let root = seed(&s);
    let snapshot = s.get(&format!("{root}/transactions"))["snapshot"]
        .as_str()
        .unwrap()
        .to_owned();
    let path = format!("{root}/transfers");
    let body = create_body();
    let canceled = s
        .client
        .post(format!("{}{path}", s.url))
        .header("Idempotency-Key", "cancel")
        .header("X-Actual-Cancel-If-Uncommitted", "true")
        .json(&body)
        .send()
        .unwrap();
    assert_eq!(canceled.status(), 409);
    let canceled: Value = canceled.json().unwrap();
    assert_eq!(canceled["error"]["code"], "operation_canceled");
    assert_eq!(
        s.write("POST", &path, "cancel", None, body.clone())
            .json::<Value>()
            .unwrap(),
        canceled
    );
    assert_eq!(s.get(&format!("{root}/transactions"))["data"], json!([]));
    ok(s.write("POST", &path, "created", None, body));
    let changes = s.get(&format!("{root}/changes?cursor={snapshot}"));
    let payees = changes["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["entity"] == "payees")
        .collect::<Vec<_>>();
    assert_eq!(payees.len(), 2, "{changes}");
    for payee in payees {
        assert!(!payee["data"]["transfer_account_id"].is_null(), "{payee}");
    }
    assert_eq!(s.get(&format!("{root}/payees"))["data"], json!([]));
}
#[test]
fn link_duplicate_close_and_account_delete_preserve_both_registers() {
    let s = Server::start();
    let root = seed(&s);
    for (n, acct, amount, date) in [(11, 2, -2500, "2026-01-01"), (12, 3, 2500, "2026-01-04")] {
        ok(s.write(
            "POST",
            &format!("{root}/transactions"),
            &format!("t{n}"),
            None,
            json!({"id":id(n),"account_id":id(acct),"amount":amount,"date":date}),
        ));
    }
    let linked = ok(s.write(
        "POST",
        &format!("{root}/transfers/link"),
        "link",
        None,
        json!({"id":id(11),"counterpart_id":id(12)}),
    ));
    assert_eq!(linked["data"]["transfer_id"], id(12));
    assert_eq!(
        s.get(&format!("{root}/transactions/{}", id(12)))["data"]["date"],
        "2026-01-04"
    );
    let duplicate = ok(s.write(
        "POST",
        &format!("{root}/transactions/{}/duplicate", id(11)),
        "duplicate",
        None,
        json!({"id":id(13)}),
    ));
    let peer = duplicate["data"]["transfer_id"].as_str().unwrap();
    assert_ne!(peer, id(12));
    assert_eq!(
        s.get(&format!("{root}/transactions/{peer}"))["data"]["date"],
        "2026-01-04"
    );
    let close_body = json!({"id":id(15),"counterpart_id":id(16),"to_account_id":id(3),"date":"2026-02-01","expected_balance":-5000});
    let close_path = format!("{root}/accounts/{}/close", id(2));
    ok(s.write("POST", &close_path, "close", None, close_body));
    assert_eq!(
        s.get(&format!("{root}/accounts"))["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id(2))
            .unwrap()["closed"],
        true
    );
    assert_eq!(
        s.get(&format!("{root}/transactions?account_id={}", id(2)))["balances"]["total"],
        0
    );
    ok(s.write(
        "POST",
        &format!("{root}/accounts/{}/reopen", id(2)),
        "reopen",
        None,
        json!({}),
    ));
    ok(s.write(
        "DELETE",
        &format!("{root}/accounts/{}", id(2)),
        "delete-account",
        None,
        json!({"confirm_name":"Account 2"}),
    ));
    let survivor = s.get(&format!("{root}/transactions/{}", id(12)));
    assert_eq!(survivor["data"]["amount"], 2500);
    assert!(survivor["data"]["transfer_id"].is_null());
    assert!(survivor["data"]["payee_id"].is_null());
}

#[test]
fn genuine_synced_transfer_source_keeps_ids_balances_original_bytes_and_hlcs() {
    let s = Server::start();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/import-transfers-26.10.0.json"
    ))
    .unwrap();
    let bytes = include_bytes!("../../tests/fixtures/import-transfers-26.10.0.zip");
    let stage = ok(s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Idempotency-Key", "source")
        .header("Content-Type", "application/zip")
        .body(bytes.as_slice())
        .send()
        .unwrap());
    assert_eq!(stage["data"]["status"], "ready");
    let import = stage["data"]["id"].as_str().unwrap();
    let budget = stage["data"]["budget_id"].as_str().unwrap();
    let root = format!("/api/v1/budgets/{budget}");
    ok(s.write(
        "POST",
        &format!("/api/v1/imports/{import}/activate"),
        "activate",
        None,
        json!({}),
    ));
    assert_eq!(
        s.client
            .get(format!("{}/api/v1/imports/{import}/original.zip", s.url))
            .send()
            .unwrap()
            .bytes()
            .unwrap()
            .as_ref(),
        bytes.as_slice()
    );
    for account in fixture["public"]["balances"].as_array().unwrap() {
        let rows = s.get(&format!(
            "{root}/transactions?account_id={}",
            account["account_id"].as_str().unwrap()
        ));
        assert_eq!(rows["balances"]["total"], account["total"]);
        assert_eq!(rows["balances"]["cleared"], account["total"]);
        assert_eq!(rows["balances"]["uncleared"], 0);
    }
    let transfer_rows = fixture["public"]["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| !t["transfer_id"].is_null())
        .collect::<Vec<_>>();
    assert_eq!(transfer_rows.len(), 2);
    for expected in transfer_rows {
        let id = expected["id"].as_str().unwrap();
        let actual = s.get(&format!("{root}/transactions/{id}"));
        for field in [
            "id",
            "amount",
            "date",
            "notes",
            "cleared",
            "reconciled",
            "transfer_id",
        ] {
            assert_eq!(
                actual["data"][field],
                manual_field(expected, field),
                "{field}"
            );
        }
        assert_eq!(actual["data"]["category_id"], expected["category"]);
        for clock in fixture["inspection"]["latest_field_clocks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["dataset"] == "transactions" && c["row"] == id)
        {
            let source = clock["column"].as_str().unwrap();
            let field = match source {
                "transferred_id" => "transfer_id",
                "acct" => "account_id",
                "description" => "payee_id",
                "category" => "category_id",
                other => other,
            };
            if [
                "transfer_id",
                "account_id",
                "payee_id",
                "category_id",
                "amount",
                "date",
                "notes",
                "cleared",
                "reconciled",
            ]
            .contains(&field)
            {
                assert_eq!(
                    actual["data"]["field_versions"][field], clock["timestamp"],
                    "{field}"
                );
            }
        }
    }
    assert_eq!(
        s.get(&format!("{root}/preferences"))["data"]["sync_transfer_date"],
        true
    );
}

fn split_fixture(s: &Server) -> String {
    let root = format!("/api/v1/budgets/{B}");
    ok(s.write(
        "POST",
        "/api/v1/budgets",
        "budget",
        None,
        json!({"id":B,"name":"Split transfers"}),
    ));
    for n in [2, 3, 4] {
        ok(s.write("POST", &format!("{root}/accounts"), &format!("account-{n}"), None, json!({"id":id(n),"name":format!("Account {n}"),"opening_balance":0,"opening_date":"2026-01-01"})));
    }
    ok(s.write("POST", &format!("{root}/transactions"), "parent", None, json!({"id":id(10),"account_id":id(2),"amount":-1000,"date":"2026-01-01","notes":"Split payment"})));
    let snapshot = s.get(&format!("{root}/transactions/{}/splits", id(10)))["snapshot"].clone();
    ok(s.write("POST", &format!("{root}/transactions/{}/splits",id(10)), "split-transfer", None, json!({"snapshot":snapshot,"children":[{"id":id(11),"amount":-600,"notes":"Transfer allocation","transfer_account_id":id(3),"counterpart_id":id(12)},{"id":id(13),"amount":-400,"notes":"Ordinary allocation"}]})));
    root
}
#[test]
fn register_transfer_filter_returns_complete_logical_rows_without_double_counting() {
    let s = Server::start();
    let root = split_fixture(&s);
    let query = json!({"filter":{"conditions":[{"field":"transfer","op":"eq","value":true}]}});
    let response = s
        .client
        .get(format!("{}{root}/transactions", s.url))
        .query(&[("query", query.to_string())])
        .send()
        .unwrap();
    let page = ok(response);
    let mut ids = page["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    ids.sort();
    assert_eq!(ids, vec![id(10), id(12)]);
    assert_eq!(page["filtered_count"], 2);
    assert_eq!(page["filtered_balances"]["total"], -400);
    let parent = page["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == id(10))
        .unwrap();
    let direct = s.get(&format!("{root}/transactions/{}", id(10)));
    assert_eq!(parent["children"], direct["data"]["children"]);
    assert_eq!(parent["children"][0]["transfer_id"], id(12));
}
fn transfer_register_case(name: &str) {
    let s = Server::start();
    let root = split_fixture(&s);
    ok(s.write("POST", &format!("{root}/accounts"), "off-account", None, json!({"id":id(5),"name":"Off budget","off_budget":true,"opening_balance":0,"opening_date":"2026-01-01"})));
    for (n, account, amount) in [(20, 3, -200), (30, 5, -300)] {
        ok(s.write("POST", &format!("{root}/transfers"), &format!("pair-{n}"), None, json!({"id":id(n),"counterpart_id":id(n+1),"account_id":id(2),"to_account_id":id(account),"amount":amount,"date":"2026-01-01"})));
    }
    ok(s.write(
        "PATCH",
        &format!("{root}/accounts/{}", id(3)),
        "rename",
        None,
        json!({"name":"Zen savings"}),
    ));
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/transfer-register.json")).unwrap();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap();
    let mut request = s
        .client
        .get(format!("{}{root}/transactions", s.url))
        .query(&[("query", case["query"].to_string())]);
    if let Some(account) = case["account"].as_str() {
        request = request.query(&[("account_id", account)]);
    }
    let page = ok(request.send().unwrap());
    let mut ids = page["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    if case["ordered"] != true {
        ids.sort();
    }
    assert_eq!(json!(ids), case["ids"], "{name}");
    assert_eq!(
        page["filtered_count"],
        case["ids"].as_array().unwrap().len(),
        "{name}"
    );
    assert_eq!(page["filtered_balances"]["total"], case["total"], "{name}");
}
#[test]
fn renamed_transfer_account_search_matches_pinned_actual() {
    transfer_register_case("renamed account search");
}

#[test]
fn renamed_transfer_payee_filter_matches_pinned_actual() {
    transfer_register_case("renamed payee name");
}

#[test]
fn uncategorized_transfer_scope_matches_pinned_actual() {
    transfer_register_case("uncategorized");
}

#[test]
fn sorted_transfer_names_and_filter_match_pinned_actual() {
    transfer_register_case("sorted transfer names");
    transfer_register_case("transfer");
}

#[test]
fn split_transfer_amount_dates_delete_and_duplicate_remain_one_aggregate() {
    let s = Server::start();
    let root = split_fixture(&s);
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/split-transfers.json")).unwrap();
    for (alias, n) in [
        ("parent", 10),
        ("child", 11),
        ("child-other", 12),
        ("ordinary", 13),
    ] {
        let actual = s.get(&format!("{root}/transactions/{}", id(n)))["data"].clone();
        let expected = fixture["snapshots"][0]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == alias)
            .unwrap();
        for f in ["amount", "date", "notes"] {
            assert_eq!(actual[f], manual_field(expected, f), "{alias}/{f}");
        }
    }
    let update = ok(s.write(
        "PATCH",
        &format!("{root}/transactions/{}", id(12)),
        "opposite-edit",
        None,
        json!({"amount":700,"notes":"Changed from other account"}),
    ));
    let parent = s.get(&format!("{root}/transactions/{}", id(10)))["data"].clone();
    assert_eq!(parent["amount"], -1100); // Deliberate exact-total correction to pinned Actual defect.
    assert_eq!(parent["children"][0]["amount"], -700);
    assert_eq!(parent["field_versions"]["amount"], update["edit_clock"]);
    ok(s.write(
        "PATCH",
        &format!("{root}/transactions/{}", id(12)),
        "sync-date",
        None,
        json!({"date":"2026-02-01","sync_transfer_date":true}),
    ));
    for n in [10, 11, 12, 13] {
        assert_eq!(
            s.get(&format!("{root}/transactions/{}", id(n)))["data"]["date"],
            "2026-02-01"
        );
    }
    let copy = ok(s.write(
        "POST",
        &format!("{root}/transactions/{}/duplicate", id(10)),
        "duplicate-split",
        None,
        json!({"id":id(20)}),
    ))["data"]
        .clone();
    let child = &copy["children"][0];
    let peer = s.get(&format!(
        "{root}/transactions/{}",
        child["transfer_id"].as_str().unwrap()
    ))["data"]
        .clone();
    assert_eq!(peer["transfer_id"], child["id"]);
    assert_eq!(peer["amount"], 700);
    ok(s.write(
        "DELETE",
        &format!("{root}/transactions/{}", id(12)),
        "detach-child",
        None,
        Value::Null,
    ));
    let child = s.get(&format!("{root}/transactions/{}", id(11)))["data"].clone();
    assert!(child["transfer_id"].is_null());
    assert!(child["payee_id"].is_null());
    assert_eq!(child["amount"], -700);
    assert_eq!(child["parent_id"], id(10));
    assert_eq!(
        s.get(&format!("{root}/transactions/{}", id(10)))["data"]["amount"],
        -1100
    );
    ok(s.write(
        "DELETE",
        &format!("{root}/transactions/{}", id(20)),
        "delete-copy",
        None,
        Value::Null,
    ));
    assert_eq!(
        s.client
            .get(format!(
                "{}{root}/transactions/{}",
                s.url,
                peer["id"].as_str().unwrap()
            ))
            .send()
            .unwrap()
            .status(),
        404
    );
}

#[test]
fn split_transfer_replacement_unsplit_recombine_and_failed_intents_are_atomic() {
    let s = Server::start();
    let root = split_fixture(&s);
    let path = format!("{root}/transactions/{}/splits", id(10));
    let before = s.get(&format!("{root}/transactions"));
    let invalid = json!({"snapshot":before["snapshot"],"children":[{"id":id(11),"amount":-600,"transfer_account_id":id(4),"counterpart_id":id(14)},{"id":id(13),"amount":-400,"category_id":id(99)}]});
    assert_eq!(
        s.write("POST", &path, "invalid-replacement", None, invalid)
            .status(),
        400
    );
    assert_eq!(
        s.get(&format!("{root}/transactions")),
        before,
        "No changed peers, balances or feed snapshot after rollback"
    );
    let replacement = json!({"snapshot":before["snapshot"],"children":[{"id":id(11),"amount":-500,"transfer_account_id":id(4),"counterpart_id":id(14)},{"id":id(13),"amount":-500}]});
    let cancel = s
        .client
        .post(format!("{}{path}", s.url))
        .header("Idempotency-Key", "canceled-replacement")
        .header("X-Actual-Cancel-If-Uncommitted", "true")
        .json(&replacement)
        .send()
        .unwrap();
    assert_eq!(cancel.status(), 409);
    let canceled: Value = cancel.json().unwrap();
    assert_eq!(canceled["error"]["code"], "operation_canceled");
    assert_eq!(
        s.write(
            "POST",
            &path,
            "canceled-replacement",
            None,
            replacement.clone()
        )
        .json::<Value>()
        .unwrap(),
        canceled
    );
    assert_eq!(s.get(&format!("{root}/transactions")), before);
    ok(s.write("POST", &path, "replace-destination", None, replacement));
    assert_eq!(
        s.client
            .get(format!("{}{root}/transactions/{}", s.url, id(12)))
            .send()
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        s.get(&format!("{root}/transactions/{}", id(14)))["data"]["amount"],
        500
    );
    let snapshot = s.get(&path)["snapshot"].clone();
    ok(s.write(
        "POST",
        &path,
        "unsplit",
        None,
        json!({"snapshot":snapshot,"action":"unsplit","detach":[id(11)]}),
    ));
    assert!(s.get(&format!("{root}/transactions/{}", id(11)))["data"]["parent_id"].is_null());
    assert_eq!(
        s.get(&format!("{root}/transactions/{}", id(14)))["data"]["transfer_id"],
        id(11)
    );
    let peer_path = format!("{root}/transactions/{}/splits", id(14));
    let snapshot = s.get(&peer_path)["snapshot"].clone();
    assert_eq!(
        s.write(
            "POST",
            &peer_path,
            "reject-parent-pair",
            None,
            json!({"snapshot":snapshot,"children":[{"id":id(15),"amount":500}]})
        )
        .status(),
        400
    );

    // A one-allocation split promotes the pair to the original transaction on unsplit.
    let ordinary = format!("{root}/transactions/{}/splits", id(13));
    let snapshot = s.get(&ordinary)["snapshot"].clone();
    ok(s.write("POST",&ordinary,"single-transfer",None,json!({"snapshot":snapshot,"children":[{"id":id(16),"amount":-500,"transfer_account_id":id(3),"counterpart_id":id(17)}]})));
    let snapshot = s.get(&ordinary)["snapshot"].clone();
    ok(s.write(
        "POST",
        &ordinary,
        "single-unsplit",
        None,
        json!({"snapshot":snapshot,"action":"unsplit"}),
    ));
    assert_eq!(
        s.get(&format!("{root}/transactions/{}", id(17)))["data"]["transfer_id"],
        id(13)
    );
    assert_eq!(
        s.get(&format!("{root}/transactions/{}", id(13)))["data"]["transfer_id"],
        id(17)
    );
}

#[test]
fn linked_children_and_stale_peer_deletion_keep_exact_totals_and_durable_receipts() {
    let s = Server::start();
    let root = split_fixture(&s);
    ok(s.write(
        "POST",
        &format!("{root}/transactions"),
        "second-parent",
        None,
        json!({"id":id(20),"account_id":id(4),"amount":500,"date":"2026-01-01"}),
    ));
    let path = format!("{root}/transactions/{}/splits", id(20));
    let snapshot = s.get(&path)["snapshot"].clone();
    ok(s.write("POST",&path,"second-split",None,json!({"snapshot":snapshot,"children":[{"id":id(21),"amount":400},{"id":id(22),"amount":100}]})));
    let invalid = s.write(
        "POST",
        &format!("{root}/transfers/link"),
        "parent-link",
        None,
        json!({"id":id(20),"counterpart_id":id(13)}),
    );
    assert_eq!(invalid.status(), 400);
    ok(s.write(
        "POST",
        &format!("{root}/transfers/link"),
        "children-link",
        None,
        json!({"id":id(13),"counterpart_id":id(21)}),
    ));
    ok(s.write(
        "PATCH",
        &format!("{root}/transactions/{}", id(21)),
        "child-amount",
        None,
        json!({"amount":450}),
    ));
    assert_eq!(
        s.get(&format!("{root}/transactions/{}", id(20)))["data"]["amount"],
        550
    );
    assert_eq!(
        s.get(&format!("{root}/transactions/{}", id(10)))["data"]["amount"],
        -1050
    );
    let old = s.get(&format!("{root}/transactions/{}", id(13)))["data"]["field_versions"]["amount"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(s.write(
        "PATCH",
        &format!("{root}/transactions/{}", id(12)),
        "independent-peer",
        None,
        json!({"date":"2026-03-03"}),
    ));
    let rejected = s
        .write(
            "DELETE",
            &format!("{root}/transactions/{}", id(10)),
            "old-parent-delete",
            Some(&old),
            Value::Null,
        )
        .json::<Value>()
        .unwrap();
    assert_eq!(rejected["error"]["code"], "deletion_superseded");
    assert_eq!(
        s.write(
            "DELETE",
            &format!("{root}/transactions/{}", id(10)),
            "old-parent-delete",
            Some(&old),
            Value::Null
        )
        .json::<Value>()
        .unwrap(),
        rejected
    );
    let snapshot = s.get(&format!("{root}/transactions/{}/splits", id(10)))["snapshot"].clone();
    ok(s.write(
        "POST",
        &format!("{root}/transactions/{}/splits", id(10)),
        "recombine",
        None,
        json!({"snapshot":snapshot,"action":"recombine"}),
    ));
    let child = s.get(&format!("{root}/transactions/{}", id(21)))["data"].clone();
    assert!(child["transfer_id"].is_null());
    assert_eq!(child["amount"], 450);
    assert_eq!(
        s.get(&format!("{root}/transactions/{}", id(20)))["data"]["amount"],
        550
    );
    assert_eq!(
        s.client
            .get(format!("{}{root}/transactions/{}", s.url, id(12)))
            .send()
            .unwrap()
            .status(),
        404
    );
}

#[test]
fn genuine_split_transfer_source_and_native_restore_keep_allocations_and_pairs() {
    let s = Server::start();
    let source = include_bytes!("../../tests/fixtures/split-transfers-26.10.0.zip").as_slice();
    let staged = ok(s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Idempotency-Key", "split-transfer-source")
        .header("Content-Type", "application/zip")
        .body(source)
        .send()
        .unwrap());
    assert_eq!(staged["data"]["status"], "ready", "{staged}");
    let import = staged["data"]["id"].as_str().unwrap();
    let b = staged["data"]["budget_id"].as_str().unwrap();
    let root = format!("/api/v1/budgets/{b}");
    ok(s.write(
        "POST",
        &format!("/api/v1/imports/{import}/activate"),
        "activate-combined",
        None,
        json!({}),
    ));
    let original = s
        .client
        .get(format!("{}/api/v1/imports/{import}/original.zip", s.url))
        .send()
        .unwrap();
    assert_eq!(original.bytes().unwrap().as_ref(), source);
    let rows = s.get(&format!("{root}/transactions"));
    let parent = rows["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["is_parent"] == true)
        .unwrap();
    let child = parent["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| !c["transfer_id"].is_null())
        .unwrap();
    let peer = s.get(&format!(
        "{root}/transactions/{}",
        child["transfer_id"].as_str().unwrap()
    ))["data"]
        .clone();
    assert_eq!(peer["transfer_id"], child["id"]);
    assert_eq!(
        peer["amount"].as_i64(),
        child["amount"].as_i64().map(|v| -v)
    );
    assert_eq!(
        parent["amount"].as_i64().unwrap(),
        parent["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["amount"].as_i64().unwrap())
            .sum::<i64>()
    );
    for row in [parent, child, &peer] {
        let companion = s.get(&format!(
            "{root}/companion?account_id={}&retain={}",
            row["account_id"].as_str().unwrap(),
            row["id"].as_str().unwrap()
        ));
        assert_eq!(companion["data"]["recent"], json!([]));
        assert_eq!(companion["data"]["unavailable"], json!([row["id"]]));
    }
    let backup = ok(s.write(
        "POST",
        &format!("{root}/backups"),
        "combined-backup",
        None,
        json!({"label":"Split transfer"}),
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
    let stage = ok(s
        .client
        .post(format!("{}/api/v1/restores", s.url))
        .header("Idempotency-Key", "combined-restore")
        .header("Content-Type", "application/zip")
        .body(bytes.to_vec())
        .send()
        .unwrap());
    let rid = stage["data"]["id"].as_str().unwrap();
    let name = s.get(&root)["data"]["name"].clone();
    ok(s.write(
        "POST",
        &format!("{root}/restores/{rid}/activate"),
        "combined-restore-activate",
        None,
        json!({"confirm_name":name}),
    ));
    assert_eq!(s.get(&format!("{root}/transactions"))["data"], rows["data"]);
}

#[test]
fn closed_counterpart_keeps_existing_split_transfer_history_editable() {
    let s = Server::start();
    let root = split_fixture(&s);
    ok(s.write(
        "POST",
        &format!("{root}/accounts/{}/close", id(3)),
        "close-other",
        None,
        json!({"id":id(30),"counterpart_id":id(31),"to_account_id":id(4),"date":"2026-01-02","expected_balance":600}),
    ));
    let path = format!("{root}/transactions/{}/splits", id(10));
    let snapshot = s.get(&path)["snapshot"].clone();
    ok(s.write("POST",&path,"edit-closed-history",None,json!({"snapshot":snapshot,"children":[{"id":id(11),"amount":-600,"notes":"Historical correction","transfer_account_id":id(3),"counterpart_id":id(12)},{"id":id(13),"amount":-400}]})));
    assert_eq!(
        s.get(&format!("{root}/transactions/{}", id(12)))["data"]["notes"],
        ""
    );
    ok(s.write(
        "PATCH",
        &format!("{root}/transactions/{}", id(12)),
        "closed-category",
        None,
        json!({"category_id":null}),
    ));
    assert_eq!(
        s.get(&format!("{root}/transactions/{}", id(10)))["data"]["amount"],
        -1000
    );
}

#[test]
fn deleted_transfer_peer_merges_stay_local_after_child_detach_and_relink() {
    let s = Server::start();
    let root = split_fixture(&s);
    let path = format!("{root}/transactions/{}", id(12));
    ok(s.write("DELETE", &path, "delete-peer", None, Value::Null));
    let before = s.get(&format!("{root}/transactions"));
    let prefs = s.get(&format!("{root}/preferences"));
    let hidden=ok(s.write("PATCH",&path,"late-deleted-edit",None,json!({"amount":900,"date":"2026-02-02","cleared":true,"notes":"Late hidden correction","sync_transfer_date":true}))) ["data"].clone();
    assert_eq!(hidden["deleted"], true);
    assert_eq!(hidden["amount"], 900);
    assert_eq!(hidden["date"], "2026-02-02");
    assert_eq!(hidden["cleared"], true);
    let after = s.get(&format!("{root}/transactions"));
    assert_eq!(
        after["data"], before["data"],
        "Retired pair cannot alter live child/parent/siblings"
    );
    assert_eq!(after["balances"], before["balances"]);
    assert_eq!(
        s.get(&format!("{root}/preferences")),
        prefs,
        "A hidden date merge cannot change shared preference"
    );
    ok(s.write(
        "POST",
        &format!("{root}/transactions"),
        "new-peer",
        None,
        json!({"id":id(14),"account_id":id(4),"amount":600,"date":"2026-01-01"}),
    ));
    ok(s.write(
        "POST",
        &format!("{root}/transfers/link"),
        "relink-child",
        None,
        json!({"id":id(11),"counterpart_id":id(14)}),
    ));
    let before = s.get(&format!("{root}/transactions"));
    let hidden=ok(s.write("PATCH",&path,"late-old-relationship",None,json!({"amount":800,"date":"2026-03-03","cleared":false,"account_id":id(4),"category_id":null,"sync_transfer_date":true}))) ["data"].clone();
    assert_eq!(hidden["deleted"], true);
    assert_eq!(hidden["account_id"], id(4));
    assert_eq!(
        s.get(&format!("{root}/transactions"))["data"],
        before["data"]
    );
    assert_eq!(
        s.write(
            "PATCH",
            &path,
            "no-hidden-relink",
            None,
            json!({"transfer_id":id(14)})
        )
        .status(),
        400
    );
    let s = s.restart();
    assert_eq!(
        s.get(&format!("{root}/transactions"))["data"],
        before["data"]
    );
    // Even a pair whose two rows are both deleted has only local hidden merges.
    ok(s.write("POST",&format!("{root}/transfers"),"ordinary-pair",None,json!({"id":id(30),"counterpart_id":id(31),"account_id":id(2),"to_account_id":id(3),"amount":-20,"date":"2026-01-01"})));
    ok(s.write(
        "DELETE",
        &format!("{root}/transactions/{}", id(30)),
        "ordinary-delete",
        None,
        Value::Null,
    ));
    ok(s.write(
        "PATCH",
        &format!("{root}/transactions/{}", id(30)),
        "hidden-one",
        None,
        json!({"amount":-50,"date":"2026-04-04","cleared":true,"sync_transfer_date":true}),
    ));
    let other = ok(s.write(
        "PATCH",
        &format!("{root}/transactions/{}", id(31)),
        "hidden-other",
        None,
        json!({"notes":"Observe other tombstone"}),
    ))["data"]
        .clone();
    assert_eq!(other["deleted"], true);
    assert_eq!(other["amount"], 20);
    assert_eq!(other["date"], "2026-01-01");
    assert_eq!(other["cleared"], true);
}

#[test]
fn genuine_schema12_upgrade_preserves_settled_splits_receipts_sources_and_native_archive() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/reconciliation-schema12.json"
    ))
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("actualbudget.sqlite3"),
        include_bytes!("../../tests/fixtures/reconciliation-schema12.sqlite3"),
    )
    .unwrap();
    let s = Server::open(directory);
    let root = format!("/api/v1/budgets/{}", fixture["budget"].as_str().unwrap());
    let split = format!(
        "{root}/transactions/{}/splits",
        fixture["parent"].as_str().unwrap()
    );
    let account = format!("{root}/accounts/{}", fixture["account"].as_str().unwrap());
    for (path, key, body, receipt) in [
        (
            format!("{root}/accounts"),
            "account",
            "account_body",
            "account_receipt",
        ),
        (split.clone(), "split", "split_body", "split_receipt"),
    ] {
        assert_eq!(
            ok(s.write("POST", &path, key, None, fixture[body].clone())),
            fixture[receipt]
        );
    }
    assert_eq!(
        s.get(&split)["data"],
        current_row(fixture["locked_split"].clone())
    );
    assert_eq!(
        s.get(&account)["data"],
        current_row(fixture["account_state"].clone())
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
    assert_ne!(
        &old[..34],
        &fresh[..34],
        "Envelope retirement rotates epoch"
    );
    assert_eq!(
        s.client
            .get(format!("{}{active}/changes?cursor={old}&limit=1000", s.url))
            .send()
            .unwrap()
            .status(),
        409
    );
    for row in original["data"].as_array_mut().unwrap() {
        *row = current_row(row.clone());
    }
    original["balances"]["cleared"] = original["balances"]["total"].clone();
    original["balances"]["uncleared"] = json!(0);
    original["filtered_balances"]["cleared"] = original["filtered_balances"]["total"].clone();
    original["filtered_balances"]["uncleared"] = json!(0);
    strip_display(&mut current);
    strip_display(&mut original);
    current.as_object_mut().unwrap().remove("snapshot");
    original.as_object_mut().unwrap().remove("snapshot");
    assert_eq!(
        current, original,
        "Rules upgrade preserves every financial row, balance and ordering"
    );
    assert_eq!(s.get(&format!("{active}/accounts")), {
        let mut expected = fixture["active_accounts"].clone();
        for row in expected["data"].as_array_mut().unwrap() {
            *row = current_row(row.clone());
        }
        expected
    });
    let original = s
        .client
        .get(format!(
            "{}/api/v1/imports/{}/original.zip",
            s.url,
            fixture["active_import"].as_str().unwrap()
        ))
        .send()
        .unwrap();
    assert_eq!(
        original.bytes().unwrap().as_ref(),
        include_bytes!("../../tests/fixtures/reconciliation-import.zip")
    );
    let upload = |key: &str| {
        ok(s.client
            .post(format!("{}/api/v1/imports/actual", s.url))
            .header("Idempotency-Key", key)
            .header("Content-Type", "application/zip")
            .body(include_bytes!("../../tests/fixtures/import-transfers-26.10.0.zip").to_vec())
            .send()
            .unwrap())
    };
    assert_eq!(upload("blocked-source"), fixture["blocked_receipt"]);
    let reviewed = upload("inspect-transfer-after-upgrade");
    assert_eq!(reviewed["data"]["status"], "ready");
    ok(s.write(
        "POST",
        &format!(
            "/api/v1/imports/{}/activate",
            reviewed["data"]["id"].as_str().unwrap()
        ),
        "activate-upgraded-transfer",
        None,
        json!({}),
    ));
    let imported = format!(
        "/api/v1/budgets/{}",
        reviewed["data"]["budget_id"].as_str().unwrap()
    );
    let source: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/import-transfers-26.10.0.json"
    ))
    .unwrap();
    // Independent imported HLC exactness remains covered by the genuine synced source test;
    // this upgrade must re-inspect the same retained original bytes without changing its old receipt.
    assert!(
        s.get(&format!("{imported}/transactions"))["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| !r["transfer_id"].is_null())
    );
    for expected in source["public"]["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| !t["transfer_id"].is_null())
    {
        let actual = s.get(&format!(
            "{imported}/transactions/{}",
            expected["id"].as_str().unwrap()
        ))["data"]
            .clone();
        for field in [
            "amount",
            "date",
            "cleared",
            "reconciled",
            "notes",
            "transfer_id",
        ] {
            assert_eq!(actual[field], manual_field(expected, field));
        }
        for clock in source["inspection"]["latest_field_clocks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["dataset"] == "transactions" && c["row"] == expected["id"])
        {
            let field = match clock["column"].as_str().unwrap() {
                "transferred_id" => "transfer_id",
                "acct" => "account_id",
                "description" => "payee_id",
                "category" => "category_id",
                other => other,
            };
            if [
                "transfer_id",
                "account_id",
                "payee_id",
                "category_id",
                "amount",
                "date",
                "notes",
                "cleared",
                "reconciled",
            ]
            .contains(&field)
            {
                assert_eq!(actual["field_versions"][field], clock["timestamp"]);
            }
        }
    }
    for archive in [
        include_bytes!("../../tests/fixtures/reconciliation-schema12.zip").to_vec(),
        {
            let backup = ok(s.write(
                "POST",
                &format!("{root}/backups"),
                "upgraded-backup",
                None,
                json!({"label":"Schema13 settled split"}),
            ));
            s.client
                .get(format!(
                    "{}{root}/backups/{}/download",
                    s.url,
                    backup["data"]["id"].as_str().unwrap()
                ))
                .send()
                .unwrap()
                .bytes()
                .unwrap()
                .to_vec()
        },
    ] {
        let target = Server::start();
        let staged = ok(target
            .client
            .post(format!("{}/api/v1/restores", target.url))
            .header("Idempotency-Key", "stage")
            .header("Content-Type", "application/zip")
            .body(archive)
            .send()
            .unwrap());
        assert_eq!(staged["data"]["report"]["schema_version"], 36);
        ok(target.write(
            "POST",
            &format!(
                "{root}/restores/{}/activate",
                staged["data"]["id"].as_str().unwrap()
            ),
            "activate",
            None,
            json!({"confirm_name":"Schema12 reconciliation"}),
        ));
        let target = target.restart();
        assert_eq!(
            target.get(&split)["data"],
            current_row(fixture["locked_split"].clone())
        );
        assert_eq!(
            target.get(&account)["data"],
            current_row(fixture["account_state"].clone())
        );
        assert_eq!(
            target.get(&format!("{root}/register-settings"))["data"]["settings"],
            fixture["settings"]
        );
        for (path, key, body, receipt) in [
            (
                format!("{root}/accounts"),
                "account",
                "account_body",
                "account_receipt",
            ),
            (split.clone(), "split", "split_body", "split_receipt"),
        ] {
            assert_eq!(
                ok(target.write("POST", &path, key, None, fixture[body].clone())),
                fixture[receipt]
            );
        }
    }
}

#[test]
fn imported_thousand_member_transfer_edit_stays_bounded_and_exact() {
    let mut s = Server::start();
    s.client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();
    let uploaded = ok(s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Idempotency-Key", "large-import")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/reconciliation-large.zip").to_vec())
        .send()
        .unwrap());
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
    let case = &metadata["accounts"][1];
    let parent = case["parent"].as_str().unwrap();

    let split = format!("{root}/transactions/{parent}/splits");
    let view = s.get(&split);
    let child = view["data"]["children"][0]["id"].as_str().unwrap();
    ok(s.write(
        "POST",
        &format!("{root}/accounts"),
        "other-account",
        None,
        json!({"id":id(2),"name":"Other","opening_balance":0,"opening_date":"2026-01-01"}),
    ));
    ok(s.write(
        "POST",
        &format!("{root}/transactions"),
        "other-entry",
        None,
        json!({"id":id(3),"account_id":id(2),"amount":1,"date":"2026-01-01","cleared":true}),
    ));
    ok(s.write(
        "POST",
        &format!("{root}/transfers/link"),
        "large-link",
        None,
        json!({"id":child,"counterpart_id":id(3)}),
    ));
    let peer = format!("{root}/transactions/{}", id(3));
    let body = json!({"amount":2});
    assert!(body.to_string().len() < 256);
    ok(s.write("PATCH", &peer, "large-confirm", None, body));
    let after = s.get(&split)["data"].clone();
    assert_eq!(after["amount"], -1002);
    assert_eq!(after["children"].as_array().unwrap().len(), 1001);
    assert!(
        after["children"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["reconciled"] == false)
    );
    assert_eq!(s.get(&peer)["data"]["amount"], 2);
}
