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
    data: TempDir,
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
            data,
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
    #[allow(dead_code)]
    fn restart(mut self) -> Self {
        self.process.kill().unwrap();
        self.process.wait().unwrap();
        let data = std::mem::replace(&mut self.data, tempfile::tempdir().unwrap());
        Self::with_data(data)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
const ARCHIVE: &[u8] = include_bytes!("../../tests/fixtures/import-26.10.0.zip");
fn imported(server: &Server) -> (String, Value) {
    let response = server
        .client
        .post(format!("{}/api/v1/imports/actual", server.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "upload")
        .body(ARCHIVE)
        .send()
        .unwrap();
    assert_eq!(response.status(), 201);
    let staged: Value = response.json().unwrap();
    let stage = &staged["data"];
    assert_eq!(
        server
            .post(
                &format!("/api/v1/imports/{}/activate", stage["id"].as_str().unwrap()),
                "activate",
                json!({})
            )
            .status(),
        201
    );
    (
        stage["budget_id"].as_str().unwrap().into(),
        stage["report"]["source"].clone(),
    )
}
#[test]
fn legacy_identity_maps_only_exact_source_and_each_field_keeps_original_order() {
    let server = Server::start();
    let (budget, source) = imported(&server);
    let mapping = server.get(&format!(
        "/api/v1/android-upgrade/sources?file_id={}&group_id={}",
        source["cloudFileId"].as_str().unwrap(),
        source["groupId"].as_str().unwrap()
    ));
    assert_eq!(mapping.status(), 200, "{}", mapping.text().unwrap());
    let candidates: Value = mapping.json().unwrap();
    assert_eq!(candidates["data"][0]["id"], budget);
    let epoch = candidates["data"][0]["epoch"].as_str().unwrap();
    let id = "04000000-0000-4000-8000-000000000002";
    let path = format!("/api/v1/budgets/{budget}/android-upgrade/operations");
    let endpoint = format!("/api/v1/budgets/{budget}/transactions/{id}");
    let patch = server
        .client
        .patch(format!("{}{endpoint}", server.url))
        .header("Idempotency-Key", "remote")
        .header(
            "X-Actual-Edit-Clock",
            "2026-11-08T23:50:11.000Z-0000-aaaaaaaaaaaaaaaa",
        )
        .json(&json!({"amount":-1100,"notes":"remote"}))
        .send()
        .unwrap();
    assert_eq!(patch.status(), 200);
    let body = json!({"source":{"file_id":source["cloudFileId"],"group_id":source["groupId"]},"epoch":epoch,"operation":{"id":"10000000-0000-4000-8000-000000000020","transactionId":id,"creates":false,"deletes":false,"createdOffline":false,"attempted":true,"changes":[{"dataset":"transactions","row":id,"column":"amount","value":-1000,"timestamp":"2026-11-08T23:50:10.000Z-0000-bbbbbbbbbbbbbbbb"},{"dataset":"transactions","row":id,"column":"notes","value":"local note","timestamp":"2026-11-08T23:50:12.000Z-0000-bbbbbbbbbbbbbbbb"}]}});
    let response = server.post(&path, "10000000-0000-4000-8000-000000000020", body.clone());
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let receipt: Value = response.json().unwrap();
    let tx = server.get(&endpoint).json::<Value>().unwrap();
    assert_eq!(tx["data"]["amount"], -1100);
    assert_eq!(tx["data"]["notes"], "local note");
    assert_eq!(
        tx["data"]["field_versions"]["notes"],
        "2026-11-08T23:50:12.000Z-0000-bbbbbbbbbbbbbbbb"
    );
    let server = server.restart();
    assert_eq!(
        server
            .post(&path, "10000000-0000-4000-8000-000000000020", body)
            .json::<Value>()
            .unwrap(),
        receipt
    );
    assert_eq!(
        server
            .get(&format!(
                "/api/v1/android-upgrade/sources?file_id={}&group_id=00000000-0000-4000-8000-000000000020",
                source["cloudFileId"].as_str().unwrap()
            ))
            .json::<Value>()
            .unwrap()["data"],
        json!([])
    );
}
