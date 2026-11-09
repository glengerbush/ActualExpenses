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
fn ordered_rules_match_the_independent_engine_without_preview_side_effects() {
    let f: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/rules-26.10.0.json")).unwrap();
    let s = Server::start();
    let b = "19190000-0000-4000-8000-000000000099";
    assert_eq!(
        s.post("/api/v1/budgets", "budget", json!({"id":b,"name":"Rules"}))
            .status(),
        201
    );
    let root = format!("/api/v1/budgets/{b}");
    assert_eq!(s.post(&format!("{root}/accounts"),"account",json!({"id":f["ids"]["account"],"name":"Cash","opening_balance":0,"opening_date":"2026-01-01"})).status(),201);
    assert_eq!(
        s.post(
            &format!("{root}/category-groups"),
            "group",
            json!({"id":f["ids"]["group"],"name":"Expenses","is_income":false})
        )
        .status(),
        201
    );
    for name in ["Food", "Travel"] {
        assert_eq!(
            s.post(
                &format!("{root}/categories"),
                name,
                json!({"id":f["ids"][name],"name":name,"is_income":false,"group_id":f["ids"]["group"]})
            )
            .status(),
            201
        );
    }
    assert_eq!(
        s.post(
            &format!("{root}/payees"),
            "shop",
            json!({"id":f["ids"]["Shop"],"name":"Shop"})
        )
        .status(),
        201
    );
    for case in f["cases"].as_array().unwrap() {
        for rule in case["rules"].as_array().unwrap() {
            let cursor = s
                .get(&format!("{root}/checkpoint"))
                .json::<Value>()
                .unwrap()["data"]["snapshot"]
                .clone();
            let mut rule = rule.clone();
            rule["snapshot"] = cursor;
            let r = s.post(
                &format!("{root}/rules"),
                rule["id"].as_str().unwrap(),
                rule.clone(),
            );
            assert_eq!(r.status(), 201, "{}", r.text().unwrap());
        }
        let checkpoint = s
            .get(&format!("{root}/checkpoint"))
            .json::<Value>()
            .unwrap();
        let mut row = case["transaction"].clone();
        for (from, to) in [
            ("account", "account_id"),
            ("payee", "payee_id"),
            ("category", "category_id"),
        ] {
            row[to] = row.as_object_mut().unwrap().remove(from).unwrap();
        }
        let response = s.post(
            &format!("{root}/rules/preview"),
            "preview",
            json!({"snapshot":checkpoint["data"]["snapshot"],"transaction":row}),
        );
        assert_eq!(response.status(), 200, "{}", response.text().unwrap());
        let preview = response.json::<Value>().unwrap();
        let after = &preview["data"]["transactions"][0]["after"];
        for (native, source) in [
            ("amount", "amount"),
            ("date", "date"),
            ("notes", "notes"),
            ("cleared", "cleared"),
            ("payee_id", "payee"),
            ("category_id", "category"),
        ] {
            if native == "payee_id" && case["new_payee_name"].is_string() {
                continue;
            }
            assert_eq!(
                after[native], case["after"][source],
                "{} {native}",
                case["name"]
            );
        }
        if case["new_payee_name"] == "Generated shop" {
            assert_eq!(after["new_payee"]["name"], "Generated shop");
        }
        if case["after"]["tombstone"] == 1 {
            assert_eq!(after["deleted"], true);
        }
        if let Some(children) = case["after"]["subtransactions"].as_array() {
            let actual = after["children"].as_array().unwrap();
            assert_eq!(actual.len(), children.len());
            for (a, c) in actual.iter().zip(children) {
                for (native, source) in [
                    ("amount", "amount"),
                    ("notes", "notes"),
                    ("payee_id", "payee"),
                    ("category_id", "category"),
                ] {
                    let expected = if native == "notes" {
                        json!(c[source].as_str().unwrap_or(""))
                    } else {
                        c[source].clone()
                    };
                    assert_eq!(a[native], expected, "{} child {native}", case["name"]);
                }
            }
        }
        assert_eq!(
            s.get(&format!("{root}/checkpoint"))
                .json::<Value>()
                .unwrap(),
            checkpoint
        );
        assert_eq!(
            s.get(&format!("{root}/transactions"))
                .json::<Value>()
                .unwrap()["data"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        for rule in case["rules"].as_array().unwrap() {
            let cursor = s
                .get(&format!("{root}/checkpoint"))
                .json::<Value>()
                .unwrap()["data"]["snapshot"]
                .clone();
            let r = s
                .client
                .delete(format!(
                    "{}{root}/rules/{}",
                    s.url,
                    rule["id"].as_str().unwrap()
                ))
                .header(
                    "Idempotency-Key",
                    format!("delete-{}", rule["id"].as_str().unwrap()),
                )
                .json(&json!({"snapshot":cursor}))
                .send()
                .unwrap();
            assert_eq!(r.status(), 200, "{}", r.text().unwrap());
        }
    }
}

const B: &str = "19190000-0000-4000-8000-000000000001";
const A: &str = "19190000-0000-4000-8000-000000000002";
const R: &str = "19190000-0000-4000-8000-000000000003";
const E: &str = "19190000-0000-4000-8000-000000000004";
const F: &str = "19190000-0000-4000-8000-000000000005";
const OWNER: &str = "19190000-0000-4000-8000-000000000009";
fn setup() -> (Server, String) {
    setup_balance(0)
}
fn setup_balance(balance: i64) -> (Server, String) {
    let s = Server::start();
    assert_eq!(
        s.post("/api/v1/budgets", "new", json!({"id":B,"name":"Rules"}))
            .status(),
        201
    );
    let root = format!("/api/v1/budgets/{B}");
    assert_eq!(
        s.post(
            &format!("{root}/accounts"),
            "account",
            json!({"id":A,"name":"Cash","opening_balance":balance,"opening_date":"2026-01-01"})
        )
        .status(),
        201
    );
    (s, root)
}
fn cursor(s: &Server, root: &str) -> Value {
    s.get(&format!("{root}/checkpoint"))
        .json::<Value>()
        .unwrap()["data"]["snapshot"]
        .clone()
}
#[test]
fn preview_selective_apply_is_one_replay_safe_owned_action() {
    let (s, root) = setup();
    let rule = json!({"id":R,"stage":null,"conditionsOp":"and","conditions":[{"field":"amount","op":"lt","value":0}],"actions":[{"op":"append-notes","value":" / checked"}],"snapshot":cursor(&s,&root)});
    assert_eq!(s.post(&format!("{root}/rules"), "rule", rule).status(), 201);
    for (id, notes) in [(E, "One"), (F, "Two")] {
        assert_eq!(
            s.post(
                &format!("{root}/transactions"),
                id,
                json!({"id":id,"account_id":A,"amount":-101,"date":"2026-01-01","notes":notes})
            )
            .status(),
            201
        );
    }
    let before = cursor(&s, &root);
    let preview = s.post(
        &format!("{root}/rules/preview"),
        "preview",
        json!({"snapshot":before}),
    );
    assert_eq!(preview.status(), 200, "{}", preview.text().unwrap());
    let preview = preview.json::<Value>().unwrap();
    assert_eq!(preview["data"]["total_count"], 2);
    assert_eq!(cursor(&s, &root), before);
    let body = json!({"snapshot":before,"ids":[E],"rule_ids":[R]});
    let send = || {
        s.client
            .post(format!("{}{root}/rules/apply", s.url))
            .header("Idempotency-Key", "apply")
            .header("X-Actual-History-Owner", OWNER)
            .json(&body)
            .send()
            .unwrap()
    };
    let applied = send();
    assert_eq!(applied.status(), 200, "{}", applied.text().unwrap());
    let applied = applied.json::<Value>().unwrap();
    assert_eq!(applied["data"]["affected_count"], 1);
    let r=s.client.patch(format!("{}{root}/rules/{R}",s.url)).header("Idempotency-Key","remote-rule-change").json(&json!({"snapshot":cursor(&s,&root),"actions":[{"op":"append-notes","value":" different"}]})).send().unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(send().json::<Value>().unwrap(), applied);
    assert_eq!(
        s.get(&format!("{root}/transactions/{E}"))
            .json::<Value>()
            .unwrap()["data"]["notes"],
        "One / checked"
    );
    assert_eq!(
        s.get(&format!("{root}/transactions/{F}"))
            .json::<Value>()
            .unwrap()["data"]["notes"],
        "Two"
    );
    let history = s
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
        .json(&json!({"action_id":history["data"]["undo"]["id"]}))
        .send()
        .unwrap();
    assert_eq!(undo.status(), 200, "{}", undo.text().unwrap());
    assert_eq!(
        s.get(&format!("{root}/transactions/{E}"))
            .json::<Value>()
            .unwrap()["data"]["notes"],
        "One"
    );
}

#[test]
fn selective_apply_shares_one_regex_budget_and_rolls_back_the_whole_command() {
    let (s, root) = setup();
    for index in 0..2 {
        let id = format!("19190000-0000-4000-8000-{:012}", 500 + index);
        let conditions = vec![json!({"field":"notes","op":"matches","value":"a"}); 32];
        let r=s.post(&format!("{root}/rules"),&id,json!({"id":id,"stage":null,"conditionsOp":"and","conditions":conditions,"actions":[{"op":"set","field":"cleared","value":true}],"snapshot":cursor(&s,&root)}));
        assert_eq!(r.status(), 201, "{}", r.text().unwrap());
    }
    let mut ids = vec![];
    for index in 0..100 {
        let id = format!("19190000-0000-4000-8000-{:012}", 1000 + index);
        let r=s.post(&format!("{root}/transactions"),&id,json!({"id":id,"account_id":A,"amount":-1,"date":"2026-01-01","notes":"a".repeat(10000)}));
        assert_eq!(r.status(), 201);
        ids.push(id);
    }
    let before = cursor(&s, &root);
    let r = s.post(
        &format!("{root}/rules/apply"),
        "bounded",
        json!({"snapshot":before,"ids":ids}),
    );
    assert_eq!(r.status(), 400, "{}", r.text().unwrap());
    assert_eq!(
        r.json::<Value>().unwrap()["error"]["code"],
        "regex_work_limit"
    );
    assert_eq!(cursor(&s, &root), before);
    let rows = s
        .get(&format!("{root}/transactions?limit=100"))
        .json::<Value>()
        .unwrap();
    assert!(
        rows["data"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["cleared"] == false)
    );
}

#[test]
fn opt_in_learning_matches_the_executed_engine_and_native_style_overrides_do_not_teach() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/rules-26.10.0.json")).unwrap();
    let today = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).date_naive();
    let frozen = chrono::NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
    let shift = today - frozen;
    let ca = "19190000-0000-4000-8000-000000000601";
    let cb = "19190000-0000-4000-8000-000000000602";
    let payee = "19190000-0000-4000-8000-000000000603";
    for case in fixture["learning"].as_array().unwrap() {
        let (s, root) = setup_balance(if case["closed"] == true {
            case["rows"].as_array().unwrap().len() as i64 * 100
        } else {
            0
        });
        for (id, name) in [(ca, "A"), (cb, "B")] {
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
        assert_eq!(
            s.post(
                &format!("{root}/payees"),
                "shop",
                json!({"id":payee,"name":"Learning shop"})
            )
            .status(),
            201
        );
        let category = |v: &Value| {
            if v == "A" {
                json!(ca)
            } else if v == "B" {
                json!(cb)
            } else {
                Value::Null
            }
        };
        for row in case["rows"].as_array().unwrap() {
            let date = chrono::NaiveDate::parse_from_str(row["date"].as_str().unwrap(), "%Y-%m-%d")
                .unwrap()
                + shift;
            let r=s.post(&format!("{root}/transactions"),row["id"].as_str().unwrap(),json!({"id":row["id"],"account_id":A,"payee_id":payee,"category_id":category(&row["category"]),"amount":row["amount"],"date":date.to_string(),"notes":row["notes"]}));
            assert_eq!(r.status(), 201, "{}", r.text().unwrap());
        }
        if case["closed"] == true {
            let r = s.post(&format!("{root}/accounts/{A}/close"), "close", json!({}));
            assert_eq!(r.status(), 200, "{}", r.text().unwrap());
        }
        for (field, key) in [("enabled", "payee-learning"), ("global", "global-learning")] {
            if case[field] == false {
                let mut body = json!({"snapshot":cursor(&s,&root),"enabled":false});
                if field == "enabled" {
                    body["payee_id"] = json!(payee);
                }
                let r = s.post(&format!("{root}/rules/learning"), key, body);
                assert_eq!(r.status(), 200, "{}", r.text().unwrap());
            }
        }
        for (index, value) in case["existing"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let id = format!("19190000-0000-4000-8000-{:012}", 700 + index);
            let r=s.post(&format!("{root}/rules"),&id,json!({"id":id,"stage":null,"conditionsOp":"and","conditions":[{"field":"payee","op":"is","value":payee}],"actions":[{"op":"set","field":"category","value":category(value)}],"snapshot":cursor(&s,&root)}));
            assert_eq!(r.status(), 201);
        }
        let mut body = json!({"category_id":category(&case["updated"]["category"])});
        if case["learn"] != false {
            body["learn_categories"] = json!(true);
        }
        let path = format!(
            "{root}/transactions/{}",
            case["updated"]["id"].as_str().unwrap()
        );
        let send = || {
            s.client
                .patch(format!("{}{path}", s.url))
                .header("Idempotency-Key", "manual")
                .header("X-Actual-History-Owner", OWNER)
                .json(&body)
                .send()
                .unwrap()
        };
        let response = send();
        assert_eq!(
            response.status(),
            200,
            "{}: {}",
            case["name"],
            response.text().unwrap()
        );
        let response = response.json::<Value>().unwrap();
        assert_eq!(send().json::<Value>().unwrap(), response);
        let rules = s.get(&format!("{root}/rules")).json::<Value>().unwrap();
        let mut categories = rules["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                if r["actions"][0]["value"] == ca {
                    "A"
                } else if r["actions"][0]["value"] == cb {
                    "B"
                } else {
                    "unexpected"
                }
            })
            .collect::<Vec<_>>();
        categories.sort();
        assert_eq!(
            json!(categories),
            case["expected_categories"],
            "{}",
            case["name"]
        );
        if case["name"] == "manual category edit learns" {
            let history = s
                .client
                .get(format!("{}{root}/history", s.url))
                .header("X-Actual-History-Owner", OWNER)
                .send()
                .unwrap()
                .json::<Value>()
                .unwrap();
            let r = s
                .client
                .post(format!("{}{root}/history/undo", s.url))
                .header("Idempotency-Key", "undo-learning")
                .header("X-Actual-History-Owner", OWNER)
                .json(&json!({"action_id":history["data"]["undo"]["id"]}))
                .send()
                .unwrap();
            assert_eq!(r.status(), 200, "{}", r.text().unwrap());
            assert_eq!(
                s.get(&path).json::<Value>().unwrap()["data"]["category_id"],
                ca
            );
            assert_eq!(
                s.get(&format!("{root}/rules")).json::<Value>().unwrap()["data"],
                json!([])
            );
        }
    }
}

