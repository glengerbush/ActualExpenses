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

#[test]
fn monthly_actuals_match_executed_tracking_and_envelope_engine() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/budgeting-26.10.0.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let s = Server::start();
        let bytes = if case["method"] == "tracking" {
            include_bytes!("../../tests/fixtures/budgeting-tracking-base.zip").as_slice()
        } else {
            include_bytes!("../../tests/fixtures/budgeting-envelope-base.zip").as_slice()
        };
        let imported = s
            .client
            .post(format!("{}/api/v1/imports/actual", s.url))
            .header("Content-Type", "application/zip")
            .header("Idempotency-Key", "import")
            .body(bytes.to_vec())
            .send()
            .unwrap()
            .json::<Value>()
            .unwrap();
        assert_eq!(imported["data"]["status"], "ready", "{imported}");
        let activated = s.post(
            &format!(
                "/api/v1/imports/{}/activate",
                imported["data"]["id"].as_str().unwrap()
            ),
            "activate",
            json!({}),
        );
        assert!(
            activated.status().is_success(),
            "{}",
            activated.text().unwrap()
        );
        let root = format!(
            "/api/v1/budgets/{}",
            imported["data"]["budget_id"].as_str().unwrap()
        );
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            if !step["action"].is_null() {
                let current = s
                    .get(&format!("{root}/budget?month=2026-11&count=4"))
                    .json::<Value>()
                    .unwrap();
                let mut body = step["action"].clone();
                body["snapshot"] = current["data"]["snapshot"].clone();
                for field in ["category", "from", "to"] {
                    if let Some(name) = body[field].as_str()
                        && name != "to-budget"
                    {
                        body[field] = case["ids"][name].clone();
                    }
                }
                let action = step["action"]["action"].as_str().unwrap();
                let r = if ["hide_category", "hide_group"].contains(&action) {
                    let table = if action == "hide_category" {
                        "categories"
                    } else {
                        "category-groups"
                    };
                    let id = if action == "hide_category" {
                        body["category"].as_str().unwrap()
                    } else {
                        case["ids"][step["action"]["group"].as_str().unwrap()]
                            .as_str()
                            .unwrap()
                    };
                    s.client
                        .patch(format!("{}{root}/{table}/{id}", s.url))
                        .header("Idempotency-Key", format!("action-{index}"))
                        .json(&json!({"hidden":body["enabled"]}))
                        .send()
                        .unwrap()
                } else {
                    s.post(&format!("{root}/budget"), &format!("action-{index}"), body)
                };
                assert_eq!(r.status(), 200, "{}", r.text().unwrap());
            }
            let response = s.get(&format!("{root}/budget?month=2026-11&count=4"));
            assert_eq!(response.status(), 200, "{}", response.text().unwrap());
            let data = response.json::<Value>().unwrap()["data"].clone();
            assert_eq!(data["method"], step["method"]);
            for (month, expected) in step["months"].as_object().unwrap() {
                for (key, value) in expected["summary"].as_object().unwrap() {
                    assert_eq!(
                        data["months"][month]["summary"][key.replace('-', "_")],
                        *value,
                        "{} {month} {key}",
                        case["method"]
                    );
                }
                for (name, expected) in expected["categories"].as_object().unwrap() {
                    let actual = data["months"][month]["categories"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|c| c["name"] == *name)
                        .unwrap();
                    for key in ["budgeted", "actual", "balance", "carryover"] {
                        assert_eq!(
                            actual[key], expected[key],
                            "{} {month} {name} {key}",
                            case["method"]
                        );
                    }
                }
            }
        }
    }
}

