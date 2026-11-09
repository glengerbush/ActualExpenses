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
        Self::with_data(std::mem::replace(
            &mut self._data,
            tempfile::tempdir().unwrap(),
        ))
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
fn splitting_preserves_totals_ids_and_companion_exclusion() {
    let server = Server::start();
    create_budget(&server);
    assert_eq!(server.post(&format!("/api/v1/budgets/{BUDGET}/accounts"), "account", json!({"id":ACCOUNT,"name":"Spending","opening_balance":0,"opening_date":"2026-01-01"})).status(),201);
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    let id = "10000000-0000-4000-8000-000000000001";
    let first = "10000000-0000-4000-8000-000000000002";
    let second = "10000000-0000-4000-8000-000000000003";
    assert_eq!(server.post(&path,"entry",json!({"id":id,"account_id":ACCOUNT,"amount":-5001,"date":"2026-01-02","cleared":true})).status(),201);
    let before: Value = server.get(&path).json().unwrap();
    let body = json!({"snapshot":before["snapshot"],"children":[{"id":first,"amount":-2000,"notes":"Food"},{"id":second,"amount":0}],"allocation":"even"});
    let response = server.post(&format!("{path}/{id}/splits"), "split", body.clone());
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let response: Value = server
        .post(&format!("{path}/{id}/splits"), "split", body)
        .json()
        .unwrap();
    assert_eq!(response["data"]["children"][1]["amount"], -3001);
    assert_eq!(response["data"]["children"][0]["id"], first);
    let register: Value = server
        .get(&format!("{path}?account_id={ACCOUNT}"))
        .json()
        .unwrap();
    assert_eq!(register["balances"]["total"], -5001);
    assert_eq!(register["data"].as_array().unwrap().len(), 1);
    let companion: Value = server
        .get(&format!(
            "/api/v1/budgets/{BUDGET}/companion?account_id={ACCOUNT}&retain={id},{first},{second}"
        ))
        .json()
        .unwrap();
    assert_eq!(companion["data"]["recent"], json!([]));
    assert_eq!(companion["data"]["retained"], json!([]));
    assert_eq!(
        companion["data"]["unavailable"].as_array().unwrap().len(),
        3
    );
}

#[test]
fn replacement_is_atomic_stale_safe_and_deletion_cascades() {
    let server = Server::start();
    create_budget(&server);
    assert_eq!(server.post(&format!("/api/v1/budgets/{BUDGET}/accounts"),"account",json!({"id":ACCOUNT,"name":"Checking","opening_balance":0,"opening_date":"2026-01-01"})).status(),201);
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    let id = "10000000-0000-4000-8000-000000000001";
    let a = "10000000-0000-4000-8000-000000000002";
    let b = "10000000-0000-4000-8000-000000000003";
    assert_eq!(
        server
            .post(
                &path,
                "entry",
                json!({"id":id,"account_id":ACCOUNT,"amount":-4000,"date":"2026-01-02"})
            )
            .status(),
        201
    );
    let before: Value = server.get(&path).json().unwrap();
    let bad = json!({"snapshot":before["snapshot"],"children":[{"id":a,"amount":-2000},{"id":b,"amount":-1000}]});
    assert_eq!(
        server
            .post(&format!("{path}/{id}/splits"), "bad", bad)
            .status(),
        400
    );
    assert_eq!(server.get(&format!("{path}/{a}")).status(), 404);
    let body = json!({"snapshot":before["snapshot"],"children":[{"id":a,"amount":-2000},{"id":b,"amount":-1000}],"allocation":"proportional"});
    let response = server.post(&format!("{path}/{id}/splits"), "proportional", body.clone());
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let result: Value = server.get(&format!("{path}/{id}")).json().unwrap();
    // Actual 26.10 floor-then-increment allocation, negative total.
    assert_eq!(result["data"]["children"][0]["amount"], -2666);
    assert_eq!(result["data"]["children"][1]["amount"], -1334);
    assert_eq!(
        server
            .post(&format!("{path}/{id}/splits"), "stale", body)
            .status(),
        409
    );
    let patch = server
        .client
        .patch(format!("{}{path}/{a}", server.url))
        .header("Idempotency-Key", "child-patch")
        .json(&json!({"amount":-1}))
        .send()
        .unwrap();
    assert_eq!(patch.status(), 409);
    let delete = server
        .client
        .delete(format!("{}{path}/{a}", server.url))
        .header("Idempotency-Key", "child-delete")
        .send()
        .unwrap();
    assert_eq!(delete.status(), 400);
    let delete = server
        .client
        .delete(format!("{}{path}/{id}", server.url))
        .header("Idempotency-Key", "parent-delete")
        .send()
        .unwrap();
    assert_eq!(delete.status(), 200);
    assert_eq!(server.get(&format!("{path}/{a}")).status(), 404);
    assert_eq!(server.get(&format!("{path}/{b}")).status(), 404);
    let register: Value = server.get(&path).json().unwrap();
    assert_eq!(register["balances"]["total"], 0);
}

