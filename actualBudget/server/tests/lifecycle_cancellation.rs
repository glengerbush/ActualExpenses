use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
};
struct Server {
    child: Child,
    url: String,
    client: Client,
    _data: tempfile::TempDir,
}
impl Server {
    fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_actualbudget-server"))
            .env("ACTUALBUDGET_DATA_DIR", data.path())
            .env("ACTUALBUDGET_HOST", "127.0.0.1")
            .env("ACTUALBUDGET_PORT", "0")
            .env_remove("ACTUALBUDGET_PUBLIC_ORIGIN")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert!(line.starts_with("LISTENING "));
        Self {
            child,
            url: format!("http://{}", line.trim().trim_start_matches("LISTENING ")),
            client: Client::new(),
            _data: data,
        }
    }
    fn write(
        &self,
        method: &str,
        path: &str,
        key: &str,
        body: &Value,
        cancel: bool,
    ) -> (u16, Value) {
        let mut r = self
            .client
            .request(method.parse().unwrap(), format!("{}{path}", self.url))
            .header("Idempotency-Key", key)
            .json(body);
        if cancel {
            r = r.header("X-Actual-Cancel-If-Uncommitted", "true");
        }
        let r = r.send().unwrap();
        (r.status().as_u16(), r.json().unwrap())
    }
    fn get(&self, path: &str) -> Value {
        self.client
            .get(format!("{}{path}", self.url))
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
#[test]
fn lifecycle_cancellation_fences_delayed_mutations_and_replays_committed_outcomes() {
    let s = Server::new();
    let budget = uuid::Uuid::new_v4().to_string();
    let root = format!("/api/v1/budgets/{budget}");
    assert_eq!(
        s.write(
            "POST",
            "/api/v1/budgets",
            "seed",
            &json!({"id":budget,"name":"Household"}),
            false
        )
        .0,
        201
    );
    let point = s.write(
        "POST",
        &format!("{root}/backups"),
        "saved-backup",
        &json!({"label":"Original"}),
        false,
    );
    assert_eq!(point.0, 201);
    let point_id = point.1["data"]["id"].as_str().unwrap();
    let bytes = s
        .client
        .get(format!("{}{root}/backups/{point_id}/download", s.url))
        .send()
        .unwrap()
        .bytes()
        .unwrap();
    let stage: Value = s
        .client
        .post(format!("{}/api/v1/restores", s.url))
        .header("Idempotency-Key", "archive")
        .header("Content-Type", "application/zip")
        .body(bytes.to_vec())
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap();
    let stage_id = stage["data"]["id"].as_str().unwrap();
    let before = s.get(&root);
    let backups = s.get(&format!("{root}/backups"));
    let operations = [
        (
            "POST",
            format!("{root}/backups"),
            json!({"label":"Delayed point"}),
        ),
        (
            "POST",
            format!("{root}/backups/{point_id}/restore"),
            json!({"confirm_name":"Household"}),
        ),
        (
            "POST",
            format!("{root}/restores/{stage_id}/activate"),
            json!({"confirm_name":"Household"}),
        ),
    ];
    for (n, (method, path, body)) in operations.iter().enumerate() {
        let key = format!("fenced-{n}");
        let canceled = s.write(method, path, &key, body, true);
        assert_eq!(canceled.0, 409, "{path}: {}", canceled.1);
        assert_eq!(canceled.1["error"]["code"], "operation_canceled");
        assert_eq!(
            s.write(method, path, &key, body, false),
            canceled,
            "delayed original must replay fence"
        );
        assert_eq!(s.get(&root), before);
        assert_eq!(s.get(&format!("{root}/backups")), backups);
    }
    assert_eq!(
        s.get("/api/v1/budgets")["data"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        s.write(
            "POST",
            &format!("{root}/backups"),
            "saved-backup",
            &json!({"label":"Original"}),
            true
        ),
        point
    );
    let committed = s.write(
        "POST",
        &format!("{root}/backups"),
        "committed-backup",
        &json!({"label":"Committed"}),
        false,
    );
    assert_eq!(committed.0, 201);
    assert_eq!(
        s.write(
            "POST",
            &format!("{root}/backups"),
            "committed-backup",
            &json!({"label":"Committed"}),
            true
        ),
        committed
    );
}
