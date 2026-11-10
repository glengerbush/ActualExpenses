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
    data: TempDir,
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
        let mut command = Command::new(env!("CARGO_BIN_EXE_actualbudget-server"));
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
            data,
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
    #[allow(dead_code)]
    fn restart(mut self) -> Self {
        self.process.kill().unwrap();
        self.process.wait().unwrap();
        let data = std::mem::replace(&mut self.data, tempfile::tempdir().unwrap());
        Self::with_data(data)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
const ARCHIVE: &[u8] = include_bytes!("../../tests/fixtures/import-26.10.0.zip");
fn imported(server: &Server) -> (String, Value) {
    import_archive(server, ARCHIVE)
}
fn import_archive(server: &Server, archive: &[u8]) -> (String, Value) {
    let response = server
        .client
        .post(format!("{}/api/v1/imports/actual", server.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "upload")
        .body(archive.to_vec())
        .send()
        .unwrap();
    assert_eq!(response.status(), 201);
    let staged: Value = response.json().unwrap();
    let stage = &staged["data"];
    assert_eq!(
        server
            .post(
                &format!("/api/v1/imports/{}/activate", stage["id"].as_str().unwrap()),
                "activate",
                json!({})
            )
            .status(),
        201
    );
    (
        stage["budget_id"].as_str().unwrap().into(),
        stage["report"]["source"].clone(),
    )
}
#[test]
fn legacy_identity_maps_only_exact_source_and_each_field_keeps_original_order() {
    let server = Server::start();
    let (budget, source) = imported(&server);
    let mapping = server.get(&format!(
        "/api/v1/android-upgrade/sources?file_id={}&group_id={}",
        source["cloudFileId"].as_str().unwrap(),
        source["groupId"].as_str().unwrap()
    ));
    assert_eq!(mapping.status(), 200, "{}", mapping.text().unwrap());
    let candidates: Value = mapping.json().unwrap();
    assert_eq!(candidates["data"][0]["id"], budget);
    let epoch = candidates["data"][0]["epoch"].as_str().unwrap();
    let id = "04000000-0000-4000-8000-000000000002";
    let path = format!("/api/v1/budgets/{budget}/android-upgrade/operations");
    let endpoint = format!("/api/v1/budgets/{budget}/transactions/{id}");
    let patch = server
        .client
        .patch(format!("{}{endpoint}", server.url))
        .header("Idempotency-Key", "remote")
        .header(
            "X-Actual-Edit-Clock",
            "2026-11-08T23:50:11.000Z-0000-aaaaaaaaaaaaaaaa",
        )
        .json(&json!({"amount":-1100,"notes":"remote"}))
        .send()
        .unwrap();
    assert_eq!(patch.status(), 200);
    let body = json!({"source":{"file_id":source["cloudFileId"],"group_id":source["groupId"]},"epoch":epoch,"operation":{"id":"10000000-0000-4000-8000-000000000020","transactionId":id,"creates":false,"deletes":false,"createdOffline":false,"attempted":true,"changes":[{"dataset":"transactions","row":id,"column":"amount","value":-1000,"timestamp":"2026-11-08T23:50:10.000Z-0000-bbbbbbbbbbbbbbbb"},{"dataset":"transactions","row":id,"column":"notes","value":"local note","timestamp":"2026-11-08T23:50:12.000Z-0000-bbbbbbbbbbbbbbbb"}]}});
    let response = server.post(&path, "10000000-0000-4000-8000-000000000020", body.clone());
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let receipt: Value = response.json().unwrap();
    assert_eq!(receipt["data"]["id"], id);
    assert_eq!(receipt["outcome"], "reconciled");
    assert_eq!(receipt["conflicts"][0]["field"], "amount");
    let tx = server.get(&endpoint).json::<Value>().unwrap();
    assert_eq!(tx["data"]["amount"], -1100);
    assert_eq!(tx["data"]["notes"], "local note");
    assert_eq!(
        tx["data"]["field_versions"]["notes"],
        "2026-11-08T23:50:12.000Z-0000-bbbbbbbbbbbbbbbb"
    );
    let server = server.restart();
    assert_eq!(
        server
            .post(&path, "10000000-0000-4000-8000-000000000020", body)
            .json::<Value>()
            .unwrap(),
        receipt
    );
    assert_eq!(
        server
            .get(&format!(
                "/api/v1/android-upgrade/sources?file_id={}&group_id=00000000-0000-4000-8000-000000000020",
                source["cloudFileId"].as_str().unwrap()
            ))
            .json::<Value>()
            .unwrap()["data"],
        json!([])
    );
}

#[test]
fn observed_source_field_is_acknowledged_before_current_split_guard_but_unknown_edits_are_retained()
{
    let server = Server::start();
    let (budget, source) = imported(&server);
    let id = "04000000-0000-4000-8000-000000000002";
    let endpoint = format!("/api/v1/budgets/{budget}/transactions/{id}");
    let original = server.get(&endpoint).json::<Value>().unwrap();
    let epoch = server
        .get(&format!("/api/v1/budgets/{budget}/checkpoint"))
        .json::<Value>()
        .unwrap()["data"]["epoch"]
        .as_str()
        .unwrap()
        .to_owned();
    let checkpoint = server
        .get(&format!("/api/v1/budgets/{budget}/checkpoint"))
        .json::<Value>()
        .unwrap()["data"]["snapshot"]
        .clone();
    let split=server.post(&format!("{endpoint}/splits"),"split",json!({"snapshot":checkpoint,"children":[{"id":"20000000-0000-4000-8000-000000000020","amount":original["data"]["amount"]}]}));
    assert_eq!(split.status(), 200, "{}", split.text().unwrap());
    let before = server.get(&endpoint).json::<Value>().unwrap();
    let path = format!("/api/v1/budgets/{budget}/android-upgrade/operations");
    let mut body = json!({"source":{"file_id":source["cloudFileId"],"group_id":source["groupId"]},"epoch":epoch,"operation":{"id":"30000000-0000-4000-8000-000000000020","transactionId":id,"creates":false,"deletes":false,"createdOffline":false,"attempted":true,"changes":[{"dataset":"transactions","row":id,"column":"notes","value":original["data"]["notes"],"timestamp":original["data"]["field_versions"]["notes"]}]}});
    let response = server.post(&path, "30000000-0000-4000-8000-000000000020", body.clone());
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    assert_eq!(server.get(&endpoint).json::<Value>().unwrap(), before);
    body["operation"]["id"] = json!("40000000-0000-4000-8000-000000000020");
    body["operation"]["changes"][0]["value"] = json!("timestamp collision");
    assert_eq!(
        server
            .post(&path, "40000000-0000-4000-8000-000000000020", body.clone())
            .status(),
        409
    );
    body["operation"]["id"] = json!("50000000-0000-4000-8000-000000000020");
    body["operation"]["changes"][0]["timestamp"] = json!(format!(
        "{}-0000-cccccccccccccccc",
        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64
        )
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    ));
    assert_eq!(
        server
            .post(&path, "50000000-0000-4000-8000-000000000020", body)
            .status(),
        409
    );
    assert_eq!(server.get(&endpoint).json::<Value>().unwrap(), before);
}

fn legacy_creation(
    source: &Value,
    epoch: &str,
    n: u32,
    amount: i64,
    date: i64,
    order: f64,
    notes: &str,
) -> Value {
    let id = format!("20000000-0000-4000-8000-{n:012}");
    let op = format!("20000000-0000-4000-8000-{:012}", n + 1000);
    let account: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/android-order.json")).unwrap();
    let fields = vec![
        ("amount", json!(amount)),
        ("date", json!(date)),
        ("acct", account["account"].clone()),
        ("description", Value::Null),
        ("category", Value::Null),
        ("notes", json!(notes)),
        ("cleared", json!(1)),
        ("reconciled", json!(0)),
        ("tombstone", json!(0)),
        ("isParent", json!(0)),
        ("isChild", json!(0)),
        ("parent_id", Value::Null),
        ("transferred_id", Value::Null),
        ("sort_order", json!(order)),
    ];
    let changes=fields.into_iter().enumerate().map(|(i,(column,value))|json!({"dataset":"transactions","row":id,"column":column,"value":value,"timestamp":format!("2026-11-09T00:00:00.000Z-{:04X}-dddddddddddddddd",n*20+i as u32)})).collect::<Vec<_>>();
    json!({"source":{"file_id":source["cloudFileId"],"group_id":source["groupId"]},"epoch":epoch,"operation":{"id":op,"transactionId":id,"creates":true,"deletes":false,"attempted":false,"createdOffline":true,"changes":changes}})
}
#[test]
fn late_legacy_order_matches_pinned_ties_backward_capture_and_latest_ten_without_moving_native_anchors()
 {
    let server = Server::start();
    let (budget, source) = import_archive(
        &server,
        include_bytes!("../../tests/fixtures/android-order-source.zip"),
    );
    let oracle: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/android-order.json")).unwrap();
    let account = oracle["account"].as_str().unwrap();
    let epoch = server
        .get(&format!("/api/v1/budgets/{budget}/checkpoint"))
        .json::<Value>()
        .unwrap()["data"]["epoch"]
        .as_str()
        .unwrap()
        .to_owned();
    let read = || {
        server
            .get(&format!(
                "/api/v1/budgets/{budget}/transactions?account_id={account}&limit=1000"
            ))
            .json::<Value>()
            .unwrap()["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(json!(read()), oracle["snapshots"][0]["ids"]);
    let create = format!("/api/v1/budgets/{budget}/transactions");
    let id = |n| format!("20000000-0000-4000-8000-{n:012}");
    assert_eq!(server.post(&create,"anchor",json!({"id":id(4),"account_id":account,"amount":-400,"date":"2026-01-01","notes":"post-cutover anchor","cleared":true})).status(),201);
    let before_native = server
        .get(&format!("{create}/{}", id(4)))
        .json::<Value>()
        .unwrap();
    assert_eq!(json!(read()), oracle["snapshots"][1]["ids"]);
    for (index, (n, order)) in [
        (2, 2000.0),
        (5, 2000.0),
        (6, 500.0),
        (7, 3500.0),
        (9, 1500.0),
    ]
    .into_iter()
    .enumerate()
    {
        let body = legacy_creation(
            &source,
            &epoch,
            n,
            -i64::from(n) * 100,
            20260101,
            order,
            &format!("pending {n}"),
        );
        let key = body["operation"]["id"].as_str().unwrap();
        let response = server.post(
            &format!("/api/v1/budgets/{budget}/android-upgrade/operations"),
            key,
            body.clone(),
        );
        assert_eq!(response.status(), 200, "{}", response.text().unwrap());
        let row = server
            .get(&format!("{create}/{}", id(n)))
            .json::<Value>()
            .unwrap();
        for (legacy, native) in [
            ("reconciled", "reconciled"),
            ("isParent", "is_parent"),
            ("parent_id", "parent_id"),
            ("transferred_id", "transfer_id"),
        ] {
            let clock = body["operation"]["changes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|m| m["column"] == legacy)
                .unwrap()["timestamp"]
                .clone();
            assert_eq!(
                row["data"]["field_versions"][native], clock,
                "original {legacy} ordering must survive creation"
            );
        }

        assert_eq!(
            json!(read()),
            oracle["snapshots"][index + 2]["ids"],
            "pending {n}"
        );
    }
    assert_eq!(
        server
            .get(&format!("{create}/{}", id(4)))
            .json::<Value>()
            .unwrap(),
        before_native,
        "Native anchor rank/fields/versions remain unchanged"
    );
    for n in 20..32 {
        assert_eq!(server.post(&create,&format!("recent-{n}"),json!({"id":id(n),"account_id":account,"amount":-1,"date":"2026-01-02","notes":"recent","cleared":true})).status(),201);
    }
    let body = legacy_creation(
        &source,
        &epoch,
        8,
        -800,
        20260101,
        2500.0,
        "late old purchase",
    );
    let key = body["operation"]["id"].as_str().unwrap();
    assert_eq!(
        server
            .post(
                &format!("/api/v1/budgets/{budget}/android-upgrade/operations"),
                key,
                body.clone()
            )
            .status(),
        200
    );
    assert_eq!(json!(read()), oracle["snapshots"][7]["ids"]);
    let companion = server
        .get(&format!(
            "/api/v1/budgets/{budget}/companion?account_id={account}"
        ))
        .json::<Value>()
        .unwrap();
    assert_eq!(
        json!(
            companion["data"]["recent"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["id"].clone())
                .collect::<Vec<_>>()
        ),
        json!(&oracle["snapshots"][7]["ids"].as_array().unwrap()[..10])
    );
}

#[test]
fn genuine_schema17_upgrade_and_portable_archive_keep_ranks_history_epochs_and_original_receipts() {
    let f: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/android-schema17.json")).unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::write(
        data.path().join("actualbudget.sqlite3"),
        include_bytes!("../../tests/fixtures/android-schema17.sqlite3"),
    )
    .unwrap();
    let server = Server::with_data(data);
    let budget = f["metadata"]["id"].as_str().unwrap();
    let account = f["account"]["id"].as_str().unwrap();
    let root = format!("/api/v1/budgets/{budget}");
    let owned = |s: &Server,
                 method: reqwest::Method,
                 path: &str,
                 key: Option<&str>,
                 body: Option<&Value>| {
        let mut request = s.client.request(method, format!("{}{path}", s.url));
        for (k, v) in f["owner_headers"].as_object().unwrap() {
            request = request.header(k, v.as_str().unwrap());
        }
        if let Some(key) = key {
            request = request.header("Idempotency-Key", key);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        request.send().unwrap()
    };
    assert_eq!(
        server
            .get(&format!(
                "{root}/transactions?account_id={account}&limit=1000"
            ))
            .json::<Value>()
            .unwrap(),
        f["register"]
    );
    assert_eq!(
        server
            .get(&format!("{root}/checkpoint"))
            .json::<Value>()
            .unwrap(),
        f["checkpoint"]
    );
    assert_eq!(
        owned(
            &server,
            reqwest::Method::GET,
            &format!("{root}/history"),
            None,
            None
        )
        .json::<Value>()
        .unwrap(),
        f["history"]
    );
    for i in 0..2 {
        let response = owned(
            &server,
            reqwest::Method::POST,
            &format!("{root}/transactions"),
            f["creation_keys"][i].as_str(),
            Some(&f["creations"][i]),
        );
        assert_eq!(response.status(), 201);
        assert_eq!(response.json::<Value>().unwrap(), f["created"][i]);
    }
    let rowpath = format!(
        "{root}/transactions/{}",
        f["creations"][0]["id"].as_str().unwrap()
    );
    assert_eq!(
        owned(
            &server,
            reqwest::Method::PATCH,
            &rowpath,
            f["edit_key"].as_str(),
            Some(&f["edit"])
        )
        .json::<Value>()
        .unwrap(),
        f["edited"]
    );
    let undo = owned(
        &server,
        reqwest::Method::POST,
        &format!("{root}/history/undo"),
        Some("undo-after-upgrade"),
        Some(&json!({"action_id":f["history"]["data"]["undo"]["id"]})),
    );
    assert_eq!(undo.status(), 200, "{}", undo.text().unwrap());
    assert_eq!(
        server.get(&rowpath).json::<Value>().unwrap()["data"]["notes"],
        f["creations"][0]["notes"]
    );
    let db = rusqlite::Connection::open_with_flags(
        server.data.path().join("actualbudget.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        22
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM android_upgrade_order WHERE budget_id=?1",
            [budget],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    drop(db);
    let target = Server::start();
    let upload = target
        .client
        .post(format!("{}/api/v1/restores", target.url))
        .header("Idempotency-Key", "native17")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/android-schema17.zip").to_vec())
        .send()
        .unwrap();
    assert_eq!(upload.status(), 201, "{}", upload.text().unwrap());
    let stage: Value = upload.json().unwrap();
    assert_eq!(stage["data"]["report"]["schema_version"], 22);
    let activate = target.post(
        &format!(
            "{root}/restores/{}/activate",
            stage["data"]["id"].as_str().unwrap()
        ),
        "activate-native17",
        json!({"confirm_name":f["metadata"]["name"]}),
    );
    assert_eq!(activate.status(), 200, "{}", activate.text().unwrap());
    let restored = target
        .get(&format!(
            "{root}/transactions?account_id={account}&limit=1000"
        ))
        .json::<Value>()
        .unwrap();
    let mut old = f["register"].clone();
    old["snapshot"] = restored["snapshot"].clone();
    assert_eq!(
        restored, old,
        "Only the intentional restore generation/cursor changes"
    );
    assert_ne!(
        target
            .get(&format!("{root}/checkpoint"))
            .json::<Value>()
            .unwrap()["data"]["epoch"],
        f["metadata"]["epoch"]
    );
    let history = target
        .client
        .get(format!("{}{root}/history", target.url))
        .header(
            "X-Actual-History-Owner",
            f["owner_headers"]["X-Actual-History-Owner"]
                .as_str()
                .unwrap(),
        )
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap();
    assert!(history["data"]["undo"].is_null());
    assert_eq!(
        owned(
            &target,
            reqwest::Method::POST,
            &format!("{root}/transactions"),
            f["creation_keys"][0].as_str(),
            Some(&f["creations"][0])
        )
        .json::<Value>()
        .unwrap(),
        f["created"][0],
        "Original receipt survives archive epoch rotation"
    );
}