#[test]
fn pinned_actual_splits_import_and_native_archive_roundtrip() {
    let server = Server::start();
    let expected: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/import-splits-26.10.0.json"
    ))
    .unwrap();
    let upload = server
        .client
        .post(format!("{}/api/v1/imports/actual", server.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "upload")
        .body(include_bytes!("../../tests/fixtures/import-splits-26.10.0.zip").as_slice())
        .send()
        .unwrap();
    assert_eq!(upload.status(), 201, "{}", upload.text().unwrap());
    let stage: Value = upload.json().unwrap();
    assert_eq!(stage["data"]["status"], "ready", "{stage}");
    assert_eq!(
        stage["data"]["report"]["balances"],
        expected["public"]["balances"]
    );
    let id = stage["data"]["id"].as_str().unwrap();
    let budget = stage["data"]["budget_id"].as_str().unwrap();
    let result = server.post(
        &format!("/api/v1/imports/{id}/activate"),
        "activate",
        json!({}),
    );
    assert_eq!(result.status(), 201, "{}", result.text().unwrap());
    let root = format!("/api/v1/budgets/{budget}");
    let split = "04000000-0000-4000-8000-000000000050";
    let before: Value = server
        .get(&format!("{root}/transactions/{split}"))
        .json()
        .unwrap();
    assert_eq!(before["data"]["amount"], -5001);
    let category = before["data"]["children"][0]["category_id"]
        .as_str()
        .unwrap();
    let allocations: Value = server
        .get(&format!("{root}/categories/{category}/transactions"))
        .json()
        .unwrap();
    assert_eq!(allocations["total"], -5967);
    assert!(
        allocations["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["parent_id"] == split)
    );

    assert_eq!(before["data"]["children"].as_array().unwrap().len(), 2);
    for child in before["data"]["children"].as_array().unwrap() {
        assert_eq!(child["parent_id"], split);
    }
    let point = server.post(
        &format!("{root}/backups"),
        "backup",
        json!({"label":"Split backup"}),
    );
    assert_eq!(point.status(), 201, "{}", point.text().unwrap());
    let point: Value = point.json().unwrap();
    let bytes = server
        .get(&format!(
            "{root}/backups/{}/download",
            point["data"]["id"].as_str().unwrap()
        ))
        .bytes()
        .unwrap();
    let target = Server::start();
    let upload = target
        .client
        .post(format!("{}/api/v1/restores", target.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "restore")
        .body(bytes.to_vec())
        .send()
        .unwrap();
    assert_eq!(upload.status(), 201, "{}", upload.text().unwrap());
    let stage: Value = upload.json().unwrap();
    let name = expected["inspection"]["metadata"]["budgetName"]
        .as_str()
        .unwrap_or("Disposable actualBudget migration fixture");
    let activated = target.post(
        &format!(
            "{root}/restores/{}/activate",
            stage["data"]["id"].as_str().unwrap()
        ),
        "restore-activate",
        json!({"confirm_name":name}),
    );
    assert_eq!(activated.status(), 200, "{}", activated.text().unwrap());
    let after: Value = target
        .get(&format!("{root}/transactions/{split}"))
        .json()
        .unwrap();
    assert_eq!(after, before);

    // Imported allocations remain editable after moving off budget, including
    // after an archive restore; their categories are latent, not new selections.
    let current: Value = target
        .get(&format!("{root}/transactions/{split}/splits"))
        .json()
        .unwrap();
    let post_restored = |path: &str, key: &str, body: Value| {
        target
            .client
            .post(format!("{}{path}", target.url))
            .header("Idempotency-Key", key)
            .header("X-Actual-Budget-Epoch", current["epoch"].as_str().unwrap())
            .json(&body)
            .send()
            .unwrap()
    };
    let off = uuid::Uuid::new_v4().to_string();
    assert_eq!(post_restored(&format!("{root}/accounts"), "import-off", json!({"id":off,"name":"Imported off budget","off_budget":true,"opening_balance":0,"opening_date":"2026-01-01"})).status(), 201);
    let mut children: Vec<Value> = after["data"]["children"].as_array().unwrap().iter().map(|c| json!({"id":c["id"],"amount":c["amount"],"category_id":c["category_id"],"payee_id":c["payee_id"],"notes":c["notes"]})).collect();
    for (key, fields) in [
        ("move-import", json!({"account_id":off})),
        ("edit-import", json!({"notes":"Edited imported parent"})),
    ] {
        let current: Value = target
            .get(&format!("{root}/transactions/{split}/splits"))
            .json()
            .unwrap();
        children[0]["notes"] = json!(key);
        let edited = post_restored(
            &format!("{root}/transactions/{split}/splits"),
            key,
            json!({"snapshot":current["snapshot"],"fields":fields,"children":children}),
        );
        assert_eq!(edited.status(), 200, "{}", edited.text().unwrap());
        let edited: Value = edited.json().unwrap();
        assert_eq!(edited["data"]["children"][0]["category_id"], category);
        assert_eq!(edited["data"]["children"][0]["notes"], key);
    }
}

#[test]
fn detach_recombine_duplicate_and_cancellation_preserve_financial_totals() {
    let server = Server::start();
    create_budget(&server);
    assert_eq!(server.post(&format!("/api/v1/budgets/{BUDGET}/accounts"),"account",json!({"id":ACCOUNT,"name":"Checking","opening_balance":0,"opening_date":"2026-01-01"})).status(),201);
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    let id = "10000000-0000-4000-8000-000000000001";
    let a = "10000000-0000-4000-8000-000000000002";
    let b = "10000000-0000-4000-8000-000000000003";
    let c = "10000000-0000-4000-8000-000000000004";
    assert_eq!(
        server
            .post(
                &path,
                "entry",
                json!({"id":id,"account_id":ACCOUNT,"amount":-600,"date":"2026-01-02"})
            )
            .status(),
        201
    );
    let before: Value = server.get(&path).json().unwrap();
    let body = json!({"snapshot":before["snapshot"],"children":[{"id":a,"amount":-100},{"id":b,"amount":-200},{"id":c,"amount":-300}]});
    let canceled = server
        .client
        .post(format!("{}{path}/{id}/splits", server.url))
        .header("Idempotency-Key", "canceled")
        .header("X-Actual-Cancel-If-Uncommitted", "true")
        .json(&body)
        .send()
        .unwrap();
    assert_eq!(canceled.status(), 409);
    let replay: Value = server
        .post(&format!("{path}/{id}/splits"), "canceled", body.clone())
        .json()
        .unwrap();
    assert_eq!(replay["error"]["code"], "operation_canceled");
    assert_eq!(
        server
            .post(&format!("{path}/{id}/splits"), "split", body)
            .status(),
        200
    );
    let copy = "10000000-0000-4000-8000-000000000005";
    let duplicate = server.post(
        &format!("{path}/{id}/duplicate"),
        "duplicate",
        json!({"id":copy}),
    );
    assert_eq!(duplicate.status(), 201);
    let duplicate: Value = duplicate.json().unwrap();
    assert_eq!(duplicate["data"]["children"].as_array().unwrap().len(), 3);
    assert_ne!(duplicate["data"]["children"][0]["id"], a);
    let current: Value = server.get(&path).json().unwrap();
    assert_eq!(current["balances"]["total"], -1200);
    let detached = server.post(
        &format!("{path}/{id}/splits"),
        "detach",
        json!({"snapshot":current["snapshot"],"action":"unsplit","detach":[a]}),
    );
    assert_eq!(detached.status(), 200, "{}", detached.text().unwrap());
    let detached: Value = server.get(&format!("{path}/{id}")).json().unwrap();
    assert_eq!(detached["data"]["amount"], -500);
    assert_eq!(detached["data"]["children"].as_array().unwrap().len(), 2);
    let current: Value = server.get(&path).json().unwrap();
    assert_eq!(current["balances"]["total"], -1200);
    assert_eq!(
        server
            .post(
                &format!("{path}/{id}/splits"),
                "detach-all",
                json!({"snapshot":current["snapshot"],"action":"unsplit"})
            )
            .status(),
        200
    );
    assert_eq!(server.get(&format!("{path}/{id}")).status(), 404);
    let current: Value = server.get(&path).json().unwrap();
    assert_eq!(current["balances"]["total"], -1200);
    assert_eq!(
        server
            .post(
                &format!("{path}/{copy}/splits"),
                "recombine",
                json!({"snapshot":current["snapshot"],"action":"recombine"})
            )
            .status(),
        200
    );
    let current: Value = server.get(&path).json().unwrap();
    assert_eq!(current["balances"]["total"], -1200);
    assert_eq!(current["data"].as_array().unwrap().len(), 4);
}

#[test]
fn allocations_match_executed_pinned_actual_callback() {
    let server = Server::start();
    create_budget(&server);
    assert_eq!(server.post(&format!("/api/v1/budgets/{BUDGET}/accounts"),"account",json!({"id":ACCOUNT,"name":"Checking","opening_balance":0,"opening_date":"2026-01-01"})).status(),201);
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/split-allocation-26.10.0.json"
    ))
    .unwrap();
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    for case in fixture["cases"].as_array().unwrap() {
        let id = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            server
                .post(
                    &path,
                    &id,
                    json!({"id":id,"account_id":ACCOUNT,"amount":case["total"],"date":"2026-01-01"})
                )
                .status(),
            201
        );
        let current: Value = server.get(&path).json().unwrap();
        let children: Vec<Value> = case["amounts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|amount| json!({"id":uuid::Uuid::new_v4().to_string(),"amount":amount}))
            .collect();
        let response = server.post(
            &format!("{path}/{id}/splits"),
            &id,
            json!({"snapshot":current["snapshot"],"children":children,"allocation":"auto"}),
        );
        assert_eq!(response.status(), 200, "{}", response.text().unwrap());
        let result: Value = response.json().unwrap();
        let amounts: Vec<Value> = result["data"]["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["amount"].clone())
            .collect();
        assert_eq!(json!(amounts), case["expected"]);
    }
}

const C1: &str = "2026-01-01T00:00:00.000Z-0000-0000000000000001";
const C2: &str = "2026-01-02T00:00:00.000Z-0000-0000000000000001";
const C3: &str = "2026-01-03T00:00:00.000Z-0000-0000000000000001";
const C4: &str = "2026-01-04T00:00:00.000Z-0000-0000000000000001";
const C5: &str = "2026-01-05T00:00:00.000Z-0000-0000000000000001";
fn clocked(
    server: &Server,
    method: &str,
    path: &str,
    key: &str,
    clock: &str,
    body: Value,
) -> Response {
    server
        .client
        .request(method.parse().unwrap(), format!("{}{path}", server.url))
        .header("Idempotency-Key", key)
        .header("X-Actual-Edit-Clock", clock)
        .json(&body)
        .send()
        .unwrap()
}
fn setup_clocked_split(server: &Server) -> (String, String) {
    create_budget(server);
    assert_eq!(server.post(&format!("/api/v1/budgets/{BUDGET}/accounts"),"account",json!({"id":ACCOUNT,"name":"Checking","opening_balance":0,"opening_date":"2026-01-01"})).status(),201);
    let path = format!("/api/v1/budgets/{BUDGET}/transactions");
    let id = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        clocked(
            server,
            "POST",
            &path,
            "create",
            C1,
            json!({"id":id,"account_id":ACCOUNT,"amount":-600,"date":"2026-01-01"})
        )
        .status(),
        201
    );
    let current: Value = server.get(&path).json().unwrap();
    let body = json!({"snapshot":current["snapshot"],"children":[{"id":uuid::Uuid::new_v4().to_string(),"amount":-100},{"id":uuid::Uuid::new_v4().to_string(),"amount":-500}]});
    let response = clocked(
        server,
        "POST",
        &format!("{path}/{id}/splits"),
        "split",
        C2,
        body,
    );
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    (path, id)
}
#[test]
fn duplicate_children_use_request_clock_and_accept_later_delete() {
    let server = Server::start();
    let (path, id) = setup_clocked_split(&server);
    let copy = uuid::Uuid::new_v4().to_string();
    let duplicate = clocked(
        &server,
        "POST",
        &format!("{path}/{id}/duplicate"),
        "duplicate",
        C3,
        json!({"id":copy}),
    );
    assert_eq!(duplicate.status(), 201);
    let row: Value = duplicate.json().unwrap();
    for child in row["data"]["children"].as_array().unwrap() {
        for clock in child["field_versions"].as_object().unwrap().values() {
            assert_eq!(
                clock, C3,
                "child fields must share the original duplicate clock"
            );
        }
    }
    let replay: Value = clocked(
        &server,
        "POST",
        &format!("{path}/{id}/duplicate"),
        "duplicate",
        C3,
        json!({"id":copy}),
    )
    .json()
    .unwrap();
    assert_eq!(replay, row);
    let deleted = clocked(
        &server,
        "DELETE",
        &format!("{path}/{copy}"),
        "delete-copy",
        C4,
        Value::Null,
    );
    assert_eq!(deleted.status(), 200, "{}", deleted.text().unwrap());
    let register: Value = server.get(&path).json().unwrap();
    assert_eq!(register["balances"]["total"], -600);
}
#[test]
fn deleted_split_parent_and_children_merge_later_fields_without_resurrection() {
    let server = Server::start();
    let (path, id) = setup_clocked_split(&server);
    let row: Value = server.get(&format!("{path}/{id}")).json().unwrap();
    let child = row["data"]["children"][0]["id"].as_str().unwrap();
    let deleted = clocked(
        &server,
        "DELETE",
        &format!("{path}/{id}"),
        "delete",
        C4,
        Value::Null,
    );
    assert_eq!(deleted.status(), 200);
    for target in [&id, child] {
        let later = clocked(
            &server,
            "PATCH",
            &format!("{path}/{target}"),
            target,
            C5,
            json!({"notes":"hidden winning note"}),
        );
        assert_eq!(later.status(), 200, "{}", later.text().unwrap());
        let hidden: Value = later.json().unwrap();
        assert_eq!(hidden["data"]["deleted"], true);
        assert_eq!(hidden["data"]["notes"], "hidden winning note");
        assert_eq!(hidden["data"]["field_versions"]["notes"], C5);
        assert_eq!(hidden["data"]["field_versions"]["tombstone"], C4);
        assert_eq!(server.get(&format!("{path}/{target}")).status(), 404);
    }
    let register: Value = server.get(&path).json().unwrap();
    assert_eq!(register["balances"]["total"], 0);
    assert_eq!(register["data"], json!([]));
}
#[test]
fn off_budget_split_moves_and_edits_preserve_existing_latent_categories() {
    let server = Server::start();
    let (path, id) = setup_clocked_split(&server);
    let off = uuid::Uuid::new_v4().to_string();
    let category = uuid::Uuid::new_v4().to_string();
    let other = uuid::Uuid::new_v4().to_string();
    assert_eq!(server.post(&format!("/api/v1/budgets/{BUDGET}/accounts"),"off",json!({"id":off,"name":"Off budget","off_budget":true,"opening_balance":0,"opening_date":"2026-01-01"})).status(),201);
    for cat in [&category, &other] {
        assert_eq!(
            server
                .post(
                    &format!("/api/v1/budgets/{BUDGET}/categories"),
                    cat,
                    json!({"id":cat,"name":cat,"is_income":false})
                )
                .status(),
            201
        );
    }
    let current: Value = server.get(&format!("{path}/{id}/splits")).json().unwrap();
    let mut children: Vec<Value> = current["data"]["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| json!({"id":c["id"],"amount":c["amount"],"category_id":category}))
        .collect();
    let categorized = server.post(
        &format!("{path}/{id}/splits"),
        "categorize",
        json!({"snapshot":current["snapshot"],"children":children}),
    );
    assert_eq!(categorized.status(), 200);
    let current: Value = server.get(&format!("{path}/{id}/splits")).json().unwrap();
    let moved = server.post(
        &format!("{path}/{id}/splits"),
        "move-off",
        json!({"snapshot":current["snapshot"],"fields":{"account_id":off},"children":children}),
    );
    assert_eq!(moved.status(), 200, "{}", moved.text().unwrap());
    children[0]["notes"] = json!("edit latent allocation");
    let current: Value = server.get(&format!("{path}/{id}/splits")).json().unwrap();
    let edited = server.post(
        &format!("{path}/{id}/splits"),
        "edit-off",
        json!({"snapshot":current["snapshot"],"children":children}),
    );
    assert_eq!(edited.status(), 200, "{}", edited.text().unwrap());
    let edited: Value = edited.json().unwrap();
    assert_eq!(edited["data"]["children"][0]["category_id"], category);
    assert_eq!(
        edited["data"]["children"][0]["notes"],
        "edit latent allocation"
    );
    let current: Value = server.get(&format!("{path}/{id}/splits")).json().unwrap();
    children[0]["category_id"] = json!(other);
    assert_eq!(
        server
            .post(
                &format!("{path}/{id}/splits"),
                "new-off-category",
                json!({"snapshot":current["snapshot"],"children":children})
            )
            .status(),
        400
    );
    assert_eq!(
        server
            .get(&format!("{path}/{id}/splits"))
            .json::<Value>()
            .unwrap(),
        current
    );
    children[0]["category_id"] = Value::Null;
    assert_eq!(
        server
            .post(
                &format!("{path}/{id}/splits"),
                "clear-off-category",
                json!({"snapshot":current["snapshot"],"children":children})
            )
            .status(),
        200
    );
    let register: Value = server
        .get(&format!("{path}?account_id={off}"))
        .json()
        .unwrap();
    assert_eq!(register["balances"]["total"], -600);
    let register: Value = server
        .get(&format!("{path}?account_id={ACCOUNT}"))
        .json()
        .unwrap();
    assert_eq!(register["balances"]["total"], 0);
}

#[test]
fn split_payee_aliases_and_removed_references_preserve_domain_rules() {
    let server = Server::start();
    create_budget(&server);
    let base = format!("/api/v1/budgets/{BUDGET}");
    assert_eq!(server.post(&format!("{base}/accounts"), "account", json!({"id":ACCOUNT,"name":"Checking","opening_balance":0,"opening_date":"2026-01-01"})).status(),201);
    let alias = uuid::Uuid::new_v4().to_string();
    let target = uuid::Uuid::new_v4().to_string();
    for (id, name) in [(&alias, "Alias"), (&target, "Target")] {
        assert_eq!(
            server
                .post(&format!("{base}/payees"), id, json!({"id":id,"name":name}))
                .status(),
            201
        );
    }
    let parent = uuid::Uuid::new_v4().to_string();
    let child = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        server
            .post(
                &format!("{base}/transactions"),
                "entry",
                json!({"id":parent,"account_id":ACCOUNT,"amount":-100,"date":"2026-01-02"})
            )
            .status(),
        201
    );
    let response = server.post(
        &format!("{base}/payees/batch"),
        "merge",
        json!({"action":"merge","ids":[alias],"target_id":target}),
    );
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let split = format!("{base}/transactions/{parent}/splits");
    let view: Value = server.get(&split).json().unwrap();
    let r=server.post(&split,"split-alias",json!({"snapshot":view["snapshot"],"children":[{"id":child,"amount":-100,"payee_id":alias}]}));
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    let result: Value = r.json().unwrap();
    assert_eq!(
        result["data"]["children"][0]["payee_id"], target,
        "Split choices resolve merged identities"
    );
    let r = server.post(
        &format!("{base}/payees/batch"),
        "remove",
        json!({"action":"remove","ids":[target]}),
    );
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    let view: Value = server.get(&split).json().unwrap();
    let r=server.post(&split,"retain",json!({"snapshot":view["snapshot"],"children":[{"id":child,"amount":-100,"payee_id":target,"notes":"Edited after removal"}]}));
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    let result: Value = r.json().unwrap();
    assert!(result["data"]["children"][0]["payee_id"].is_null());
    assert_eq!(result["data"]["children"][0]["retained_payee_id"], target);
    let view: Value = server.get(&split).json().unwrap();
    let r=server.post(&split,"new-removed",json!({"snapshot":view["snapshot"],"children":[{"id":child,"amount":-50,"payee_id":target},{"id":uuid::Uuid::new_v4(),"amount":-50,"payee_id":target}]}));
    assert_eq!(
        r.status(),
        400,
        "New allocations cannot select a removed payee: {}",
        r.text().unwrap()
    );
    let unchanged: Value = server.get(&split).json().unwrap();
    assert_eq!(
        unchanged, view,
        "Rejected split leaves aggregate and snapshot unchanged"
    );
}

