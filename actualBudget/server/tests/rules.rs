#[path = "support/retired_rows.rs"]
mod retirement;
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
    for original_case in f["cases"].as_array().unwrap() {
        let mut case = original_case.clone();
        if case["rules"].as_array().unwrap().iter().any(|r| {
            r["conditions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["field"] == "notes")
        }) {
            let rule = case["rules"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| {
                    r["conditions"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|c| c["field"] == "notes")
                })
                .unwrap();
            assert_eq!(s.post(&format!("{root}/rules/preview"), "retired-condition", json!({"snapshot":cursor(&s,&root),"rule":rule,"transaction":{"id":E,"account_id":f["ids"]["account"],"amount":-100,"date":"2026-01-01"}})).status(), 400);
            continue;
        }
        let note_only = case["rules"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| r["actions"].as_array().unwrap())
            .all(|a| matches!(a["op"].as_str(), Some("append-notes" | "prepend-notes")));
        let matched = original_case["after"]["notes"] != original_case["transaction"]["notes"];
        for rule in case["rules"].as_array_mut().unwrap() {
            let actions = rule["actions"].as_array_mut().unwrap();
            if note_only {
                for action in actions {
                    *action = json!({"op":"set","field":"date","value":"2025-12-31"});
                }
            } else {
                actions.retain(|a| {
                    !(a["field"] == "notes"
                        || matches!(a["op"].as_str(), Some("append-notes" | "prepend-notes")))
                });
            }
        }
        if note_only && matched {
            case["after"]["date"] = json!("2025-12-31");
        }
        case = retirement::current(case);
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
        assert_eq!(
            response.status(),
            200,
            "{} {}",
            case["name"],
            response.text().unwrap()
        );
        let preview = response.json::<Value>().unwrap();
        let after = &preview["data"]["transactions"][0]["after"];
        for (native, source) in [
            ("amount", "amount"),
            ("date", "date"),
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
        case = retirement::current(case);
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
    let rule = json!({"id":R,"stage":null,"conditionsOp":"and","conditions":[{"field":"amount","op":"lt","value":0}],"actions":[{"op":"set","field":"date","value":"2026-01-02"}],"snapshot":cursor(&s,&root)});
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
    let r=s.client.patch(format!("{}{root}/rules/{R}",s.url)).header("Idempotency-Key","remote-rule-change").json(&json!({"snapshot":cursor(&s,&root),"actions":[{"op":"set","field":"date","value":"2026-01-03"}]})).send().unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(send().json::<Value>().unwrap(), applied);
    assert_eq!(
        s.get(&format!("{root}/transactions/{E}"))
            .json::<Value>()
            .unwrap()["data"]["date"],
        "2026-01-02"
    );
    assert_eq!(
        s.get(&format!("{root}/transactions/{F}"))
            .json::<Value>()
            .unwrap()["data"]["date"],
        "2026-01-01"
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
            .unwrap()["data"]["date"],
        "2026-01-01"
    );
}

#[test]
fn selective_apply_shares_one_regex_budget_and_rolls_back_the_whole_command() {
    let (s, root) = setup();
    for index in 0..32 {
        let id = format!("19190000-0000-4000-8000-{:012}", 500 + index);
        let conditions = vec![json!({"field":"payee_name","op":"matches","value":"a"}); 32];
        let r=s.post(&format!("{root}/rules"),&id,json!({"id":id,"stage":null,"conditionsOp":"and","conditions":conditions,"actions":[{"op":"set","field":"date","value":"2026-01-02"}],"snapshot":cursor(&s,&root)}));
        assert_eq!(r.status(), 201, "{}", r.text().unwrap());
    }
    assert_eq!(
        s.post(
            &format!("{root}/payees"),
            "long-payee",
            json!({"id":F,"name":"a".repeat(120)})
        )
        .status(),
        201
    );
    let mut ids = vec![];
    for index in 0..100 {
        let id = format!("19190000-0000-4000-8000-{:012}", 1000 + index);
        let r = s.post(
            &format!("{root}/transactions"),
            &id,
            json!({"id":id,"account_id":A,"amount":-1,"date":"2026-01-01","payee_id":F}),
        );
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
            .all(|row| row["date"] == "2026-01-01")
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
    let expected_row = retirement::current(f["row"].clone());
    let actual_row = s.get(&rowpath).json::<Value>().unwrap();
    for (key, value) in expected_row.as_object().unwrap() {
        assert_eq!(&actual_row["data"][key], value, "{key}");
    }
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
    let planning = s
        .get(&format!("{root}/budget?month=2026-01"))
        .json::<Value>()
        .unwrap();
    let expected = &f["planning_view"]["data"]["months"]["2026-01"];
    let actual = &planning["data"]["months"]["2026-01"];
    for category in expected["categories"].as_array().unwrap() {
        let id = &category["id"];
        let actual_category = actual["categories"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| &c["id"] == id)
            .unwrap();
        for field in ["actual", "budgeted", "balance"] {
            assert_eq!(actual_category[field], category[field], "{id}/{field}");
        }
    }
    for (key, value) in expected["summary"].as_object().unwrap() {
        assert_eq!(&actual["summary"][key], value, "{key}");
    }
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
    assert_eq!(activated.status(), 409);
    assert_eq!(
        activated.json::<Value>().unwrap()["error"]["code"],
        "import_budget_conflict"
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
    assert_eq!(upload["data"]["report"]["schema_version"], 37, "{upload}");
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
    let restored = target.get(&rowpath).json::<Value>().unwrap();
    for (key, value) in expected_row.as_object().unwrap() {
        assert_eq!(&restored["data"][key], value, "{key}");
    }
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
    assert_eq!(rules["data"].as_array().unwrap().len(), 2);
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
    assert_eq!(stage["data"]["report"]["schema_version"], 37, "{stage}");
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
    let mut state = json!({"cash":left.into_iter().map(|r|json!({"amount":r["amount"],"date":r["date"],"cleared":r["cleared"],"notes":r["notes"],"children":r["children"].as_array().map(|cs|cs.iter().map(|c|json!({"amount":c["amount"],"date":c["date"],"cleared":c["cleared"],"notes":c["notes"],"paired":c["transfer_id"].is_string()})).collect::<Vec<_>>()).unwrap_or_default()})).collect::<Vec<_>>(),"other":right.into_iter().map(|r|json!({"amount":r["amount"],"date":r["date"],"cleared":r["cleared"],"notes":r["notes"],"paired":r["transfer_id"].is_string()})).collect::<Vec<_>>(),"balances":[cash["balance"].clone(),other["balance"].clone()],"reciprocal_balanced":flat.iter().filter(|r|r["transfer_id"].is_string()).all(|r|flat.iter().any(|p|p["id"]==r["transfer_id"]&&p["transfer_id"]==r["id"]&&p["amount"].as_i64()==r["amount"].as_i64().map(|v|-v)))});
    state["other"].as_array_mut().unwrap().sort_by_key(|r| {
        (
            r["amount"].as_i64().unwrap(),
            r["date"].as_str().unwrap().to_owned(),
            r["notes"].as_str().unwrap().to_owned(),
        )
    });
    state
}
#[test]
fn persisted_transfer_rule_oracle_preserves_valid_pairs_and_refuses_contradictory_amounts_atomically()
 {
    let oracle: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/rules-26.10.0.json"))
            .map(retirement::current)
            .unwrap();
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
            30000 + index * 100 + usize::from(case["split"] == true && case["parent"] != true)
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
        if case["replacement_refusal"] == true
            && case["actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["field"] != "cleared")
        {
            assert!(
                [400, 409].contains(&r.status().as_u16()),
                "{} {}",
                case["name"],
                r.text().unwrap()
            );
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

fn import_financial(index: usize) -> (Server, String, String, Value) {
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
    assert_eq!(r.status(), 201);
    let root = format!(
        "/api/v1/budgets/{}",
        uploaded["data"]["budget_id"].as_str().unwrap()
    );
    let id = format!("19190000-0000-4000-8000-{:012}", 30000 + index * 100);
    let row = s
        .get(&format!("{root}/transactions/{id}"))
        .json::<Value>()
        .unwrap()["data"]
        .clone();
    (s, root, id, row)
}
#[test]
fn selected_peers_keep_only_each_rules_explicit_field_intent() {
    for only_amount in [false, true] {
        let (s, root, id, row) = import_financial(0);
        let peer = row["transfer_id"].as_str().unwrap();
        let other = s
            .get(&format!("{root}/transactions/{peer}"))
            .json::<Value>()
            .unwrap()["data"]
            .clone();
        let actions = if only_amount {
            json!([{"op":"set","field":"amount","value":-77}])
        } else {
            json!([{"op":"set","field":"date","value":"2026-12-01"}])
        };
        let rule = json!({"id":R,"stage":null,"conditionsOp":"and","conditions":[{"field":"account","op":"is","value":row["account_id"]}],"actions":actions,"snapshot":cursor(&s,&root)});
        assert_eq!(
            s.post(&format!("{root}/rules"), "ruleA", rule).status(),
            201
        );
        if !only_amount {
            let rule = json!({"id":F,"stage":null,"conditionsOp":"and","conditions":[{"field":"account","op":"is","value":other["account_id"]}],"actions":[{"op":"set","field":"date","value":"2026-12-02"}],"snapshot":cursor(&s,&root)});
            assert_eq!(
                s.post(&format!("{root}/rules"), "ruleB", rule).status(),
                201
            );
        }
        let r = s.post(
            &format!("{root}/rules/apply"),
            "apply",
            json!({"ids":[id,peer],"snapshot":cursor(&s,&root)}),
        );
        assert_eq!(r.status(), 200, "{}", r.text().unwrap());
        let a = s
            .get(&format!("{root}/transactions/{id}"))
            .json::<Value>()
            .unwrap()["data"]
            .clone();
        let b = s
            .get(&format!("{root}/transactions/{peer}"))
            .json::<Value>()
            .unwrap()["data"]
            .clone();
        if only_amount {
            assert_eq!(a["amount"], -77);
            assert_eq!(b["amount"], 77);
        } else {
            assert_eq!(a["date"], "2026-12-01");
            assert_eq!(b["date"], "2026-12-02");
            assert_eq!(b["cleared"], true);
        }
    }
}

#[test]
fn automatic_entry_protection_matches_the_executed_desktop_helper() {
    let (s, root) = setup();
    for id in [E, F] {
        assert_eq!(
            s.post(
                &format!("{root}/categories"),
                &format!("category{id}"),
                json!({"id":id,"name":id,"is_income":false})
            )
            .status(),
            201
        );
        assert_eq!(
            s.post(
                &format!("{root}/payees"),
                &format!("payee{id}"),
                json!({"id":id,"name":id})
            )
            .status(),
            201
        );
        assert_eq!(
            s.post(
                &format!("{root}/accounts"),
                &format!("account{id}"),
                json!({"id":id,"name":id,"opening_balance":0,"opening_date":"2026-01-01"})
            )
            .status(),
            201
        );
    }
    let oracle: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/rules-26.10.0.json")).unwrap();
    for case in oracle["manual_protection"].as_array().unwrap() {
        let field = case["field"].as_str().unwrap();
        if matches!(field, "imported_payee" | "notes") {
            continue;
        }
        let key = match field {
            "payee" => "payee_id",
            "category" => "category_id",
            "account" => "account_id",
            v => v,
        };
        let convert = |v: &Value| {
            if ["payee", "category", "account"].contains(&field) && v.is_string() {
                json!(if v == "manual" { E } else { F })
            } else {
                v.clone()
            }
        };
        let current = convert(&case["current"]);
        let next = convert(&case["next"]);
        let mut row = json!({"id":R,"account_id":A,"date":"2026-01-01","amount":-100,"notes":"","cleared":false,"category_id":null,"payee_id":null});
        row[key] = current.clone();
        let rule = json!({"id":R,"stage":null,"conditionsOp":"and","conditions":[{"field":"amount","op":"gte","value":-1000}],"actions":[{"op":"set","field":field,"value":next}]});
        let r = s.post(
            &format!("{root}/rules/preview"),
            "preview",
            json!({"snapshot":cursor(&s,&root),"rule":rule,"transaction":row,"manual_entry":true}),
        );
        assert_eq!(r.status(), 200, "{case} {}", r.text().unwrap());
        let r = r.json::<Value>().unwrap();
        assert_eq!(
            r["data"]["transactions"][0]["after"][key],
            if case["apply"] == true { next } else { current },
            "{case}"
        );
    }
}
#[test]
fn automatic_entry_is_one_atomic_owned_action_with_manual_values_and_exact_replay() {
    check_automatic_entry();
}
#[test]
fn calculated_rule_actions_are_rejected_without_writes() {
    let (s, root) = setup();
    let before = cursor(&s, &root);
    let r=s.post(&format!("{root}/rules"), "retired-formula", json!({"id":R,"stage":null,"conditionsOp":"and","conditions":[],"actions":[{"op":"set","field":"amount","value":null,"options":{"formula":"=-7.77"}}],"snapshot":before}));
    assert_eq!(r.status(), 400);
    assert_eq!(cursor(&s, &root), before);
}
fn check_automatic_entry() {
    let (s, root) = setup();
    for (id, name) in [(E, "Food"), (F, "Travel")] {
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
    let rule = json!({"id":R,"stage":null,"conditionsOp":"and","conditions":[{"field":"amount","op":"lt","value":0}],"actions":[{"op":"set","field":"category","value":E},{"op":"set","field":"amount","value":-777},{"op":"set","field":"date","value":"2025-01-01"},{"op":"set","field":"cleared","value":true}],"snapshot":cursor(&s,&root)});
    assert_eq!(s.post(&format!("{root}/rules"), "rule", rule).status(), 201);
    let transaction = json!({"id":E,"account_id":A,"amount":-100,"date":"2026-01-01","notes":"Manual","category_id":F});
    let mut body = transaction;
    body["snapshot"] = cursor(&s, &root);
    let send = || {
        s.client
            .post(format!("{}{root}/rules/transactions", s.url))
            .header("Idempotency-Key", "automatic")
            .header("X-Actual-History-Owner", OWNER)
            .json(&body)
            .send()
            .unwrap()
    };
    let r = send();
    assert_eq!(r.status(), 201, "{}", r.text().unwrap());
    let original = r.json::<Value>().unwrap();
    for (field, value) in [
        ("amount", json!(-100)),
        ("date", json!("2026-01-01")),
        ("category_id", json!(F)),
        ("cleared", json!(true)),
        ("notes", json!("")),
    ] {
        assert_eq!(original["data"][field], value);
    }
    let r=s.client.patch(format!("{}{root}/rules/{R}",s.url)).header("Idempotency-Key","remote").json(&json!({"snapshot":cursor(&s,&root),"actions":[{"op":"set","field":"date","value":"2026-01-03"}]})).send().unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(send().json::<Value>().unwrap(), original);
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
    assert_eq!(s.get(&format!("{root}/transactions/{E}")).status(), 404);
}

#[test]
fn rules_contract_cancellation_lookup_and_field_clocks_keep_original_intent() {
    let (s, root) = setup();
    let spec = s.get("/api/v1/openapi.json").json::<Value>().unwrap();
    for suffix in [
        "",
        "/{id}",
        "/preview",
        "/apply",
        "/learning",
        "/associations",
        "/rename-payee",
        "/transactions",
    ] {
        assert!(spec["paths"][format!("/api/v1/budgets/{{budget}}/rules{suffix}")].is_object());
    }
    let info = s.get("/api/v1/info").json::<Value>().unwrap();
    assert!(
        info["data"]["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("rule_entry"))
    );
    let clock = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let a = format!("{clock}-0001-0000000000000019");
    let b = format!("{clock}-0002-0000000000000019");
    let body = json!({"id":R,"stage":null,"conditionsOp":"and","conditions":[{"field":"amount","op":"lt","value":0}],"actions":[{"op":"set","field":"date","value":"2026-01-02"}],"snapshot":cursor(&s,&root)});
    let post = |key: &str, lookup: bool, cancel: bool| {
        let mut r = s
            .client
            .post(format!("{}{root}/rules", s.url))
            .header("Idempotency-Key", key)
            .header("X-Actual-Edit-Clock", &a)
            .header("X-Actual-History-Owner", OWNER)
            .json(&body);
        if lookup {
            r = r.header("X-Actual-Receipt-Only", "true");
        }
        if cancel {
            r = r.header("X-Actual-Cancel-If-Uncommitted", "true");
        }
        r.send().unwrap()
    };
    let before = cursor(&s, &root);
    let canceled = post("cancel", false, true);
    assert_eq!(canceled.status(), 409);
    assert_eq!(
        canceled.json::<Value>().unwrap()["error"]["code"],
        "operation_canceled"
    );
    assert_eq!(post("cancel", false, false).status(), 409);
    assert_eq!(cursor(&s, &root), before);
    let absent = post("create", true, false);
    assert_eq!(absent.status(), 404);
    assert_eq!(
        absent.json::<Value>().unwrap()["error"]["code"],
        "operation_uncommitted"
    );
    assert_eq!(cursor(&s, &root), before);
    let created = post("create", false, false);
    assert_eq!(created.status(), 201, "{}", created.text().unwrap());
    let original = created.json::<Value>().unwrap();
    assert_eq!(
        post("create", true, false).json::<Value>().unwrap(),
        original
    );
    let newer = json!({"snapshot":cursor(&s,&root),"actions":[{"op":"set","field":"date","value":"2026-01-03"}]});
    let r = s
        .client
        .patch(format!("{}{root}/rules/{R}", s.url))
        .header("Idempotency-Key", "winning")
        .header("X-Actual-Edit-Clock", &b)
        .json(&newer)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    for clock in [&a, &b] {
        let before = cursor(&s, &root);
        let r = s
            .client
            .patch(format!("{}{root}/rules/{R}", s.url))
            .header("Idempotency-Key", format!("stale{clock}"))
            .header("X-Actual-Edit-Clock", clock)
            .json(&json!({"snapshot":before,"actions":[{"op":"set","field":"date","value":"2026-01-04"}]}))
            .send()
            .unwrap();
        assert_eq!(r.status(), 409, "{}", r.text().unwrap());
        assert_eq!(cursor(&s, &root), before);
    }
    let current = s.get(&format!("{root}/rules/{R}")).json::<Value>().unwrap();
    assert_eq!(current["data"]["actions"][0]["value"], "2026-01-03");
    assert_eq!(current["data"]["field_versions"]["actions"], b);
    let malformed=s.post(&format!("{root}/rules/preview"),"bad",json!({"snapshot":cursor(&s,&root),"transaction":{"id":E,"account_id":A,"amount":"wrong","date":"2026-01-01"}}));
    assert_eq!(malformed.status(), 400);
    assert_eq!(
        s.post(
            &format!("{root}/payees"),
            "rename-target",
            json!({"id":F,"name":"Target"})
        )
        .status(),
        201
    );
    let mut rename_id = Value::Null;
    for (key, names) in [
        ("renameA", json!(["RAW", ""])),
        ("renameB", json!(["RAW", "OTHER"])),
    ] {
        let r = s.post(
            &format!("{root}/rules/rename-payee"),
            key,
            json!({"snapshot":cursor(&s,&root),"payee_id":F,"imported_payees":names}),
        );
        assert_eq!(r.status(), 200, "{}", r.text().unwrap());
        let r = r.json::<Value>().unwrap();
        if rename_id.is_null() {
            rename_id = r["data"]["id"].clone();
        } else {
            assert_eq!(r["data"]["id"], rename_id);
            assert_eq!(r["data"]["conditions"][0]["value"], json!(["RAW", "OTHER"]));
        }
    }
    let associated = s
        .get(&format!("{root}/rules?payee_id={F}"))
        .json::<Value>()
        .unwrap();
    assert_eq!(associated["data"].as_array().unwrap().len(), 1);
    // Retired expressions are rejected even when submitted directly.
    let action =
        json!({"op":"set","field":"notes","value":"","options":{"template":"{{payee_name}}"}});
    let r=s.post(&format!("{root}/rules"),"template42",json!({"id":F,"stage":null,"conditionsOp":"and","conditions":[],"actions":[action],"snapshot":cursor(&s,&root)}));
    assert_eq!(r.status(), 400, "{}", r.text().unwrap());
}

#[test]
fn rule_history_refuses_a_newer_overlapping_pair_edit() {
    let (s, root, id, row) = import_financial(0);
    let peer = row["transfer_id"].as_str().unwrap();
    let r=s.post(&format!("{root}/rules"),"rule",json!({"id":R,"stage":null,"conditionsOp":"and","conditions":[{"field":"amount","op":"lt","value":0}],"actions":[{"op":"set","field":"amount","value":-77}],"snapshot":cursor(&s,&root)}));
    assert_eq!(r.status(), 201);
    let applied = s
        .client
        .post(format!("{}{root}/rules/apply", s.url))
        .header("Idempotency-Key", "owned-apply")
        .header("X-Actual-History-Owner", OWNER)
        .json(&json!({"snapshot":cursor(&s,&root),"ids":[id]}))
        .send()
        .unwrap();
    assert_eq!(applied.status(), 200, "{}", applied.text().unwrap());
    assert_eq!(
        s.get(&format!("{root}/transactions/{peer}"))
            .json::<Value>()
            .unwrap()["data"]["amount"],
        77
    );
    let history = s
        .client
        .get(format!("{}{root}/history", s.url))
        .header("X-Actual-History-Owner", OWNER)
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap();
    let remote = s
        .client
        .patch(format!("{}{root}/transactions/{id}", s.url))
        .header("Idempotency-Key", "remote-amount")
        .json(&json!({"amount":-88}))
        .send()
        .unwrap();
    assert_eq!(remote.status(), 200, "{}", remote.text().unwrap());
    let before = cursor(&s, &root);
    let state = financial_state(&s, &root);
    let undo = s
        .client
        .post(format!("{}{root}/history/undo", s.url))
        .header("Idempotency-Key", "refuse-overlap")
        .header("X-Actual-History-Owner", OWNER)
        .json(&json!({"action_id":history["data"]["undo"]["id"]}))
        .send()
        .unwrap();
    assert_eq!(undo.status(), 409, "{}", undo.text().unwrap());
    assert_eq!(cursor(&s, &root), before);
    assert_eq!(financial_state(&s, &root), state);
    assert_eq!(
        s.get(&format!("{root}/transactions/{peer}"))
            .json::<Value>()
            .unwrap()["data"]["amount"],
        88
    );
}

#[test]
fn historical_native_archive_retires_note_rules_without_losing_financial_actions() {
    use sha2::{Digest, Sha256};
    use std::io::{Cursor, Read, Write};
    for predicate in [false, true] {
        // Authentic old-server archive structure, with a legacy note action/predicate.
        // Recompute only the changed entry's manifest; original receipts stay byte-for-byte data.
        let mut source = zip::ZipArchive::new(Cursor::new(include_bytes!(
            "../../tests/fixtures/budget-automation-schema19.zip"
        )))
        .unwrap();
        let mut entries = std::collections::BTreeMap::new();
        for index in 0..source.len() {
            let mut file = source.by_index(index).unwrap();
            let mut bytes = vec![];
            file.read_to_end(&mut bytes).unwrap();
            entries.insert(file.name().to_owned(), bytes);
        }
        let mut state: Value = serde_json::from_slice(&entries["budget.json"]).unwrap();
        let original_receipts = state["receipts"].clone();
        let budget = state["budget"]["id"].as_str().unwrap().to_owned();
        let name = state["budget"]["name"].clone();
        let row = &mut state["tables"]["rules"]["rows"][0];
        let original_actions: Value = serde_json::from_str(row[5].as_str().unwrap()).unwrap();
        let mut actions = original_actions.clone();
        actions
            .as_array_mut()
            .unwrap()
            .push(json!({"op":"append-notes","value":"legacy memo"}));
        row[5] = json!(actions.to_string());
        if predicate {
            let mut conditions: Value = serde_json::from_str(row[4].as_str().unwrap()).unwrap();
            conditions
                .as_array_mut()
                .unwrap()
                .push(json!({"field":"notes","op":"contains","value":"legacy memo"}));
            row[4] = json!(conditions.to_string());
        }
        assert_eq!(state["receipts"], original_receipts);
        let payload = serde_json::to_vec(&state).unwrap();
        let mut manifest: Value = serde_json::from_slice(&entries["manifest.json"]).unwrap();
        manifest["entries"]["budget.json"] =
            json!({"bytes":payload.len(),"sha256":format!("{:x}",Sha256::digest(&payload))});
        entries.insert("budget.json".into(), payload);
        entries.insert(
            "manifest.json".into(),
            serde_json::to_vec(&manifest).unwrap(),
        );
        let mut zip = zip::ZipWriter::new(Cursor::new(vec![]));
        for (name, bytes) in entries {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&bytes).unwrap();
        }
        let s = Server::start();
        let upload = s
            .client
            .post(format!("{}/api/v1/restores", s.url))
            .header("Idempotency-Key", "historical-notes")
            .header("Content-Type", "application/zip")
            .body(zip.finish().unwrap().into_inner())
            .send()
            .unwrap();
        assert_eq!(upload.status(), 201, "{}", upload.text().unwrap());
        let stage = upload.json::<Value>().unwrap();
        let root = format!("/api/v1/budgets/{budget}");
        let activated = s.post(
            &format!(
                "{root}/restores/{}/activate",
                stage["data"]["id"].as_str().unwrap()
            ),
            "activate",
            json!({"confirm_name":name}),
        );
        assert_eq!(activated.status(), 200, "{}", activated.text().unwrap());
        let rules = s.get(&format!("{root}/rules")).json::<Value>().unwrap();
        assert_eq!(
            rules["data"].as_array().unwrap().len(),
            usize::from(!predicate)
        );
        if !predicate {
            assert_eq!(rules["data"][0]["actions"], original_actions);
        }
        let schedules = s.get(&format!("{root}/schedules")).json::<Value>().unwrap();
        assert_eq!(
            schedules["data"]["schedules"].as_array().unwrap().len(),
            usize::from(!predicate)
        );
        let epoch = s.get(&root).json::<Value>().unwrap()["data"]["epoch"]
            .as_str()
            .unwrap()
            .to_owned();
        let backup = s
            .client
            .post(format!("{}{root}/backups", s.url))
            .header("Idempotency-Key", "receipt-proof")
            .header("X-Actual-Budget-Epoch", epoch)
            .json(&json!({}))
            .send()
            .unwrap();
        assert_eq!(backup.status(), 201, "{}", backup.text().unwrap());
        let backup = backup.json::<Value>().unwrap();
        let bytes = s
            .get(&format!(
                "{root}/backups/{}/download",
                backup["data"]["id"].as_str().unwrap()
            ))
            .bytes()
            .unwrap();
        let mut exported = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut restored = vec![];
        exported
            .by_name("budget.json")
            .unwrap()
            .read_to_end(&mut restored)
            .unwrap();
        let restored: Value = serde_json::from_slice(&restored).unwrap();
        for receipt in original_receipts.as_array().unwrap() {
            assert!(
                restored["receipts"].as_array().unwrap().contains(receipt),
                "Historical receipt changed: {receipt}"
            );
        }
        let transactions = &state["tables"]["transactions"];
        let columns = transactions["columns"].as_array().unwrap();
        let index = |field: &str| columns.iter().position(|c| c == field).unwrap();
        for row in transactions["rows"].as_array().unwrap() {
            if row[index("tombstone")] == 1 {
                continue;
            }
            let actual = s
                .get(&format!(
                    "{root}/transactions/{}",
                    row[index("id")].as_str().unwrap()
                ))
                .json::<Value>()
                .unwrap();
            for field in ["amount", "date", "account_id"] {
                assert_eq!(actual["data"][field], row[index(field)], "{field}");
            }
        }
    }
}
