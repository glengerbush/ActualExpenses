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
        self.process.kill().unwrap();
        self.process.wait().unwrap();
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
                .json(&json!({"id":id,"name":"Preferences fixture","budget_type":"tracking"}))
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
const BUDGET: &str = "00000000-0000-4000-8000-000000000043";
#[test]
fn preferences_are_budget_scoped_and_changes_do_not_replace_unrelated_fields() {
    let server = Server::start();
    server.budget(BUDGET);
    let path = format!("/api/v1/budgets/{BUDGET}/preferences");
    let response = server.get(&path);
    assert_eq!(response.status(), 200);
    let defaults: Value = response.json().unwrap();
    assert_eq!(defaults["data"]["number_format"], "comma-dot");
    assert!(defaults["data"].get("experimental").is_none());
    let response = server.patch(
        &path,
        "format",
        json!({"number_format":"dot-comma","first_day_of_week":1}),
    );
    assert_eq!(response.status(), 200);
    let response = server.patch(&path, "fraction", json!({"hide_fraction":true}));
    assert_eq!(response.status(), 200);
    let values: Value = response.json().unwrap();
    assert_eq!(values["data"]["number_format"], "dot-comma");
    assert_eq!(values["data"]["hide_fraction"], true);
    let other = "00000000-0000-4000-8000-000000000044";
    server.budget(other);
    let isolated: Value = server
        .get(&format!("/api/v1/budgets/{other}/preferences"))
        .json()
        .unwrap();
    assert_eq!(isolated["data"]["hide_fraction"], false);
}

#[test]
fn preference_writes_are_idempotent_validated_and_resettable() {
    let server = Server::start();
    server.budget(BUDGET);
    let path = format!("/api/v1/budgets/{BUDGET}/preferences");
    let changes = json!({"date_format":"dd.MM.yyyy"});
    let first: Value = server.patch(&path, "same", changes.clone()).json().unwrap();
    let again: Value = server.patch(&path, "same", changes).json().unwrap();
    assert_eq!(first, again);
    assert_eq!(first["data"]["revision"], 1);
    assert_eq!(
        server
            .patch(&path, "same", json!({"hide_fraction":true}))
            .status(),
        409
    );
    for (index, invalid) in [
        json!({"theme":"dark"}),
        json!({"currency_code":"ZZZ"}),
        json!({"first_day_of_week":7}),
        json!({"hide_fraction":"true"}),
        json!({"experimental":{"unknown":true}}),
        json!({"date_format":"invalid"}),
        json!({"number_format":"invalid"}),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            server
                .patch(&path, &format!("invalid-{index}"), invalid)
                .status(),
            400
        );
    }
    let reset: Value = server
        .patch(&path, "reset", json!({"date_format":null}))
        .json()
        .unwrap();
    assert_eq!(reset["data"]["date_format"], "MM/dd/yyyy");
    assert!(reset["data"].get("experimental").is_none());
    assert_eq!(reset["data"]["revision"], 2);
    assert_eq!(
        server
            .get("/api/v1/budgets/00000000-0000-0000-0000-000000000099/preferences")
            .status(),
        404
    );
}

#[test]
fn released_preference_keys_convert_without_silently_dropping_unknown_data() {
    let server = Server::start();
    let response = server.client.post(format!("{}/api/v1/preferences/convert-actual",server.url)).json(&json!({
        "synced":{"numberFormat":"space-comma","dateFormat":"dd/MM/yyyy","hideFraction":"true","firstDayOfWeekIdx":"1","defaultCurrencyCode":"JPY","currencySymbolPosition":"after","currencySpaceBetweenAmountAndSymbol":"true","flags.currency":"true","flags.sankeyReport":false,"customReportOrder":"retained"},
        "global":{"theme":"auto","preferred-dark-theme":"midnight","language":"fr-CA","user-token":"must-not-return","customTheme":"retained"}
    })).send().unwrap();
    assert_eq!(response.status(), 200);
    let result: Value = response.json().unwrap();
    let data = &result["data"];
    assert_eq!(data["shared"]["number_format"], "space-comma");
    assert_eq!(data["shared"]["date_format"], "dd/MM/yyyy");
    assert_eq!(data["shared"]["first_day_of_week"], 1);
    assert_eq!(data["shared"]["hide_fraction"], true);
    assert_eq!(
        data["device"],
        json!({"theme":"system","preferred_dark_theme":"midnight","language":"fr"})
    );
    assert_eq!(data["unmapped"]["synced"]["customReportOrder"], "retained");
    assert_eq!(data["unmapped"]["global"]["customTheme"], "retained");
    assert!(!result.to_string().contains("must-not-return"));
    assert_eq!(data["warnings"].as_array().unwrap().len(), 3);
}