const B: &str = "17180000-0000-4000-8000-000000000001";
const C: &str = "17180000-0000-4000-8000-000000000002";
const D: &str = "17180000-0000-4000-8000-000000000003";
const OWNER: &str = "17180000-0000-4000-8000-000000000009";
fn setup() -> (Server, String) {
    let s = Server::start();
    assert_eq!(
        s.post(
            "/api/v1/budgets",
            "new",
            json!({"id":B,"name":"Plan","budget_type":"envelope"})
        )
        .status(),
        201
    );
    let root = format!("/api/v1/budgets/{B}");
    for (id, name) in [(C, "Food"), (D, "Travel")] {
        assert_eq!(
            s.post(
                &format!("{root}/categories"),
                id,
                json!({"id":id,"name":name,"is_income":false})
            )
            .status(),
            201
        );
    }
    (s, root)
}
fn plan(s: &Server, root: &str) -> Value {
    s.get(&format!("{root}/budget?month=2026-01"))
        .json::<Value>()
        .unwrap()["data"]
        .clone()
}
fn write_plan(s: &Server, root: &str, key: &str, mut body: Value, clock: Option<&str>) -> Response {
    body["snapshot"] = plan(s, root)["snapshot"].clone();
    body["month"] = json!("2026-01");
    let mut r = s
        .client
        .post(format!("{}{root}/budget", s.url))
        .header("Idempotency-Key", key)
        .header("X-Actual-History-Owner", OWNER)
        .json(&body);
    if let Some(c) = clock {
        r = r.header("X-Actual-Edit-Clock", c);
    }
    r.send().unwrap()
}
fn allocation(v: &Value, id: &str) -> Value {
    v["months"]["2026-01"]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .unwrap()["budgeted"]
        .clone()
}
#[test]
fn malformed_months_refuse_without_panicking_or_mutating() {
    let (s, root) = setup();
    for m in [
        "2026-+1",
        "+001-01",
        "2026-00",
        "0000-01",
        "2026-13",
        "２０２６-01",
    ] {
        let before = plan(&s, &root);
        let r=s.post(&format!("{root}/budget"),m,json!({"action":"set","month":m,"category":C,"amount":100,"snapshot":before["snapshot"]}));
        assert_eq!(r.status(), 400, "{m}: {}", r.text().unwrap());
        assert_eq!(plan(&s, &root)["snapshot"], before["snapshot"]);
    }
}
#[test]
fn stale_field_clock_refuses_entire_composite_action_with_fresh_snapshot() {
    let (s, root) = setup();
    let now = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now());
    let clock = |offset: i64| {
        format!(
            "{}-0000-1718000000000000",
            (now + chrono::Duration::seconds(offset))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        )
    };
    let a = clock(1);
    let b = clock(2);
    for (id, key) in [(C, "food"), (D, "travel")] {
        assert_eq!(
            write_plan(
                &s,
                &root,
                key,
                json!({"action":"set","category":id,"amount":200}),
                Some(&b)
            )
            .status(),
            200
        );
    }
    for (i, body) in [
        json!({"action":"set","category":C,"amount":100}),
        json!({"action":"move","from":C,"to":D,"amount":100}),
        json!({"action":"zero"}),
        json!({"action":"copy_year","category":C}),
    ]
    .into_iter()
    .enumerate()
    {
        // Copy-year targets different months; it is valid despite a newer source.
        let r = write_plan(&s, &root, &format!("stale-{i}"), body, Some(&a));
        assert_eq!(
            r.status(),
            if i == 3 { 200 } else { 409 },
            "{}",
            r.text().unwrap()
        );
        let v = plan(&s, &root);
        assert_eq!(allocation(&v, C), 200);
        assert_eq!(allocation(&v, D), 200);
    }
    for (i, body) in [
        json!({"action":"carryover","category":C,"enabled":true}),
        json!({"action":"notes","notes":"new"}),
        json!({"action":"category_notes","category":C,"notes":"new"}),
        json!({"action":"hold","amount":0}),
        json!({"action":"method","method":"envelope"}),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            write_plan(&s, &root, &format!("new-{i}"), body.clone(), Some(&b)).status(),
            200
        );
        assert_eq!(
            write_plan(&s, &root, &format!("old-{i}"), body, Some(&a)).status(),
            409
        );
    }
}

