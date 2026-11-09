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
const BUDGET: &str = "7b7e5bcc-9b19-418d-8bc2-20661dd6cf09";
const ACCOUNT: &str = "b4cdd385-e9e6-4dca-827a-c4cf9a153eaa";
fn create_budget(server: &Server) {
    let response = server.post(
        "/api/v1/budgets",
        "budget-one",
        json!({"id":BUDGET,"name":"Home","budget_type":"tracking"}),
    );
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
}
#[test]
fn bulk_update_is_one_replay_safe_undo_group_and_preserves_another_devices_note() {
    let server = Server::start();
    create_budget(&server);
    assert_eq!(server.post(&format!("/api/v1/budgets/{BUDGET}/accounts"), "account", json!({"id":ACCOUNT,"name":"Spending","opening_balance":0,"opening_date":"2026-01-01"})).status(),201);
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    let ids = [
        "10000000-0000-4000-8000-000000000001",
        "10000000-0000-4000-8000-000000000002",
    ];
    for (id, amount) in ids.iter().zip([-1234, -5678]) {
        assert_eq!(
            server
                .post(
                    &path,
                    id,
                    json!({"id":id,"account_id":ACCOUNT,"amount":amount,"date":"2026-01-02"})
                )
                .status(),
            201
        );
    }
    let owner = "20000000-0000-4000-8000-000000000001";
    let snapshot: Value = server.get(&path).json().unwrap();
    let body = json!({"snapshot":snapshot["snapshot"],"action":"update","ids":ids,"fields":{"cleared":true}});
    let send = |route: &str, key: &str, body: &Value| {
        server
            .client
            .post(format!("{}{route}", server.url))
            .header("Idempotency-Key", key)
            .header("X-Actual-History-Owner", owner)
            .json(body)
            .send()
            .unwrap()
    };
    let response = send(&format!("{path}/bulk"), "bulk", &body);
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let replay: Value = send(&format!("{path}/bulk"), "bulk", &body).json().unwrap();
    assert_eq!(replay["data"]["affected_count"], 2);
    assert_eq!(
        server
            .client
            .patch(format!("{}{path}/{}", server.url, ids[0]))
            .header("Idempotency-Key", "other-device-note")
            .json(&json!({"notes":"Remote note"}))
            .send()
            .unwrap()
            .status(),
        200
    );
    let history_path = format!("/api/v1/budgets/{BUDGET}/history");
    let history: Value = server
        .client
        .get(format!("{}{history_path}", server.url))
        .header("X-Actual-History-Owner", owner)
        .send()
        .unwrap()
        .json()
        .unwrap();
    let undo = send(
        &format!("{history_path}/undo"),
        "undo",
        &json!({"action_id":history["data"]["undo"]["id"]}),
    );
    assert_eq!(undo.status(), 200, "{}", undo.text().unwrap());
    let register: Value = server.get(&path).json().unwrap();
    assert_eq!(register["balances"]["total"], -6912);
    assert_eq!(register["balances"]["cleared"], 0);
    let row: Value = server.get(&format!("{path}/{}", ids[0])).json().unwrap();
    assert_eq!(row["data"]["notes"], "Remote note");
    let history: Value = server
        .client
        .get(format!("{}{history_path}", server.url))
        .header("X-Actual-History-Owner", owner)
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert!(history["data"]["undo"].is_null());
    assert_eq!(
        send(
            &format!("{history_path}/redo"),
            "redo",
            &json!({"action_id":history["data"]["redo"]["id"]})
        )
        .status(),
        200
    );
    let register: Value = server.get(&path).json().unwrap();
    assert_eq!(register["balances"]["cleared"], -6912);
}

