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
    fn restart(mut self) -> Self {
        self.process.kill().unwrap();
        self.process.wait().unwrap();
        let data = std::mem::replace(&mut self._data, tempfile::tempdir().unwrap());
        Self::with_data(data)
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
fn csv_preview_matches_a_manual_expense_and_repeated_commit_does_not_duplicate_it() {
    let s = Server::start();
    let b = "25260000-0000-4000-8000-000000000001";
    let a = "25260000-0000-4000-8000-000000000002";
    let p = "25260000-0000-4000-8000-000000000003";
    let id = "25260000-0000-4000-8000-000000000004";
    let root = format!("/api/v1/budgets/{b}");
    assert_eq!(
        s.post(
            "/api/v1/budgets",
            "budget",
            json!({"id":b,"name":"CSV matching"})
        )
        .status(),
        201
    );
    assert_eq!(
        s.post(
            &format!("{root}/accounts"),
            "account",
            json!({"id":a,"name":"Checking","opening_balance":0,"opening_date":"2026-01-01"})
        )
        .status(),
        201
    );
    assert_eq!(
        s.post(
            &format!("{root}/payees"),
            "payee",
            json!({"id":p,"name":"Cafe"})
        )
        .status(),
        201
    );
    assert_eq!(s.post(&format!("{root}/transactions"),"manual",json!({"id":id,"account_id":a,"payee_id":p,"amount":-1234,"date":"2026-11-01","notes":"Manual note","cleared":false})).status(),201);
    let cp: Value = s.get(&format!("{root}/checkpoint")).json().unwrap();
    let before: Value = s.get(&format!("{root}/transactions")).json().unwrap();
    let upload = s
        .client
        .post(format!(
            "{}{root}/accounts/{a}/transaction-imports?filename=bank.csv",
            s.url
        ))
        .header("Content-Type", "text/csv")
        .header("Idempotency-Key", "csv-file")
        .body("Date,Payee,Amount,Memo\n2026-11-01,Cafe,-12.34,Bank memo\n")
        .send()
        .unwrap();
    assert_eq!(upload.status(), 201, "{}", upload.text().unwrap());
    let stage: Value = upload.json().unwrap();
    let stage = stage["data"]["id"].as_str().unwrap();
    let body = json!({"snapshot":cp["data"]["snapshot"],"mapping":{"date":"Date","payee":"Payee","amount":"Amount","notes":"Memo"},"options":{"has_header_row":true,"date_format":"yyyy mm dd"}});
    let preview = s.post(
        &format!("{root}/transaction-imports/{stage}/preview"),
        "preview",
        body.clone(),
    );
    assert_eq!(preview.status(), 200, "{}", preview.text().unwrap());
    let preview: Value = preview.json().unwrap();
    assert_eq!(preview["data"]["errors"], json!([]));
    assert_eq!(preview["data"]["rows"][0]["match_id"], id);
    assert_eq!(preview["data"]["rows"][0]["transaction"]["amount"], -1234);
    assert_eq!(
        s.get(&format!("{root}/checkpoint"))
            .json::<Value>()
            .unwrap(),
        cp
    );
    assert_eq!(
        s.get(&format!("{root}/transactions"))
            .json::<Value>()
            .unwrap(),
        before
    );
    let mut command = body;
    command["rows"] = json!([{"row":0,"action":"merge","match_id":id}]);
    command["remember"] = json!(true);
    let send = |key: &str, body: &Value| {
        s.client
            .post(format!(
                "{}{root}/transaction-imports/{stage}/commit",
                s.url
            ))
            .header("Idempotency-Key", key)
            .header(
                "X-Actual-Budget-Epoch",
                cp["data"]["epoch"].as_str().unwrap(),
            )
            .json(body)
            .send()
            .unwrap()
    };
    let result = send("commit", &command);
    assert_eq!(result.status(), 200, "{}", result.text().unwrap());
    let receipt: Value = result.json().unwrap();
    let after: Value = s.get(&format!("{root}/transactions")).json().unwrap();
    assert_eq!(after["data"].as_array().unwrap().len(), 1);
    assert_eq!(after["data"][0]["id"], id);
    assert_eq!(after["data"][0]["amount"], -1234);
    assert_eq!(after["data"][0]["notes"], "");
    assert_eq!(after["data"][0]["cleared"], true);
    assert_eq!(after["balances"]["total"], -1234);
    assert_eq!(send("commit", &command).json::<Value>().unwrap(), receipt);
    let original = command.clone();
    let fresh: Value = s.get(&format!("{root}/checkpoint")).json().unwrap();
    command["snapshot"] = fresh["data"]["snapshot"].clone();
    let repeat = send("repeat-file", &command);
    assert_eq!(repeat.status(), 200, "{}", repeat.text().unwrap());
    let final_view: Value = s.get(&format!("{root}/transactions")).json().unwrap();
    assert_eq!(final_view["data"], after["data"]);
    assert_eq!(final_view["balances"], after["balances"]);
    let checkpoint: Value = s.get(&format!("{root}/checkpoint")).json().unwrap();
    let s = s.restart();
    let replay = s
        .client
        .post(format!(
            "{}{root}/transaction-imports/{stage}/commit",
            s.url
        ))
        .header("Idempotency-Key", "commit")
        .header(
            "X-Actual-Budget-Epoch",
            cp["data"]["epoch"].as_str().unwrap(),
        )
        .json(&original)
        .send()
        .unwrap();
    assert_eq!(replay.status(), 200);
    assert_eq!(replay.json::<Value>().unwrap(), receipt);
    assert_eq!(
        s.get(&format!("{root}/checkpoint"))
            .json::<Value>()
            .unwrap(),
        checkpoint
    );
}

#[test]
fn remembered_options_keep_individual_original_clocks_files_and_receipts_across_native_restore() {
    let s = Server::start();
    let b = "25260000-0000-4000-8000-000000000101";
    let a = "25260000-0000-4000-8000-000000000102";
    let root = format!("/api/v1/budgets/{b}");
    assert_eq!(
        s.post(
            "/api/v1/budgets",
            "budget",
            json!({"id":b,"name":"Remembered import"})
        )
        .status(),
        201
    );
    assert_eq!(
        s.post(
            &format!("{root}/accounts"),
            "account",
            json!({"id":a,"name":"Checking","opening_balance":0,"opening_date":"2026-01-01"})
        )
        .status(),
        201
    );
    let bytes = "Date,Payee,Amount,Memo\n2026-11-09,Cafe,-12.34,Bank memo\n";
    let upload = s
        .client
        .post(format!(
            "{}{root}/accounts/{a}/transaction-imports?filename=bank.csv",
            s.url
        ))
        .header("Idempotency-Key", "file")
        .body(bytes)
        .send()
        .unwrap();
    assert_eq!(upload.status(), 201);
    let file = upload.json::<Value>().unwrap()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let path = format!("{root}/transaction-imports/{file}/commit");
    let settings_path = format!("{root}/accounts/{a}/transaction-import-settings?format=csv");
    let cp = s
        .get(&format!("{root}/checkpoint"))
        .json::<Value>()
        .unwrap();
    let mut command = json!({"snapshot":cp["data"]["snapshot"],"mapping":{"date":"Date","payee":"Payee","amount":"Amount","notes":"Memo","in_out":"Direction"},"options":{"has_header_row":true,"encoding":"windows-1252","import_notes":false},"rows":[{"row":0,"action":"skip"}],"remember":true});
    let original = command.clone();
    let epoch = cp["data"]["epoch"].as_str().unwrap().to_owned();
    let c1 = "2026-01-01T00:00:00.000Z-0001-2526000000000001";
    let c2 = "2026-01-01T00:00:00.000Z-0002-2526000000000001";
    let c3 = "2026-01-01T00:00:00.000Z-0003-2526000000000001";
    let send = |key: &str, clock: &str, body: &Value| {
        s.client
            .post(format!("{}{path}", s.url))
            .header("Idempotency-Key", key)
            .header("X-Actual-Budget-Epoch", &epoch)
            .header("X-Actual-Edit-Clock", clock)
            .json(body)
            .send()
            .unwrap()
    };
    let response = send("remember", c1, &command);
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let receipt = response.json::<Value>().unwrap();
    command["snapshot"] = s
        .get(&format!("{root}/checkpoint"))
        .json::<Value>()
        .unwrap()["data"]["snapshot"]
        .clone();
    command["options"]["encoding"] = json!("utf-8");
    let response = send("encoding", c3, &command);
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    command["snapshot"] = s
        .get(&format!("{root}/checkpoint"))
        .json::<Value>()
        .unwrap()["data"]["snapshot"]
        .clone();
    command["options"]["import_notes"] = json!(true);
    let response = send("notes", c2, &command);
    assert_eq!(
        response.status(),
        200,
        "Independent older field may win: {}",
        response.text().unwrap()
    );
    let settings = s.get(&settings_path).json::<Value>().unwrap();
    assert_eq!(settings["data"]["options"]["encoding"], "utf-8");
    assert_eq!(settings["data"]["options"]["import_notes"], true);
    assert_eq!(
        settings["data"]["field_versions"][format!("csv-encoding-{a}")]["value"],
        c3
    );
    assert_eq!(
        settings["data"]["field_versions"][format!("import-notes-{a}-csv")]["value"],
        c2
    );
    assert_eq!(
        settings["data"]["field_versions"][format!("csv-mappings-{a}")]["value"],
        c1
    );
    assert_eq!(settings["data"]["mapping"]["in_out"], "Direction");
    let final_cp = s
        .get(&format!("{root}/checkpoint"))
        .json::<Value>()
        .unwrap();
    command["snapshot"] = final_cp["data"]["snapshot"].clone();
    command["options"]["encoding"] = json!("windows-1252");
    assert_eq!(send("older", c2, &command).status(), 409);
    command["options"]["encoding"] = json!("utf-8");
    command["options"]["import_notes"] = json!(false);
    assert_eq!(send("equal", c2, &command).status(), 409);
    assert_eq!(s.get(&settings_path).json::<Value>().unwrap(), settings);
    assert_eq!(
        s.get(&format!("{root}/checkpoint"))
            .json::<Value>()
            .unwrap(),
        final_cp
    );
    let backup = s.post(
        &format!("{root}/backups"),
        "backup",
        json!({"label":"Import settings"}),
    );
    assert_eq!(backup.status(), 201);
    let backup = backup.json::<Value>().unwrap();
    let archive = s
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
        .header("Idempotency-Key", "native")
        .header("Content-Type", "application/zip")
        .body(archive)
        .send()
        .unwrap();
    assert_eq!(stage.status(), 201);
    let stage = stage.json::<Value>().unwrap();
    assert_eq!(stage["data"]["report"]["schema_version"], 37);
    let activate = target.post(
        &format!(
            "{root}/restores/{}/activate",
            stage["data"]["id"].as_str().unwrap()
        ),
        "activate",
        json!({"confirm_name":"Remembered import"}),
    );
    assert_eq!(activate.status(), 200, "{}", activate.text().unwrap());
    let restored = target.get(&settings_path).json::<Value>().unwrap();
    for field in ["options", "mapping", "raw_preferences", "field_versions"] {
        assert_eq!(restored["data"][field], settings["data"][field], "{field}");
    }
    assert_ne!(restored["data"]["epoch"], settings["data"]["epoch"]);
    assert_eq!(
        target
            .get(&format!("{root}/transaction-imports/{file}/original"))
            .text()
            .unwrap(),
        bytes
    );
    let replay = target
        .client
        .post(format!("{}{path}", target.url))
        .header("Idempotency-Key", "remember")
        .header("X-Actual-Budget-Epoch", epoch)
        .header("X-Actual-Edit-Clock", c1)
        .json(&original)
        .send()
        .unwrap();
    assert_eq!(replay.status(), 200);
    assert_eq!(replay.json::<Value>().unwrap(), receipt);
    assert_eq!(
        target
            .get(&format!("{root}/transactions"))
            .json::<Value>()
            .unwrap()["data"],
        json!([])
    );
}

fn manual_merge_recovery(newer_peer_note: bool) {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/transaction-merge-source.json"
    ))
    .unwrap();
    let f = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "transfer-and-manual-onbudget")
        .unwrap();
    let s = Server::start();
    let uploaded = s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "source")
        .body(
            include_bytes!(
                "../../tests/fixtures/transaction-merge-transfer-and-manual-onbudget-26.10.zip"
            )
            .to_vec(),
        )
        .send()
        .unwrap();
    assert_eq!(uploaded.status(), 201);
    let stage = uploaded.json::<Value>().unwrap();
    assert_eq!(stage["data"]["status"], "ready");
    let imported = s.post(
        &format!(
            "/api/v1/imports/{}/activate",
            stage["data"]["id"].as_str().unwrap()
        ),
        "activate",
        json!({}),
    );
    assert_eq!(imported.status(), 201);
    let root = format!(
        "/api/v1/budgets/{}",
        stage["data"]["budget_id"].as_str().unwrap()
    );
    if !newer_peer_note {
        for row in f["before"]["transactions"].as_array().unwrap() {
            let id = row["id"].as_str().unwrap();
            let cleared = s
                .client
                .patch(format!("{}{root}/transactions/{id}", s.url))
                .header("Idempotency-Key", format!("clear-{id}"))
                .json(&json!({"cleared":true}))
                .send()
                .unwrap();
            assert_eq!(cleared.status(), 200);
        }
    }
    let cp = s
        .get(&format!("{root}/checkpoint"))
        .json::<Value>()
        .unwrap();
    let epoch = cp["data"]["epoch"].as_str().unwrap();
    let owner = "25260000-0000-4000-8000-000000000111";
    let path = format!("{root}/transactions/merge");
    let body = json!({"snapshot":cp["data"]["snapshot"],"ids":[f["ids"]["a"],f["ids"]["b"]]});
    let send = |server: &Server, key: &str, body: &Value, extra: &[(&str, &str)]| {
        let mut r = server
            .client
            .post(format!("{}{path}", server.url))
            .header("Idempotency-Key", key)
            .header("X-Actual-Budget-Epoch", epoch)
            .header("X-Actual-History-Owner", owner);
        for (name, value) in extra {
            r = r.header(*name, *value);
        }
        r.json(body).send().unwrap()
    };
    let before = s
        .get(&format!("{root}/transactions"))
        .json::<Value>()
        .unwrap();
    assert_eq!(
        send(
            &s,
            "cancel-merge",
            &body,
            &[("X-Actual-Receipt-Only", "true")]
        )
        .status(),
        404
    );
    let canceled = send(
        &s,
        "cancel-merge",
        &body,
        &[("X-Actual-Cancel-If-Uncommitted", "true")],
    );
    assert_eq!(canceled.status(), 409, "{}", canceled.text().unwrap());
    let canceled = canceled.json::<Value>().unwrap();
    assert_eq!(canceled["error"]["code"], "operation_canceled");
    assert_eq!(
        send(&s, "cancel-merge", &body, &[])
            .json::<Value>()
            .unwrap(),
        canceled
    );
    assert_eq!(
        s.get(&format!("{root}/transactions"))
            .json::<Value>()
            .unwrap(),
        before
    );
    assert_eq!(
        s.get(&format!("{root}/checkpoint"))
            .json::<Value>()
            .unwrap(),
        cp
    );
    let response = send(&s, "merge", &body, &[]);
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let receipt = response.json::<Value>().unwrap();
    let keeper = f["kept"].as_str().unwrap();
    assert_eq!(receipt["data"]["keeper_id"], keeper);
    assert_eq!(
        send(&s, "merge", &body, &[("X-Actual-Receipt-Only", "true")])
            .json::<Value>()
            .unwrap(),
        receipt
    );
    let s = s.restart();
    assert_eq!(
        send(&s, "merge", &body, &[]).json::<Value>().unwrap(),
        receipt
    );
    let peer = f["after"]["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] != keeper)
        .unwrap();
    let peer_id = peer["id"].as_str().unwrap();
    let history = || {
        s.client
            .get(format!("{}{root}/history", s.url))
            .header("X-Actual-History-Owner", owner)
            .header("X-Actual-Budget-Epoch", epoch)
            .send()
            .unwrap()
            .json::<Value>()
            .unwrap()
    };
    let action = history()["data"]["undo"]["id"].as_str().unwrap().to_owned();
    if newer_peer_note {
        let remote = s
            .client
            .patch(format!("{}{root}/transactions/{peer_id}", s.url))
            .header("Idempotency-Key", "remote-note")
            .header("X-Actual-Budget-Epoch", epoch)
            .json(&json!({"notes":"Remote peer note"}))
            .send()
            .unwrap();
        assert_eq!(remote.status(), 200, "{}", remote.text().unwrap());
        let current = s
            .get(&format!("{root}/transactions"))
            .json::<Value>()
            .unwrap();
        let cp = s
            .get(&format!("{root}/checkpoint"))
            .json::<Value>()
            .unwrap();
        let original_history = history();
        let refused = s
            .client
            .post(format!("{}{root}/history/undo", s.url))
            .header("Idempotency-Key", "inverse-after-peer-edit")
            .header("X-Actual-History-Owner", owner)
            .header("X-Actual-Budget-Epoch", epoch)
            .json(&json!({"action_id":action}))
            .send()
            .unwrap();
        assert_eq!(refused.status(), 409);
        assert_eq!(
            s.get(&format!("{root}/transactions"))
                .json::<Value>()
                .unwrap(),
            current
        );
        assert_eq!(
            s.get(&format!("{root}/checkpoint"))
                .json::<Value>()
                .unwrap(),
            cp
        );
        assert_eq!(history(), original_history);
        assert_eq!(
            send(&s, "merge", &body, &[("X-Actual-Receipt-Only", "true")])
                .json::<Value>()
                .unwrap(),
            receipt
        );
        return;
    }
    for direction in ["undo", "redo"] {
        let response = s
            .client
            .post(format!("{}{root}/history/{direction}", s.url))
            .header("Idempotency-Key", format!("merge-{direction}"))
            .header("X-Actual-History-Owner", owner)
            .header("X-Actual-Budget-Epoch", epoch)
            .json(&json!({"action_id":action}))
            .send()
            .unwrap();
        assert_eq!(
            response.status(),
            200,
            "{direction}: {}",
            response.text().unwrap()
        );
        let rows = s
            .get(&format!("{root}/transactions"))
            .json::<Value>()
            .unwrap();
        for id in [keeper, peer_id] {
            assert_eq!(
                rows["data"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|row| row["id"] == id)
                    .unwrap()["notes"],
                ""
            );
        }
    }
    let remote = s
        .client
        .patch(format!("{}{root}/transactions/{keeper}", s.url))
        .header("Idempotency-Key", "remote-date-overlap")
        .header("X-Actual-Budget-Epoch", epoch)
        .json(&json!({"date":"2026-11-10"}))
        .send()
        .unwrap();
    assert_eq!(remote.status(), 200, "{}", remote.text().unwrap());
    let protected = s
        .get(&format!("{root}/transactions"))
        .json::<Value>()
        .unwrap();
    let protected_cp = s
        .get(&format!("{root}/checkpoint"))
        .json::<Value>()
        .unwrap();
    let protected_history = history();
    let refused = s
        .client
        .post(format!("{}{root}/history/undo", s.url))
        .header("Idempotency-Key", "old-merge-inverse")
        .header("X-Actual-History-Owner", owner)
        .header("X-Actual-Budget-Epoch", epoch)
        .json(&json!({"action_id":action}))
        .send()
        .unwrap();
    assert_eq!(refused.status(), 409);
    assert_eq!(
        s.get(&format!("{root}/transactions"))
            .json::<Value>()
            .unwrap(),
        protected
    );
    assert_eq!(
        s.get(&format!("{root}/checkpoint"))
            .json::<Value>()
            .unwrap(),
        protected_cp
    );
    assert_eq!(history(), protected_history);
    assert_eq!(
        send(&s, "merge", &body, &[("X-Actual-Receipt-Only", "true")])
            .json::<Value>()
            .unwrap(),
        receipt
    );
}

#[test]
fn manual_merge_cancel_restart_preserves_newer_peer_edit_and_original_receipt() {
    manual_merge_recovery(true);
}
#[test]
fn manual_merge_inverse_refuses_new_overlapping_edit() {
    manual_merge_recovery(false);
}