#[test]
fn genuine_schema15_upgrade_retains_history_and_replay_while_archive_rotates_owner_epoch() {
    let f: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/budgeting-schema15.json")).unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::write(
        data.path().join("actualbudget.sqlite3"),
        include_bytes!("../../tests/fixtures/budgeting-schema15.sqlite3"),
    )
    .unwrap();
    let s = Server::with_data(data);
    let budget = f["metadata"]["id"].as_str().unwrap();
    let root = format!("/api/v1/budgets/{budget}");
    let owner = f["owner_headers"]["X-Actual-History-Owner"]
        .as_str()
        .unwrap();
    let request = |server: &Server,
                   method: reqwest::Method,
                   path: &str,
                   key: Option<&str>,
                   body: Option<&Value>| {
        let mut r = server
            .client
            .request(method, format!("{}{path}", server.url))
            .header("X-Actual-History-Owner", owner)
            .header(
                "X-Actual-Budget-Epoch",
                f["metadata"]["epoch"].as_str().unwrap(),
            );
        if let Some(key) = key {
            r = r.header("Idempotency-Key", key);
        }
        if let Some(body) = body {
            r = r.json(body);
        }
        r.send().unwrap()
    };
    let history = request(
        &s,
        reqwest::Method::GET,
        &format!("{root}/history"),
        None,
        None,
    )
    .json::<Value>()
    .unwrap();
    assert_eq!(history, f["history"]);
    let rowpath = format!("{root}/transactions/{}", f["row"]["id"].as_str().unwrap());
    assert_eq!(s.get(&rowpath).json::<Value>().unwrap()["data"], f["row"]);
    assert_eq!(
        s.get(&root).json::<Value>().unwrap()["data"]["epoch"],
        f["metadata"]["epoch"]
    );
    // Exact original receipt keys are recorded by the authentic old-server generator.
    for (method, path, key, body, result) in [
        (
            reqwest::Method::POST,
            format!("{root}/transactions"),
            "create15",
            &f["creation"],
            &f["created"],
        ),
        (
            reqwest::Method::PATCH,
            rowpath.clone(),
            "edit15",
            &f["edit"],
            &f["edited"],
        ),
    ] {
        let expected = if method == reqwest::Method::POST {
            201
        } else {
            200
        };
        let r = request(&s, method, &path, Some(key), Some(body));
        assert_eq!(r.status(), expected, "{}", r.text().unwrap());
        assert_eq!(r.json::<Value>().unwrap(), *result);
    }
    let target = Server::start();
    let upload = target
        .client
        .post(format!("{}/api/v1/restores", target.url))
        .header("Idempotency-Key", "old-native")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/budgeting-schema15.zip").to_vec())
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap();
    assert_eq!(upload["data"]["report"]["schema_version"], 18, "{upload}");
    let activate = target.post(
        &format!(
            "{root}/restores/{}/activate",
            upload["data"]["id"].as_str().unwrap()
        ),
        "activate",
        json!({"confirm_name":f["metadata"]["name"]}),
    );
    assert_eq!(activate.status(), 200, "{}", activate.text().unwrap());
    assert_ne!(
        target.get(&root).json::<Value>().unwrap()["data"]["epoch"],
        f["metadata"]["epoch"]
    );
    assert_eq!(
        target.get(&rowpath).json::<Value>().unwrap()["data"],
        f["row"]
    );
    let h = target
        .client
        .get(format!("{}{root}/history", target.url))
        .header("X-Actual-History-Owner", owner)
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap();
    assert!(h["data"]["undo"].is_null());
}