const OWNER: &str = "20000000-0000-4000-8000-000000000001";
const ENTRY: &str = "10000000-0000-4000-8000-000000000001";
impl Server {
    fn owned(&self, method: &str, path: &str, key: &str, body: Value) -> Response {
        self.client
            .request(method.parse().unwrap(), format!("{}{path}", self.url))
            .header("Idempotency-Key", key)
            .header("X-Actual-History-Owner", OWNER)
            .json(&body)
            .send()
            .unwrap()
    }
    fn history(&self) -> Value {
        self.client
            .get(format!("{}/api/v1/budgets/{BUDGET}/history", self.url))
            .header("X-Actual-History-Owner", OWNER)
            .send()
            .unwrap()
            .json::<Value>()
            .unwrap()["data"]
            .clone()
    }
    fn step(&self, direction: &str, key: &str) -> Response {
        let id = self.history()[direction]["id"].clone();
        self.owned(
            "POST",
            &format!("/api/v1/budgets/{BUDGET}/history/{direction}"),
            key,
            json!({"action_id":id}),
        )
    }
    fn setup() -> Self {
        let s = Self::start();
        create_budget(&s);
        assert_eq!(s.post(&format!("/api/v1/budgets/{BUDGET}/accounts"),"account",json!({"id":ACCOUNT,"name":"Spending","opening_balance":0,"opening_date":"2026-01-01"})).status(),201);
        s
    }
}
#[test]
fn successive_undo_redo_walks_same_field_history_without_false_remote_conflicts() {
    let s = Server::setup();
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    assert_eq!(
        s.owned(
            "POST",
            &path,
            "create",
            json!({"id":ENTRY,"account_id":ACCOUNT,"amount":-1000,"date":"2026-01-02"})
        )
        .status(),
        201
    );
    for (key, amount) in [("first", -2000), ("second", -3000)] {
        assert_eq!(
            s.owned(
                "PATCH",
                &format!("{path}/{ENTRY}"),
                key,
                json!({"amount":amount})
            )
            .status(),
            200
        );
    }
    for (key, amount) in [("u1", -2000), ("u2", -1000)] {
        let r = s.step("undo", key);
        assert_eq!(r.status(), 200, "{}", r.text().unwrap());
        let row: Value = s.get(&format!("{path}/{ENTRY}")).json().unwrap();
        assert_eq!(row["data"]["amount"], amount);
    }
    for (key, amount) in [("r1", -2000), ("r2", -3000)] {
        let r = s.step("redo", key);
        assert_eq!(r.status(), 200, "{}", r.text().unwrap());
        let row: Value = s.get(&format!("{path}/{ENTRY}")).json().unwrap();
        assert_eq!(row["data"]["amount"], amount);
    }
}
#[test]
fn split_actions_duplicate_delete_and_history_restore_whole_aggregates() {
    let s = Server::setup();
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    assert_eq!(s.post(&path,"create",json!({"id":ENTRY,"account_id":ACCOUNT,"amount":-5001,"date":"2026-01-02","cleared":true})).status(),201);
    let snapshot: Value = s.get(&path).json().unwrap();
    let children = json!([{"id":"10000000-0000-4000-8000-000000000002","amount":-2000},{"id":"10000000-0000-4000-8000-000000000003","amount":0}]);
    assert_eq!(
        s.owned(
            "POST",
            &format!("{path}/{ENTRY}/splits"),
            "split",
            json!({"snapshot":snapshot["snapshot"],"children":children,"allocation":"even"})
        )
        .status(),
        200
    );
    for (direction, key, count) in [("undo", "u-split", 0), ("redo", "r-split", 2)] {
        let r = s.step(direction, key);
        assert_eq!(r.status(), 200, "{}", r.text().unwrap());
        let row: Value = s.get(&format!("{path}/{ENTRY}")).json().unwrap();
        assert_eq!(
            row["data"]["children"].as_array().map_or(0, |v| v.len()),
            count
        );
    }
    let snapshot: Value = s.get(&path).json().unwrap();
    let body = json!({"snapshot":snapshot["snapshot"],"action":"duplicate","ids":[ENTRY],"duplicate_ids":["10000000-0000-4000-8000-000000000004"]});
    assert_eq!(
        s.owned("POST", &format!("{path}/bulk"), "dup", body.clone())
            .status(),
        200
    );
    assert_eq!(
        s.owned("POST", &format!("{path}/bulk"), "dup", body)
            .status(),
        200
    );
    let register: Value = s.get(&path).json().unwrap();
    assert_eq!(register["balances"]["total"], -10002);
    assert_eq!(register["balances"]["cleared"], -5001);
    let r = s.step("undo", "u-dup");
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    let r = s.step("redo", "r-dup");
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    let snapshot: Value = s.get(&path).json().unwrap();
    assert_eq!(s.owned("POST",&format!("{path}/bulk"),"delete",json!({"snapshot":snapshot["snapshot"],"action":"delete","ids":[ENTRY,"10000000-0000-4000-8000-000000000004"]})).status(),200);
    let register: Value = s.get(&path).json().unwrap();
    assert_eq!(register["balances"]["total"], 0);
    let r = s.step("undo", "u-delete");
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    let register: Value = s.get(&path).json().unwrap();
    assert_eq!(register["balances"]["total"], -10002);
    assert_eq!(register["data"].as_array().unwrap().len(), 2);
}
#[test]
fn failures_overlap_and_remote_deletion_never_partially_change_a_bulk_action() {
    let s = Server::setup();
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    let other = "10000000-0000-4000-8000-000000000002";
    for id in [ENTRY, other] {
        assert_eq!(
            s.post(
                &path,
                id,
                json!({"id":id,"account_id":ACCOUNT,"amount":-1000,"date":"2026-01-02"})
            )
            .status(),
            201
        );
    }
    let snapshot: Value = s.get(&path).json().unwrap();
    let r=s.owned("POST",&format!("{path}/bulk"),"overflow",json!({"snapshot":snapshot["snapshot"],"action":"update","ids":[ENTRY,other],"fields":{"amount":9007199254740991_i64}}));
    assert_eq!(r.status(), 400);
    let unchanged: Value = s.get(&path).json().unwrap();
    assert_eq!(unchanged["snapshot"], snapshot["snapshot"]);
    assert_eq!(unchanged["balances"]["total"], -2000);
    assert!(s.history()["undo"].is_null());
    assert_eq!(s.owned("POST",&format!("{path}/bulk"),"edit",json!({"snapshot":snapshot["snapshot"],"action":"update","ids":[ENTRY,other],"fields":{"notes":"Joint edit"}})).status(),200);
    let action = s.history()["undo"].clone();
    assert_eq!(
        s.client
            .patch(format!("{}{path}/{other}", s.url))
            .header("Idempotency-Key", "remote")
            .json(&json!({"notes":"Joint edit"}))
            .send()
            .unwrap()
            .status(),
        200
    );
    assert_eq!(s.step("undo", "blocked").status(), 409);
    assert_eq!(s.history()["undo"], action);
    let row: Value = s.get(&format!("{path}/{ENTRY}")).json().unwrap();
    assert_eq!(row["data"]["notes"], "Joint edit");
    assert_eq!(
        s.client
            .delete(format!("{}{path}/{ENTRY}", s.url))
            .header("Idempotency-Key", "remote-delete")
            .send()
            .unwrap()
            .status(),
        200
    );
    assert_eq!(s.step("undo", "blocked-delete").status(), 409);
    assert_eq!(s.get(&format!("{path}/{ENTRY}")).status(), 404);
}
#[test]
fn pinned_history_limit_redo_branch_and_engine_ownership() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/history-26.10.0.json")).unwrap();
    let s = Server::setup();
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    assert_eq!(
        s.post(
            &path,
            "create",
            json!({"id":ENTRY,"account_id":ACCOUNT,"amount":0,"date":"2026-01-02"})
        )
        .status(),
        201
    );
    for amount in 1..=25 {
        assert_eq!(
            s.owned(
                "PATCH",
                &format!("{path}/{ENTRY}"),
                &format!("amount-{amount}"),
                json!({"amount":amount})
            )
            .status(),
            200
        );
    }
    let outsider: Value = s
        .client
        .get(format!("{}/api/v1/budgets/{BUDGET}/history", s.url))
        .header(
            "X-Actual-History-Owner",
            "20000000-0000-4000-8000-000000000002",
        )
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert!(outsider["data"]["undo"].is_null());
    for index in 0..20 {
        let r = s.step("undo", &format!("undo-{index}"));
        assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    }
    assert!(s.history()["undo"].is_null());
    let row: Value = s.get(&format!("{path}/{ENTRY}")).json().unwrap();
    assert_eq!(row["data"]["amount"], fixture["retention"]["twentyUndo"]);
    for index in 0..20 {
        let r = s.step("redo", &format!("redo-{index}"));
        assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    }
    let row: Value = s.get(&format!("{path}/{ENTRY}")).json().unwrap();
    assert_eq!(row["data"]["amount"], fixture["retention"]["twentyRedo"]);
    assert_eq!(s.step("undo", "branch-undo").status(), 200);
    assert_eq!(
        s.owned(
            "PATCH",
            &format!("{path}/{ENTRY}"),
            "branch",
            json!({"amount":900})
        )
        .status(),
        200
    );
    assert!(s.history()["redo"].is_null());
    let row: Value = s.get(&format!("{path}/{ENTRY}")).json().unwrap();
    assert_eq!(row["data"]["amount"], fixture["retention"]["branchRedo"]);
}
#[test]
fn creating_inline_references_is_undone_and_redone_with_the_expense() {
    let s = Server::setup();
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    let r=s.owned("POST",&path,"create",json!({"id":ENTRY,"account_id":ACCOUNT,"amount":-1234,"date":"2026-01-02","new_payee":{"id":"30000000-0000-4000-8000-000000000001","name":"Grocer"},"new_category":{"id":"30000000-0000-4000-8000-000000000002","name":"Food"}}));
    assert_eq!(r.status(), 201, "{}", r.text().unwrap());
    let r = s.step("undo", "undo");
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    assert_eq!(s.get(&format!("{path}/{ENTRY}")).status(), 404);
    let r = s.step("redo", "redo");
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    let row: Value = s.get(&format!("{path}/{ENTRY}")).json().unwrap();
    assert_eq!(row["data"]["category_name"], "Food");
    assert_eq!(row["data"]["payee_name"], "Grocer");
}
impl Server {
    fn restart(mut self) -> Self {
        self.process.kill().unwrap();
        self.process.wait().unwrap();
        let data = std::mem::replace(&mut self._data, tempfile::tempdir().unwrap());
        Self::with_data(data)
    }
}
#[test]
fn restart_receipt_owner_fences_cancellation_and_native_restore_history_epoch() {
    let s = Server::setup();
    let root = format!("/api/v1/budgets/{BUDGET}");
    let path = format!("{root}/transactions");
    assert_eq!(
        s.post(
            &path,
            "create",
            json!({"id":ENTRY,"account_id":ACCOUNT,"amount":-1234,"date":"2026-01-02"})
        )
        .status(),
        201
    );
    let checkpoint: Value = s.get(&path).json().unwrap();
    let body = json!({"snapshot":checkpoint["snapshot"],"action":"duplicate","ids":[ENTRY],"duplicate_ids":["10000000-0000-4000-8000-000000000002"]});
    let response: Value = s
        .owned("POST", &format!("{path}/bulk"), "duplicate", body.clone())
        .json()
        .unwrap();
    let s = s.restart();
    let replay: Value = s
        .owned("POST", &format!("{path}/bulk"), "duplicate", body.clone())
        .json()
        .unwrap();
    assert_eq!(replay, response);
    assert!(s.history()["undo"].is_object());
    let wrong = s
        .client
        .post(format!("{}{path}/bulk", s.url))
        .header("Idempotency-Key", "duplicate")
        .header(
            "X-Actual-History-Owner",
            "20000000-0000-4000-8000-000000000002",
        )
        .json(&body)
        .send()
        .unwrap();
    assert_eq!(wrong.status(), 409);
    let cancel = s
        .client
        .post(format!("{}{path}/bulk", s.url))
        .header("Idempotency-Key", "uncommitted")
        .header("X-Actual-History-Owner", OWNER)
        .header("X-Actual-Cancel-If-Uncommitted", "true")
        .json(&body)
        .send()
        .unwrap();
    assert_eq!(cancel.status(), 409);
    let canceled: Value = cancel.json().unwrap();
    assert_eq!(canceled["error"]["code"], "operation_canceled");
    assert_eq!(
        s.owned("POST", &format!("{path}/bulk"), "uncommitted", body.clone())
            .json::<Value>()
            .unwrap(),
        canceled
    );
    let lookup = s
        .client
        .post(format!("{}{path}/bulk", s.url))
        .header("Idempotency-Key", "duplicate")
        .header("X-Actual-History-Owner", OWNER)
        .header("X-Actual-Receipt-Only", "true")
        .json(&body)
        .send()
        .unwrap();
    assert_eq!(lookup.json::<Value>().unwrap(), response);
    let undo_id = s.history()["undo"]["id"].clone();
    let undo_body = json!({"action_id":undo_id});
    let undo: Value = s
        .owned(
            "POST",
            &format!("{root}/history/undo"),
            "undo",
            undo_body.clone(),
        )
        .json()
        .unwrap();
    let companion:Value=s.get(&format!("{root}/companion?account_id={ACCOUNT}&retain={ENTRY},10000000-0000-4000-8000-000000000002")).json().unwrap();
    assert_eq!(companion["data"]["recent"].as_array().unwrap().len(), 1);
    let backup = s.post(
        &format!("{root}/backups"),
        "backup",
        json!({"label":"History result"}),
    );
    assert_eq!(backup.status(), 201, "{}", backup.text().unwrap());
    let backup: Value = backup.json().unwrap();
    let bytes = s
        .get(&format!(
            "{root}/backups/{}/download",
            backup["data"]["id"].as_str().unwrap()
        ))
        .bytes()
        .unwrap();
    let target = Server::start();
    let stage = target
        .client
        .post(format!("{}/api/v1/restores", target.url))
        .header("Idempotency-Key", "stage")
        .header("Content-Type", "application/zip")
        .body(bytes.to_vec())
        .send()
        .unwrap();
    assert_eq!(stage.status(), 201, "{}", stage.text().unwrap());
    let stage: Value = stage.json().unwrap();
    assert_eq!(
        target
            .post(
                &format!(
                    "{root}/restores/{}/activate",
                    stage["data"]["id"].as_str().unwrap()
                ),
                "restore",
                json!({"confirm_name":"Home"})
            )
            .status(),
        200
    );
    assert!(target.history()["undo"].is_null());
    assert!(target.history()["redo"].is_null());
    // Portable financial state retains operation receipts, not engine-local history.
    let original: Value = target
        .owned("POST", &format!("{root}/history/undo"), "undo", undo_body)
        .json()
        .unwrap();
    assert_eq!(original, undo);
    assert_eq!(
        target
            .owned(
                "POST",
                &format!("{root}/history/redo"),
                "stale-redo",
                json!({"action_id":undo_id})
            )
            .status(),
        409
    );
    let rows: Value = target.get(&path).json().unwrap();
    assert_eq!(rows["balances"]["total"], -1234);
}
#[test]
fn selected_split_children_update_together_and_support_note_modes() {
    let s = Server::setup();
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    let a = "10000000-0000-4000-8000-000000000002";
    let b = "10000000-0000-4000-8000-000000000003";
    s.post(
        &path,
        "create",
        json!({"id":ENTRY,"account_id":ACCOUNT,"amount":-5000,"date":"2026-01-02"}),
    );
    let page: Value = s.get(&path).json().unwrap();
    assert_eq!(s.post(&format!("{path}/{ENTRY}/splits"),"split",json!({"snapshot":page["snapshot"],"children":[{"id":a,"amount":-2000,"notes":"Shop 12"},{"id":b,"amount":-3000,"notes":"Shop 34"}]})).status(),200);
    let page: Value = s.get(&path).json().unwrap();
    let r=s.owned("POST",&format!("{path}/bulk"),"balance-children",json!({"snapshot":page["snapshot"],"action":"update","ids":[a,b],"fields":{"amount":-2500}}));
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    let parent: Value = s.get(&format!("{path}/{ENTRY}")).json().unwrap();
    assert_eq!(parent["data"]["children"][0]["amount"], -2500);
    assert_eq!(parent["data"]["children"][1]["amount"], -2500);
    for (key, mode, value, extra, expected) in [
        ("append", "append", " paid", json!({}), "Shop 12 paid"),
        ("prepend", "prepend", "At ", json!({}), "At Shop 12 paid"),
        (
            "literal",
            "find_replace",
            "store",
            json!({"find":"Shop","use_regex":false}),
            "At store 12 paid",
        ),
        (
            "regex",
            "find_replace",
            "[$1]",
            json!({"find":"(\\d+)","use_regex":true}),
            "At store [12] paid",
        ),
    ] {
        let page: Value = s.get(&path).json().unwrap();
        let mut notes = json!({"mode":mode});
        notes
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let r=s.owned("POST",&format!("{path}/bulk"),key,json!({"snapshot":page["snapshot"],"action":"update","ids":[a],"fields":{"notes":value},"notes":notes}));
        assert_eq!(r.status(), 200, "{}", r.text().unwrap());
        let row: Value = s.get(&format!("{path}/{a}")).json().unwrap();
        assert_eq!(row["data"]["notes"], expected);
    }
    let r = s.step("undo", "undo-regex");
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    let row: Value = s.get(&format!("{path}/{a}")).json().unwrap();
    assert_eq!(row["data"]["notes"], "At store 12 paid");
}
#[test]
fn note_find_replace_matches_pinned_ecmascript_and_rejects_invalid_patterns_atomically() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/history-26.10.0.json")).unwrap();
    let s = Server::setup();
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    s.post(
        &path,
        "create",
        json!({"id":ENTRY,"account_id":ACCOUNT,"amount":-1,"date":"2026-01-02"}),
    );
    for (index, case) in fixture["noteCases"].as_array().unwrap().iter().enumerate() {
        s.client
            .patch(format!("{}{path}/{ENTRY}", s.url))
            .header("Idempotency-Key", format!("seed-{index}"))
            .json(&json!({"notes":case["text"]}))
            .send()
            .unwrap();
        let page: Value = s.get(&path).json().unwrap();
        let r=s.owned("POST",&format!("{path}/bulk"),&format!("notes-{index}"),json!({"snapshot":page["snapshot"],"action":"update","ids":[ENTRY],"fields":{"notes":case["replace"]},"notes":{"mode":"find_replace","find":case["find"],"use_regex":case["use_regex"]}}));
        assert_eq!(r.status(), 200, "{}", r.text().unwrap());
        let row: Value = s.get(&format!("{path}/{ENTRY}")).json().unwrap();
        assert_eq!(row["data"]["notes"], case["expected"], "{case}");
    }
    let page: Value = s.get(&path).json().unwrap();
    let r=s.owned("POST",&format!("{path}/bulk"),"invalid-regex",json!({"snapshot":page["snapshot"],"action":"update","ids":[ENTRY],"fields":{"notes":"changed"},"notes":{"mode":"find_replace","find":"(","use_regex":true}}));
    assert_eq!(r.status(), 400);
    let unchanged: Value = s.get(&path).json().unwrap();
    assert_eq!(unchanged["snapshot"], page["snapshot"]);
}
#[test]
fn metadata_undo_preserves_newer_transactions_but_refuses_new_references() {
    let s = Server::setup();
    let root = format!("/api/v1/budgets/{BUDGET}");
    assert_eq!(
        s.owned(
            "PATCH",
            &format!("{root}/accounts/{ACCOUNT}"),
            "rename",
            json!({"name":"Renamed"})
        )
        .status(),
        200
    );
    s.post(
        &format!("{root}/transactions"),
        "newer",
        json!({"id":ENTRY,"account_id":ACCOUNT,"amount":-3456,"date":"2026-01-02"}),
    );
    let r = s.step("undo", "undo-name");
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    let accounts: Value = s.get(&format!("{root}/accounts")).json().unwrap();
    assert_eq!(accounts["data"][0]["name"], "Spending");
    assert_eq!(accounts["data"][0]["balance"], -3456);
    let category = "30000000-0000-4000-8000-000000000001";
    assert_eq!(
        s.owned(
            "POST",
            &format!("{root}/categories"),
            "category",
            json!({"id":category,"name":"Food","is_income":false})
        )
        .status(),
        201
    );
    s.client
        .patch(format!("{}{root}/transactions/{ENTRY}", s.url))
        .header("Idempotency-Key", "reference-new-category")
        .json(&json!({"category_id":category}))
        .send()
        .unwrap();
    assert_eq!(
        s.step("undo", "cannot-remove-referenced-category").status(),
        409
    );
    let row: Value = s
        .get(&format!("{root}/transactions/{ENTRY}"))
        .json()
        .unwrap();
    assert_eq!(row["data"]["category_id"], category);
}

