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
#[test]
fn account_lifecycle_matches_pinned_public_oracle_and_survives_restart() {
    let mut s = Server::start();
    let root = format!("/api/v1/budgets/{B}");
    ok(s.write(
        "POST",
        "/api/v1/budgets",
        "budget",
        None,
        json!({"id":B,"name":"Accounts"}),
    ));
    let aliases = [("spending", 2), ("reserve", 3), ("empty", 4), ("doomed", 5)];
    for (alias, n) in aliases {
        ok(s.write("POST",&format!("{root}/accounts"),alias,None,json!({"id":id(n),"name":match alias{"spending"=>"Spending","reserve"=>"Reserve","empty"=>"Empty",_=>"Delete with history"},"off_budget":alias=="reserve","opening_balance":0,"opening_date":"2026-01-01"})));
    }
    for (n, account, amount) in [
        (11, 2, 10000),
        (12, 2, -1250),
        (13, 3, -5000),
        (14, 5, -321),
    ] {
        ok(s.write("POST",&format!("{root}/transactions"),&format!("tx{n}"),None,json!({"id":id(n),"account_id":id(account),"amount":amount,"date":"2026-01-01","cleared":true})));
    }
    ok(s.write(
        "POST",
        &format!("{root}/account-groups"),
        "group",
        None,
        json!({"id":id(6),"name":"Daily accounts"}),
    ));
    let oracle: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/account-lifecycle.json")).unwrap();
    for (index, expected) in oracle["snapshots"].as_array().unwrap().iter().enumerate() {
        match index {
            1 => {
                ok(s.write("PATCH",&format!("{root}/accounts/{}",id(2)),"edit",None,json!({"name":"Daily spending","group_id":id(6),"notes":"Account note with #travel and two lines\nRetain history"})));
                ok(s.write(
                    "PATCH",
                    &format!("{root}/accounts/{}", id(3)),
                    "group-reserve",
                    None,
                    json!({"group_id":id(6)}),
                ));
                ok(s.write(
                    "PATCH",
                    &format!("{root}/account-groups/{}", id(6)),
                    "rename-group",
                    None,
                    json!({"name":"Everyday"}),
                ));
            }
            2 => {
                assert_eq!(
                    s.write(
                        "POST",
                        &format!("{root}/accounts/{}/close", id(2)),
                        "nonzero",
                        None,
                        json!({})
                    )
                    .status(),
                    409
                );
            }
            3 => {
                ok(s.write(
                    "POST",
                    &format!("{root}/transactions"),
                    "zero",
                    None,
                    json!({"id":id(15),"account_id":id(2),"amount":-8750,"date":"2026-01-02"}),
                ));
                ok(s.write(
                    "POST",
                    &format!("{root}/accounts/{}/close", id(2)),
                    "close",
                    None,
                    json!({}),
                ));
            }
            4 => {
                assert_eq!(
                    s.client
                        .get(format!("{}{root}/companion?account_id={}", s.url, id(2)))
                        .send()
                        .unwrap()
                        .status(),
                    404
                );
                let closed = ok(s.write(
                    "DELETE",
                    &format!("{root}/accounts/{}", id(2)),
                    "closed-delete",
                    None,
                    json!({"confirm_name":"Daily spending"}),
                ));
                assert_eq!(closed["data"]["closed"], true);
                assert_eq!(
                    s.get(&format!("{root}/transactions?account_id={}", id(2)))["data"]
                        .as_array()
                        .unwrap()
                        .len(),
                    3
                );
                ok(s.write(
                    "POST",
                    &format!("{root}/accounts/{}/reopen", id(2)),
                    "reopen",
                    None,
                    json!({}),
                ));
            }
            5 => {
                assert!(
                    s.client
                        .get(format!("{}{root}/companion?account_id={}", s.url, id(2)))
                        .send()
                        .unwrap()
                        .status()
                        .is_success()
                );
                ok(s.write(
                    "POST",
                    &format!("{root}/accounts/{}/close", id(4)),
                    "empty-close",
                    None,
                    json!({}),
                ));
            }
            6 => {
                let body = json!({"confirm_name":"Delete with history"});
                let path = format!("{root}/accounts/{}", id(5));
                let first = ok(s.write("DELETE", &path, "delete", None, body.clone()));
                assert_eq!(ok(s.write("DELETE", &path, "delete", None, body)), first);
            }
            7 => {
                ok(s.write(
                    "DELETE",
                    &format!("{root}/account-groups/{}", id(6)),
                    "delete-group",
                    None,
                    json!({}),
                ));
                s = s.restart();
            }
            _ => {}
        }
        let accounts = s.get(&format!("{root}/accounts"))["data"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(
            accounts.len(),
            expected["accounts"].as_array().unwrap().len()
        );
        for item in expected["accounts"].as_array().unwrap() {
            let n = aliases.iter().find(|(a, _)| item["id"] == *a).unwrap().1;
            let actual = accounts.iter().find(|a| a["id"] == id(n)).unwrap();
            for (a, b) in [
                ("name", "name"),
                ("off_budget", "offbudget"),
                ("closed", "closed"),
                ("balance", "total"),
            ] {
                assert_eq!(actual[a], item[b], "step {index}: {a}");
            }
            assert_eq!(
                actual["group_id"],
                if item["group"].is_null() {
                    Value::Null
                } else {
                    json!(id(6))
                }
            );
            assert_eq!(
                actual["notes"].as_str().unwrap(),
                item["note"]["note"].as_str().unwrap_or("")
            );
            let register = s.get(&format!("{root}/transactions?account_id={}", id(n)));
            for expected_tx in item["transactions"].as_array().unwrap() {
                let number = expected_tx["id"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("transaction-")
                    .unwrap()
                    .parse::<usize>()
                    .unwrap()
                    + 10;
                let row = register["data"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|row| row["id"] == id(number))
                    .unwrap();
                for field in ["amount", "date", "cleared"] {
                    assert_eq!(row[field], expected_tx[field]);
                }
                assert_eq!(
                    row["notes"].as_str().unwrap_or(""),
                    expected_tx["notes"].as_str().unwrap_or("")
                );
            }
            assert_eq!(
                register["data"].as_array().unwrap().len(),
                item["transactions"].as_array().unwrap().len()
            );
        }
        assert_eq!(
            s.get(&format!("{root}/account-groups"))["data"]
                .as_array()
                .unwrap()
                .len(),
            expected["groups"].as_array().unwrap().len()
        );
    }
    assert_eq!(
        s.get(&format!("{root}/transactions"))["data"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn ordering_field_conflicts_and_native_archive_roundtrip() {
    let s = Server::start();
    let root = format!("/api/v1/budgets/{B}");
    ok(s.write(
        "POST",
        "/api/v1/budgets",
        "b",
        None,
        json!({"id":B,"name":"Accounts"}),
    ));
    for n in 2..5 {
        ok(s.write("POST",&format!("{root}/accounts"),&format!("a{n}"),None,json!({"id":id(n),"name":format!("Account {n}"),"opening_balance":0,"opening_date":"2026-01-01"})));
    }
    let newer = "2026-01-03T00:00:00.000Z-0000-0000000000000001";
    let older = "2026-01-02T00:00:00.000Z-0000-0000000000000001";
    let path = format!("{root}/accounts/{}", id(2));
    ok(s.write(
        "PATCH",
        &path,
        "name",
        Some(newer),
        json!({"name":"Newest"}),
    ));
    let result = ok(s.write(
        "PATCH",
        &path,
        "late",
        Some(older),
        json!({"name":"Older","notes":"Independent note"}),
    ));
    assert_eq!(result["data"]["name"], "Newest");
    assert_eq!(result["data"]["notes"], "Independent note");
    assert_eq!(result["conflicts"][0]["field"], "name");
    ok(s.write(
        "POST",
        &format!("{root}/accounts/{}/move", id(4)),
        "move",
        None,
        json!({"before_id":id(2)}),
    ));
    assert_eq!(
        s.get(&format!("{root}/accounts"))["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![id(4), id(2), id(3)]
    );
    ok(s.write(
        "POST",
        &format!("{root}/account-groups"),
        "g",
        None,
        json!({"id":id(6),"name":"Everyday"}),
    ));
    ok(s.write(
        "PATCH",
        &path,
        "membership",
        None,
        json!({"group_id":id(6)}),
    ));
    let before = s.get(&format!("{root}/accounts"));
    let groups = s.get(&format!("{root}/account-groups"));
    let backup = ok(s.write(
        "POST",
        &format!("{root}/backups"),
        "backup",
        None,
        json!({"label":"Accounts state"}),
    ));
    ok(s.write(
        "DELETE",
        &format!("{root}/account-groups/{}", id(6)),
        "deletegroup",
        None,
        json!({}),
    ));
    ok(s.write(
        "POST",
        &format!(
            "{root}/backups/{}/restore",
            backup["data"]["id"].as_str().unwrap()
        ),
        "restore",
        None,
        json!({"confirm_name":"Accounts"}),
    ));
    assert_eq!(before, s.get(&format!("{root}/accounts")));
    assert_eq!(groups, s.get(&format!("{root}/account-groups")));
    assert_eq!(
        s.write(
            "PATCH",
            &path,
            "unknown-old-epoch",
            None,
            json!({"notes":"must not apply"})
        )
        .status(),
        409
    );
    let replay = ok(s.write(
        "PATCH",
        &path,
        "late",
        Some(older),
        json!({"name":"Older","notes":"Independent note"}),
    ));
    assert_eq!(result, replay);
}

#[test]
fn pinned_actual_account_archive_import_preserves_groups_notes_order_and_closed_history() {
    let s = Server::start();
    let expected: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/import-accounts-26.10.0.json"
    ))
    .unwrap();
    let stage = ok(s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Idempotency-Key", "upload")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/import-accounts-26.10.0.zip").as_slice())
        .send()
        .unwrap());
    assert_eq!(stage["data"]["status"], "ready", "{stage}");
    let import = stage["data"]["id"].as_str().unwrap();
    let budget = ok(s.write(
        "POST",
        &format!("/api/v1/imports/{import}/activate"),
        "activate",
        None,
        json!({}),
    ));
    let root = format!("/api/v1/budgets/{}", budget["data"]["id"].as_str().unwrap());
    let actual = s.get(&format!("{root}/accounts"));
    let accounts = actual["data"].as_array().unwrap();
    assert_eq!(
        accounts.len(),
        expected["public"]["accounts"].as_array().unwrap().len()
    );
    for a in accounts {
        let source = expected["public"]["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == a["id"])
            .unwrap();
        assert_eq!(a["name"], source["name"]);
        assert_eq!(a["off_budget"], source["offbudget"]);
        assert_eq!(a["closed"], source["closed"]);
        assert_eq!(a["group_id"], source["account_group_id"]);
        let balance = expected["public"]["balances"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["account_id"] == a["id"])
            .unwrap();
        assert_eq!(a["balance"], balance["total"]);
        let note = expected["public"]["notes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == a["id"])
            .map(|v| v["note"].as_str().unwrap())
            .unwrap_or("");
        assert_eq!(a["notes"], note);
    }
    let register = s.get(&format!("{root}/transactions"));
    assert_eq!(
        register["data"].as_array().unwrap().len(),
        expected["public"]["transactions"].as_array().unwrap().len()
    );
    for expected_tx in expected["public"]["transactions"].as_array().unwrap() {
        let row = register["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == expected_tx["id"])
            .unwrap();
        for field in ["amount", "date", "cleared", "reconciled"] {
            assert_eq!(row[field], expected_tx[field]);
        }
        assert_eq!(
            row["notes"].as_str().unwrap_or(""),
            expected_tx["notes"].as_str().unwrap_or("")
        );
        assert_eq!(row["account_id"], expected_tx["account"]);
    }
    let groups = s.get(&format!("{root}/account-groups"));
    assert_eq!(groups["data"].as_array().unwrap().len(), 1);
    assert_eq!(groups["data"][0]["name"], "Daily accounts");
    let s = s.restart();
    assert_eq!(s.get(&format!("{root}/accounts")), actual);
    let copy = ok(s.write(
        "POST",
        &format!("{root}/duplicate"),
        "duplicate",
        None,
        json!({"id":id(90),"name":"Copied accounts"}),
    ));
    assert_eq!(copy["data"]["name"], "Copied accounts");
    assert_eq!(
        s.get(&format!("/api/v1/budgets/{}/accounts", id(90))),
        actual
    );
}

#[test]
fn stale_account_deletion_does_not_erase_newer_financial_edits() {
    let s = Server::start();
    let root = format!("/api/v1/budgets/{B}");
    ok(s.write(
        "POST",
        "/api/v1/budgets",
        "b",
        None,
        json!({"id":B,"name":"Concurrent deletion"}),
    ));
    ok(s.write(
        "POST",
        &format!("{root}/accounts"),
        "a",
        None,
        json!({"id":id(2),"name":"Account","opening_balance":0,"opening_date":"2026-01-01"}),
    ));
    let old = "2026-01-01T00:00:00.000Z-0000-0000000000000001";
    let new = "2026-01-02T00:00:00.000Z-0000-0000000000000001";
    ok(s.write(
        "POST",
        &format!("{root}/transactions"),
        "tx",
        Some(new),
        json!({"id":id(3),"account_id":id(2),"amount":-200,"date":"2026-01-01"}),
    ));
    assert_eq!(
        s.write(
            "DELETE",
            &format!("{root}/accounts/{}", id(2)),
            "stale-delete",
            Some(old),
            json!({"confirm_name":"Account"})
        )
        .status(),
        409
    );
    assert_eq!(
        s.get(&format!("{root}/accounts"))["data"][0]["balance"],
        -200
    );
}

#[test]
fn genuine_schema_six_archive_restores_accounts_with_new_metadata_defaults() {
    let s = Server::start();
    let expected: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native-schema6-accounts.json"
    ))
    .unwrap();
    let stage = ok(s
        .client
        .post(format!("{}/api/v1/restores", s.url))
        .header("Idempotency-Key", "stage")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/native-schema6-accounts.zip").as_slice())
        .send()
        .unwrap());
    let root = format!(
        "/api/v1/budgets/{}",
        expected["budget_id"].as_str().unwrap()
    );
    ok(s.write(
        "POST",
        &format!(
            "{root}/restores/{}/activate",
            stage["data"]["id"].as_str().unwrap()
        ),
        "restore",
        None,
        json!({"confirm_name":"Schema six account fixture"}),
    ));
    let rows = s.get(&format!("{root}/accounts"));
    let row = &rows["data"][0];
    for (key, value) in expected["accounts"][0].as_object().unwrap() {
        assert_eq!(&row[key], value);
    }
    assert!(row["group_id"].is_null());
    assert_eq!(row["deleted"], false);
    assert_eq!(row["sort_order"], 0);
    let restored = s.restart();
    assert_eq!(rows, restored.get(&format!("{root}/accounts")));
}

#[test]
fn account_delete_failure_rolls_back_history_balances_and_identity_receipt() {
    let s = Server::start();
    let root = format!("/api/v1/budgets/{B}");
    ok(s.write(
        "POST",
        "/api/v1/budgets",
        "b",
        None,
        json!({"id":B,"name":"Boundary"}),
    ));
    for (n, amount) in [
        (2, -9007199254740991_i64),
        (3, 9007199254740991_i64),
        (4, 9007199254740991_i64),
    ] {
        ok(s.write("POST",&format!("{root}/accounts"),&format!("a{n}"),None,json!({"id":id(n),"name":format!("Account {n}"),"opening_balance":amount,"opening_date":"2026-01-01"})));
    }
    let before = s.get(&format!("{root}/accounts"));
    let transactions = s.get(&format!("{root}/transactions"));
    let failed = s.write(
        "DELETE",
        &format!("{root}/accounts/{}", id(2)),
        "delete-boundary",
        None,
        json!({"confirm_name":"Account 2"}),
    );
    assert_eq!(failed.status(), 400);
    assert_eq!(before, s.get(&format!("{root}/accounts")));
    assert_eq!(transactions, s.get(&format!("{root}/transactions")));
}

#[test]
fn deleted_account_history_accepts_hidden_field_merges_without_resurrection() {
    let s = Server::start();
    let root = format!("/api/v1/budgets/{B}");
    let c1 = "2026-01-01T00:00:00.000Z-0000-0000000000000001";
    let c2 = "2026-01-02T00:00:00.000Z-0000-0000000000000001";
    let c3 = "2026-01-03T00:00:00.000Z-0000-0000000000000001";
    let c4 = "2026-01-04T00:00:00.000Z-0000-0000000000000001";
    ok(s.write(
        "POST",
        "/api/v1/budgets",
        "b",
        None,
        json!({"id":B,"name":"Hidden history"}),
    ));
    for n in [2, 3, 4] {
        ok(s.write("POST", &format!("{root}/accounts"), &format!("a{n}"), None, json!({"id":id(n),"name":format!("Account {n}"),"opening_balance":0,"opening_date":"2026-01-01"})));
    }
    for (n, account, amount) in [(11, 2, -1250), (12, 4, 2500)] {
        ok(s.write("POST", &format!("{root}/transactions"), &format!("tx{n}"), Some(c1), json!({"id":id(n),"account_id":id(account),"amount":amount,"date":"2026-01-01","notes":"Original"})));
    }
    for n in [2, 3] {
        ok(s.write(
            "DELETE",
            &format!("{root}/accounts/{}", id(n)),
            &format!("delete{n}"),
            Some(c2),
            json!({"confirm_name":format!("Account {n}")}),
        ));
    }
    let path = format!("{root}/transactions/{}", id(11));
    let latest = json!({"notes":"Newest hidden note"});
    let merged = ok(s.write("PATCH", &path, "latest", Some(c4), latest.clone()));
    assert_eq!(merged["data"]["deleted"], true);
    assert_eq!(merged["data"]["notes"], "Newest hidden note");
    let independent = ok(s.write(
        "PATCH",
        &path,
        "independent",
        Some(c3),
        json!({"notes":"Older note","amount":-2500}),
    ));
    assert_eq!(independent["data"]["notes"], "Newest hidden note");
    assert_eq!(independent["data"]["amount"], -2500);
    assert_eq!(independent["data"]["deleted"], true);
    assert_eq!(independent["conflicts"][0]["field"], "notes");
    assert_eq!(independent["data"]["field_versions"]["notes"], c4);
    assert_eq!(independent["data"]["field_versions"]["amount"], c3);
    let s = s.restart();
    assert_eq!(
        ok(s.write("PATCH", &path, "latest", Some(c4), latest)),
        merged
    );
    // Only retention of the same deleted account is permitted. Moves and new
    // visible rows must still target an active account.
    for (key, target) in [
        ("hidden-move", path.clone()),
        ("visible-move", format!("{root}/transactions/{}", id(12))),
    ] {
        assert_eq!(
            s.write("PATCH", &target, key, None, json!({"account_id":id(3)}))
                .status(),
            404
        );
    }
    assert_eq!(
        s.write(
            "POST",
            &format!("{root}/transactions"),
            "new-in-deleted",
            None,
            json!({"id":id(13),"account_id":id(2),"amount":100,"date":"2026-01-01"})
        )
        .status(),
        404
    );
    let accounts = s.get(&format!("{root}/accounts"));
    assert_eq!(accounts["data"].as_array().unwrap().len(), 1);
    assert_eq!(accounts["data"][0]["id"], id(4));
    assert_eq!(accounts["data"][0]["balance"], 2500);
    let transactions = s.get(&format!("{root}/transactions"));
    assert_eq!(transactions["data"].as_array().unwrap().len(), 1);
    assert_eq!(transactions["data"][0]["id"], id(12));
    assert_eq!(transactions["balances"]["total"], 2500);
    assert_eq!(
        s.client
            .get(format!("{}{path}", s.url))
            .send()
            .unwrap()
            .status(),
        404
    );
}
