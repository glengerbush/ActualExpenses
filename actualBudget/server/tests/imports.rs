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
#[test]
fn pinned_archive_stages_then_activates_atomically_and_replays_after_restart() {
    let server = Server::start();
    let expected: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/import-26.10.0.json")).unwrap();
    let upload = server
        .client
        .post(format!("{}/api/v1/imports/actual", server.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "upload")
        .body(ARCHIVE)
        .send()
        .unwrap();
    assert_eq!(upload.status(), 201, "{}", upload.text().unwrap());
    let staged: Value = upload.json().unwrap();
    assert_eq!(staged["data"]["status"], "ready");
    let id = staged["data"]["id"].as_str().unwrap();
    let budget = staged["data"]["budget_id"].as_str().unwrap();
    assert_eq!(
        staged["data"]["report"]["source"],
        expected["inspection"]["metadata"]
    );
    assert_eq!(
        staged["data"]["report"]["archive_sha256"],
        expected["inspection"]["archive_sha256"]
    );
    assert_eq!(
        staged["data"]["report"]["balances"],
        expected["public"]["balances"]
    );
    assert_eq!(
        server.get("/api/v1/budgets").json::<Value>().unwrap()["data"],
        json!([])
    );
    assert_eq!(
        server
            .get(&format!("/api/v1/imports/{id}/original.zip"))
            .bytes()
            .unwrap()
            .as_ref(),
        ARCHIVE
    );
    let activation = server.post(
        &format!("/api/v1/imports/{id}/activate"),
        "activate",
        json!({}),
    );
    assert_eq!(activation.status(), 201, "{}", activation.text().unwrap());
    let result: Value = activation.json().unwrap();
    assert_eq!(result["data"]["id"], budget);
    let server = server.restart();
    assert_eq!(
        server
            .post(
                &format!("/api/v1/imports/{id}/activate"),
                "activate",
                json!({})
            )
            .json::<Value>()
            .unwrap(),
        result
    );
    assert_eq!(
        server.get("/api/v1/budgets").json::<Value>().unwrap()["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let accounts = server
        .get(&format!("/api/v1/budgets/{budget}/accounts"))
        .json::<Value>()
        .unwrap();
    assert_eq!(
        accounts["data"].as_array().unwrap().len(),
        expected["public"]["accounts"].as_array().unwrap().len()
    );
    for expected_account in expected["public"]["accounts"].as_array().unwrap() {
        let account = accounts["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == expected_account["id"])
            .unwrap();
        assert_eq!(account["name"], expected_account["name"]);
        let rows = server
            .get(&format!(
                "/api/v1/budgets/{budget}/transactions?account_id={}",
                account["id"].as_str().unwrap()
            ))
            .json::<Value>()
            .unwrap();
        let balances = expected["public"]["balances"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["account_id"] == account["id"])
            .unwrap();
        assert_eq!(rows["balances"]["total"], balances["total"]);
        assert_eq!(rows["balances"]["cleared"], balances["cleared"]);
        assert_eq!(rows["balances"]["uncleared"], balances["uncleared"]);
        assert_eq!(
            rows["data"].as_array().unwrap().len(),
            expected["public"]["transactions"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row["account"] == account["id"])
                .count()
        );
        for original in expected["public"]["transactions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["account"] == account["id"])
        {
            let converted = rows["data"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["id"] == original["id"])
                .unwrap();
            for field in ["amount", "date", "cleared", "reconciled"] {
                assert_eq!(converted[field], original[field], "{field}");
            }
            assert_eq!(converted["payee_id"], original["payee"]);
            assert_eq!(converted["category_id"], original["category"]);
        }
    }
    let preferences = server
        .get(&format!("/api/v1/budgets/{budget}/preferences"))
        .json::<Value>()
        .unwrap();
    assert_eq!(preferences["data"]["date_format"], "yyyy-MM-dd");
    assert_eq!(
        staged["data"]["report"]["source_preferences"],
        expected["inspection"]["records"]["preferences"]
    );
    assert_eq!(preferences["data"]["currency_code"], "");
}

fn upload(server: &Server, key: &str, bytes: Vec<u8>) -> Response {
    server
        .client
        .post(format!("{}/api/v1/imports/actual", server.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", key)
        .body(bytes)
        .send()
        .unwrap()
}
#[test]
fn unsupported_rule_is_retained_reported_and_cannot_activate() {
    let server = Server::start();
    let bytes = include_bytes!("../../tests/fixtures/import-unsupported-rule-26.10.0.zip").to_vec();
    let response = upload(&server, "rule", bytes.clone());
    assert_eq!(response.status(), 201);
    let stage: Value = response.json().unwrap();
    assert_eq!(stage["data"]["status"], "blocked");
    assert!(
        stage["data"]["report"]["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["feature"] == "rules" && r["blocking"] == true && r["count"] == 1)
    );
    let id = stage["data"]["id"].as_str().unwrap();
    let response = server.post(
        &format!("/api/v1/imports/{id}/activate"),
        "activate",
        json!({}),
    );
    assert_eq!(response.status(), 409);
    assert_eq!(
        response.json::<Value>().unwrap()["error"]["code"],
        "import_requires_conversion"
    );
    let server = server.restart();
    assert_eq!(
        server
            .get(&format!("/api/v1/imports/{id}/original.zip"))
            .bytes()
            .unwrap()
            .as_ref(),
        bytes
    );
    assert_eq!(
        server.get("/api/v1/budgets").json::<Value>().unwrap()["data"],
        json!([])
    );
    assert_eq!(
        upload(&server, "different-upload-key", bytes)
            .json::<Value>()
            .unwrap()["data"]["id"],
        id
    );
    assert_eq!(
        server.get("/api/v1/imports").json::<Value>().unwrap()["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
fn source_entries() -> Vec<(String, Vec<u8>)> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(ARCHIVE)).unwrap();
    (0..zip.len())
        .map(|index| {
            let mut file = zip.by_index(index).unwrap();
            let name = file.name().to_owned();
            let mut data = Vec::new();
            file.read_to_end(&mut data).unwrap();
            (name, data)
        })
        .collect()
}
fn repack(entries: Vec<(String, Vec<u8>)>) -> Vec<u8> {
    use std::io::Write;
    let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        archive
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        archive.write_all(&bytes).unwrap();
    }
    archive.finish().unwrap().into_inner()
}
fn alter_source(sql: &str) -> Vec<u8> {
    let mut entries = source_entries();
    let db = entries
        .iter_mut()
        .find(|(name, _)| name == "db.sqlite")
        .unwrap();
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), &db.1).unwrap();
    let connection = rusqlite::Connection::open(file.path()).unwrap();
    connection.execute_batch(sql).unwrap();
    connection.close().unwrap();
    db.1 = std::fs::read(file.path()).unwrap();
    repack(entries)
}
#[test]
fn corrupt_encrypted_and_dangling_source_inputs_leave_existing_budget_untouched() {
    let server = Server::start();
    let existing = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        server
            .post(
                "/api/v1/budgets",
                "existing",
                json!({"id":existing,"name":"Existing"})
            )
            .status(),
        201
    );
    let mut encrypted = source_entries();
    let metadata = encrypted
        .iter_mut()
        .find(|(name, _)| name == "metadata.json")
        .unwrap();
    let mut value: Value = serde_json::from_slice(&metadata.1).unwrap();
    value["encryptKeyId"] = json!("unsupported-encrypted-key");
    metadata.1 = value.to_string().into_bytes();
    let mut corrupt = source_entries();
    corrupt
        .iter_mut()
        .find(|(name, _)| name == "db.sqlite")
        .unwrap()
        .1 = b"SQLite format 3\0broken".to_vec();
    let mut traversal = source_entries();
    traversal.push(("../outside".into(), vec![1]));
    let mut duplicate = source_entries();
    duplicate.push(("DB.SQLITE".into(), vec![1]));
    let mut missing = source_entries();
    missing.retain(|(name, _)| name != "metadata.json");
    for (index,bytes) in [b"not zip".to_vec(),repack(encrypted),repack(corrupt),repack(traversal),repack(duplicate),repack(missing),alter_source("UPDATE transactions SET acct='00000000-0000-4000-8000-000000000099' WHERE notes='Corrected expense'"),alter_source("UPDATE transactions SET amount=-9223372036854775808 WHERE notes='Corrected expense'")].into_iter().enumerate(){
        let response=upload(&server,&format!("invalid-{index}"),bytes);
        assert_eq!(response.status(),422,"case{index}:{}",response.text().unwrap());
        assert_eq!(response.json::<Value>().unwrap()["error"]["code"],"invalid_archive");
    }
    let budgets = server.get("/api/v1/budgets").json::<Value>().unwrap();
    assert_eq!(budgets["data"].as_array().unwrap().len(), 1);
    assert_eq!(budgets["data"][0]["id"], existing);
    assert_eq!(
        server.get("/api/v1/imports").json::<Value>().unwrap()["data"],
        json!([])
    );
}
#[test]
fn original_field_order_and_tombstone_survive_activation_and_transfer_payees_stay_out_of_selection()
{
    let server = Server::start();
    let expected: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/import-26.10.0.json")).unwrap();
    let stage = upload(&server, "upload", ARCHIVE.to_vec())
        .json::<Value>()
        .unwrap();
    let id = stage["data"]["id"].as_str().unwrap();
    let budget = stage["data"]["budget_id"].as_str().unwrap();
    assert_eq!(
        server
            .post(
                &format!("/api/v1/imports/{id}/activate"),
                "activate",
                json!({})
            )
            .status(),
        201
    );
    let rows = server
        .get(&format!("/api/v1/budgets/{budget}/transactions"))
        .json::<Value>()
        .unwrap();
    for original in expected["inspection"]["latest_field_clocks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["dataset"] == "transactions")
    {
        let Some(row) = rows["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == original["row"])
        else {
            continue;
        };
        let field = match original["column"].as_str().unwrap() {
            "acct" => "account_id",
            "description" => "payee_id",
            "category" => "category_id",
            "starting_balance_flag" => "starting_balance",
            field => field,
        };
        if row["field_versions"].get(field).is_some() {
            assert_eq!(row["field_versions"][field], original["timestamp"]);
        }
    }
    assert!(
        !rows["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == "04000000-0000-4000-8000-000000000004")
    );
    let payees = server
        .get(&format!("/api/v1/budgets/{budget}/payees"))
        .json::<Value>()
        .unwrap();
    for raw in expected["inspection"]["records"]["payees"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| !row["transfer_acct"].is_null())
    {
        assert!(
            !payees["data"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["id"] == raw["id"])
        );
    }
    assert!(
        stage["data"]["report"]["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["feature"] == "transfer_payees" && row["blocking"] == false)
    );
}
#[test]
fn duplicate_exact_archive_names_and_unknown_source_columns_cannot_silently_activate() {
    let server = Server::start();
    let mut entries = source_entries();
    entries.push((
        "DB.SQLITE".into(),
        entries
            .iter()
            .find(|(name, _)| name == "db.sqlite")
            .unwrap()
            .1
            .clone(),
    ));
    let mut bytes = repack(entries);
    for index in 0..bytes.len() - 8 {
        if &bytes[index..index + 9] == b"DB.SQLITE" {
            bytes[index..index + 9].copy_from_slice(b"db.sqlite");
        }
    }
    let response = upload(&server, "duplicate", bytes);
    assert_eq!(response.status(), 422, "{}", response.text().unwrap());
    let response = upload(
        &server,
        "unknown",
        alter_source(
            "ALTER TABLE transactions ADD COLUMN future_feature TEXT; UPDATE transactions SET future_feature='unconverted financial state' WHERE notes='Corrected expense'",
        ),
    );
    assert_eq!(response.status(), 201);
    let stage: Value = response.json().unwrap();
    assert_eq!(stage["data"]["status"], "blocked");
    assert!(
        stage["data"]["report"]["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["feature"]
                .as_str()
                .is_some_and(|s| s.contains("future_feature"))
                && r["blocking"] == true)
    );
}
#[test]
fn upload_and_decompression_limits_fail_without_partial_stage() {
    let server = Server::start();
    let response = upload(&server, "compressed-limit", vec![0; 16 * 1024 * 1024 + 1]);
    assert_eq!(response.status(), 413);
    assert_eq!(
        response.json::<Value>().unwrap()["error"]["code"],
        "archive_too_large"
    );
    let mut entries = source_entries();
    entries.push(("expanded.bin".into(), vec![0; 64 * 1024 * 1024 + 1]));
    let response = upload(&server, "expanded-limit", repack(entries));
    assert_eq!(response.status(), 422);
    assert!(
        response.json::<Value>().unwrap()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("64 MiB")
    );
    assert_eq!(
        server.get("/api/v1/imports").json::<Value>().unwrap()["data"],
        json!([])
    );
}
#[test]
fn malformed_and_future_source_clocks_never_activate_with_rewritten_order() {
    let server = Server::start();
    let bad = alter_source(
        "UPDATE messages_crdt SET timestamp='invalid-clock' WHERE id=(SELECT MIN(id) FROM messages_crdt)",
    );
    assert_eq!(upload(&server, "bad-clock", bad).status(), 422);
    let future = alter_source(
        "UPDATE messages_crdt SET timestamp='2099-01-01T00:00:00.000Z-FFFF-aaaaaaaaaaaaaaaa' WHERE id=(SELECT MAX(id) FROM messages_crdt)",
    );
    let result = upload(&server, "future-clock", future.clone());
    assert_eq!(result.status(), 201);
    let stage: Value = result.json().unwrap();
    assert_eq!(stage["data"]["status"], "blocked");
    assert!(
        stage["data"]["report"]["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["feature"] == "future_source_clock" && r["blocking"] == true)
    );
    let id = stage["data"]["id"].as_str().unwrap();
    assert_eq!(
        server
            .get(&format!("/api/v1/imports/{id}/original.zip"))
            .bytes()
            .unwrap()
            .as_ref(),
        future
    );
    assert_eq!(
        server
            .post(
                &format!("/api/v1/imports/{id}/activate"),
                "activate",
                json!({})
            )
            .status(),
        409
    );
}
#[test]
fn staged_archive_larger_than_generic_json_limit_is_retained_with_explicit_extra_data_requirement()
{
    use sha2::{Digest, Sha256};
    let server = Server::start();
    let mut entries = source_entries();
    let mut extra = Vec::new();
    for index in 0u32..4096 {
        extra.extend_from_slice(&Sha256::digest(index.to_le_bytes()));
    }
    entries.push(("extra-data.bin".into(), extra));
    let bytes = repack(entries);
    assert!(bytes.len() > 65536);
    let response = upload(&server, "large-valid", bytes.clone());
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    let stage: Value = response.json().unwrap();
    assert_eq!(stage["data"]["status"], "blocked");
    assert!(
        stage["data"]["report"]["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["feature"] == "extra_archive_entries")
    );
    let id = stage["data"]["id"].as_str().unwrap();
    assert_eq!(
        server
            .get(&format!("/api/v1/imports/{id}/original.zip"))
            .bytes()
            .unwrap()
            .as_ref(),
        bytes
    );
}
#[test]
fn activation_cannot_replace_existing_budget_and_upload_keys_cannot_change_payload() {
    let server = Server::start();
    let stage = upload(&server, "upload", ARCHIVE.to_vec())
        .json::<Value>()
        .unwrap();
    let id = stage["data"]["id"].as_str().unwrap();
    let budget = stage["data"]["budget_id"].as_str().unwrap();
    let changed = upload(
        &server,
        "upload",
        include_bytes!("../../tests/fixtures/import-unsupported-rule-26.10.0.zip").to_vec(),
    );
    assert_eq!(changed.status(), 409);
    assert_eq!(
        changed.json::<Value>().unwrap()["error"]["code"],
        "idempotency_conflict"
    );
    assert_eq!(
        server
            .post(
                "/api/v1/budgets",
                "existing",
                json!({"id":budget,"name":"Existing budget"})
            )
            .status(),
        201
    );
    let result = server.post(
        &format!("/api/v1/imports/{id}/activate"),
        "activate",
        json!({}),
    );
    assert_eq!(result.status(), 409);
    assert_eq!(
        result.json::<Value>().unwrap()["error"]["code"],
        "import_budget_conflict"
    );
    let server = server.restart();
    assert_eq!(
        server
            .get(&format!("/api/v1/budgets/{budget}/accounts"))
            .json::<Value>()
            .unwrap()["data"],
        json!([])
    );
    assert_eq!(
        server.get("/api/v1/budgets").json::<Value>().unwrap()["data"][0]["name"],
        "Existing budget"
    );
    assert_eq!(
        server
            .get(&format!("/api/v1/imports/{id}/original.zip"))
            .bytes()
            .unwrap()
            .as_ref(),
        ARCHIVE
    );
}