#[test]
fn removed_parent_payee_changes_only_children_sharing_its_retained_identity() {
    for (clear, alias_child) in [(false, false), (true, false), (false, true)] {
        let server = Server::start();
        create_budget(&server);
        let base = format!("/api/v1/budgets/{BUDGET}");
        assert_eq!(server.post(&format!("{base}/accounts"),"account",json!({"id":ACCOUNT,"name":"Checking","opening_balance":0,"opening_date":"2026-01-01"})).status(),201);
        let old = uuid::Uuid::new_v4().to_string();
        let new = uuid::Uuid::new_v4().to_string();
        let alias = uuid::Uuid::new_v4().to_string();
        for (id, name) in [(&old, "Old"), (&new, "New"), (&alias, "Alias")] {
            assert_eq!(
                server
                    .post(&format!("{base}/payees"), id, json!({"id":id,"name":name}))
                    .status(),
                201
            );
        }
        let parent = uuid::Uuid::new_v4().to_string();
        let shared = uuid::Uuid::new_v4().to_string();
        let blank = uuid::Uuid::new_v4().to_string();
        assert_eq!(server.post(&format!("{base}/transactions"),"entry",json!({"id":parent,"account_id":ACCOUNT,"amount":-100,"date":"2026-01-02","payee_id":old})).status(),201);
        let path = format!("{base}/transactions/{parent}/splits");
        let view: Value = server.get(&path).json().unwrap();
        let r=server.post(&path,"split",json!({"snapshot":view["snapshot"],"children":[{"id":shared,"amount":-60,"payee_id":old},{"id":blank,"amount":-40}]}));
        assert_eq!(r.status(), 200, "{}", r.text().unwrap());
        assert_eq!(
            server
                .post(
                    &format!("{base}/payees/batch"),
                    "alias",
                    json!({"action":"merge","ids":[alias],"target_id":old})
                )
                .status(),
            200
        );
        assert_eq!(
            server
                .post(
                    &format!("{base}/payees/batch"),
                    "remove",
                    json!({"action":"remove","ids":[old]})
                )
                .status(),
            200
        );
        let view: Value = server.get(&path).json().unwrap();
        assert_eq!(view["data"]["retained_payee_id"], old);
        assert!(view["data"]["children"][1]["retained_payee_id"].is_null());
        let choice = if clear { Value::Null } else { json!(new) };
        let r=server.post(&path,"change-parent",json!({"snapshot":view["snapshot"],"fields":{"payee_id":choice},"children":[{"id":shared,"amount":-60,"payee_id":if alias_child{&alias}else{&old}},{"id":blank,"amount":-40}]}));
        assert_eq!(r.status(), 200, "{}", r.text().unwrap());
        let result: Value = r.json().unwrap();
        assert_eq!(
            result["data"]["children"][0]["retained_payee_id"], choice,
            "Shared removed payee follows the parent, including an alias selection"
        );
        assert!(
            result["data"]["children"][1]["retained_payee_id"].is_null(),
            "Unrelated blank child remains blank"
        );
    }
}