#[test]
fn movement_notes_use_supported_currency_precision_and_undo_both_allocations_and_notes() {
    let (s, root) = setup();
    let prefs = s
        .client
        .patch(format!("{}{root}/preferences", s.url))
        .header("Idempotency-Key", "yen")
        .json(&json!({"experimental":{"currency":true},"currency_code":"JPY"}))
        .send()
        .unwrap();
    assert_eq!(prefs.status(), 200);
    assert_eq!(
        write_plan(
            &s,
            &root,
            "fund",
            json!({"action":"set","category":C,"amount":500}),
            None
        )
        .status(),
        200
    );
    let r = write_plan(
        &s,
        &root,
        "move",
        json!({"action":"move","from":C,"to":D,"amount":100}),
        None,
    );
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    let v = plan(&s, &root);
    assert_eq!(allocation(&v, C), 400);
    assert_eq!(allocation(&v, D), 100);
    assert!(
        v["months"]["2026-01"]["notes"]
            .as_str()
            .unwrap()
            .contains("Reassigned 100 from Food")
    );
    let h = s
        .client
        .get(format!("{}{root}/history", s.url))
        .header("X-Actual-History-Owner", OWNER)
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap();
    let undo = s
        .client
        .post(format!("{}{root}/history/undo", s.url))
        .header("Idempotency-Key", "undo")
        .header("X-Actual-History-Owner", OWNER)
        .json(&json!({"action_id":h["data"]["undo"]["id"]}))
        .send()
        .unwrap();
    assert_eq!(undo.status(), 200, "{}", undo.text().unwrap());
    let v = plan(&s, &root);
    assert_eq!(allocation(&v, C), 500);
    assert_eq!(allocation(&v, D), 0);
    assert_eq!(v["months"]["2026-01"]["notes"], "");
}