#[test]
fn genuine_schema16_upgrade_retains_history_and_replay_while_archive_rotates_owner_epoch() {
    let f: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/rules-schema16.json")).unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::write(
        data.path().join("actualbudget.sqlite3"),
        include_bytes!("../../tests/fixtures/rules-schema16.sqlite3"),
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
            "create16",
            &f["creation"],
            &f["created"],
        ),
        (
            reqwest::Method::PATCH,
            rowpath.clone(),
            "edit16",
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
    let plan = request(
        &s,
        reqwest::Method::POST,
        &format!("{root}/budget"),
        Some("plan16"),
        Some(&f["planning_body"]),
    );
    assert_eq!(plan.status(), 200, "{}", plan.text().unwrap());
    assert_eq!(plan.json::<Value>().unwrap(), f["planning"]);
    assert_eq!(
        s.get(&format!("{root}/budget?month=2026-01"))
            .json::<Value>()
            .unwrap(),
        f["planning_view"]
    );
    let stage_id = f["stage"]["data"]["id"].as_str().unwrap();
    let stage = s
        .get(&format!("/api/v1/imports/{stage_id}"))
        .json::<Value>()
        .unwrap();
    assert_eq!(stage["data"]["status"], "ready", "{stage}");
    let activated = s.post(
        &format!("/api/v1/imports/{stage_id}/activate"),
        "activate-supported-rules",
        json!({}),
    );
    assert_eq!(activated.status(), 201, "{}", activated.text().unwrap());
    let imported = stage["data"]["budget_id"].as_str().unwrap();
    assert_eq!(
        s.get(&format!("/api/v1/budgets/{imported}/rules"))
            .json::<Value>()
            .unwrap()["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let target = Server::start();
    let upload = target
        .client
        .post(format!("{}/api/v1/restores", target.url))
        .header("Idempotency-Key", "old-native")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/rules-schema16.zip").to_vec())
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap();
    assert_eq!(upload["data"]["report"]["schema_version"], 17, "{upload}");
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
fn genuine_rule_import_and_current_native_restore_keep_execution_and_simple_associations() {
    let s = Server::start();
    let uploaded = s
        .client
        .post(format!("{}/api/v1/imports/actual", s.url))
        .header("Idempotency-Key", "source")
        .header("Content-Type", "application/zip")
        .body(include_bytes!("../../tests/fixtures/rules-base-26.10.0.zip").to_vec())
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap();
    assert_eq!(uploaded["data"]["status"], "ready", "{uploaded}");
    let stage = uploaded["data"]["id"].as_str().unwrap();
    let r = s.post(
        &format!("/api/v1/imports/{stage}/activate"),
        "activate",
        json!({}),
    );
    assert_eq!(r.status(), 201, "{}", r.text().unwrap());
    let root = format!(
        "/api/v1/budgets/{}",
        uploaded["data"]["budget_id"].as_str().unwrap()
    );
    let rules = s.get(&format!("{root}/rules")).json::<Value>().unwrap();
    assert_eq!(rules["data"].as_array().unwrap().len(), 3);
    let f: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/rules-26.10.0.json")).unwrap();
    let associations = s
        .get(&format!("{root}/rules/associations"))
        .json::<Value>()
        .unwrap();
    assert_eq!(
        associations["data"][f["ids"]["Shop"].as_str().unwrap()],
        f["ids"]["Food"]
    );
    let backup = s
        .post(&format!("{root}/backups"), "backup", json!({}))
        .json::<Value>()
        .unwrap();
    let id = backup["data"]["id"].as_str().unwrap();
    let zip = s
        .get(&format!("{root}/backups/{id}/download"))
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
    assert_eq!(stage["data"]["report"]["schema_version"], 17, "{stage}");
    let name = s.get(&root).json::<Value>().unwrap()["data"]["name"].clone();
    let r = t.post(
        &format!(
            "{root}/restores/{}/activate",
            stage["data"]["id"].as_str().unwrap()
        ),
        "activate-native",
        json!({"confirm_name":name}),
    );
    assert_eq!(r.status(), 200, "{}", r.text().unwrap());
    assert_eq!(
        t.get(&format!("{root}/rules")).json::<Value>().unwrap()["data"],
        rules["data"]
    );
    assert_eq!(
        t.get(&format!("{root}/rules/associations"))
            .json::<Value>()
            .unwrap()["data"],
        associations["data"]
    );
}

fn financial_state(s: &Server, root: &str) -> Value {
    let accounts = s.get(&format!("{root}/accounts")).json::<Value>().unwrap();
    let cash = accounts["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "Cash")
        .unwrap();
    let other = accounts["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "Other")
        .unwrap();
    let rows = s
        .get(&format!("{root}/transactions?limit=100"))
        .json::<Value>()
        .unwrap();
    let rows = rows["data"].as_array().unwrap();
    let left = rows
        .iter()
        .filter(|r| r["account_id"] == cash["id"] && r["parent_id"].is_null())
        .collect::<Vec<_>>();
    let right = rows
        .iter()
        .filter(|r| r["account_id"] == other["id"] && r["parent_id"].is_null())
        .collect::<Vec<_>>();
    let mut flat = rows.iter().collect::<Vec<_>>();
    for r in rows {
        if let Some(children) = r["children"].as_array() {
            flat.extend(children);
        }
    }
    json!({"cash":left.into_iter().map(|r|json!({"amount":r["amount"],"notes":r["notes"],"children":r["children"].as_array().map(|cs|cs.iter().map(|c|json!({"amount":c["amount"],"notes":c["notes"],"paired":c["transfer_id"].is_string()})).collect::<Vec<_>>()).unwrap_or_default()})).collect::<Vec<_>>(),"other":right.into_iter().map(|r|json!({"amount":r["amount"],"notes":r["notes"],"paired":r["transfer_id"].is_string()})).collect::<Vec<_>>(),"balances":[cash["balance"].clone(),other["balance"].clone()],"reciprocal_balanced":flat.iter().filter(|r|r["transfer_id"].is_string()).all(|r|flat.iter().any(|p|p["id"]==r["transfer_id"]&&p["transfer_id"]==r["id"]&&p["amount"].as_i64()==r["amount"].as_i64().map(|v|-v)))})
}
#[test]
fn persisted_transfer_rule_oracle_preserves_valid_pairs_and_refuses_contradictory_amounts_atomically()
 {
    let oracle: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/rules-26.10.0.json")).unwrap();
    for (index, case) in oracle["financial"].as_array().unwrap().iter().enumerate() {
        let s = Server::start();
        let source = std::fs::read(format!(
            "{}/../tests/fixtures/rules-financial-{index}-26.10.0.zip",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let uploaded = s
            .client
            .post(format!("{}/api/v1/imports/actual", s.url))
            .header("Idempotency-Key", "source")
            .header("Content-Type", "application/zip")
            .body(source)
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
        let root = format!(
            "/api/v1/budgets/{}",
            uploaded["data"]["budget_id"].as_str().unwrap()
        );
        assert_eq!(
            financial_state(&s, &root),
            case["initial"],
            "initial {}",
            case["name"]
        );
        let id = format!(
            "19190000-0000-4000-8000-{:012}",
            30000 + index * 100 + usize::from(case["split"] == true)
        );
        let row = s
            .get(&format!("{root}/transactions/{id}"))
            .json::<Value>()
            .unwrap();
        let mut ids = vec![json!(id)];
        if case["both"] == true {
            ids.push(row["data"]["transfer_id"].clone());
        }
        let rule = json!({"id":R,"stage":null,"conditionsOp":"and","conditions":[{"field":"amount","op":"gte","value":-1000}],"actions":case["actions"],"snapshot":cursor(&s,&root)});
        let r = s.post(&format!("{root}/rules"), "rule", rule);
        assert_eq!(r.status(), 201, "{}", r.text().unwrap());
        let before = cursor(&s, &root);
        let epoch = s
            .get(&format!("{root}/checkpoint"))
            .json::<Value>()
            .unwrap()["data"]["epoch"]
            .clone();
        let r = s
            .client
            .post(format!("{}{root}/rules/apply", s.url))
            .header("Idempotency-Key", "apply")
            .header("X-Actual-History-Owner", OWNER)
            .header("X-Actual-Budget-Epoch", epoch.as_str().unwrap())
            .json(&json!({"snapshot":before,"ids":ids}))
            .send()
            .unwrap();
        if case["after"]["reciprocal_balanced"] == false {
            assert_eq!(r.status(), 409, "{}", r.text().unwrap());
            assert_eq!(cursor(&s, &root), before);
            assert_eq!(financial_state(&s, &root), case["initial"]);
            let h = s
                .client
                .get(format!("{}{root}/history", s.url))
                .header("X-Actual-History-Owner", OWNER)
                .send()
                .unwrap()
                .json::<Value>()
                .unwrap();
            assert!(h["data"]["undo"].is_null(), "{h}");
        } else {
            assert_eq!(r.status(), 200, "{} {}", case["name"], r.text().unwrap());
            assert_eq!(
                financial_state(&s, &root),
                case["after"],
                "{}",
                case["name"]
            );
        }
    }
}
