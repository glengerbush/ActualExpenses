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
    assert_eq!(defaults["data"]["experimental"]["currency"], false);
    let response = server.patch(
        &path,
        "format",
        json!({"number_format":"dot-comma","first_day_of_week":1}),
    );
    assert_eq!(response.status(), 200);
    let response = server.patch(
        &path,
        "currency",
        json!({"currency_code":"EUR","experimental":{"currency":true}}),
    );
    assert_eq!(response.status(), 200);
    let values: Value = response.json().unwrap();
    assert_eq!(values["data"]["number_format"], "dot-comma");
    assert_eq!(values["data"]["currency_code"], "EUR");
    assert_eq!(values["data"]["experimental"]["currency"], true);
    let other = "00000000-0000-4000-8000-000000000044";
    server.budget(other);
    let isolated: Value = server
        .get(&format!("/api/v1/budgets/{other}/preferences"))
        .json()
        .unwrap();
    assert_eq!(isolated["data"]["currency_code"], "");
}

#[test]
fn preference_writes_are_idempotent_validated_and_resettable() {
    let server = Server::start();
    server.budget(BUDGET);
    let path = format!("/api/v1/budgets/{BUDGET}/preferences");
    let changes = json!({"date_format":"dd.MM.yyyy","experimental":{"currency":true}});
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
        .patch(
            &path,
            "reset",
            json!({"date_format":null,"experimental":null}),
        )
        .json()
        .unwrap();
    assert_eq!(reset["data"]["date_format"], "MM/dd/yyyy");
    assert_eq!(reset["data"]["experimental"]["currency"], false);
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
    assert_eq!(data["shared"]["currency_code"], "JPY");
    assert_eq!(data["shared"]["currency_symbol_position"], "after");
    assert_eq!(data["shared"]["currency_space"], true);
    assert_eq!(data["shared"]["experimental"]["currency"], true);
    assert_eq!(data["shared"]["experimental"]["sankeyReport"], false);
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
    let changes = json!({"number_format":"comma-dot-in","experimental":{"sankeyReport":true}});
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
