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
            .env("ACTUALBUDGET_BACKUP_INTERVAL_SECONDS", "1")
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
const B: &str = "10000000-0000-4000-8000-000000000001";
const G: &str = "10000000-0000-4000-8000-000000000002";
const C: &str = "10000000-0000-4000-8000-000000000003";
#[test]
fn category_groups_are_durable_ordered_and_field_merged() {
    let s = Server::start();
    assert_eq!(
        s.write(
            "POST",
            "/api/v1/budgets",
            "budget",
            None,
            json!({"id":B,"name":"Categories"})
        )
        .status(),
        201
    );
    let path = format!("/api/v1/budgets/{B}/category-groups");
    let r = s.write(
        "POST",
        &path,
        "group",
        None,
        json!({"id":G,"name":"Daily","sort_order":2}),
    );
    assert_eq!(r.status(), 201, "{}", r.text().unwrap());
    let path = format!("/api/v1/budgets/{B}/categories");
    let r = s.write(
        "POST",
        &path,
        "category",
        Some("2026-11-06T00:00:00.000Z-0000-0000000000000001"),
        json!({"id":C,"name":"Food","group_id":G,"notes":"Keep receipts","sort_order":3}),
    );
    assert_eq!(r.status(), 201, "{}", r.text().unwrap());
    let path = format!("{path}/{C}");
    let clock = "2026-11-08T00:00:00.000Z-0000-0000000000000001";
    let r = s.write(
        "PATCH",
        &path,
        "rename",
        Some(clock),
        json!({"name":"Meals","hidden":true}),
    );
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    let first = s.get(&path);
    assert_eq!(first["data"]["name"], "Meals");
    assert_eq!(first["data"]["notes"], "Keep receipts");
    let r = s.write(
        "PATCH",
        &path,
        "stale",
        Some("2026-11-07T00:00:00.000Z-0000-0000000000000001"),
        json!({"name":"Old","notes":"Independent note"}),
    );
    assert_eq!(r.status(), 200);
    assert_eq!(r.json::<Value>().unwrap()["conflicts"][0]["field"], "name");
    let current = s.get(&path);
    assert_eq!(current["data"]["name"], "Meals");
    assert_eq!(current["data"]["notes"], "Independent note");
    assert_eq!(s.restart().get(&path), current);
}
fn ok(r: Response) -> Value {
    let status = r.status();
    let body = r.json::<Value>().unwrap();
    assert!(status.is_success(), "{status}: {body}");
    body
}
#[test]
fn lifecycle_matches_published_actual_oracle_and_archives_history() {
    let s = Server::start();
    ok(s.write(
        "POST",
        "/api/v1/budgets",
        "b",
        None,
        json!({"id":B,"name":"Oracle"}),
    ));
    let base = format!("/api/v1/budgets/{B}");
    let aid = "10000000-0000-4000-8000-000000000010";
    ok(s.write(
        "POST",
        &format!("{base}/accounts"),
        "a",
        None,
        json!({"id":aid,"name":"Checking","opening_balance":0,"opening_date":"2026-01-01"}),
    ));
    let ids = [
        ("daily", G),
        ("other", "10000000-0000-4000-8000-000000000004"),
        ("income", "10000000-0000-4000-8000-000000000005"),
        ("food", C),
        ("dining", "10000000-0000-4000-8000-000000000006"),
        ("salary", "10000000-0000-4000-8000-000000000007"),
    ];
    let id = |label: &str| ids.iter().find(|v| v.0 == label).unwrap().1;
    for name in ["daily", "other", "income"] {
        ok(s.write(
            "POST",
            &format!("{base}/category-groups"),
            name,
            None,
            json!({"id":id(name),"name":name,"is_income":name=="income"}),
        ));
    }
    for (name, group, title) in [
        ("food", "daily", "Food"),
        ("dining", "other", "Dining"),
        ("salary", "income", "Salary"),
    ] {
        ok(s.write(
            "POST",
            &format!("{base}/categories"),
            name,
            None,
            json!({"id":id(name),"name":title,"group_id":id(group),"is_income":name=="salary"}),
        ));
    }
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/category-lifecycle.json")).unwrap();
    for tx in fixture["snapshots"][0]["transactions"].as_array().unwrap() {
        let n = tx["id"].as_str().unwrap().split('-').next_back().unwrap();
        ok(s.write("POST",&format!("{base}/transactions"),&format!("tx-{n}"),None,json!({"id":format!("11000000-0000-4000-8000-{:012}",n.parse::<u32>().unwrap()),"account_id":aid,"amount":tx["amount"],"date":"2026-01-01","notes":tx["notes"],"category_id":tx["category"].as_str().map(id)})));
    }
    for step in 0..4 {
        match step {
            1 => {
                ok(s.write("PATCH",&format!("{base}/categories/{C}"),"edit",None,json!({"name":"Meals","group_id":id("other"),"hidden":true,"notes":"Keep receipts\nReimbursements stay here"})));
            }
            2 => {
                let body = json!({"replacement_id":id("dining")});
                let first = ok(s.write(
                    "DELETE",
                    &format!("{base}/categories/{C}"),
                    "merge",
                    None,
                    body.clone(),
                ));
                assert_eq!(
                    first,
                    ok(s.write(
                        "DELETE",
                        &format!("{base}/categories/{C}"),
                        "merge",
                        None,
                        body
                    ))
                );
            }
            3 => {
                ok(s.write(
                    "DELETE",
                    &format!("{base}/categories/{}", id("dining")),
                    "delete",
                    None,
                    json!({"replacement_id":null}),
                ));
            }
            _ => {}
        }
        let label = |v: &Value| -> Value {
            v.as_str()
                .and_then(|v| ids.iter().find(|i| i.1 == v).map(|i| json!(i.0)))
                .unwrap_or(Value::Null)
        };
        let data = s.get(&format!("{base}/transactions"));
        let mut txs=data["data"].as_array().unwrap().iter().map(|t|{let n=t["id"].as_str().unwrap().rsplit('-').next().unwrap().parse::<u32>().unwrap();json!({"id":format!("transaction-{n}"),"amount":t["amount"],"category":label(&t["category_id"]),"notes":t["notes"]})}).collect::<Vec<_>>();
        txs.sort_by_key(|t| t["id"].as_str().unwrap().to_owned());
        assert_eq!(json!(txs), fixture["snapshots"][step]["transactions"]);
        assert_eq!(
            s.get(&format!("{base}/accounts"))["data"][0]["balance"],
            9301
        );
    }
    assert_eq!(
        s.get(&format!("{base}/categories/{C}"))["data"]["notes"],
        "Keep receipts\nReimbursements stay here"
    );
    let drill = s.get(&format!("{base}/categories/uncategorized/transactions"));
    assert_eq!(drill["total"], -699);
    assert_eq!(drill["count"], 4);
    let snapshot = s.get(&format!("{base}/categories?include_deleted=true"));
    let backup = ok(s.write(
        "POST",
        &format!("{base}/backups"),
        "export",
        None,
        json!({"label":"Category history"}),
    ));
    let r = s
        .client
        .get(format!(
            "{}{base}/backups/{}/download",
            s.url,
            backup["data"]["id"].as_str().unwrap()
        ))
        .send()
        .unwrap();
    assert!(r.status().is_success());
    let bytes = r.bytes().unwrap();
    let r = s
        .client
        .post(format!("{}/api/v1/restores", s.url))
        .header("Idempotency-Key", "restore-upload")
        .header("Content-Type", "application/zip")
        .body(bytes)
        .send()
        .unwrap();
    let stage = ok(r);
    let stage = stage["data"]["id"].as_str().unwrap();
    ok(s.write(
        "POST",
        &format!("{base}/restores/{stage}/activate"),
        "activate",
        None,
        json!({"confirm_name":"Oracle"}),
    ));
    assert_eq!(
        s.get(&format!("{base}/categories?include_deleted=true")),
        snapshot
    );
    assert_eq!(
        s.get(&format!("{base}/categories/uncategorized/transactions"))["total"],
        -699
    );
}
#[test]
fn published_category_metadata_imports_with_notes_order_hidden_and_financial_drilldowns() {
    let s = Server::start();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/category-import-26.10.0.json"
    ))
    .unwrap();
    let stage = ok(s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "category-import")
        .body(include_bytes!("../../tests/fixtures/category-import-26.10.0.zip").as_slice())
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
    ))["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let base = format!("/api/v1/budgets/{budget}");
    for expected in fixture["snapshot"]["categories"].as_array().unwrap() {
        let id = fixture["aliases"]
            .as_object()
            .unwrap()
            .iter()
            .find(|(_, label)| **label == expected["id"])
            .unwrap()
            .0;
        let actual = s.get(&format!("{base}/categories/{id}"))["data"].clone();
        assert_eq!(actual["name"], expected["name"]);
        assert_eq!(actual["hidden"], expected["hidden"]);
        assert_eq!(actual["is_income"], expected["income"]);
        assert_eq!(
            fixture["aliases"][actual["group_id"].as_str().unwrap()],
            expected["group"]
        );
        if expected["id"] == "food" {
            assert_eq!(actual["notes"], fixture["snapshot"]["note"]);
            let drill = s.get(&format!("{base}/categories/{id}/transactions"));
            assert_eq!(drill["count"], 3);
            assert_eq!(drill["total"], -600);
        }
    }
    let backup = ok(s.write(
        "POST",
        &format!("{base}/backups"),
        "point",
        None,
        json!({"label":"Imported category metadata"}),
    ));
    let bytes = s
        .client
        .get(format!(
            "{}{base}/backups/{}/download",
            s.url,
            backup["data"]["id"].as_str().unwrap()
        ))
        .send()
        .unwrap()
        .bytes()
        .unwrap();
    ok(s.client
        .post(format!("{}/api/v1/restores", s.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "native-roundtrip")
        .body(bytes)
        .send()
        .unwrap());
}
#[test]
fn genuine_schema6_archive_restores_and_legacy_creation_receipt_replays() {
    let s = Server::start();
    let b = "16000000-0000-4000-8000-000000000001";
    let c = "16000000-0000-4000-8000-000000000002";
    let base = format!("/api/v1/budgets/{b}");
    let stage = ok(s
        .client
        .post(format!("{}/api/v1/restores", s.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "legacy-stage")
        .body(include_bytes!("../../tests/fixtures/category-schema6.zip").as_slice())
        .send()
        .unwrap());
    let id = stage["data"]["id"].as_str().unwrap();
    ok(s.write(
        "POST",
        &format!("{base}/restores/{id}/activate"),
        "legacy-activate",
        None,
        json!({"confirm_name":"Legacy categories"}),
    ));
    let row = s.get(&format!("{base}/categories/{c}"));
    assert_eq!(row["data"]["name"], "Legacy food");
    assert_eq!(row["data"]["notes"], "");
    assert_eq!(row["data"]["hidden"], false);
    assert_eq!(s.get(&format!("{base}/category-groups"))["data"], json!([]));
    let r = s.write(
        "POST",
        &format!("{base}/categories"),
        "legacy-category",
        None,
        json!({"id":c,"name":"Legacy food"}),
    );
    assert_eq!(r.status(), 201, "{}", r.text().unwrap());
    assert_eq!(
        r.json::<Value>().unwrap()["data"],
        json!({"id":c,"name":"Legacy food","is_income":false})
    );
    let nullable = ok(s.write(
        "POST",
        &format!("{base}/categories"),
        "legacy-category",
        None,
        json!({"id":c,"name":"Legacy food","is_income":null}),
    ));
    assert_eq!(nullable["data"]["name"], "Legacy food");
}
#[test]
fn retired_actual_categories_keep_notes_and_uncategorized_references() {
    let s = Server::start();
    let stage = ok(s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "retired-import")
        .body(include_bytes!("../../tests/fixtures/category-retired-26.10.0.zip").as_slice())
        .send()
        .unwrap());
    assert_eq!(stage["data"]["status"], "ready", "{stage}");
    let id = stage["data"]["id"].as_str().unwrap();
    let budget = ok(s.write(
        "POST",
        &format!("/api/v1/imports/{id}/activate"),
        "retired-activate",
        None,
        json!({}),
    ))["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let base = format!("/api/v1/budgets/{budget}");
    let rows = s.get(&format!("{base}/categories?include_deleted=true"));
    let meals = rows["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "Meals")
        .unwrap();
    assert_eq!(meals["deleted"], true);
    assert_eq!(meals["notes"], "Keep receipts\nReimbursements stay here");
    assert_eq!(
        s.get(&format!("{base}/categories/uncategorized/transactions"))["total"],
        -699
    );
}
#[test]
fn stale_references_redirect_and_group_deletion_is_atomic_across_hidden_history() {
    let s = Server::start();
    ok(s.write(
        "POST",
        "/api/v1/budgets",
        "b",
        None,
        json!({"id":B,"name":"Concurrent"}),
    ));
    let base = format!("/api/v1/budgets/{B}");
    let account = "17000000-0000-4000-8000-000000000001";
    let target = "17000000-0000-4000-8000-000000000002";
    let income = "17000000-0000-4000-8000-000000000003";
    let tx = "17000000-0000-4000-8000-000000000004";
    ok(s.write(
        "POST",
        &format!("{base}/accounts"),
        "a",
        None,
        json!({"id":account,"name":"Checking","opening_balance":0,"opening_date":"2026-01-01"}),
    ));
    ok(s.write(
        "POST",
        &format!("{base}/category-groups"),
        "g",
        None,
        json!({"id":G,"name":"Group"}),
    ));
    for (id, name, income, group) in [
        (C, "Food", false, Some(G)),
        (target, "Target", false, None),
        (income, "Income", true, None),
    ] {
        ok(s.write(
            "POST",
            &format!("{base}/categories"),
            name,
            None,
            json!({"id":id,"name":name,"is_income":income,"group_id":group}),
        ));
    }
    ok(s.write(
        "POST",
        &format!("{base}/transactions"),
        "tx",
        None,
        json!({"id":tx,"account_id":account,"amount":-123,"date":"2026-01-01","category_id":C}),
    ));
    let refused = s.write(
        "DELETE",
        &format!("{base}/categories/{C}"),
        "wrong-type",
        None,
        json!({"replacement_id":income}),
    );
    assert_eq!(refused.status(), 400);
    assert_eq!(
        s.get(&format!("{base}/transactions/{tx}"))["data"]["category_id"],
        C
    );
    let hidden = "17000000-0000-4000-8000-000000000005";
    ok(s.write(
        "POST",
        &format!("{base}/transactions"),
        "hidden",
        None,
        json!({"id":hidden,"account_id":account,"amount":-777,"date":"2026-01-01","category_id":C}),
    ));
    ok(s.write(
        "DELETE",
        &format!("{base}/transactions/{hidden}"),
        "hide",
        None,
        Value::Null,
    ));
    ok(s.write(
        "DELETE",
        &format!("{base}/category-groups/{G}"),
        "delete-group",
        None,
        json!({"replacement_id":target}),
    ));
    assert_eq!(
        s.get(&format!("{base}/categories/{C}"))["data"]["deleted"],
        true
    );
    assert_eq!(
        s.get(&format!("{base}/transactions/{tx}"))["data"]["category_id"],
        target
    );
    let hidden_result = ok(s.write(
        "PATCH",
        &format!("{base}/transactions/{hidden}"),
        "hidden-note",
        None,
        json!({"notes":"Retained history"}),
    ));
    assert_eq!(hidden_result["data"]["category_id"], target);
    assert_eq!(hidden_result["data"]["amount"], -777);
    assert_eq!(hidden_result["data"]["deleted"], true);
    assert_eq!(
        s.get(&format!("{base}/accounts"))["data"][0]["balance"],
        -123
    );
    let delayed = ok(s.write(
        "PATCH",
        &format!("{base}/transactions/{tx}"),
        "delayed-ref",
        None,
        json!({"category_id":C,"notes":"A delayed category selection"}),
    ));
    assert_eq!(delayed["data"]["category_id"], target);
    assert_eq!(delayed["data"]["amount"], -123);
    let checkpoint = s.get(&format!("{base}/checkpoint"));
    let point = ok(s.write(
        "POST",
        &format!("{base}/backups"),
        "backup",
        None,
        json!({"label":"Fence"}),
    ));
    ok(s.write(
        "POST",
        &format!(
            "{base}/backups/{}/restore",
            point["data"]["id"].as_str().unwrap()
        ),
        "restore",
        None,
        json!({"confirm_name":"Concurrent"}),
    ));
    let response = s
        .client
        .patch(format!("{}{base}/categories/{target}", s.url))
        .header("Idempotency-Key", "retired-intent")
        .header(
            "X-Actual-Budget-Epoch",
            checkpoint["data"]["epoch"].as_str().unwrap(),
        )
        .json(&json!({"name":"Stale"}))
        .send()
        .unwrap();
    assert_eq!(response.status(), 409);
    assert_eq!(
        response.json::<Value>().unwrap()["error"]["code"],
        "budget_restored"
    );
    // Original success remains immutable even after restore.
    assert_eq!(
        ok(s.write(
            "PATCH",
            &format!("{base}/transactions/{tx}"),
            "delayed-ref",
            None,
            json!({"category_id":C,"notes":"A delayed category selection"})
        )),
        delayed
    );
}
fn assert_legacy_import_metadata(s: &Server, budget: &str, post_import: Option<&Value>) {
    let base = format!("/api/v1/budgets/{budget}");
    let pinned: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/import-26.10.0.json")).unwrap();
    for source in pinned["public"]["categories"].as_array().unwrap() {
        let category = s.get(&format!(
            "{base}/categories/{}",
            source["id"].as_str().unwrap()
        ));
        for field in ["name", "is_income", "hidden", "group_id"] {
            assert_eq!(category["data"][field], source[field], "{field}");
        }
    }
    let groups = s.get(&format!("{base}/category-groups"));
    assert_eq!(groups["data"].as_array().unwrap().len(), 2);
    if let Some(expected) = post_import {
        assert_eq!(
            s.get(&format!(
                "{base}/categories/{}",
                expected["native_category"].as_str().unwrap()
            ))["data"]["name"],
            "Added after import"
        );
        let actual = s.get(&format!("{base}/transactions"));
        for before in expected["transactions"].as_array().unwrap() {
            let after = actual["data"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["id"] == before["id"])
                .unwrap();
            assert_eq!(after, before);
        }
    }
}
#[test]
fn genuine_pre10_active_and_staged_imports_gain_metadata_without_replaying_history() {
    let expected: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/category-legacy-active.json"
    ))
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("actualbudget.sqlite3"),
        include_bytes!("../../tests/fixtures/category-legacy-active.sqlite3"),
    )
    .unwrap();
    let s = Server::open(directory);
    let budget = expected["budget"].as_str().unwrap();
    assert_legacy_import_metadata(&s, budget, Some(&expected));
    let receipt = ok(s.write(
        "PATCH",
        &format!(
            "/api/v1/budgets/{budget}/transactions/{}",
            expected["transaction"].as_str().unwrap()
        ),
        "edit-after-import",
        None,
        json!({"notes":"Changed after import","category_id":expected["native_category"]}),
    ));
    assert_eq!(receipt["data"]["notes"], "Changed after import");
    let stage = ok(s
        .client
        .post(format!("{}/api/v1/restores", s.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "legacy-native-import")
        .body(include_bytes!("../../tests/fixtures/category-import-schema6.zip").as_slice())
        .send()
        .unwrap());
    ok(s.write(
        "POST",
        &format!(
            "/api/v1/budgets/{budget}/restores/{}/activate",
            stage["data"]["id"].as_str().unwrap()
        ),
        "legacy-native-activate",
        None,
        json!({"confirm_name":expected["name"]}),
    ));
    assert_legacy_import_metadata(&s, budget, Some(&expected));
    let expected: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/category-legacy-staged.json"
    ))
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("actualbudget.sqlite3"),
        include_bytes!("../../tests/fixtures/category-legacy-staged.sqlite3"),
    )
    .unwrap();
    let s = Server::open(directory);
    let budget = expected["budget"].as_str().unwrap();
    ok(s.write(
        "POST",
        &format!(
            "/api/v1/imports/{}/activate",
            expected["import"].as_str().unwrap()
        ),
        "legacy-stage-activate",
        None,
        json!({}),
    ));
    assert_legacy_import_metadata(&s, budget, None);
}
#[test]
fn new_upload_intent_reviews_old_blocked_categories_without_changing_original_receipt() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("actualbudget.sqlite3"),
        include_bytes!("../../tests/fixtures/category-legacy-blocked.sqlite3"),
    )
    .unwrap();
    let s = Server::open(directory);
    let upload = |key: &str| {
        ok(s.client
            .post(format!("{}/api/v1/imports/actual", s.url))
            .header("Content-Type", "application/zip")
            .header("Idempotency-Key", key)
            .body(include_bytes!("../../tests/fixtures/category-import-26.10.0.zip").as_slice())
            .send()
            .unwrap())
    };
    let old = upload("import");
    assert_eq!(old["data"]["status"], "blocked");
    let reviewed = upload("review-category-support");
    assert_eq!(reviewed["data"]["status"], "ready");
    assert_eq!(reviewed["data"]["id"], old["data"]["id"]);
    assert_eq!(upload("import"), old);
    let activated = ok(s.write(
        "POST",
        &format!(
            "/api/v1/imports/{}/activate",
            reviewed["data"]["id"].as_str().unwrap()
        ),
        "activate-reviewed",
        None,
        json!({}),
    ));
    let categories = s.get(&format!(
        "/api/v1/budgets/{}/categories",
        activated["data"]["id"].as_str().unwrap()
    ));
    let meals = categories["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "Meals")
        .unwrap();
    assert_eq!(meals["hidden"], true);
    assert_eq!(meals["notes"], "Keep receipts\nReimbursements stay here");
}
#[test]
fn category_order_preserves_null_negative_fractional_positions_and_snapshot_guard() {
    let s = Server::start();
    ok(s.write(
        "POST",
        "/api/v1/budgets",
        "order-budget",
        None,
        json!({"id":B,"name":"Ordering"}),
    ));
    let base = format!("/api/v1/budgets/{B}");
    for (n, name, position) in [
        (1, "Unset", Value::Null),
        (2, "Negative", json!(-4)),
        (3, "Fractional", json!(0.25)),
    ] {
        ok(s.write("POST",&format!("{base}/category-groups"),name,None,json!({"id":format!("19000000-0000-4000-8000-{n:012}"),"name":name,"sort_order":position})));
    }
    let groups = s.get(&format!("{base}/category-groups"));
    assert_eq!(
        groups["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["name"].clone())
            .collect::<Vec<_>>(),
        vec![json!("Unset"), json!("Negative"), json!("Fractional")]
    );
    assert_eq!(groups["data"][0]["sort_order"], Value::Null);
    assert_eq!(groups["data"][2]["sort_order"], 0.25);
    let checkpoint = s.get(&format!("{base}/checkpoint"));
    ok(s.write(
        "PATCH",
        &format!("{base}/category-groups/19000000-0000-4000-8000-000000000003"),
        "group-note",
        None,
        json!({"notes":"Group history","hidden":true,"name":"Trips"}),
    ));
    let response = s
        .client
        .get(format!(
            "{}{base}/category-groups?snapshot={}",
            s.url,
            checkpoint["data"]["snapshot"].as_str().unwrap()
        ))
        .send()
        .unwrap();
    assert_eq!(response.status(), 409);
    assert_eq!(
        response.json::<Value>().unwrap()["error"]["code"],
        "snapshot_changed"
    );
    let group = s.get(&format!(
        "{base}/category-groups/19000000-0000-4000-8000-000000000003"
    ));
    assert_eq!(group["data"]["notes"], "Group history");
    assert_eq!(group["data"]["hidden"], true);
}