#[test]
fn account_close_and_forced_delete_restore_their_local_financial_changes() {
    let s = Server::setup();
    let root = format!("/api/v1/budgets/{BUDGET}");
    let account = format!("{root}/accounts/{ACCOUNT}");
    assert_eq!(
        s.owned(
            "POST",
            &format!("{account}/close"),
            "close-empty",
            json!({})
        )
        .status(),
        200
    );
    assert!(
        s.history()["undo"]["id"].is_string(),
        "baseline closeAccount wraps local changes withUndo"
    );
    assert_eq!(s.step("undo", "undo-empty-close").status(), 200);
    assert_eq!(
        s.get(&account).json::<Value>().unwrap()["data"]["deleted"],
        false
    );
    assert_eq!(
        s.post(
            &format!("{root}/transactions"),
            "expense",
            json!({"id":ENTRY,"account_id":ACCOUNT,"amount":-1234,"date":"2026-01-02"})
        )
        .status(),
        201
    );
    assert_eq!(
        s.owned(
            "DELETE",
            &account,
            "delete-account",
            json!({"confirm_name":"Spending"})
        )
        .status(),
        200
    );
    let r = s.step("undo", "undo-account-deletion");
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    let data: Value = s.get(&format!("{root}/transactions")).json().unwrap();
    assert_eq!(data["data"].as_array().unwrap().len(), 1);
    assert_eq!(data["balances"]["total"], -1234);
    assert_eq!(s.step("redo", "redo-account-deletion").status(), 200);
    assert_eq!(
        s.get(&format!("{root}/transactions"))
            .json::<Value>()
            .unwrap()["data"],
        json!([])
    );
}