#[test]
fn preferences_and_idempotency_survive_server_replacement() {
    let mut server = Server::start();
    server.budget(BUDGET);
    let path = format!("/api/v1/budgets/{BUDGET}/preferences");
    let changes = json!({"number_format":"comma-dot-in"});
    let acknowledged: Value = server
        .patch(&path, "durable", changes.clone())
        .json()
        .unwrap();
    server.restart();
    let persisted: Value = server.get(&path).json().unwrap();
    assert_eq!(persisted, acknowledged);
    let replay: Value = server.patch(&path, "durable", changes).json().unwrap();
    assert_eq!(replay, acknowledged);
}

#[test]
fn experimental_preferences_are_retired_and_cannot_be_enabled() {
    let server = Server::start();
    server.budget(BUDGET);
    let path = format!("/api/v1/budgets/{BUDGET}/preferences");
    for (i, change) in [
        json!({"experimental":{"formulaMode":true}}),
        json!({"currency_code":"JPY"}),
        json!({"experimental":null}),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            server
                .patch(&path, &format!("retired-{i}"), change)
                .status(),
            400
        );
    }
    let values: Value = server.get(&path).json().unwrap();
    assert!(values["data"].get("experimental").is_none());
    assert!(values["data"].get("currency_code").is_none());
}

#[test]
fn retired_privacy_is_ignored_on_upgrade_and_import_and_rejected_on_write() {
    let mut server = Server::start();
    let contract: Value = server.get("/api/v1/openapi.json").json().unwrap();
    assert!(
        contract["components"]["schemas"]["BudgetPreferences"]["properties"]
            .get("privacy_mode")
            .is_none()
    );
    assert_eq!(
        contract["components"]["schemas"]["BudgetPreferences"]["properties"]["sync_transfer_date"]
            ["type"],
        "boolean"
    );
    let response = server.client.post(format!("{}/api/v1/preferences/convert-actual", server.url))
        .json(&json!({"synced":{"isPrivacyEnabled":"true"},"global":{"showNewsFeed":false,"notifyWhenUpdateIsAvailable":true,"lastSeenNewsDate":"2026-10-01"}})).send().unwrap();
    assert_eq!(response.status(), 200);
    let converted: Value = response.json().unwrap();
    assert!(converted["data"]["shared"].get("privacy_mode").is_none());
    assert_eq!(converted["data"]["device"]["show_release_notices"], false);
    assert_eq!(
        converted["data"]["unmapped"]["global"]["lastSeenNewsDate"],
        "2026-10-01"
    );
    let id = "00000000-0000-4000-8000-000000000045";
    server.budget(id);
    let path = format!("/api/v1/budgets/{id}/preferences");
    for (index, value) in [json!(true), json!("true"), Value::Null]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            server
                .patch(
                    &path,
                    &format!("retired-{index}"),
                    json!({"privacy_mode":value})
                )
                .status(),
            400
        );
    }
    // A real old database preference remains readable after upgrading.
    let db = rusqlite::Connection::open(server._data.path().join("actualbudget.sqlite3")).unwrap();
    db.execute(
        "INSERT INTO budget_preferences(budget_id,settings,revision) VALUES (?1,?2,1)",
        rusqlite::params![
            id,
            json!({"privacy_mode":true,"experimental":{}}).to_string()
        ],
    )
    .unwrap();
    drop(db);
    server.restart();
    let values: Value = server.get(&path).json().unwrap();
    assert!(values["data"].get("privacy_mode").is_none());
    assert_eq!(values["data"]["revision"], 1);
}