fn imported_large_splits() -> (Server, String, Value) {
    let server = Server::start();
    let imported: Value = server
        .client
        .post(format!("{}/api/v1/imports/actual", server.url))
        .header("Idempotency-Key", "large-allocation-import")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/reconciliation-large.zip").to_vec())
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(imported["data"]["status"], "ready");
    let activated = server.post(
        &format!(
            "/api/v1/imports/{}/activate",
            imported["data"]["id"].as_str().unwrap()
        ),
        "activate-large",
        json!({}),
    );
    assert_eq!(activated.status(), 201, "{}", activated.text().unwrap());
    let root = format!(
        "/api/v1/budgets/{}",
        imported["data"]["budget_id"].as_str().unwrap()
    );
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/reconciliation-large.json"
    ))
    .unwrap();
    (server, root, fixture)
}

#[test]
fn imported_large_allocations_can_be_replaced_beyond_the_ordinary_body_limit() {
    let (server, root, fixture) = imported_large_splits();
    for case in fixture["accounts"].as_array().unwrap() {
        let count = case["count"].as_u64().unwrap();
        let path = format!(
            "{root}/transactions/{}/splits",
            case["parent"].as_str().unwrap()
        );
        let before: Value = server.get(&path).json().unwrap();
        let children: Vec<Value> = before["data"]["children"].as_array().unwrap().iter().map(|c| json!({"id":c["id"],"amount":c["amount"],"notes":"Edited imported allocation", "payee_id":c["retained_payee_id"], "category_id":c["category_id"]})).collect();
        let body = json!({"snapshot":before["snapshot"],"children":children});
        if count == 1001 {
            assert!(body.to_string().len() > 65536);
        }
        let result = server.post(&path, &format!("replace-{count}"), body.clone());
        assert_eq!(result.status(), 200, "{}", result.text().unwrap());
        let receipt: Value = server
            .post(&path, &format!("replace-{count}"), body)
            .json()
            .unwrap();
        let after: Value = server.get(&path).json().unwrap();
        assert_eq!(receipt["data"], after["data"]);
        assert_eq!(after["data"]["amount"], -i64::try_from(count).unwrap());
        assert_eq!(
            after["data"]["children"].as_array().unwrap().len() as u64,
            count
        );
        for (old, new) in before["data"]["children"]
            .as_array()
            .unwrap()
            .iter()
            .zip(after["data"]["children"].as_array().unwrap())
        {
            assert_eq!(new["id"], old["id"]);
            assert_eq!(new["amount"], -1);
            assert_eq!(new["notes"], "Edited imported allocation");
            assert_eq!(new["category_id"], old["category_id"]);
        }
    }
}

