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
const BUDGET: &str = "12140000-0000-4000-8000-000000000099";
const ACCOUNT: &str = "12140000-0000-4000-8000-000000000098";
fn read(server: &Server, path: &str, query: Option<&Value>) -> Value {
    let mut request = server.client.get(format!("{}{path}", server.url));
    if let Some(query) = query {
        request = request.query(&[("query", query.to_string())]);
    }
    let response = request.send().unwrap();
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    response.json().unwrap()
}
fn write(server: &Server, path: &str, body: Value) -> Value {
    let response = server.post(path, &uuid::Uuid::new_v4().to_string(), body);
    assert!(
        response.status().is_success(),
        "{}",
        response.text().unwrap()
    );
    response.json().unwrap()
}
fn setup(server: &Server) -> (String, Value) {
    write(
        server,
        "/api/v1/budgets",
        json!({"id":BUDGET,"name":"Register integration"}),
    );
    let root = format!("/api/v1/budgets/{BUDGET}");
    write(
        server,
        &format!("{root}/accounts"),
        json!({"id":ACCOUNT,"name":"Spending","opening_balance":0,"opening_date":"2026-01-01"}),
    );
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/register-integration-26.10.json"
    ))
    .unwrap();
    for (key, path) in [
        ("groups", "category-groups"),
        ("categories", "categories"),
        ("payees", "payees"),
    ] {
        for row in fixture[key].as_array().unwrap() {
            write(server, &format!("{root}/{path}"), row.clone());
        }
    }
    let path = format!("{root}/transactions");
    for source in fixture["inputs"].as_array().unwrap() {
        let mut row = source.clone();
        let children = row.as_object_mut().unwrap().remove("children");
        row["account_id"] = json!(ACCOUNT);
        row["date"] = json!("2026-01-02");
        write(server, &path, row.clone());
        if let Some(children) = children {
            let before = read(server, &path, None);
            write(
                server,
                &format!("{path}/{}/splits", row["id"].as_str().unwrap()),
                json!({"snapshot":before["snapshot"],"children":children,"allocation":"exact"}),
            );
        }
    }
    (root, fixture)
}
#[test]
fn pinned_grouped_predicates_include_null_and_match_one_complete_child() {
    let server = Server::start();
    let (root, fixture) = setup(&server);
    let path = format!("{root}/transactions");
    for case in fixture["cases"].as_array().unwrap() {
        let mut query = case["query"].clone();
        if case["name"].as_str().unwrap().contains("tags") {
            let response = server
                .client
                .get(format!("{}{path}", server.url))
                .query(&[("query", query.to_string())])
                .send()
                .unwrap();
            assert_eq!(response.status(), 400);
            continue;
        }
        // Keep the same parent/child conjunction using a retained amount field.
        if case["name"] == "parent and child AND fails" {
            query["filter"]["conditions"][0] = json!({"field":"amount","op":"eq","value":-300});
        }
        if query["search"] == "Second allocation" {
            query["search"] = json!("Second category");
            query["filter"]["conditions"]
                .as_array_mut()
                .unwrap()
                .push(json!({"field":"amount","op":"eq","value":-200}));
        }
        let result = read(&server, &path, Some(&query));
        let mut ids: Vec<_> = result["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].clone())
            .collect();
        ids.sort_by_key(|v| v.as_str().unwrap().to_owned());
        assert_eq!(json!(ids), case["ids"], "{}: {result}", case["name"]);
        assert_eq!(
            result["filtered_count"],
            json!(ids.len()),
            "{}",
            case["name"]
        );
        assert_eq!(
            result["filtered_balances"]["total"], case["total"],
            "{}",
            case["name"]
        );
        assert_eq!(result["balances"]["total"], -450);
        for row in result["data"].as_array().unwrap() {
            assert!(row["parent_id"].is_null());
        }
    }
}
#[test]
fn pages_hydrate_complete_versioned_children_and_legacy_filters_intersect_or() {
    let server = Server::start();
    let (root, _) = setup(&server);
    let path = format!("{root}/transactions");
    let all = read(&server, &path, None);
    assert_eq!(all["filtered_count"], 4);
    assert_eq!(all["balances"]["total"], -450);
    let parent = all["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["is_parent"] == true)
        .unwrap();
    assert_eq!(parent["children"].as_array().unwrap().len(), 2);
    let direct = read(
        &server,
        &format!("{path}/{}", parent["id"].as_str().unwrap()),
        None,
    );
    assert_eq!(parent["children"], direct["data"]["children"]);
    for child in parent["children"].as_array().unwrap() {
        assert!(child["field_versions"].is_object());
    }
    for ordinary in all["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["is_parent"] != true)
    {
        assert!(ordinary.get("is_parent").is_none());
        assert!(ordinary.get("parent_id").is_none());
    }
    let query = json!({"filter":{"op":"or","conditions":[{"field":"category","op":"eq","value":"12140000-0000-4000-8000-000000000003"},{"field":"amount","op":"eq","value":-60}]}});
    let result = read(
        &server,
        &format!("{path}?payee_id=12140000-0000-4000-8000-000000000006"),
        Some(&query),
    );
    assert_eq!(result["filtered_count"], 1, "{result}");
    assert_eq!(result["filtered_balances"]["total"], -60);
    assert_eq!(result["balances"]["total"], -450);
    assert!(result["data"][0]["running_balance"].is_null());
    let result = read(
        &server,
        &path,
        Some(
            &json!({"filter":{"conditions":[{"field":"category","op":"eq","value":"12140000-0000-4000-8000-000000000004"}]}}),
        ),
    );
    assert_eq!(result["filtered_count"], 2);
    assert_eq!(result["filtered_balances"]["total"], -360);
    let first = read(&server, &format!("{path}?limit=1"), None);
    let next = read(
        &server,
        &format!(
            "{path}?limit=1&offset=1&snapshot={}",
            first["snapshot"].as_str().unwrap()
        ),
        None,
    );
    assert_ne!(first["data"][0]["id"], next["data"][0]["id"]);
    assert_eq!(next["filtered_count"], 4);
}

#[test]
fn merged_alias_filters_and_dense_split_pages_preserve_parent_identity() {
    let server = Server::start();
    let (root, _) = setup(&server);
    let path = format!("{root}/transactions");
    write(
        &server,
        &format!("{root}/payees/batch"),
        json!({"action":"merge","ids":["12140000-0000-4000-8000-000000000005"],"target_id":"12140000-0000-4000-8000-000000000006"}),
    );
    for query in [
        json!({"filter":{"conditions":[{"field":"payee","op":"eq","value":"12140000-0000-4000-8000-000000000005"}]}}),
        json!({"filter":{"conditions":[{"field":"payee","op":"in","value":["12140000-0000-4000-8000-000000000005"]}]}}),
    ] {
        let result = read(&server, &path, Some(&query));
        assert_eq!(result["filtered_count"], 3, "{result}");
        assert_eq!(result["filtered_balances"]["total"], -410);
    }
    let legacy = read(
        &server,
        &format!("{path}?payee_id=12140000-0000-4000-8000-000000000005"),
        None,
    );
    assert_eq!(legacy["filtered_count"], 3);
    assert!(
        legacy["data"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["running_balance"].is_null())
    );
    for _ in 0..12 {
        let id = uuid::Uuid::new_v4().to_string();
        write(
            &server,
            &path,
            json!({"id":id,"account_id":ACCOUNT,"amount":-100,"date":"2026-01-03","notes":"Dense parent"}),
        );
        let view = read(&server, &format!("{path}/{id}/splits"), None);
        let children:Vec<_>=(0..100).map(|i|json!({"id":uuid::Uuid::new_v4().to_string(),"amount":-1,"category_id": if i == 99 {Some("12140000-0000-4000-8000-000000000003")} else {None}})).collect();
        write(
            &server,
            &format!("{path}/{id}/splits"),
            json!({"snapshot":view["snapshot"],"children":children}),
        );
    }
    let page = read(&server, &format!("{path}?limit=1000"), None);
    assert_eq!(page["filtered_count"], 16);
    assert_eq!(page["balances"]["total"], -1650);
    let mut child_count = 0;
    for parent in page["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["is_parent"] == true)
    {
        let children = parent["children"].as_array().unwrap();
        child_count += children.len();
        for child in children {
            assert_eq!(child["parent_id"], parent["id"]);
            assert!(child["field_versions"]["amount"].as_str().is_some());
        }
    }
    assert_eq!(child_count, 1202);
    let filtered = read(
        &server,
        &path,
        Some(
            &json!({"filter":{"conditions":[{"field":"category","op":"eq","value":"12140000-0000-4000-8000-000000000003"},{"field":"amount","op":"eq","value":-1}]}}),
        ),
    );
    assert_eq!(filtered["filtered_count"], 12);
    assert_eq!(filtered["filtered_balances"]["total"], -1200);
}
