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
    data: TempDir,
    url: String,
    client: Client,
}
impl Server {
    fn start() -> Self {
        Self::open(tempfile::tempdir().unwrap())
    }
    fn open(data: TempDir) -> Self {
        let mut process = Command::new(env!("CARGO_BIN_EXE_actualbudget-server"))
            .env("ACTUALBUDGET_DATA_DIR", data.path())
            .env("ACTUALBUDGET_PORT", "0")
            .env("ACTUALBUDGET_HOST", "127.0.0.1")
            .env("ACTUALBUDGET_BACKUP_INTERVAL_SECONDS", "1")
            .env_remove("ACTUALBUDGET_PUBLIC_ORIGIN")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(process.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert!(line.starts_with("LISTENING "));
        Self {
            process,
            data,
            url: format!("http://{}", line.trim().trim_start_matches("LISTENING ")),
            client: Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
        }
    }
    fn get(&self, path: &str) -> Value {
        let r = self
            .client
            .get(format!("{}{path}", self.url))
            .send()
            .unwrap();
        assert!(r.status().is_success(), "{}", r.text().unwrap());
        r.json().unwrap()
    }
    fn write(
        &self,
        method: &str,
        path: &str,
        key: &str,
        clock: Option<&str>,
        body: Value,
    ) -> Response {
        let mut r = self
            .client
            .request(method.parse().unwrap(), format!("{}{path}", self.url))
            .header("Idempotency-Key", key);
        if let Some(clock) = clock {
            r = r.header("X-Actual-Edit-Clock", clock);
        }
        if !body.is_null() {
            r = r.json(&body);
        }
        r.send().unwrap()
    }
    fn restart(mut self) -> Self {
        self.process.kill().unwrap();
        self.process.wait().unwrap();
        let data = std::mem::replace(&mut self.data, tempfile::tempdir().unwrap());
        Self::open(data)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
const B: &str = "80000000-0000-4000-8000-000000000001";
const A: &str = "80000000-0000-4000-8000-000000000002";
const T: &str = "80000000-0000-4000-8000-000000000003";
const OLD: &str = "2026-01-01T00:00:00.000Z-0000-0000000000000001";
const NEW: &str = "2026-01-02T00:00:00.000Z-0000-0000000000000001";
fn seed(s: &Server) {
    assert_eq!(
        s.write(
            "POST",
            "/api/v1/budgets",
            "budget",
            None,
            json!({"id":B,"name":"Household"})
        )
        .status(),
        201
    );
    assert_eq!(
        s.write(
            "POST",
            &format!("/api/v1/budgets/{B}/accounts"),
            "account",
            None,
            json!({"id":A,"name":"Spending","opening_balance":0,"opening_date":"2026-01-01"})
        )
        .status(),
        201
    );
    assert_eq!(
        s.write(
            "POST",
            &format!("/api/v1/budgets/{B}/transactions"),
            "transaction",
            Some(OLD),
            json!({"id":T,"account_id":A,"amount":-100,"date":"2026-01-01"})
        )
        .status(),
        201
    );
}
fn epoch_write(
    s: &Server,
    method: &str,
    path: &str,
    key: &str,
    epoch: &str,
    body: Value,
) -> Response {
    s.client
        .request(method.parse().unwrap(), format!("{}{path}", s.url))
        .header("Idempotency-Key", key)
        .header("X-Actual-Budget-Epoch", epoch)
        .json(&body)
        .send()
        .unwrap()
}
#[test]
fn budget_restore_rotates_epoch_preserves_post_backup_receipts_and_restart() {
    let s = Server::start();
    seed(&s);
    let root = format!("/api/v1/budgets/{B}");
    let backup = s.write(
        "POST",
        &format!("{root}/backups"),
        "manual",
        None,
        json!({"label":"Before changes"}),
    );
    assert_eq!(backup.status(), 201);
    let backup: Value = backup.json().unwrap();
    let point = backup["data"]["id"].as_str().unwrap();
    let checkpoint = s.get(&format!("{root}/checkpoint"))["data"].clone();
    let epoch = checkpoint["epoch"].as_str().unwrap();
    let edited = s.write(
        "PATCH",
        &format!("{root}/transactions/{T}"),
        "post-backup",
        Some(NEW),
        json!({"amount":-200}),
    );
    assert_eq!(edited.status(), 200);
    let edited: Value = edited.json().unwrap();
    let denied = epoch_write(
        &s,
        "POST",
        &format!("{root}/backups/{point}/restore"),
        "restore",
        epoch,
        json!({"confirm_name":"Wrong"}),
    );
    assert_eq!(denied.status(), 409);
    let restore = epoch_write(
        &s,
        "POST",
        &format!("{root}/backups/{point}/restore"),
        "restore",
        epoch,
        json!({"confirm_name":"Household"}),
    );
    assert_eq!(restore.status(), 200);
    let restored: Value = restore.json().unwrap();
    let next = restored["data"]["budget"]["epoch"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(next, epoch);
    assert_eq!(restored["data"]["budget"]["name"], "Household");
    assert_eq!(
        s.get(&format!("{root}/transactions/{T}"))["data"]["amount"],
        -100
    );
    assert_eq!(
        s.write(
            "PATCH",
            &format!("{root}/transactions/{T}"),
            "post-backup",
            Some(NEW),
            json!({"amount":-200})
        )
        .json::<Value>()
        .unwrap(),
        edited
    );
    let stale = epoch_write(
        &s,
        "PATCH",
        &format!("{root}/transactions/{T}"),
        "stale",
        epoch,
        json!({"notes":"must not apply"}),
    );
    assert_eq!(stale.status(), 409);
    assert_eq!(
        stale.json::<Value>().unwrap()["error"]["code"],
        "budget_restored"
    );
    let missing = s.write(
        "PATCH",
        &format!("{root}/transactions/{T}"),
        "missing",
        None,
        json!({"notes":"must not apply"}),
    );
    assert_eq!(missing.status(), 409);
    assert_eq!(
        epoch_write(
            &s,
            "PATCH",
            &format!("{root}/transactions/{T}"),
            "reviewed-new-intent",
            &next,
            json!({"amount":-345})
        )
        .status(),
        200
    );
    let s = s.restart();
    assert_eq!(
        s.get(&format!("{root}/transactions/{T}"))["data"]["amount"],
        -345
    );
}

#[test]
fn portable_archive_stages_without_changes_then_restores_in_a_fresh_server() {
    let source = Server::start();
    seed(&source);
    let root = format!("/api/v1/budgets/{B}");
    let point: Value = source
        .write(
            "POST",
            &format!("{root}/backups"),
            "portable",
            None,
            json!({"label":"Portable"}),
        )
        .json()
        .unwrap();
    let bytes = source
        .client
        .get(format!(
            "{}{root}/backups/{}/download",
            source.url,
            point["data"]["id"].as_str().unwrap()
        ))
        .send()
        .unwrap()
        .bytes()
        .unwrap();
    let target = Server::start();
    let staged = target
        .client
        .post(format!("{}/api/v1/restores", target.url))
        .header("Idempotency-Key", "stage")
        .header("Content-Type", "application/zip")
        .body(bytes.to_vec())
        .send()
        .unwrap();
    assert_eq!(staged.status(), 201);
    let stage: Value = staged.json().unwrap();
    assert!(
        target.get("/api/v1/budgets")["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let path = format!(
        "{root}/restores/{}/activate",
        stage["data"]["id"].as_str().unwrap()
    );
    let restored = target.write(
        "POST",
        &path,
        "activate",
        None,
        json!({"confirm_name":"Household"}),
    );
    assert_eq!(restored.status(), 200);
    let restored: Value = restored.json().unwrap();
    assert_eq!(restored["data"]["budget"]["generation"], 1);
    assert_eq!(
        target.get(&format!("{root}/transactions/{T}"))["data"]["amount"],
        -100
    );
    assert_eq!(
        target.get(&format!("{root}/transactions/{T}"))["data"]["field_versions"],
        source.get(&format!("{root}/transactions/{T}"))["data"]["field_versions"]
    );
    let replay = target.write(
        "POST",
        &format!("{root}/transactions"),
        "transaction",
        Some(OLD),
        json!({"id":T,"account_id":A,"amount":-100,"date":"2026-01-01"}),
    );
    assert_eq!(replay.status(), 201);
    let target = target.restart();
    assert_eq!(
        target
            .write(
                "POST",
                &path,
                "activate",
                None,
                json!({"confirm_name":"Household"})
            )
            .json::<Value>()
            .unwrap(),
        restored
    );
    assert_eq!(
        target
            .client
            .post(format!("{}/api/v1/restores", target.url))
            .header("Idempotency-Key", "corrupt")
            .header("Content-Type", "application/zip")
            .body("not an archive")
            .send()
            .unwrap()
            .status(),
        422
    );
    assert_eq!(
        target.get(&format!("{root}/transactions/{T}"))["data"]["amount"],
        -100
    );
    assert_eq!(
        source.get(&format!("{root}/transactions/{T}"))["data"]["amount"],
        -100
    );
}

#[test]
fn automatic_recovery_is_dirty_driven_retained_and_restorable() {
    let s = Server::start();
    seed(&s);
    let root = format!("/api/v1/budgets/{B}");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let points = loop {
        let points = s.get(&format!("{root}/backups"))["data"].clone();
        if !points.as_array().unwrap().is_empty() {
            break points;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Dirty budget never backed up"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(points[0]["kind"], "automatic");
    let id = points[0]["id"].as_str().unwrap();
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(
        s.get(&format!("{root}/backups"))["data"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "Idle reads must not produce backups"
    );
    let s = s.restart();
    let restored = s.write(
        "POST",
        &format!("{root}/backups/{id}/restore"),
        "auto-restore",
        None,
        json!({"confirm_name":"Household"}),
    );
    assert_eq!(restored.status(), 200, "{}", restored.text().unwrap());
    assert_eq!(
        s.get(&format!("{root}/transactions/{T}"))["data"]["amount"],
        -100
    );
}

#[test]
fn single_budget_rejects_additional_budgets_and_retired_lifecycle_actions() {
    let s = Server::start();
    seed(&s);
    let root = format!("/api/v1/budgets/{B}");
    assert_eq!(s.get("/api/v1/budget")["data"]["id"], B);
    assert_eq!(
        s.write(
            "POST",
            "/api/v1/budgets",
            "second",
            None,
            json!({"id":uuid::Uuid::new_v4().to_string(),"name":"Second"})
        )
        .status(),
        409
    );
    assert_eq!(
        s.write("PATCH", &root, "rename", None, json!({"name":"Renamed"}))
            .status(),
        405
    );
    assert_eq!(
        s.write(
            "DELETE",
            &root,
            "delete",
            None,
            json!({"confirm_name":"Household"})
        )
        .status(),
        405
    );
    assert_eq!(
        s.write(
            "POST",
            &format!("{root}/duplicate"),
            "copy",
            None,
            json!({"id":uuid::Uuid::new_v4().to_string(),"name":"Copy"})
        )
        .status(),
        404
    );
    assert_eq!(
        s.get("/api/v1/budgets")["data"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        s.get(&format!("{root}/transactions/{T}"))["data"]["amount"],
        -100
    );
}

#[test]
fn interrupted_export_and_upload_keep_recovery_source_and_active_state() {
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpStream};
    let source = Server::start();
    seed(&source);
    let target = Server::start();
    seed(&target);
    let root = format!("/api/v1/budgets/{B}");
    let point = source
        .write(
            "POST",
            &format!("{root}/backups"),
            "interrupt-source",
            None,
            json!({"label":"Preserved source"}),
        )
        .json::<Value>()
        .unwrap();
    let path = format!(
        "{root}/backups/{}/download",
        point["data"]["id"].as_str().unwrap()
    );
    let mut stream = TcpStream::connect(source.url.trim_start_matches("http://")).unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        source.url.trim_start_matches("http://")
    )
    .unwrap();
    let mut first = [0; 128];
    assert!(stream.read(&mut first).unwrap() > 0);
    drop(stream);
    let bytes = source
        .client
        .get(format!("{}{path}", source.url))
        .send()
        .unwrap()
        .bytes()
        .unwrap();
    assert!(!bytes.is_empty());
    let mut stream = TcpStream::connect(target.url.trim_start_matches("http://")).unwrap();
    write!(stream,"POST /api/v1/restores HTTP/1.1\r\nHost: {}\r\nContent-Type: application/zip\r\nIdempotency-Key: interrupted-upload\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",target.url.trim_start_matches("http://"),bytes.len()).unwrap();
    stream.write_all(&bytes[..bytes.len() / 2]).unwrap();
    stream.shutdown(Shutdown::Write).unwrap();
    let mut response = vec![];
    stream.read_to_end(&mut response).unwrap();
    drop(stream);
    let target = target.restart();
    assert_eq!(
        target.get(&format!("{root}/transactions/{T}"))["data"]["amount"],
        -100
    );
    let retry = target
        .client
        .post(format!("{}/api/v1/restores", target.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "interrupted-upload")
        .body(bytes.to_vec())
        .send()
        .unwrap();
    assert_eq!(retry.status(), 201, "{}", retry.text().unwrap());
    assert_eq!(
        source
            .client
            .get(format!("{}{path}", source.url))
            .send()
            .unwrap()
            .bytes()
            .unwrap(),
        bytes
    );
}

#[test]
fn automatic_retention_keeps_three_today_and_never_prunes_manual_points() {
    let s = Server::start();
    seed(&s);
    let root = format!("/api/v1/budgets/{B}");
    let manual = s
        .write(
            "POST",
            &format!("{root}/backups"),
            "retained-manual",
            None,
            json!({"label":"Keep this point"}),
        )
        .json::<Value>()
        .unwrap();
    let mut previous = String::new();
    for index in 0..5 {
        let edit = s.write(
            "PATCH",
            &format!("{root}/transactions/{T}"),
            &format!("dirty-{index}"),
            None,
            json!({"notes":format!("Change {index}")}),
        );
        assert_eq!(edit.status(), 200);
        let until = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let points = s.get(&format!("{root}/backups"));
            let automatic = points["data"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| p["kind"] == "automatic")
                .collect::<Vec<_>>();
            if let Some(latest) = automatic.first() {
                let id = latest["id"].as_str().unwrap();
                if id != previous {
                    previous = id.to_owned();
                    assert!(automatic.len() <= 3);
                    break;
                }
            }
            assert!(
                std::time::Instant::now() < until,
                "Automatic point did not advance after mutation"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let points = s.get(&format!("{root}/backups"));
    assert_eq!(
        points["data"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["kind"] == "automatic")
            .count(),
        3
    );
    assert!(
        points["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == manual["data"]["id"])
    );
}
