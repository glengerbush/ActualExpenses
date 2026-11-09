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
fn setup(s: &Server) -> String {
    ok(s.write(
        "POST",
        "/api/v1/budgets",
        "budget",
        None,
        json!({"id":B,"name":"Locations"}),
    ));
    let root = format!("/api/v1/budgets/{B}");
    ok(s.write(
        "POST",
        &format!("{root}/accounts"),
        "account",
        None,
        json!({"id":id(2),"name":"Daily","opening_balance":0,"opening_date":"2026-01-01"}),
    ));
    ok(s.write(
        "POST",
        &format!("{root}/payees"),
        "payee",
        None,
        json!({"id":id(3),"name":"Shop"}),
    ));
    root
}
#[test]
fn multiple_branches_forgetting_replay_restart_and_companion_projection() {
    let s = Server::start();
    let root = setup(&s);
    let path = format!("{root}/payee-locations");
    let checkpoint = s.get(&format!("{root}/checkpoint"));
    let body = json!({"id":id(4),"payee_id":id(3),"latitude":40.1234567,"longitude":-74.9876543});
    let saved = ok(s.write("POST", &path, "save", None, body.clone()));
    ok(s.write(
        "POST",
        &path,
        "branch",
        None,
        json!({"id":id(5),"payee_id":id(3),"latitude":41.5,"longitude":-75.25}),
    ));
    assert_eq!(s.get(&path)["data"].as_array().unwrap().len(), 2);
    assert_eq!(
        s.get(&format!("{path}/{}", id(4)))["data"]["latitude"],
        body["latitude"]
    );
    let stale = s
        .client
        .get(format!(
            "{}{path}?snapshot={}",
            s.url,
            checkpoint["data"]["snapshot"].as_str().unwrap()
        ))
        .send()
        .unwrap();
    assert_eq!(stale.status(), 409);
    let changed = s.get(&format!(
        "{root}/changes?cursor={}",
        checkpoint["data"]["cursor"]
            .as_str()
            .or(checkpoint["data"]["snapshot"].as_str())
            .unwrap()
    ));
    assert_eq!(
        changed["data"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["entity"] == "payee_locations")
            .count(),
        2
    );
    // Existing Android expense projection remains usable, without coordinate payloads.
    ok(s.write(
        "POST",
        &format!("{root}/transactions"),
        "expense",
        None,
        json!({"id":id(6),"account_id":id(2),"payee_id":id(3),"amount":-125,"date":"2026-01-02"}),
    ));
    let view = s.get(&format!("{root}/companion?account_id={}", id(2)));
    assert_eq!(view["data"]["recent"][0]["amount"], -125);
    assert!(!view.to_string().contains("latitude"));
    let s = s.restart();
    assert_eq!(
        ok(s.write("POST", &path, "save", None, body.clone())),
        saved
    );
    assert_eq!(
        s.write(
            "POST",
            &path,
            "save",
            None,
            json!({"id":id(4),"payee_id":id(3),"latitude":1,"longitude":2})
        )
        .status(),
        409
    );
    ok(s.write(
        "PATCH",
        &format!("{path}/{}", id(4)),
        "forget",
        None,
        json!({"deleted":true}),
    ));
    assert_eq!(s.get(&path)["data"].as_array().unwrap().len(), 1);
    assert_eq!(s.get(&format!("{path}/{}", id(4)))["data"]["deleted"], true);
    assert_eq!(
        ok(s.write("POST", &path, "save", None, body.clone())),
        saved
    );
    assert_eq!(s.get(&path)["data"].as_array().unwrap().len(), 1);
    let backup = ok(s.write(
        "POST",
        &format!("{root}/backups"),
        "location-receipts",
        None,
        json!({"label":"Location receipts"}),
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
    let restored = Server::start();
    let stage = ok(restored
        .client
        .post(format!("{}/api/v1/restores", restored.url))
        .header("Idempotency-Key", "location-native")
        .header("Content-Type", "application/zip")
        .body(bytes)
        .send()
        .unwrap());
    ok(restored.write(
        "POST",
        &format!(
            "{root}/restores/{}/activate",
            stage["data"]["id"].as_str().unwrap()
        ),
        "activate",
        None,
        json!({"confirm_name":"Locations"}),
    ));
    assert_eq!(ok(restored.write("POST", &path, "save", None, body)), saved);
    assert_eq!(restored.get(&path)["data"].as_array().unwrap().len(), 1);
    assert_eq!(
        restored.get(&format!("{path}/{}", id(4)))["data"]["deleted"],
        true
    );
}
#[test]
fn validation_atomic_new_payee_cancellation_and_restore_epoch() {
    let s = Server::start();
    let root = setup(&s);
    let path = format!("{root}/payee-locations");
    for body in [
        json!({"id":id(4),"payee_id":id(3),"latitude":91,"longitude":0}),
        json!({"id":id(4),"payee_id":id(3),"latitude":0,"longitude":-181}),
        json!({"id":id(4),"payee_id":id(3),"latitude":"40","longitude":0}),
        json!({"id":id(4),"new_payee":{"id":id(5),"name":"New"},"latitude":0,"longitude":0,"unknown":true}),
    ] {
        assert_eq!(
            s.write("POST", &path, &uuid::Uuid::new_v4().to_string(), None, body)
                .status(),
            400
        );
    }
    assert_eq!(
        s.get(&format!("{root}/payees"))["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let epoch = s.get(&format!("{root}/checkpoint"))["data"]["epoch"]
        .as_str()
        .unwrap()
        .to_owned();
    let backup = ok(s.write(
        "POST",
        &format!("{root}/backups"),
        "backup",
        None,
        json!({"label":"Before"}),
    ));
    let body =
        json!({"id":id(4),"new_payee":{"id":id(5),"name":"New"},"latitude":40,"longitude":-74});
    let canceled = s
        .client
        .post(format!("{}{path}", s.url))
        .header("Idempotency-Key", "canceled")
        .header("X-Actual-Cancel-If-Uncommitted", "true")
        .json(&body)
        .send()
        .unwrap();
    assert_eq!(canceled.status(), 409);
    assert_eq!(
        canceled.json::<Value>().unwrap()["error"]["code"],
        "operation_canceled"
    );
    assert_eq!(
        s.write("POST", &path, "canceled", None, body.clone())
            .status(),
        409
    );
    let saved = ok(s.write("POST", &path, "created", None, body.clone()));
    let late_cancel = s
        .client
        .post(format!("{}{path}", s.url))
        .header("Idempotency-Key", "created")
        .header("X-Actual-Cancel-If-Uncommitted", "true")
        .json(&body)
        .send()
        .unwrap();
    assert_eq!(ok(late_cancel), saved);
    ok(s.write(
        "POST",
        &format!(
            "{root}/backups/{}/restore",
            backup["data"]["id"].as_str().unwrap()
        ),
        "restore",
        None,
        json!({"confirm_name":"Locations"}),
    ));
    let stale = s
        .client
        .post(format!("{}{path}", s.url))
        .header("Idempotency-Key", "stale")
        .header("X-Actual-Budget-Epoch", &epoch)
        .json(&body)
        .send()
        .unwrap();
    assert_eq!(stale.status(), 409);
    assert_eq!(s.get(&path)["data"], json!([]));
    assert_eq!(ok(s.write("POST", &path, "created", None, body)), saved);
    assert_eq!(s.get(&path)["data"], json!([]));
}
#[test]
fn pinned_source_conversion_native_archive_and_alias_retention() {
    let s = Server::start();
    let source = include_bytes!("../../tests/fixtures/payee-locations-import.zip");
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/payee-locations-import.json"
    ))
    .unwrap();
    let staged = ok(s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Idempotency-Key", "source")
        .header("Content-Type", "application/zip")
        .body(source.to_vec())
        .send()
        .unwrap());
    assert_eq!(staged["data"]["status"], "ready", "{staged}");
    let budget = ok(s.write(
        "POST",
        &format!(
            "/api/v1/imports/{}/activate",
            staged["data"]["id"].as_str().unwrap()
        ),
        "activate",
        None,
        json!({}),
    ));
    let root = format!("/api/v1/budgets/{}", budget["data"]["id"].as_str().unwrap());
    let path = format!("{root}/payee-locations");
    let rows = s.get(&format!("{path}?include_deleted=true"));
    assert_eq!(
        rows["data"].as_array().unwrap().len(),
        fixture["locations"].as_array().unwrap().len()
    );
    for loc in fixture["locations"].as_array().unwrap() {
        let row = rows["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == loc["id"])
            .unwrap();
        assert_eq!(row["latitude"].as_f64(), loc["latitude"].as_f64());
        assert_eq!(row["longitude"].as_f64(), loc["longitude"].as_f64());
        assert_eq!(row["payee_id"], loc["payee_id"]);
        assert!(
            row["field_versions"].as_object().unwrap().is_empty(),
            "Pinned export strips source clocks; do not invent migration-time ordering"
        );
    }
    let deleted_id = fixture["locations"][0]["id"].as_str().unwrap();
    assert_eq!(
        s.get(&format!("{path}/{deleted_id}"))["data"]["deleted"],
        true
    );
    // Aliases retain source coordinates; no reassignment to merge target.
    let merged = fixture["payees"][0].as_str().unwrap();
    assert_eq!(
        s.get(&format!("{root}/payees/{merged}"))["data"]["deleted"],
        true
    );
    let backup = ok(s.write(
        "POST",
        &format!("{root}/backups"),
        "native",
        None,
        json!({"label":"Locations"}),
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
    let restored = Server::start();
    let stage = ok(restored
        .client
        .post(format!("{}/api/v1/restores", restored.url))
        .header("Idempotency-Key", "native")
        .header("Content-Type", "application/zip")
        .body(bytes)
        .send()
        .unwrap());
    ok(restored.write(
        "POST",
        &format!(
            "{root}/restores/{}/activate",
            stage["data"]["id"].as_str().unwrap()
        ),
        "activate",
        None,
        json!({"confirm_name":budget["data"]["name"]}),
    ));
    assert_eq!(restored.get(&format!("{path}?include_deleted=true")), rows);
    let duplicate = ok(s.write(
        "POST",
        &format!("{root}/duplicate"),
        "copy",
        None,
        json!({"id":id(900),"name":"Copy"}),
    ));
    assert_eq!(duplicate["data"]["id"], id(900));
    assert_eq!(
        s.get(&format!(
            "/api/v1/budgets/{}/payee-locations?include_deleted=true",
            id(900)
        ))["data"],
        rows["data"]
    );
}
#[test]
fn genuine_schema11_database_and_archive_upgrade_preserve_receipts_and_unblock_source() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/locations-schema11.json")).unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::write(
        data.path().join("actualbudget.sqlite3"),
        include_bytes!("../../tests/fixtures/locations-schema11.sqlite3"),
    )
    .unwrap();
    let s = Server::open(data);
    let budget = fixture["budget"]["data"]["id"].as_str().unwrap();
    let root = format!("/api/v1/budgets/{budget}");
    assert_eq!(
        ok(s.write(
            "POST",
            &format!("{root}/accounts"),
            "native-account",
            None,
            fixture["account_body"].clone()
        )),
        fixture["account"]
    );
    assert_eq!(s.get(&format!("{root}/payee-locations"))["data"], json!([]));
    let source = include_bytes!("../../tests/fixtures/payee-locations-import.zip");
    let replay = ok(s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Idempotency-Key", "blocked-locations")
        .header("Content-Type", "application/zip")
        .body(source.to_vec())
        .send()
        .unwrap());
    assert_eq!(replay, fixture["stage"]);
    let stage_id = fixture["stage"]["data"]["id"].as_str().unwrap();
    let refreshed = s.get(&format!("/api/v1/imports/{stage_id}"));
    assert_eq!(refreshed["data"]["status"], "ready");
    let activated = ok(s.write(
        "POST",
        &format!("/api/v1/imports/{stage_id}/activate"),
        "activate",
        None,
        json!({}),
    ));
    let imported = activated["data"]["id"].as_str().unwrap();
    assert_eq!(
        s.get(&format!(
            "/api/v1/budgets/{imported}/payee-locations?include_deleted=true"
        ))["data"]
            .as_array()
            .unwrap()
            .len(),
        15
    );
    let restored = Server::start();
    let staged = ok(restored
        .client
        .post(format!("{}/api/v1/restores", restored.url))
        .header("Idempotency-Key", "legacy-native")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/locations-schema11.zip").to_vec())
        .send()
        .unwrap());
    ok(restored.write(
        "POST",
        &format!(
            "{root}/restores/{}/activate",
            staged["data"]["id"].as_str().unwrap()
        ),
        "restore",
        None,
        json!({"confirm_name":"Schema11 native"}),
    ));
    assert_eq!(
        restored.get(&format!("{root}/accounts"))["data"][0]["balance"],
        12345
    );
    assert_eq!(
        restored.get(&format!("{root}/payee-locations"))["data"],
        json!([])
    );
    assert_eq!(
        ok(restored.write(
            "POST",
            &format!("{root}/accounts"),
            "native-account",
            None,
            fixture["account_body"].clone()
        )),
        fixture["account"]
    );
}

// Robustness fixture: a genuine pinned export with one preserved source clock,
// or one invalid coordinate. Not claimed as an additional pinned oracle result.
fn source_with_clock(invalid_coordinate: bool) -> Vec<u8> {
    use std::io::{Cursor, Read, Write};
    let mut zip = zip::ZipArchive::new(Cursor::new(include_bytes!(
        "../../tests/fixtures/payee-locations-import.zip"
    )))
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db.sqlite");
    let mut database = vec![];
    zip.by_name("db.sqlite")
        .unwrap()
        .read_to_end(&mut database)
        .unwrap();
    std::fs::write(&path, database).unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/payee-locations-import.json"
    ))
    .unwrap();
    let location = fixture["locations"][1]["id"].as_str().unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    if invalid_coordinate {
        db.execute(
            "UPDATE payee_locations SET latitude=91 WHERE id=?1",
            [location],
        )
        .unwrap();
    } else {
        db.execute("INSERT INTO messages_crdt(timestamp,dataset,row,column,value) VALUES('2026-01-02T00:00:00.000Z-0000-0000000000000001','payee_locations',?1,'tombstone','N:0')",[location]).unwrap();
    }
    drop(db);
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for name in ["db.sqlite", "metadata.json"] {
        output.start_file(name, options).unwrap();
        let bytes = if name == "db.sqlite" {
            std::fs::read(&path).unwrap()
        } else {
            let mut bytes = vec![];
            zip.by_name(name).unwrap().read_to_end(&mut bytes).unwrap();
            bytes
        };
        output.write_all(&bytes).unwrap();
    }
    output.finish().unwrap().into_inner()
}
#[test]
fn source_clocks_preserve_forget_order_and_invalid_coordinates_reject_before_activation() {
    let s = Server::start();
    let staged = ok(s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Idempotency-Key", "clock-source")
        .header("Content-Type", "application/zip")
        .body(source_with_clock(false))
        .send()
        .unwrap());
    assert_eq!(staged["data"]["status"], "ready", "{staged}");
    let activated = ok(s.write(
        "POST",
        &format!(
            "/api/v1/imports/{}/activate",
            staged["data"]["id"].as_str().unwrap()
        ),
        "activate-clock",
        None,
        json!({}),
    ));
    let b = activated["data"]["id"].as_str().unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/payee-locations-import.json"
    ))
    .unwrap();
    let id = fixture["locations"][1]["id"].as_str().unwrap();
    let path = format!("/api/v1/budgets/{b}/payee-locations/{id}");
    assert_eq!(
        s.get(&path)["data"]["field_versions"]["tombstone"],
        "2026-01-02T00:00:00.000Z-0000-0000000000000001"
    );
    let lost = ok(s.write(
        "PATCH",
        &path,
        "old-forget",
        Some("2026-01-01T00:00:00.000Z-0000-0000000000000001"),
        json!({"deleted":true}),
    ));
    assert_eq!(lost["data"]["deleted"], false);
    assert_eq!(lost["conflicts"][0]["field"], "tombstone");
    ok(s.write(
        "PATCH",
        &path,
        "new-forget",
        Some("2026-01-03T00:00:00.000Z-0000-0000000000000001"),
        json!({"deleted":true}),
    ));
    assert_eq!(s.get(&path)["data"]["deleted"], true);
    let invalid = s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Idempotency-Key", "bad-source")
        .header("Content-Type", "application/zip")
        .body(source_with_clock(true))
        .send()
        .unwrap();
    assert_eq!(invalid.status(), 422);
    assert_eq!(
        s.get("/api/v1/budgets")["data"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn genuine_schema13_upgrade_retains_protected_transfer_state_and_original_receipts() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/locations-schema13.json")).unwrap();
    let budget = fixture["budget"]["data"]["id"].as_str().unwrap();
    let root = format!("/api/v1/budgets/{budget}");
    let account = fixture["account"]["id"].as_str().unwrap();
    for from_archive in [false, true] {
        let data = tempfile::tempdir().unwrap();
        if !from_archive {
            std::fs::write(
                data.path().join("actualbudget.sqlite3"),
                include_bytes!("../../tests/fixtures/locations-schema13.sqlite3"),
            )
            .unwrap();
        }
        let s = Server::open(data);
        if from_archive {
            let stage = ok(s
                .client
                .post(format!("{}/api/v1/restores", s.url))
                .header("Idempotency-Key", "schema13")
                .header("Content-Type", "application/zip")
                .body(include_bytes!("../../tests/fixtures/locations-schema13.zip").to_vec())
                .send()
                .unwrap());
            assert_eq!(stage["data"]["report"]["schema_version"], 15);
            ok(s.write(
                "POST",
                &format!(
                    "{root}/restores/{}/activate",
                    stage["data"]["id"].as_str().unwrap()
                ),
                "activate13",
                None,
                json!({"confirm_name":fixture["budget"]["data"]["name"]}),
            ));
        }
        assert_eq!(
            s.get(&format!("{root}/accounts/{account}"))["data"],
            fixture["account"]
        );
        for row in fixture["rows"].as_array().unwrap() {
            assert_eq!(
                s.get(&format!(
                    "{root}/transactions/{}",
                    row["id"].as_str().unwrap()
                ))["data"],
                *row
            );
        }
        assert_eq!(
            ok(s.write(
                "POST",
                &format!("{root}/transfers"),
                "transfer13",
                None,
                fixture["transfer_body"].clone()
            )),
            fixture["transfer"]
        );
        assert_eq!(
            ok(s.write(
                "POST",
                &format!("{root}/accounts/{account}/reconciliation"),
                "reconcile13",
                None,
                fixture["reconciliation_body"].clone()
            )),
            fixture["reconciliation"]
        );
        assert_eq!(s.get(&format!("{root}/payee-locations"))["data"], json!([]));
        assert_eq!(
            s.get(&format!("{root}/companion?account_id={account}"))["data"]["recent"],
            json!([])
        );
        if !from_archive {
            let replay = ok(s
                .client
                .post(format!("{}/api/v1/imports/actual", s.url))
                .header("Idempotency-Key", "blocked-locations13")
                .header("Content-Type", "application/zip")
                .body(include_bytes!("../../tests/fixtures/payee-locations-import.zip").to_vec())
                .send()
                .unwrap());
            assert_eq!(replay, fixture["stage"]);
            assert_eq!(
                s.get(&format!(
                    "/api/v1/imports/{}",
                    fixture["stage"]["data"]["id"].as_str().unwrap()
                ))["data"]["status"],
                "ready"
            );
        }
    }
}