fn import_zip(s: &Server, bytes: &[u8]) -> (String, String) {
    let uploaded = s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "import")
        .body(bytes.to_vec())
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap();
    assert_eq!(uploaded["data"]["status"], "ready", "{uploaded}");
    let r = s.post(
        &format!(
            "/api/v1/imports/{}/activate",
            uploaded["data"]["id"].as_str().unwrap()
        ),
        "activate",
        json!({}),
    );
    assert_eq!(r.status(), 201, "{}", r.text().unwrap());
    let budget = uploaded["data"]["budget_id"].as_str().unwrap().to_owned();
    (format!("/api/v1/budgets/{budget}"), budget)
}
#[test]
fn allocated_actual_exports_and_native_archives_preserve_both_methods_notes_holds_and_actual_drilldown()
 {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/budgeting-26.10.0.json")).unwrap();
    for (case, bytes) in fixture["cases"].as_array().unwrap().iter().zip([
        include_bytes!("../../tests/fixtures/budgeting-tracking-allocated.zip").as_slice(),
        include_bytes!("../../tests/fixtures/budgeting-envelope-allocated.zip").as_slice(),
    ]) {
        let s = Server::start();
        let (root, _) = import_zip(&s, bytes);
        let v = s
            .get(&format!("{root}/budget?month=2026-11&count=4"))
            .json::<Value>()
            .unwrap()["data"]
            .clone();
        let expected = case["steps"].as_array().unwrap().last().unwrap();
        for (m, month) in expected["months"].as_object().unwrap() {
            for (k, n) in month["summary"].as_object().unwrap() {
                assert_eq!(
                    v["months"][m]["summary"][k.replace('-', "_")],
                    *n,
                    "{m} {k}"
                );
            }
            for c in v["months"][m]["categories"].as_array().unwrap() {
                let r = s
                    .get(&format!(
                        "{root}/budget/transactions?month={m}&category={}&snapshot={}",
                        c["id"].as_str().unwrap(),
                        v["snapshot"].as_str().unwrap()
                    ))
                    .json::<Value>()
                    .unwrap();
                assert_eq!(r["actual_total"], c["actual"]);
                assert!(
                    r["data"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|row| row["is_parent"] != true)
                );
            }
        }
        let backup = s
            .post(&format!("{root}/backups"), "backup", json!({}))
            .json::<Value>()
            .unwrap();
        let zip = s
            .get(&format!(
                "{root}/backups/{}/download",
                backup["data"]["id"].as_str().unwrap()
            ))
            .bytes()
            .unwrap();
        let t = Server::start();
        let stage = t
            .client
            .post(format!("{}/api/v1/restores", t.url))
            .header("Idempotency-Key", "native")
            .header("Content-Type", "application/zip")
            .body(zip)
            .send()
            .unwrap()
            .json::<Value>()
            .unwrap();
        assert_eq!(stage["data"]["report"]["schema_version"], 18, "{stage}");
        let metadata = s.get(&root).json::<Value>().unwrap();
        let r = t.post(
            &format!(
                "{root}/restores/{}/activate",
                stage["data"]["id"].as_str().unwrap()
            ),
            "restore",
            json!({"confirm_name":metadata["data"]["name"]}),
        );
        assert_eq!(r.status(), 200, "{}", r.text().unwrap());
        let restored = t
            .get(&format!("{root}/budget?month=2026-11&count=4"))
            .json::<Value>()
            .unwrap();
        assert_eq!(restored["data"]["months"], v["months"]);
        assert_eq!(restored["data"]["method"], v["method"]);
        assert_ne!(restored["data"]["epoch"], v["epoch"]);
    }
}
#[test]
fn category_merge_moves_plan_and_actuals_atomically_and_refuses_overflow() {
    let (s, root) = setup();
    for (id, n) in [(C, 200), (D, 300)] {
        assert_eq!(
            write_plan(
                &s,
                &root,
                id,
                json!({"action":"set","category":id,"amount":n}),
                None
            )
            .status(),
            200
        );
    }
    let r = s
        .client
        .delete(format!("{}{root}/categories/{C}", s.url))
        .header("Idempotency-Key", "merge")
        .header("X-Actual-History-Owner", OWNER)
        .json(&json!({"replacement_id":D}))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    let v = plan(&s, &root);
    assert_eq!(allocation(&v, D), 500);
    assert!(
        v["months"]["2026-01"]["categories"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["id"] != C)
    );
    let h = s
        .client
        .get(format!("{}{root}/history", s.url))
        .header("X-Actual-History-Owner", OWNER)
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap();
    let undo = s
        .client
        .post(format!("{}{root}/history/undo", s.url))
        .header("Idempotency-Key", "undo-merge")
        .header("X-Actual-History-Owner", OWNER)
        .json(&json!({"action_id":h["data"]["undo"]["id"]}))
        .send()
        .unwrap();
    assert_eq!(undo.status(), 200, "{}", undo.text().unwrap());
    assert_eq!(allocation(&plan(&s, &root), C), 200);
    assert_eq!(allocation(&plan(&s, &root), D), 300);
    let before = plan(&s, &root);
    let r = write_plan(
        &s,
        &root,
        "overflow",
        json!({"action":"set","category":C,"amount":9007199254740991i64}),
        None,
    );
    assert_eq!(r.status(), 400, "{}", r.text().unwrap());
    assert_eq!(plan(&s, &root)["snapshot"], before["snapshot"]);
    assert_eq!(allocation(&plan(&s, &root), C), 200);
}

#[test]
fn exact_receipts_lookup_cancellation_and_equal_clock_have_public_route_contracts() {
    let (s, root) = setup();
    let v = plan(&s, &root);
    let clock = format!(
        "{}-0000-1718000000000000",
        chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    );
    let body = json!({"action":"set","month":"2026-01","category":C,"amount":200,"snapshot":v["snapshot"]});
    let send = |key: &str, body: &Value, header: Option<&str>| {
        let mut r = s
            .client
            .post(format!("{}{root}/budget", s.url))
            .header("Idempotency-Key", key)
            .header("X-Actual-Edit-Clock", &clock)
            .header("X-Actual-History-Owner", OWNER)
            .header("X-Actual-Budget-Epoch", v["epoch"].as_str().unwrap())
            .json(body);
        if let Some(h) = header {
            r = r.header(h, "true");
        }
        r.send().unwrap()
    };
    let unknown = send("first", &body, Some("X-Actual-Receipt-Only"));
    assert_eq!(unknown.status(), 404, "{}", unknown.text().unwrap());
    let first = send("first", &body, None);
    assert_eq!(first.status(), 200);
    let first = first.json::<Value>().unwrap();
    assert_eq!(send("first", &body, None).json::<Value>().unwrap(), first);
    assert_eq!(
        send("first", &body, Some("X-Actual-Receipt-Only"))
            .json::<Value>()
            .unwrap(),
        first
    );
    assert_eq!(
        send("first", &body, Some("X-Actual-Cancel-If-Uncommitted"))
            .json::<Value>()
            .unwrap(),
        first
    );
    let mut changed = body.clone();
    changed["snapshot"] = plan(&s, &root)["snapshot"].clone();
    changed["amount"] = json!(100);
    assert_eq!(send("equal-clock", &changed, None).status(), 409);
    assert_eq!(allocation(&plan(&s, &root), C), 200);
    let canceled = send("canceled", &changed, Some("X-Actual-Cancel-If-Uncommitted"));
    assert_eq!(canceled.status(), 409);
    assert_eq!(
        canceled.json::<Value>().unwrap()["error"]["code"],
        "operation_canceled"
    );
    assert_eq!(
        send("canceled", &changed, None).json::<Value>().unwrap()["error"]["code"],
        "operation_canceled"
    );
    assert_eq!(allocation(&plan(&s, &root), C), 200);
}