#[test]
fn undo_account_group_rename_refuses_a_name_claimed_by_another_device() {
    metadata_undo_name_collision("account-groups", "Original", "Renamed");
}
#[test]
fn undo_tag_rename_refuses_a_name_claimed_by_another_device() {
    metadata_undo_name_collision("tags", "old", "new");
}
fn metadata_undo_name_collision(domain: &str, original: &str, renamed: &str) {
    let server = Server::start();
    create_budget(&server);
    let field = if domain == "tags" { "tag" } else { "name" };
    let root = format!("/api/v1/budgets/{BUDGET}");
    let a = "90000000-0000-4000-8000-000000000001";
    let b = "90000000-0000-4000-8000-000000000002";
    let owner = "90000000-0000-4000-8000-000000000003";
    let create = server.post(
        &format!("{root}/{domain}"),
        "first",
        json!({"id":a,(field):original}),
    );
    assert_eq!(create.status(), 201, "{}", create.text().unwrap());
    let rename = server
        .client
        .patch(format!("{}{root}/{domain}/{a}", server.url))
        .header("Idempotency-Key", "rename")
        .header("X-Actual-History-Owner", owner)
        .json(&json!({(field):renamed}))
        .send()
        .unwrap();
    assert_eq!(rename.status(), 200, "{}", rename.text().unwrap());
    let create = server.post(
        &format!("{root}/{domain}"),
        "second",
        json!({"id":b,(field):original}),
    );
    assert_eq!(create.status(), 201, "{}", create.text().unwrap());
    let history = || {
        server
            .client
            .get(format!("{}{root}/history", server.url))
            .header("X-Actual-History-Owner", owner)
            .send()
            .unwrap()
            .json::<Value>()
            .unwrap()
    };
    let before = history();
    let entries: Value = server.get(&format!("{root}/{domain}")).json().unwrap();
    let undo = server
        .client
        .post(format!("{}{root}/history/undo", server.url))
        .header("Idempotency-Key", "undo-collision")
        .header("X-Actual-History-Owner", owner)
        .json(&json!({"action_id":before["data"]["undo"]["id"]}))
        .send()
        .unwrap();
    assert_eq!(undo.status(), 409, "{}", undo.text().unwrap());
    assert_eq!(
        undo.json::<Value>().unwrap()["error"]["code"],
        "history_conflict"
    );
    assert_eq!(history(), before);
    assert_eq!(
        server
            .get(&format!("{root}/{domain}"))
            .json::<Value>()
            .unwrap(),
        entries
    );
}