#[test]
fn sparse_imported_allocations_redistribute_remove_duplicate_and_detach_atomically() {
    let (server, root, fixture) = imported_large_splits();
    let case = &fixture["accounts"][1];
    let path = format!(
        "{root}/transactions/{}/splits",
        case["parent"].as_str().unwrap()
    );
    let payee = "14000000-0000-4000-8000-000000000010";
    let category = "14000000-0000-4000-8000-000000000011";
    assert_eq!(
        server
            .post(
                &format!("{root}/payees"),
                "large-payee",
                json!({"id":payee,"name":"Allocation payee"})
            )
            .status(),
        201
    );
    assert_eq!(
        server
            .post(
                &format!("{root}/categories"),
                "large-category",
                json!({"id":category,"name":"Allocation category","is_income":false})
            )
            .status(),
        201
    );
    let before: Value = server.get(&path).json().unwrap();
    let children = before["data"]["children"].as_array().unwrap();
    let first = children[0]["id"].as_str().unwrap();
    let last = children[1000]["id"].as_str().unwrap();
    let body = json!({"snapshot":before["snapshot"],"allocation":"even","changes":{"upsert":[{"id":first,"amount":0,"notes":"Only this allocation changed","payee_id":payee,"category_id":category}],"remove":[last]}});
    assert!(body.to_string().len() < 512);
    let response = server.post(&path, "sparse-remove", body.clone());
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let receipt: Value = server.post(&path, "sparse-remove", body).json().unwrap();
    let after: Value = server.get(&path).json().unwrap();
    assert_eq!(receipt["data"], after["data"]);
    let changed = after["data"]["children"].as_array().unwrap();
    assert_eq!(changed.len(), 1000);
    assert_eq!(changed[0]["amount"], -2);
    assert_eq!(changed[0]["payee_id"], payee);
    assert_eq!(changed[0]["category_id"], category);
    assert_eq!(changed[0]["notes"], "Only this allocation changed");
    assert_eq!(changed[999]["id"], children[999]["id"]);
    assert_eq!(changed[999]["notes"], children[999]["notes"]);
    assert_eq!(after["data"]["amount"], -1001);
    // Duplicate IDs/unknown removals and invalid sums fail without partial field edits.
    for (name, changes) in [
        ("unknown-remove", json!({"remove":[last]})),
        (
            "duplicate-upsert",
            json!({"upsert":[{"id":first,"amount":-2},{"id":first,"amount":-2}]}),
        ),
        (
            "remove-upsert",
            json!({"upsert":[{"id":first,"amount":-2}],"remove":[first]}),
        ),
        ("bad-sum", json!({"upsert":[{"id":first,"amount":1}]})),
    ] {
        let failed = server.post(&path,name,json!({"snapshot":after["snapshot"],"changes":changes,"fields":{"notes":"must roll back"}}));
        assert_eq!(failed.status(), 400, "{}", failed.text().unwrap());
        assert_eq!(server.get(&path).json::<Value>().unwrap(), after);
    }
    let duplicate_id = "14000000-0000-4000-8000-000000000001";
    let duplicate_path = format!(
        "{root}/transactions/{}/duplicate",
        case["parent"].as_str().unwrap()
    );
    let duplicated = server.post(
        &duplicate_path,
        "duplicate-large",
        json!({"id":duplicate_id}),
    );
    assert_eq!(duplicated.status(), 201, "{}", duplicated.text().unwrap());
    let duplicate: Value = server
        .get(&format!("{root}/transactions/{duplicate_id}/splits"))
        .json()
        .unwrap();
    assert_eq!(duplicate["data"]["amount"], -1001);
    assert_eq!(
        duplicate["data"]["children"].as_array().unwrap().len(),
        1000
    );
    for (source, copy) in changed
        .iter()
        .zip(duplicate["data"]["children"].as_array().unwrap())
    {
        assert_ne!(source["id"], copy["id"]);
        assert_eq!(source["amount"], copy["amount"]);
        assert_eq!(source["notes"], copy["notes"]);
        assert_eq!(source["payee_id"], copy["payee_id"]);
        assert_eq!(source["category_id"], copy["category_id"]);
        assert_eq!(copy["reconciled"], false);
        assert_eq!(copy["cleared"], false);
    }
    let current: Value = server.get(&path).json().unwrap();
    let detached: Vec<Value> = changed[899..].iter().map(|c| c["id"].clone()).collect();
    let result = server.post(
        &path,
        "detach-many",
        json!({"snapshot":current["snapshot"],"action":"unsplit","detach":detached}),
    );
    assert_eq!(result.status(), 200, "{}", result.text().unwrap());
    let remaining: Value = server.get(&path).json().unwrap();
    assert_eq!(remaining["data"]["amount"], -900);
    assert_eq!(remaining["data"]["children"].as_array().unwrap().len(), 899);
    let register: Value = server
        .get(&format!(
            "{root}/transactions?account_id={}",
            case["account"].as_str().unwrap()
        ))
        .json()
        .unwrap();
    assert_eq!(register["balances"]["total"], -2002);
    // Companion eligibility stays strict after edits/duplication; detached ordinary rows are eligible.
    let companion: Value = server
        .get(&format!(
            "{root}/companion?account_id={}&retain={},{},{}",
            case["account"].as_str().unwrap(),
            case["parent"].as_str().unwrap(),
            duplicate_id,
            first
        ))
        .json()
        .unwrap();
    assert_eq!(companion["data"]["retained"], json!([]));
    assert_eq!(
        companion["data"]["unavailable"].as_array().unwrap().len(),
        3
    );
}

