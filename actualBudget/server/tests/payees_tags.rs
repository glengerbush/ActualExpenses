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
fn payees_rename_merge_remove_preserve_history_and_receipts() {
    let s = Server::start();
    let root = format!("/api/v1/budgets/{B}");
    ok(s.write(
        "POST",
        "/api/v1/budgets",
        "budget",
        None,
        json!({"id":B,"name":"Payees"}),
    ));
    ok(s.write("POST",&format!("{root}/accounts"),"account",None,json!({"id":id(2),"name":"Spending","off_budget":false,"opening_balance":0,"opening_date":"2026-01-01"})));
    for (n, name) in [(3, "CORNER SHOP 001"), (4, "Corner shop")] {
        ok(s.write(
            "POST",
            &format!("{root}/payees"),
            &format!("p{n}"),
            None,
            json!({"id":id(n),"name":name}),
        ));
    }
    ok(s.write("POST",&format!("{root}/transactions"),"tx",None,json!({"id":id(5),"account_id":id(2),"payee_id":id(4),"amount":-1250,"date":"2026-01-02","notes":"#Food ##Food #Foodie #food #Food#trip"})));
    let renamed = ok(s.write(
        "PATCH",
        &format!("{root}/payees/{}", id(3)),
        "rename",
        None,
        json!({"name":"Corner Shop","favorite":true}),
    ));
    assert_eq!(renamed["data"]["name"], "Corner Shop");
    let body = json!({"action":"merge","ids":[id(4)],"target_id":id(3)});
    let merged = ok(s.write(
        "POST",
        &format!("{root}/payees/batch"),
        "merge",
        None,
        body.clone(),
    ));
    assert_eq!(
        ok(s.write("POST", &format!("{root}/payees/batch"), "merge", None, body)),
        merged
    );
    let tx = s.get(&format!("{root}/transactions/{}", id(5)));
    assert_eq!(tx["data"]["payee_id"], id(3));
    assert_eq!(tx["data"]["amount"], -1250);
    assert_eq!(
        s.get(&format!("{root}/payees"))["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    ok(s.write(
        "POST",
        &format!("{root}/payees/batch"),
        "remove",
        None,
        json!({"action":"remove","ids":[id(3)]}),
    ));
    assert_eq!(
        s.get(&format!("{root}/transactions/{}", id(5)))["data"]["payee_id"],
        Value::Null
    );
    assert!(
        s.get(&format!("{root}/payees"))["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn tags_rename_exact_tokens_visibility_filter_and_archive_round_trip() {
    let s = Server::start();
    let root = format!("/api/v1/budgets/{B}");
    ok(s.write(
        "POST",
        "/api/v1/budgets",
        "budget",
        None,
        json!({"id":B,"name":"Tags"}),
    ));
    ok(s.write("POST",&format!("{root}/accounts"),"account",None,json!({"id":id(2),"name":"Spending","off_budget":false,"opening_balance":0,"opening_date":"2026-01-01"})));
    for (n, notes) in [
        (5, "#Food ##Food #Foodie #food #Food#trip"),
        (6, "##Food #Foodie #food"),
    ] {
        ok(s.write(
            "POST",
            &format!("{root}/transactions"),
            &format!("tx{n}"),
            None,
            json!({"id":id(n),"account_id":id(2),"amount":-1250,"date":"2026-01-02","notes":notes}),
        ));
    }
    ok(s.write(
        "POST",
        &format!("{root}/tags"),
        "tag",
        None,
        json!({"id":id(3),"tag":"Food","color":"#ff0000","description":"Meals"}),
    ));
    assert_eq!(
        s.get(&format!("{root}/transactions?tag=Food"))["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    ok(s.write(
        "PATCH",
        &format!("{root}/tags/{}", id(3)),
        "rename",
        None,
        json!({"tag":"Meals","hidden":true}),
    ));
    assert_eq!(
        s.get(&format!("{root}/transactions/{}", id(5)))["data"]["notes"],
        "#Meals ##Food #Foodie #food #Meals#trip"
    );
    assert!(
        s.get(&format!("{root}/tags"))["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        s.get(&format!("{root}/tags?include_hidden=true"))["data"][0]["tag"],
        "Meals"
    );
    assert_eq!(
        s.get(&format!("{root}/transactions?tag=Meals"))["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    ok(s.write(
        "POST",
        &format!("{root}/tags/batch"),
        "remove-tag",
        None,
        json!({"action":"remove","ids":[id(3)]}),
    ));
    let recreated = ok(s.write(
        "POST",
        &format!("{root}/tags"),
        "recreate-tag",
        None,
        json!({"id":id(9),"tag":"Meals"}),
    ));
    assert_eq!(recreated["data"]["id"], id(3));
    let s = s.restart();
    assert_eq!(
        s.get(&format!("{root}/tags?include_hidden=true"))["data"][0]["hidden"],
        true
    );
    let backup = ok(s.write(
        "POST",
        &format!("{root}/backups"),
        "backup",
        None,
        json!({"label":"Tags"}),
    ));
    let changed = ok(s.write(
        "PATCH",
        &format!("{root}/tags/{}", id(3)),
        "post-backup-tag",
        None,
        json!({"hidden":false}),
    ));
    ok(s.write(
        "POST",
        &format!(
            "{root}/backups/{}/restore",
            backup["data"]["id"].as_str().unwrap()
        ),
        "restore-existing",
        None,
        json!({"confirm_name":"Tags"}),
    ));
    assert_eq!(
        s.get(&format!("{root}/tags?include_hidden=true"))["data"][0]["hidden"],
        true
    );
    assert_eq!(
        ok(s.write(
            "PATCH",
            &format!("{root}/tags/{}", id(3)),
            "post-backup-tag",
            None,
            json!({"hidden":false})
        )),
        changed
    );
    let archive = s
        .client
        .get(format!(
            "{}{root}/backups/{}/download",
            s.url,
            backup["data"]["id"].as_str().unwrap()
        ))
        .send()
        .unwrap();
    assert!(archive.status().is_success(), "{}", archive.text().unwrap());
    let restored = Server::start();
    let staged = ok(restored
        .client
        .post(format!("{}/api/v1/restores", restored.url))
        .header("Idempotency-Key", "native")
        .header("Content-Type", "application/zip")
        .body(archive.bytes().unwrap())
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
        json!({"confirm_name":"Tags"}),
    ));
    assert_eq!(
        restored.get(&format!("{root}/tags?include_hidden=true")),
        s.get(&format!("{root}/tags?include_hidden=true"))
    );
    assert_eq!(
        restored.get(&format!("{root}/transactions/{}", id(5)))["data"],
        s.get(&format!("{root}/transactions/{}", id(5)))["data"]
    );
    assert_eq!(
        restored
            .write(
                "PATCH",
                &format!("{root}/tags/{}", id(3)),
                "retired",
                None,
                json!({"hidden":false})
            )
            .status(),
        409
    );
    let replay = ok(restored.write(
        "POST",
        &format!("{root}/tags"),
        "recreate-tag",
        None,
        json!({"id":id(9),"tag":"Meals"}),
    ));
    assert_eq!(replay, recreated);
}
#[test]
fn pinned_import_converts_removed_payees_aliases_imported_text_and_tags() {
    let s = Server::start();
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/payees-tags-import.json")).unwrap();
    let bytes = include_bytes!("../../tests/fixtures/payees-tags-import.zip");
    let staged = s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Idempotency-Key", "import")
        .header("Content-Type", "application/zip")
        .body(bytes.as_slice())
        .send()
        .unwrap();
    let staged = ok(staged);
    assert_eq!(staged["data"]["status"], "ready", "{staged}");
    let imported = ok(s.write(
        "POST",
        &format!(
            "/api/v1/imports/{}/activate",
            staged["data"]["id"].as_str().unwrap()
        ),
        "activate",
        None,
        json!({}),
    ));
    let b = imported["data"]["id"].as_str().unwrap();
    let root = format!("/api/v1/budgets/{b}");
    let tx = s.get(&format!(
        "{root}/transactions/11000000-0000-4000-8000-000000000001"
    ));
    assert_eq!(tx["data"]["imported_payee"], "CARD CORNER 001");
    assert_eq!(tx["data"]["payee_id"], Value::Null);
    assert_eq!(tx["data"]["amount"], -1250);
    let payees = s.get(&format!(
        "{root}/payees?include_deleted=true&include_transfers=true"
    ));
    assert_eq!(payees["data"].as_array().unwrap().len(), 3);
    let transfer = payees["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == fixture["transfer"])
        .unwrap();
    assert_eq!(transfer["transfer_account_id"], fixture["account"]);
    let alias = payees["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == fixture["second"])
        .unwrap();
    assert_eq!(alias["merged_into"], fixture["first"]);
    assert_eq!(
        s.get(&format!("{root}/tags?include_hidden=true"))["data"][0]["color"],
        "#ff0000"
    );
    let companion = s.get(&format!(
        "{root}/companion?account_id={}",
        fixture["account"].as_str().unwrap()
    ));
    assert_eq!(companion["data"]["recent"].as_array().unwrap().len(), 2);
}

#[test]
fn atomic_batches_clock_order_cancellation_and_cursor_cover_public_failures() {
    let s = Server::start();
    let root = format!("/api/v1/budgets/{B}");
    ok(s.write(
        "POST",
        "/api/v1/budgets",
        "b",
        None,
        json!({"id":B,"name":"Failures"}),
    ));
    let first = "2026-01-01T00:00:00.000Z-0000-0000000000000001";
    let second = "2026-01-02T00:00:00.000Z-0000-0000000000000001";
    let third = "2026-01-03T00:00:00.000Z-0000-0000000000000001";
    for n in 2..5 {
        ok(s.write(
            "POST",
            &format!("{root}/payees"),
            &format!("p{n}"),
            Some(first),
            json!({"id":id(n),"name":format!("Payee {n}")}),
        ));
    }
    ok(s.write(
        "POST",
        &format!("{root}/accounts"),
        "inline-account",
        None,
        json!({"id":id(8),"name":"Inline","opening_balance":0,"opening_date":"2026-01-01"}),
    ));
    ok(s.write("POST",&format!("{root}/transactions"),"inline-payee",Some(third),json!({"id":id(9),"account_id":id(8),"amount":-1,"date":"2026-01-01","new_payee":{"id":id(10),"name":"Created with transaction"}})));
    let earlier = ok(s.write(
        "PATCH",
        &format!("{root}/payees/{}", id(10)),
        "earlier-name",
        Some(second),
        json!({"name":"Older name"}),
    ));
    assert_eq!(earlier["data"]["name"], "Created with transaction");
    let cursor = s.get(&format!("{root}/checkpoint"))["data"]["snapshot"]
        .as_str()
        .unwrap()
        .to_owned();
    let latest = ok(s.write(
        "PATCH",
        &format!("{root}/payees/{}", id(2)),
        "latest",
        Some(third),
        json!({"name":"Newest"}),
    ));
    let old = ok(s.write(
        "PATCH",
        &format!("{root}/payees/{}", id(2)),
        "older",
        Some(second),
        json!({"name":"Older","favorite":true}),
    ));
    assert_eq!(old["data"]["name"], "Newest");
    assert_eq!(old["data"]["favorite"], true);
    assert_eq!(old["conflicts"][0]["field"], "name");
    assert_eq!(
        s.write(
            "POST",
            &format!("{root}/payees/batch"),
            "atomic",
            Some(second),
            json!({"action":"remove","ids":[id(3),id(2)]})
        )
        .status(),
        409
    );
    assert_eq!(
        s.get(&format!("{root}/payees/{}", id(3)))["data"]["deleted"],
        false
    );
    assert_eq!(
        s.write(
            "PATCH",
            &format!("{root}/payees/{}", id(2)),
            "latest",
            Some(second),
            json!({"name":"Newest"})
        )
        .status(),
        409
    );
    let canceled = s
        .client
        .patch(format!("{}{root}/payees/{}", s.url, id(3)))
        .header("Idempotency-Key", "cancel")
        .header("X-Actual-Edit-Clock", third)
        .header("X-Actual-Cancel-If-Uncommitted", "true")
        .json(&json!({"name":"Unsent"}))
        .send()
        .unwrap();
    assert_eq!(canceled.status(), 409);
    assert_eq!(
        canceled.json::<Value>().unwrap()["error"]["code"],
        "operation_canceled"
    );
    let late = s.write(
        "PATCH",
        &format!("{root}/payees/{}", id(3)),
        "cancel",
        Some(third),
        json!({"name":"Unsent"}),
    );
    assert_eq!(late.status(), 409);
    assert_eq!(
        late.json::<Value>().unwrap()["error"]["code"],
        "operation_canceled"
    );
    assert_eq!(
        ok(s.write(
            "PATCH",
            &format!("{root}/payees/{}", id(2)),
            "latest",
            Some(third),
            json!({"name":"Newest"})
        )),
        latest
    );
    let changes = s.get(&format!("{root}/changes?cursor={cursor}"));
    assert!(
        changes["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["entity"] == "payees" && row["data"]["name"] == "Newest")
    );
    assert_eq!(
        s.write(
            "POST",
            &format!("{root}/payees/batch"),
            "badtarget",
            None,
            json!({"action":"merge","ids":[id(3)],"target_id":id(3)})
        )
        .status(),
        400
    );
}
#[test]
fn genuine_schema7_database_and_archive_backfill_preserve_native_edits_and_receipts() {
    let expected: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/payees-schema7.json")).unwrap();
    let b = expected["budget_id"].as_str().unwrap();
    let root = format!("/api/v1/budgets/{b}");
    for native in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        if !native {
            std::fs::write(
                directory.path().join("actualbudget.sqlite3"),
                include_bytes!("../../tests/fixtures/payees-schema7.sqlite3"),
            )
            .unwrap();
        }
        let s = Server::open(directory);
        if native {
            let stage = ok(s
                .client
                .post(format!("{}/api/v1/restores", s.url))
                .header("Idempotency-Key", "stage7")
                .header("Content-Type", "application/zip")
                .body(include_bytes!("../../tests/fixtures/payees-schema7-native.zip").as_slice())
                .send()
                .unwrap());
            ok(s.write(
                "POST",
                &format!(
                    "{root}/restores/{}/activate",
                    stage["data"]["id"].as_str().unwrap()
                ),
                "restore7",
                None,
                json!({"confirm_name":expected["activation_receipt"]["data"]["name"]}),
            ));
        }
        let tx = s.get(&format!(
            "{root}/transactions/{}",
            expected["transaction_id"].as_str().unwrap()
        ));
        for (key, value) in expected["transaction"]["data"].as_object().unwrap() {
            assert_eq!(&tx["data"][key], value, "{key}");
        }
        assert_eq!(s.get(&format!("{root}/accounts")), expected["accounts"]);
        assert_eq!(
            ok(s.write(
                "PATCH",
                &format!(
                    "{root}/transactions/{}",
                    expected["transaction_id"].as_str().unwrap()
                ),
                "native-edit7",
                expected["clock"].as_str(),
                expected["patch"].clone()
            )),
            expected["receipt"]
        );
        assert_eq!(
            ok(s.write(
                "POST",
                &format!("{root}/payees"),
                "native-payee7",
                None,
                json!({"id":expected["payee_id"],"name":"Native payee after import"})
            )),
            expected["payee_receipt"]
        );
        assert!(
            s.get(&format!("{root}/payees?include_transfers=true"))["data"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| !p["transfer_account_id"].is_null())
        );
        assert!(
            s.get(&format!("{root}/tags"))["data"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}
#[test]
fn pinned_tag_rename_oracle_matches_public_note_updates_including_unicode_boundaries() {
    let s = Server::start();
    let oracle: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/payees-tags.json")).unwrap();
    for (n, case) in oracle["tag_renames"].as_array().unwrap().iter().enumerate() {
        let b = id(100 + n);
        let root = format!("/api/v1/budgets/{b}");
        ok(s.write(
            "POST",
            "/api/v1/budgets",
            &format!("b{n}"),
            None,
            json!({"id":b,"name":"Tags"}),
        ));
        ok(s.write(
            "POST",
            &format!("{root}/accounts"),
            "a",
            None,
            json!({"id":id(2),"name":"Spending","opening_balance":0,"opening_date":"2026-01-01"}),
        ));
        ok(s.write("POST",&format!("{root}/transactions"),"t",None,json!({"id":id(3),"account_id":id(2),"amount":-1250,"date":"2026-01-02","notes":case["notes"]})));
        ok(s.write(
            "POST",
            &format!("{root}/tags"),
            "tag",
            None,
            json!({"id":id(4),"tag":case["old"]}),
        ));
        ok(s.write(
            "PATCH",
            &format!("{root}/tags/{}", id(4)),
            "rename",
            None,
            json!({"tag":case["next"]}),
        ));
        let tx = s.get(&format!("{root}/transactions/{}", id(3)));
        assert_eq!(tx["data"]["notes"], case["expected"], "{case}");
        assert_eq!(
            tx["data"]["amount"],
            oracle["snapshots"][0]["transactions"][0]["amount"]
        );
    }
}
#[test]
fn successive_merges_keep_original_queued_identity_usable_after_140_destinations() {
    let s = Server::start();
    let root = format!("/api/v1/budgets/{B}");
    ok(s.write(
        "POST",
        "/api/v1/budgets",
        "b",
        None,
        json!({"id":B,"name":"Long merge history"}),
    ));
    ok(s.write(
        "POST",
        &format!("{root}/accounts"),
        "a",
        None,
        json!({"id":id(2),"name":"Spending","opening_balance":0,"opening_date":"2026-01-01"}),
    ));
    for n in 100..=240 {
        ok(s.write(
            "POST",
            &format!("{root}/payees"),
            &format!("p{n}"),
            None,
            json!({"id":id(n),"name":format!("Payee {n}")}),
        ));
        if n > 100 {
            ok(s.write(
                "POST",
                &format!("{root}/payees/batch"),
                &format!("m{n}"),
                None,
                json!({"action":"merge","ids":[id(n-1)],"target_id":id(n)}),
            ));
        }
    }
    let tx=ok(s.write("POST",&format!("{root}/transactions"),"queued-original",None,json!({"id":id(3),"account_id":id(2),"payee_id":id(100),"amount":-1250,"date":"2026-01-01"})));
    assert_eq!(tx["data"]["payee_id"], id(240));
    assert_eq!(tx["data"]["amount"], -1250);
    assert_eq!(
        s.get(&format!("{root}/payees"))["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
/// Synthetic source stress fixture (not a claim that Actual public merges emit chains).
/// Only input ZIP bytes are constructed here; all replacement observations use HTTP.
fn source_with_payee_chain(cycle: bool) -> Vec<u8> {
    use std::io::{Cursor, Read, Write};
    let mut original = zip::ZipArchive::new(Cursor::new(include_bytes!(
        "../../tests/fixtures/import-26.10.0.zip"
    )))
    .unwrap();
    let mut bytes = vec![];
    original
        .by_name("db.sqlite")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.sqlite");
    std::fs::write(&path, bytes).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let target: String = db
        .query_row("SELECT id FROM payees WHERE name='Fixture shop'", [], |r| {
            r.get(0)
        })
        .unwrap();
    for n in 1000..1140 {
        db.execute(
            "INSERT INTO payees(id,name,tombstone) VALUES(?1,?2,1)",
            rusqlite::params![id(n), format!("Legacy alias {n}")],
        )
        .unwrap();
        let next = if n == 1139 {
            if cycle { id(1000) } else { target.clone() }
        } else {
            id(n + 1)
        };
        db.execute(
            "INSERT INTO payee_mapping(id,targetId) VALUES(?1,?2)",
            rusqlite::params![id(n), next],
        )
        .unwrap();
    }
    db.execute(
        "UPDATE transactions SET description=?1 WHERE id='04000000-0000-4000-8000-000000000002'",
        [id(1000)],
    )
    .unwrap();
    drop(db);
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for name in ["db.sqlite", "metadata.json"] {
        output.start_file(name, options).unwrap();
        let value = if name == "db.sqlite" {
            std::fs::read(&path).unwrap()
        } else {
            let mut value = vec![];
            original
                .by_name(name)
                .unwrap()
                .read_to_end(&mut value)
                .unwrap();
            value
        };
        output.write_all(&value).unwrap();
    }
    output.finish().unwrap().into_inner()
}
#[test]
fn source_alias_chains_have_no_arbitrary_depth_limit_and_cycles_reject_before_activation() {
    let s = Server::start();
    let staged = ok(s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Idempotency-Key", "long-source")
        .header("Content-Type", "application/zip")
        .body(source_with_payee_chain(false))
        .send()
        .unwrap());
    assert_eq!(staged["data"]["status"], "ready", "{staged}");
    let result = ok(s.write(
        "POST",
        &format!(
            "/api/v1/imports/{}/activate",
            staged["data"]["id"].as_str().unwrap()
        ),
        "activate-chain",
        None,
        json!({}),
    ));
    let b = result["data"]["id"].as_str().unwrap();
    let tx = s.get(&format!(
        "/api/v1/budgets/{b}/transactions/04000000-0000-4000-8000-000000000002"
    ));
    assert_eq!(tx["data"]["payee_name"], "Fixture shop");
    assert_eq!(tx["data"]["amount"], -1200);
    let failed = s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Idempotency-Key", "cyclic-source")
        .header("Content-Type", "application/zip")
        .body(source_with_payee_chain(true))
        .send()
        .unwrap();
    assert_eq!(failed.status(), 422);
    assert_eq!(
        s.get("/api/v1/budgets")["data"].as_array().unwrap().len(),
        1
    );
}
