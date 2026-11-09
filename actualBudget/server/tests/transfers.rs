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
            assert_eq!(row[field], expected[field], "{field}");
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
    assert_eq!(other["data"]["notes"], "Corrected payment");
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
            assert_eq!(actual["data"][field], expected[field]);
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
        for field in ["total", "cleared", "uncleared"] {
            assert_eq!(rows["balances"][field], account[field]);
        }
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
            assert_eq!(actual["data"][field], expected[field], "{field}");
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