#[test]
fn sparse_large_protection_receipts_cancellation_and_native_epochs_stay_atomic() {
    let (mut server, root, fixture) = imported_large_splits();
    let case = &fixture["accounts"][1];
    let path = format!(
        "{root}/transactions/{}/splits",
        case["parent"].as_str().unwrap()
    );
    let reconciliation = format!(
        "{root}/accounts/{}/reconciliation",
        case["account"].as_str().unwrap()
    );
    let preview: Value = server.get(&reconciliation).json().unwrap();
    let lock = server.post(&reconciliation,"lock-large",json!({"action":"lock","snapshot":preview["data"]["snapshot"],"statement_balance":-1001,"date":"2026-02-28"}));
    assert_eq!(lock.status(), 200, "{}", lock.text().unwrap());
    let before: Value = server.get(&path).json().unwrap();
    let child = before["data"]["children"][1000]["id"].as_str().unwrap();
    let mut body = json!({"snapshot":before["snapshot"],"changes":{"upsert":[{"id":child,"amount":-1,"notes":"Protected final allocation"}]}});
    let refusal = server.post(&path, "no-proof", body.clone());
    assert_eq!(refusal.status(), 409, "{}", refusal.text().unwrap());
    assert_eq!(server.get(&path).json::<Value>().unwrap(), before);
    body["confirm_reconciled"] = json!(true);
    body["reconciliation_token"] = before["reconciliation_token"].clone();
    let response = server.post(&path, "protected-sparse", body.clone());
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let receipt: Value = response.json().unwrap();
    assert!(
        receipt["data"]["children"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["reconciled"] == true)
    );
    let current: Value = server.get(&path).json().unwrap();
    let mut stale = body.clone();
    stale["snapshot"] = current["snapshot"].clone();
    assert_eq!(server.post(&path, "stale-protection", stale).status(), 409);
    assert_eq!(server.get(&path).json::<Value>().unwrap(), current);
    server = server.restart();
    assert_eq!(
        server
            .post(&path, "protected-sparse", body.clone())
            .json::<Value>()
            .unwrap(),
        receipt
    );
    let mut canceled = body.clone();
    canceled["snapshot"] = current["snapshot"].clone();
    canceled["reconciliation_token"] = current["reconciliation_token"].clone();
    canceled["changes"]["upsert"][0]["notes"] = json!("Canceled large edit");
    let cancellation = server
        .client
        .post(format!("{}{path}", server.url))
        .header("Idempotency-Key", "cancel-large-sparse")
        .header("X-Actual-Cancel-If-Uncommitted", "true")
        .json(&canceled)
        .send()
        .unwrap();
    assert_eq!(cancellation.status(), 409);
    assert_eq!(
        cancellation.json::<Value>().unwrap()["error"]["code"],
        "operation_canceled"
    );
    server = server.restart();
    assert_eq!(
        server
            .post(&path, "cancel-large-sparse", canceled.clone())
            .json::<Value>()
            .unwrap()["error"]["code"],
        "operation_canceled"
    );
    assert_eq!(server.get(&path).json::<Value>().unwrap(), current);
    let backup = server.post(
        &format!("{root}/backups"),
        "large-native",
        json!({"label":"Large split allocation"}),
    );
    assert_eq!(backup.status(), 201, "{}", backup.text().unwrap());
    let backup: Value = backup.json().unwrap();
    let bytes = server
        .get(&format!(
            "{root}/backups/{}/download",
            backup["data"]["id"].as_str().unwrap()
        ))
        .bytes()
        .unwrap();
    let target = Server::start();
    let upload = target
        .client
        .post(format!("{}/api/v1/restores", target.url))
        .header("Idempotency-Key", "large-native-stage")
        .header("Content-Type", "application/zip")
        .body(bytes.to_vec())
        .send()
        .unwrap();
    assert_eq!(upload.status(), 201, "{}", upload.text().unwrap());
    let staged: Value = upload.json().unwrap();
    let activate = target.post(
        &format!(
            "{root}/restores/{}/activate",
            staged["data"]["id"].as_str().unwrap()
        ),
        "large-native-activate",
        json!({"confirm_name":"Large reconciled imports"}),
    );
    assert_eq!(activate.status(), 200, "{}", activate.text().unwrap());
    let target = target.restart();
    let restored: Value = target.get(&path).json().unwrap();
    assert_eq!(restored["data"], current["data"]);
    assert_ne!(restored["epoch"], current["epoch"]);
    assert_eq!(
        target
            .post(&path, "protected-sparse", body)
            .json::<Value>()
            .unwrap(),
        receipt
    );
    canceled["snapshot"] = restored["snapshot"].clone();
    let fenced = target
        .client
        .post(format!("{}{path}", target.url))
        .header("Idempotency-Key", "stale-epoch-sparse")
        .header("X-Actual-Budget-Epoch", current["epoch"].as_str().unwrap())
        .json(&canceled)
        .send()
        .unwrap();
    assert_eq!(fenced.status(), 409);
    assert_eq!(
        fenced.json::<Value>().unwrap()["error"]["code"],
        "budget_restored"
    );
    assert_eq!(target.get(&path).json::<Value>().unwrap(), restored);
}
