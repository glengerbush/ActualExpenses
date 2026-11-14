use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
};
use tempfile::TempDir;

struct Server {
    process: Child,
    url: String,
    client: Client,
    _data: TempDir,
}
impl Server {
    fn start() -> Self {
        let data = tempfile::tempdir().unwrap();
        let web = data.path().join("web");
        std::fs::create_dir(&web).unwrap();
        std::fs::write(web.join("index.html"), "<!doctype html>preferences").unwrap();
        let mut process = Command::new(env!("CARGO_BIN_EXE_actualbudget-server"))
            .env("ACTUALBUDGET_DATA_DIR", data.path())
            .env("ACTUALBUDGET_WEB_DIR", web)
            .env("ACTUALBUDGET_HOST", "127.0.0.1")
            .env("ACTUALBUDGET_PORT", "0")
            .env_remove("ACTUALBUDGET_PUBLIC_ORIGIN")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(process.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert!(line.starts_with("LISTENING "), "{line}");
        Self {
            process,
            url: format!("http://{}", line.trim().trim_start_matches("LISTENING ")),
            client: Client::new(),
            _data: data,
        }
    }
    fn restart(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        self.process = Command::new(env!("CARGO_BIN_EXE_actualbudget-server"))
            .env("ACTUALBUDGET_DATA_DIR", self._data.path())
            .env("ACTUALBUDGET_WEB_DIR", self._data.path().join("web"))
            .env("ACTUALBUDGET_HOST", "127.0.0.1")
            .env("ACTUALBUDGET_PORT", "0")
            .env_remove("ACTUALBUDGET_PUBLIC_ORIGIN")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(self.process.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert!(line.starts_with("LISTENING "));
        self.url = format!("http://{}", line.trim().trim_start_matches("LISTENING "));
    }
    fn budget(&self, id: &str) {
        assert_eq!(
            self.client
                .post(format!("{}/api/v1/budgets", self.url))
                .header("Idempotency-Key", id)
                .json(&json!({"id":id,"name":"Recovery fixture","budget_type":"tracking"}))
                .send()
                .unwrap()
                .status(),
            201
        );
    }
    fn get(&self, path: &str) -> reqwest::blocking::Response {
        self.client
            .get(format!("{}{path}", self.url))
            .send()
            .unwrap()
    }
    fn patch(&self, path: &str, key: &str, body: Value) -> reqwest::blocking::Response {
        self.client
            .patch(format!("{}{path}", self.url))
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

fn post(
    s: &Server,
    path: &str,
    body: Value,
    key: &str,
    epoch: Option<&str>,
) -> reqwest::blocking::Response {
    let mut r = s
        .client
        .post(format!("{}{path}", s.url))
        .header("Idempotency-Key", key)
        .json(&body);
    if let Some(epoch) = epoch {
        r = r.header("X-Actual-Budget-Epoch", epoch);
    }
    r.send().unwrap()
}
const B: &str = "00000000-0000-4000-8000-000000000049";
const A: &str = "00000000-0000-4000-8000-000000000001";
#[test]
fn recovery_preview_is_read_only_and_recalculates_with_restorable_backup_and_receipt() {
    let mut s = Server::start();
    s.budget(B);
    let r = post(
        &s,
        &format!("/api/v1/budgets/{B}/accounts"),
        json!({"id":A,"name":"Recovery account","opening_balance":1000,"opening_date":"2026-11-09","off_budget":false}),
        "account",
        None,
    );
    assert_eq!(r.status(), 201, "{}", r.text().unwrap());
    // Fault injection on a DISPOSABLE COPY of current-schema server data, not an old-schema fixture.
    s.process.kill().unwrap();
    s.process.wait().unwrap();
    let copied = tempfile::tempdir().unwrap();
    let origin = rusqlite::Connection::open(s._data.path().join("actualbudget.sqlite3")).unwrap();
    origin
        .execute(
            "VACUUM INTO ?1",
            [copied.path().join("actualbudget.sqlite3").to_str().unwrap()],
        )
        .unwrap();
    drop(origin);
    std::fs::copy(
        copied.path().join("actualbudget.sqlite3"),
        s._data.path().join("actualbudget.sqlite3"),
    )
    .unwrap();
    let injected = rusqlite::Connection::open(s._data.path().join("actualbudget.sqlite3")).unwrap();
    injected
        .execute(
            "UPDATE ledger_totals SET total=9999,cleared=9999 WHERE budget_id=?1",
            [B],
        )
        .unwrap();
    drop(injected);
    s.restart();
    let root = format!("/api/v1/budgets/{B}");
    let before: Value = s.get(&format!("{root}/checkpoint")).json().unwrap();
    let r = s.get(&format!("{root}/recovery"));
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    let plan: Value = r.json().unwrap();
    let p = &plan["data"];
    let contract: Value = s.get("/api/v1/openapi.json").json().unwrap();
    assert_eq!(
        contract["components"]["schemas"]["RecoveryPreview"]["properties"]["can_apply"]["type"],
        "boolean"
    );
    assert_eq!(p["accounts"][0]["before"]["total"], 9999);
    assert_eq!(p["accounts"][0]["after"]["total"], 1000);
    let unchanged: Value = s.get(&format!("{root}/accounts")).json().unwrap();
    assert_eq!(unchanged["data"][0]["balance"], 9999);
    assert_eq!(
        s.get(&format!("{root}/checkpoint"))
            .json::<Value>()
            .unwrap(),
        before
    );
    let body = json!({"preview_token":p["preview_token"],"snapshot":p["snapshot"],"confirm_name":"Recovery fixture"});
    let epoch = p["epoch"].as_str().unwrap();
    let r = post(
        &s,
        &format!("{root}/recovery"),
        body.clone(),
        "repair",
        Some(epoch),
    );
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    let applied: Value = r.json().unwrap();
    assert!(applied["data"]["backup"]["id"].is_string());
    assert_eq!(
        s.get(&format!("{root}/accounts")).json::<Value>().unwrap()["data"][0]["balance"],
        1000
    );
    s.restart();
    assert_eq!(
        post(
            &s,
            &format!("{root}/recovery"),
            body.clone(),
            "repair",
            Some(epoch)
        )
        .json::<Value>()
        .unwrap(),
        applied
    );
    let backup = applied["data"]["backup"]["id"].as_str().unwrap();
    let downloaded = s
        .get(&format!("{root}/backups/{backup}/download"))
        .bytes()
        .unwrap();
    assert!(downloaded.starts_with(b"PK"));
    let r = post(
        &s,
        &format!("{root}/backups/{backup}/restore"),
        json!({"confirm_name":"Recovery fixture"}),
        "restore-recovery",
        Some(epoch),
    );
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    assert_eq!(
        s.get(&format!("{root}/accounts")).json::<Value>().unwrap()["data"][0]["balance"],
        1000
    );
    let restored: Value = s.get(&format!("{root}/checkpoint")).json().unwrap();
    let restored_epoch = restored["data"]["epoch"].as_str().unwrap();
    assert_ne!(restored_epoch, epoch);
    assert_eq!(
        post(
            &s,
            &format!("{root}/recovery"),
            body.clone(),
            "repair",
            Some(epoch)
        )
        .json::<Value>()
        .unwrap(),
        applied
    );
    assert_eq!(
        post(
            &s,
            &format!("{root}/recovery"),
            body.clone(),
            "repair",
            Some(restored_epoch)
        )
        .json::<Value>()
        .unwrap()["error"]["code"],
        "idempotency_conflict"
    );
    assert_eq!(
        post(
            &s,
            &format!("{root}/recovery"),
            body,
            "delayed-old-preview",
            Some(epoch)
        )
        .json::<Value>()
        .unwrap()["error"]["code"],
        "budget_restored"
    );
}

// All mutations below are labeled fault injection on disposable COPIES of valid
// current-schema data produced through HTTP. No fabricated historical DB fixture.
fn fault(s: &mut Server, sql: &str) {
    let _ = s.process.kill();
    let _ = s.process.wait();
    let dbpath = s._data.path().join("actualbudget.sqlite3");
    let copy = s
        ._data
        .path()
        .join(format!("fault-copy-{}.sqlite3", uuid::Uuid::new_v4()));
    let source = rusqlite::Connection::open(&dbpath).unwrap();
    source
        .execute("VACUUM INTO ?1", [copy.to_str().unwrap()])
        .unwrap();
    drop(source);
    let copied = rusqlite::Connection::open(&copy).unwrap();
    copied.execute_batch(sql).unwrap();
    drop(copied);
    std::fs::rename(copy, dbpath).unwrap();
    s.restart();
}
fn create(s: &Server, path: &str, body: Value) {
    let r = post(s, path, body, &uuid::Uuid::new_v4().to_string(), None);
    assert!(r.status().is_success(), "{}", r.text().unwrap());
}
const T: &str = "10000000-0000-4000-8000-000000000001";
const C1: &str = "10000000-0000-4000-8000-000000000002";
const C2: &str = "10000000-0000-4000-8000-000000000003";
const P: &str = "10000000-0000-4000-8000-000000000004";
const C: &str = "10000000-0000-4000-8000-000000000005";
const Z: &str = "10000000-0000-4000-8000-000000000006";
const X: &str = "10000000-0000-4000-8000-000000000007";
const Y: &str = "10000000-0000-4000-8000-000000000008";
fn financial_fixture() -> Server {
    let s = Server::start();
    s.budget(B);
    let r = format!("/api/v1/budgets/{B}");
    for (id, name) in [(A, "Spending"), (Z, "Savings")] {
        create(
            &s,
            &format!("{r}/accounts"),
            json!({"id":id,"name":name,"opening_balance":0,"opening_date":"2026-11-09","off_budget":false}),
        );
    }
    create(&s, &format!("{r}/payees"), json!({"id":P,"name":"Market"}));
    create(
        &s,
        &format!("{r}/categories"),
        json!({"id":C,"name":"Food"}),
    );
    create(
        &s,
        &format!("{r}/transactions"),
        json!({"id":T,"account_id":A,"amount":-500,"date":"2026-11-09","payee_id":P,"notes":"Unchanged purchase"}),
    );
    let current: Value = s.get(&format!("{r}/transactions")).json().unwrap();
    create(
        &s,
        &format!("{r}/transactions/{T}/splits"),
        json!({"snapshot":current["snapshot"],"children":[{"id":C1,"amount":-200,"payee_id":P,"category_id":C},{"id":C2,"amount":-300,"payee_id":P,"category_id":C}]}),
    );
    create(
        &s,
        &format!("{r}/transfers"),
        json!({"id":X,"counterpart_id":Y,"account_id":A,"to_account_id":Z,"amount":-100,"date":"2026-11-09","cleared":false,"notes":"Same class transfer"}),
    );
    s
}
fn plan(s: &Server) -> Value {
    s.get(&format!("/api/v1/budgets/{B}/recovery"))
        .json::<Value>()
        .unwrap()["data"]
        .clone()
}
fn apply_plan(s: &Server, p: &Value, key: &str) -> reqwest::blocking::Response {
    post(
        s,
        &format!("/api/v1/budgets/{B}/recovery"),
        json!({"preview_token":p["preview_token"],"snapshot":p["snapshot"],"confirm_name":"Recovery fixture"}),
        key,
        p["epoch"].as_str(),
    )
}
#[test]
fn source_backed_repairs_are_atomic_keep_graph_amounts_and_preserve_a_durable_original_backup() {
    let mut s = financial_fixture();
    let root = format!("/api/v1/budgets/{B}");
    fault(
        &mut s,
        &format!(
            "UPDATE transactions SET payee_id=NULL WHERE id='{C1}'; UPDATE transactions SET category_id='{C}' WHERE id IN ('{T}','{X}');"
        ),
    );
    let p = plan(&s);
    assert_eq!(p["can_apply"], true, "{p}");
    assert_eq!(p["repairs"].as_array().unwrap().len(), 3);
    // Failure after backup commit must roll back every source repair, not just the failed field.
    fault(
        &mut s,
        &format!(
            "CREATE TRIGGER injected_repair_failure BEFORE UPDATE OF category_id ON transactions WHEN OLD.id='{T}' BEGIN SELECT RAISE(ABORT,'Injected transactional repair failure'); END;"
        ),
    );
    let p = plan(&s);
    let failed = apply_plan(&s, &p, "atomic-repair");
    assert_eq!(failed.status(), 500);
    let backups: Value = s.get(&format!("{root}/backups")).json().unwrap();
    assert_eq!(backups["data"].as_array().unwrap().len(), 1);
    let unchanged = plan(&s);
    assert_eq!(unchanged["repairs"].as_array().unwrap().len(), 3);
    fault(&mut s, "DROP TRIGGER injected_repair_failure;");
    let r = apply_plan(&s, &p, "atomic-repair");
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    let repaired: Value = r.json().unwrap();
    let after = plan(&s);
    assert!(after["repairs"].as_array().unwrap().is_empty(), "{after}");
    assert_eq!(
        s.get(&format!("{root}/backups")).json::<Value>().unwrap()["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let rows: Value = s.get(&format!("{root}/transactions")).json().unwrap();
    assert_eq!(rows["data"].as_array().unwrap().len(), 3); // split parent + two transfer sides
    let split: Value = s
        .get(&format!("{root}/transactions/{T}/splits"))
        .json()
        .unwrap();
    assert_eq!(split["data"]["amount"], -500);
    assert_eq!(split["data"]["children"][0]["amount"], -200);
    assert_eq!(split["data"]["children"][1]["amount"], -300);
    assert!(
        split["data"]["children"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["payee_id"] == P)
    );
    let companion: Value = s
        .get(&format!("{root}/companion?account_id={A}"))
        .json()
        .unwrap();
    assert!(
        companion["data"]["recent"].as_array().unwrap().is_empty(),
        "{companion}"
    );
    let backup = repaired["data"]["backup"]["id"].as_str().unwrap();
    let restored = post(
        &s,
        &format!("{root}/backups/{backup}/restore"),
        json!({"confirm_name":"Recovery fixture"}),
        "restore-original-canonicalization",
        p["epoch"].as_str(),
    );
    assert_eq!(restored.status(), 200, "{}", restored.text().unwrap());
    let original = plan(&s);
    assert_eq!(
        original["repairs"].as_array().unwrap().len(),
        3,
        "Original category/payee fields must be recoverable, not just the post-repair values"
    );
}
#[test]
fn ambiguous_references_or_split_amounts_are_review_only_after_retired_status_cleanup() {
    let mut s = financial_fixture();
    fault(
        &mut s,
        &format!(
            "UPDATE transactions SET category_id='{C}' WHERE id='{X}'; UPDATE transactions SET reconciled=1 WHERE id='{Y}';"
        ),
    );
    let protected = plan(&s);
    assert!(!protected["repairs"].as_array().unwrap().is_empty());
    assert!(
        !protected["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["domain"] == "protection")
    );
    fault(
        &mut s,
        &format!(
            "PRAGMA foreign_keys=OFF; UPDATE transactions SET category_id='20000000-0000-4000-8000-000000000001' WHERE id='{C1}'; UPDATE transactions SET amount=-999 WHERE id='{C2}'; UPDATE transactions SET transfer_id=NULL WHERE id='{Y}';"
        ),
    );
    let p = plan(&s);
    assert_eq!(p["can_apply"], false);
    assert_eq!(p["backup_restorable"], false);
    for domain in ["references", "splits", "transfers"] {
        assert!(
            p["issues"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["domain"] == domain),
            "{p}"
        );
    }
    let before: Value = s
        .get(&format!("/api/v1/budgets/{B}/checkpoint"))
        .json()
        .unwrap();
    assert_eq!(apply_plan(&s, &p, "refuse-ambiguous").status(), 409);
    assert_eq!(
        s.get(&format!("/api/v1/budgets/{B}/checkpoint"))
            .json::<Value>()
            .unwrap(),
        before
    );
    let split: Value = s
        .get(&format!("/api/v1/budgets/{B}/transactions/{T}/splits"))
        .json()
        .unwrap();
    assert!(
        split["data"]["children"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["amount"] == -999)
    );
}
#[test]
fn stale_preview_epoch_and_explicit_cancellation_cannot_mutate_or_duplicate_repairs() {
    let mut s = financial_fixture();
    fault(
        &mut s,
        "UPDATE ledger_totals SET total=900 WHERE account_id='00000000-0000-4000-8000-000000000001';",
    );
    let p = plan(&s);
    let root = format!("/api/v1/budgets/{B}");
    assert_eq!(
        s.patch(
            &format!("{root}/accounts/{A}"),
            "new-note",
            json!({"notes":"Later source edit"})
        )
        .status(),
        200
    );
    assert_eq!(apply_plan(&s, &p, "stale").status(), 409);
    let p = plan(&s);
    let body = json!({"preview_token":p["preview_token"],"snapshot":p["snapshot"],"confirm_name":"Recovery fixture"});
    let canceled = s
        .client
        .post(format!("{}{root}/recovery", s.url))
        .header("Idempotency-Key", "canceled")
        .header("X-Actual-Budget-Epoch", p["epoch"].as_str().unwrap())
        .header("X-Actual-Cancel-If-Uncommitted", "true")
        .json(&body)
        .send()
        .unwrap();
    assert_eq!(canceled.status(), 409);
    let replay = apply_plan(&s, &p, "canceled");
    assert_eq!(replay.status(), 409);
    assert_eq!(
        replay.json::<Value>().unwrap()["error"]["code"],
        "operation_canceled"
    );
    assert!(
        s.get(&format!("{root}/backups")).json::<Value>().unwrap()["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn backup_failure_prevents_every_financial_change_and_unchanged_request_can_retry() {
    let mut s = financial_fixture();
    fault(
        &mut s,
        "UPDATE ledger_totals SET total=900 WHERE account_id='00000000-0000-4000-8000-000000000001'; CREATE TRIGGER injected_backup_failure BEFORE INSERT ON budget_backups BEGIN SELECT RAISE(ABORT,'Injected backup failure'); END;",
    );
    let p = plan(&s);
    assert_eq!(p["can_apply"], true);
    assert_eq!(apply_plan(&s, &p, "failed-backup").status(), 500);
    let root = format!("/api/v1/budgets/{B}");
    assert_eq!(
        s.get(&format!("{root}/accounts")).json::<Value>().unwrap()["data"][0]["balance"],
        900
    );
    assert!(
        s.get(&format!("{root}/backups")).json::<Value>().unwrap()["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    fault(&mut s, "DROP TRIGGER injected_backup_failure;");
    assert_eq!(apply_plan(&s, &p, "failed-backup").status(), 200);
}

#[test]
fn corrupt_field_ordering_is_review_only_and_pending_clocks_are_not_reset() {
    let mut s = financial_fixture();
    fault(
        &mut s,
        &format!(
            "UPDATE ledger_totals SET total=900 WHERE account_id='{A}'; UPDATE field_versions SET clock='2040-01-01T00:00:00.000Z-0000-0000000000000049' WHERE entity_id='{C1}' AND field='category_id';"
        ),
    );
    let p = plan(&s);
    assert_eq!(p["can_apply"], false);
    assert!(
        p["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["domain"] == "ordering")
    );
    assert_eq!(apply_plan(&s, &p, "retain-clock").status(), 409);
    assert!(
        s.get(&format!("/api/v1/budgets/{B}/backups"))
            .json::<Value>()
            .unwrap()["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
