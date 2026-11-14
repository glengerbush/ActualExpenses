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
fn genuine_actual_payee_names_survive_import_without_normalization_or_financial_changes() {
    let server = Server::start();
    let bytes = include_bytes!("../../tests/fixtures/import-payee-names.zip");
    let expected: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/import-payee-names.json")).unwrap();
    let response = upload(&server, "source-payee-names", bytes.to_vec());
    assert_eq!(response.status(), 201, "{}", response.text().unwrap());
    let stage: Value = response.json().unwrap();
    assert_eq!(stage["data"]["status"], "ready");
    let id = stage["data"]["id"].as_str().unwrap();
    let budget = stage["data"]["budget_id"].as_str().unwrap();
    assert_eq!(
        server
            .get(&format!("/api/v1/imports/{id}/original.zip"))
            .bytes()
            .unwrap()
            .as_ref(),
        bytes
    );
    let activation = server.post(
        &format!("/api/v1/imports/{id}/activate"),
        "activate-names",
        json!({}),
    );
    assert_eq!(activation.status(), 201, "{}", activation.text().unwrap());
    let server = server.restart();
    let root = format!("/api/v1/budgets/{budget}");
    let payees: Value = server
        .get(&format!("{root}/payees?include_deleted=true"))
        .json()
        .unwrap();
    for expected_payee in expected["payees"].as_array().unwrap() {
        let imported = payees["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == expected_payee["id"])
            .unwrap();
        assert_eq!(imported["name"], expected_payee["name"]);
    }
    let deleted = payees["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == expected["deletedPayee"])
        .unwrap();
    assert_eq!(deleted["name"], "");
    assert_eq!(deleted["deleted"], true);
    let transactions: Value = server.get(&format!("{root}/transactions")).json().unwrap();
    assert_eq!(
        transactions["data"].as_array().unwrap().len(),
        expected["transactions"].as_array().unwrap().len()
    );
    for original in expected["transactions"].as_array().unwrap() {
        let imported = transactions["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == original["id"])
            .unwrap();
        assert_eq!(imported["payee_id"], original["payee"]);
        assert_eq!(imported["amount"], original["amount"]);
        assert_eq!(imported["notes"], "");
    }
    let accounts: Value = server.get(&format!("{root}/accounts")).json().unwrap();
    assert_eq!(accounts["data"][0]["balance"], expected["balance"]);

    let backup_response = server.post(
        &format!("{root}/backups"),
        "backup-names",
        json!({"label":"Imported names"}),
    );
    assert!(
        backup_response.status().is_success(),
        "{}",
        backup_response.text().unwrap()
    );
    let backup: Value = backup_response.json().unwrap();
    let backup_id = backup["data"]["id"].as_str().unwrap();
    let archive = server.get(&format!("{root}/backups/{backup_id}/download"));
    assert!(archive.status().is_success());
    let restored = Server::start();
    let response = restored
        .client
        .post(format!("{}/api/v1/restores", restored.url))
        .header("Content-Type", "application/zip")
        .header("Idempotency-Key", "restore-names")
        .body(archive.bytes().unwrap())
        .send()
        .unwrap();
    assert!(
        response.status().is_success(),
        "{}",
        response.text().unwrap()
    );
    let stage: Value = response.json().unwrap();
    let restore_id = stage["data"]["id"].as_str().unwrap();
    let response = restored.post(
        &format!("{root}/restores/{restore_id}/activate"),
        "activate-restore",
        json!({"confirm_name":"Source payee names"}),
    );
    assert!(
        response.status().is_success(),
        "{}",
        response.text().unwrap()
    );
    let restored_payees: Value = restored
        .get(&format!("{root}/payees?include_deleted=true"))
        .json()
        .unwrap();
    assert_eq!(restored_payees["data"], payees["data"]);
    let restored_transactions: Value = restored
        .get(&format!("{root}/transactions"))
        .json()
        .unwrap();
    assert_eq!(restored_transactions["data"], transactions["data"]);
}

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
        assert_eq!(rows["balances"]["cleared"], balances["total"]);
        assert_eq!(rows["balances"]["uncleared"], 0);
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
            assert_eq!(converted["cleared"], true);
            assert_eq!(converted["reconciled"], false);
            for field in ["amount", "date"] {
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
    assert!(preferences["data"].get("currency_code").is_none());
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
fn retired_template_rule_does_not_block_financial_import() {
    let server = Server::start();
    let bytes =
        include_bytes!("../../tests/fixtures/rules-template-requirement-26.10.0.zip").to_vec();
    let response = upload(&server, "rule", bytes.clone());
    assert_eq!(response.status(), 201);
    let stage: Value = response.json().unwrap();
    assert_eq!(stage["data"]["status"], "ready");
    assert!(
        !stage["data"]["report"]["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["blocking"] == true)
    );
    let id = stage["data"]["id"].as_str().unwrap();
    let response = server.post(
        &format!("/api/v1/imports/{id}/activate"),
        "activate",
        json!({}),
    );
    assert_eq!(response.status(), 201);
    let budget = stage["data"]["budget_id"].as_str().unwrap();
    let rules = server
        .get(&format!("/api/v1/budgets/{budget}/rules"))
        .json::<Value>()
        .unwrap();
    assert!(
        !rules["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == "19190000-0000-4000-8000-000000009942")
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
        server.get("/api/v1/budgets").json::<Value>().unwrap()["data"]
            .as_array()
            .unwrap()
            .len(),
        1
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
    for (index,bytes) in [b"not zip".to_vec(),repack(encrypted),repack(corrupt),repack(traversal),repack(duplicate),repack(missing),alter_source("UPDATE transactions SET acct='00000000-0000-4000-8000-000000000099' WHERE notes='Corrected expense'"),alter_source("UPDATE transactions SET amount=-9223372036854775808 WHERE notes='Corrected expense'"),alter_source("UPDATE transactions SET sort_order=1e999 WHERE notes='Corrected expense'"),alter_source("UPDATE transactions SET sort_order='bad-order' WHERE notes='Corrected expense'")].into_iter().enumerate(){
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
    assert_eq!(
        server
            .post(
                &format!("/api/v1/budgets/{budget}/categories"),
                "existing-category",
                json!({"id":uuid::Uuid::new_v4().to_string(),"name":"Existing category"})
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

#[test]
fn imported_equal_sort_values_remain_equal_without_losing_fractional_order() {
    let server = Server::start();
    // Synthetic source variant: equal sort positions must remain a tie so each
    // consumer can apply its documented ID tie-break. No destination DB access.
    let bytes = alter_source(
        "UPDATE transactions SET date=20260102;
         UPDATE transactions SET sort_order=CASE id
         WHEN '04000000-0000-4000-8000-000000000001' THEN -2.5
         WHEN '04000000-0000-4000-8000-000000000002' THEN 0.25
         WHEN '04000000-0000-4000-8000-000000000003' THEN 0.25
         ELSE 1.5 END
         WHERE id IN ('04000000-0000-4000-8000-000000000001',
                      '04000000-0000-4000-8000-000000000002',
                      '04000000-0000-4000-8000-000000000003',
                      '04000000-0000-4000-8000-000000000005')",
    );
    let stage = upload(&server, "tied-source", bytes.clone())
        .json::<Value>()
        .unwrap();
    assert_eq!(stage["data"]["status"], "ready");
    let import = stage["data"]["id"].as_str().unwrap();
    let budget = stage["data"]["budget_id"].as_str().unwrap();
    assert_eq!(
        server
            .post(
                &format!("/api/v1/imports/{import}/activate"),
                "activate-ties",
                json!({})
            )
            .status(),
        201
    );
    let response = server
        .get(&format!("/api/v1/budgets/{budget}/transactions"))
        .json::<Value>()
        .unwrap();
    let rows = response["data"].as_array().unwrap();
    let rank = |suffix: &str| {
        rows.iter()
            .find(|row| row["id"].as_str().unwrap().ends_with(suffix))
            .unwrap()["sort_order"]
            .as_i64()
            .expect("public ordering rank")
    };
    assert_eq!(
        rank("000000000002"),
        rank("000000000003"),
        "equal source sort positions must remain equal"
    );
    assert!(rank("000000000001") < rank("000000000002"));
    assert!(rank("000000000003") < rank("000000000005"));
    // Pinned AQL default: date DESC, starting_balance_flag ASC,
    // sort_order DESC, id ASC (reference schema/index.ts transaction order).
    // With all dates equal, the opening balance follows ordinary entries even
    // though its source sort value is larger. Running amounts are the worked
    // example: 12500 + 20000 - 1200 + 234 - 100 = 31434.
    let account = rows
        .iter()
        .find(|row| row["id"] == "04000000-0000-4000-8000-000000000002")
        .unwrap()["account_id"]
        .as_str()
        .unwrap();
    let path = format!("/api/v1/budgets/{budget}/transactions?account_id={account}");
    let register = server.get(&path).json::<Value>().unwrap();
    let ordered = register["data"].as_array().unwrap();
    assert_eq!(
        ordered
            .iter()
            .take(4)
            .map(|row| row["id"].as_str().unwrap().rsplit('-').next().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "000000000005",
            "000000000002",
            "000000000003",
            "000000000001"
        ]
    );
    assert_eq!(
        ordered
            .iter()
            .map(|row| row["running_balance"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![31434, 31534, 32734, 32500, 12500]
    );
    let page = server
        .get(&format!(
            "{path}&limit=2&offset=2&snapshot={}",
            register["snapshot"].as_str().unwrap()
        ))
        .json::<Value>()
        .unwrap();
    assert_eq!(page["data"], json!(ordered[2..4]));
    assert_eq!(
        server
            .get(&format!("/api/v1/imports/{import}/original.zip"))
            .bytes()
            .unwrap()
            .as_ref(),
        bytes
    );
}

#[test]
fn imported_tied_running_balances_use_display_order_at_the_money_boundary() {
    for (positive_id, negative_id, expected_status) in [(2, 3, 201), (3, 2, 400)] {
        let server = Server::start();
        // Worked source: MAX opening, then -1 followed by +1 in chronological
        // (reverse register) order is safe. Swapping those tied IDs exceeds MAX
        // transiently and must reject the whole activation despite the same total.
        let bytes = alter_source(&format!(
            "UPDATE transactions SET date=20260102,sort_order=0,amount=0,cleared=1;
             UPDATE transactions SET amount=9007199254740991 WHERE id='c4e26ad7-701c-483f-8e0f-2a6b8f7e3261';
             UPDATE transactions SET amount=1 WHERE id='04000000-0000-4000-8000-{positive_id:012}';
             UPDATE transactions SET amount=-1 WHERE id='04000000-0000-4000-8000-{negative_id:012}';"
        ));
        let stage = upload(&server, "boundary", bytes).json::<Value>().unwrap();
        assert_eq!(stage["data"]["status"], "ready");
        let id = stage["data"]["id"].as_str().unwrap();
        let budget = stage["data"]["budget_id"].as_str().unwrap();
        let activated = server.post(
            &format!("/api/v1/imports/{id}/activate"),
            "boundary-activate",
            json!({}),
        );
        assert_eq!(
            activated.status(),
            expected_status,
            "{}",
            activated.text().unwrap()
        );
        if expected_status == 201 {
            let register = server.get(&format!("/api/v1/budgets/{budget}/transactions?account_id=0165d0a7-99e2-4719-a102-c007b7a8daaa")).json::<Value>().unwrap();
            assert_eq!(
                register["data"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|row| row["running_balance"].as_i64().unwrap())
                    .collect::<Vec<_>>(),
                vec![
                    9007199254740991_i64,
                    9007199254740991,
                    9007199254740990,
                    9007199254740991,
                    9007199254740991
                ]
            );
        } else {
            assert_eq!(
                server.get("/api/v1/budgets").json::<Value>().unwrap()["data"],
                json!([])
            );
            assert_eq!(
                server
                    .get(&format!("/api/v1/imports/{id}"))
                    .json::<Value>()
                    .unwrap()["data"]["status"],
                "ready"
            );
        }
    }
}

#[test]
fn imported_null_sort_position_stays_below_negative_and_signed_zero_positions() {
    let server = Server::start();
    let bytes = alter_source(
        "UPDATE transactions SET date=20260102;
         UPDATE transactions SET sort_order=NULL WHERE id='04000000-0000-4000-8000-000000000001';
         UPDATE transactions SET sort_order=-2.5 WHERE id='04000000-0000-4000-8000-000000000002';
         UPDATE transactions SET sort_order=-0.0 WHERE id='04000000-0000-4000-8000-000000000003';
         UPDATE transactions SET sort_order=0.0 WHERE id='04000000-0000-4000-8000-000000000005';",
    );
    let stage = upload(&server, "null-order", bytes)
        .json::<Value>()
        .unwrap();
    assert_eq!(stage["data"]["status"], "ready");
    let import = stage["data"]["id"].as_str().unwrap();
    let budget = stage["data"]["budget_id"].as_str().unwrap();
    assert_eq!(
        server
            .post(
                &format!("/api/v1/imports/{import}/activate"),
                "activate-null",
                json!({})
            )
            .status(),
        201
    );
    let register = server
        .get(&format!(
            "/api/v1/budgets/{budget}/transactions?account_id=0165d0a7-99e2-4719-a102-c007b7a8daaa"
        ))
        .json::<Value>()
        .unwrap();
    let rows = register["data"].as_array().unwrap();
    // SQLite DESC puts NULL below every numeric position, including negatives.
    assert_eq!(
        rows.iter()
            .take(4)
            .map(|row| row["id"].as_str().unwrap().rsplit('-').next().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "000000000003",
            "000000000005",
            "000000000002",
            "000000000001"
        ]
    );
    assert_eq!(rows[0]["sort_order"], rows[1]["sort_order"]);
    assert!(rows[1]["sort_order"].as_i64().unwrap() > rows[2]["sort_order"].as_i64().unwrap());
    assert!(rows[2]["sort_order"].as_i64().unwrap() > rows[3]["sort_order"].as_i64().unwrap());
}