#[test]
fn change_feed_serializes_cells_months_and_method_without_breaking_reference_refresh() {
    let (s, root) = setup();
    let before = plan(&s, &root);
    for (key, body) in [
        ("set", json!({"action":"set","category":C,"amount":500})),
        ("notes", json!({"action":"notes","notes":"Plan"})),
        ("method", json!({"action":"method","method":"tracking"})),
    ] {
        assert_eq!(write_plan(&s, &root, key, body, None).status(), 200);
    }
    let r = s.get(&format!(
        "{root}/changes?cursor={}",
        before["snapshot"].as_str().unwrap()
    ));
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    let v = r.json::<Value>().unwrap();
    for entity in ["budget_cells", "budget_months", "budget_settings"] {
        let row = v["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["entity"] == entity)
            .unwrap();
        assert_eq!(row["data"]["id"], row["id"]);
        assert_eq!(row["data"]["deleted"], false);
    }
    assert_eq!(v["next_cursor"], plan(&s, &root)["snapshot"]);
}

#[test]
fn planning_receipt_survives_restore_but_unknown_old_epoch_cannot_authorize_a_new_plan() {
    let (s, root) = setup();
    let v = plan(&s, &root);
    let body = json!({"action":"set","month":"2026-01","category":C,"amount":200,"snapshot":v["snapshot"]});
    let send = |key: &str, body: &Value| {
        s.client
            .post(format!("{}{root}/budget", s.url))
            .header("Idempotency-Key", key)
            .header("X-Actual-History-Owner", OWNER)
            .header("X-Actual-Budget-Epoch", v["epoch"].as_str().unwrap())
            .json(body)
            .send()
            .unwrap()
    };
    let original = send("original", &body);
    assert_eq!(original.status(), 200);
    let original = original.json::<Value>().unwrap();
    let backup = s
        .post(&format!("{root}/backups"), "backup", json!({}))
        .json::<Value>()
        .unwrap();
    let restored = s.post(
        &format!(
            "{root}/backups/{}/restore",
            backup["data"]["id"].as_str().unwrap()
        ),
        "restore",
        json!({"confirm_name":"Plan"}),
    );
    assert_eq!(restored.status(), 200, "{}", restored.text().unwrap());
    assert_ne!(plan(&s, &root)["epoch"], v["epoch"]);
    assert_eq!(send("original", &body).json::<Value>().unwrap(), original);
    let mut unknown = body;
    unknown["snapshot"] = plan(&s, &root)["snapshot"].clone();
    unknown["amount"] = json!(999);
    let r = send("unknown-old-epoch", &unknown);
    assert_eq!(r.status(), 409);
    assert_eq!(
        r.json::<Value>().unwrap()["error"]["code"],
        "budget_restored"
    );
    assert_eq!(allocation(&plan(&s, &root), C), 200);
}
