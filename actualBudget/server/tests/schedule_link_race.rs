//! Link/catch-up concurrency preserves source-permitted financial outcomes.
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Arc, Barrier},
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
    fn start(data: TempDir) -> Self {
        let web = data.path().join("web");
        std::fs::create_dir_all(&web).unwrap();
        std::fs::write(
            web.join("index.html"),
            "<!doctype html><title>Budget</title>",
        )
        .unwrap();
        let mut process = Command::new(env!("CARGO_BIN_EXE_actualbudget-server"))
            .env("ACTUALBUDGET_WEB_DIR", web)
            .env_remove("ACTUALBUDGET_PUBLIC_ORIGIN")
            .env("ACTUALBUDGET_DATA_DIR", data.path())
            .env("ACTUALBUDGET_PORT", "0")
            .env("ACTUALBUDGET_HOST", "127.0.0.1")
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
            data,
            client: Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
        }
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
    fn restart(mut self) -> Self {
        self.process.kill().unwrap();
        self.process.wait().unwrap();
        let data = std::mem::replace(&mut self.data, tempfile::tempdir().unwrap());
        Self::start(data)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

fn post(server: &Server, path: &str, key: &str, body: &Value) -> Value {
    server
        .client
        .post(format!("{}{path}", server.url))
        .header("Idempotency-Key", key)
        .json(body)
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap()
}
fn financial(row: &Value) -> Value {
    let mut result = serde_json::Map::new();
    for field in [
        "id",
        "account_id",
        "amount",
        "date",
        "payee_id",
        "category_id",
        "notes",
        "cleared",
        "reconciled",
        "parent_id",
        "is_parent",
        "transfer_id",
    ] {
        result.insert(field.into(), row[field].clone());
    }
    Value::Object(result)
}

#[test]
fn explicit_link_racing_catch_up_keeps_existing_payment_and_at_most_one_generated_occurrence() {
    for transfer in [false, true] {
        let mut server = Server::start(tempfile::tempdir().unwrap());
        let b = uuid::Uuid::new_v4().to_string();
        let account = uuid::Uuid::new_v4().to_string();
        let peer = uuid::Uuid::new_v4().to_string();
        let original = uuid::Uuid::new_v4().to_string();
        let counterpart = uuid::Uuid::new_v4().to_string();
        let schedule = uuid::Uuid::new_v4().to_string();
        post(
            &server,
            "/api/v1/budgets",
            "budget",
            &json!({"id":b,"name":"Link race","budget_type":"tracking"}),
        );
        let root = format!("/api/v1/budgets/{b}");
        for (id, name) in [(&account, "Spending"), (&peer, "Savings")] {
            post(
                &server,
                &format!("{root}/accounts"),
                name,
                &json!({"id":id,"name":name,"opening_balance":0,"opening_date":"2026-01-01"}),
            );
        }
        let today = server.get(&format!("{root}/schedules/due"))["data"]["today"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut original_body = json!({"id":original,"account_id":account,"amount":-100,"date":today,"cleared":true,"notes":"Existing payment"});
        if transfer {
            original_body["counterpart_id"] = json!(counterpart);
            original_body["to_account_id"] = json!(peer);
        }
        post(
            &server,
            &format!(
                "{root}/{}",
                if transfer {
                    "transfers"
                } else {
                    "transactions"
                }
            ),
            "existing-payment",
            &original_body,
        );
        let before = server.get(&format!("{root}/transactions"));
        let originals = before["data"].as_array().unwrap().clone();
        assert_eq!(originals.len(), if transfer { 2 } else { 1 });
        let mut conditions = vec![
            json!({"field":"account","op":"is","value":account}),
            json!({"field":"amount","op":"is","value":-100}),
            json!({"field":"date","op":"is","value":today}),
        ];
        if transfer {
            conditions.push(json!({"field":"payee","op":"is","value":originals.iter().find(|r|r["id"]==original).unwrap()["payee_id"]}));
        }
        let cp = server.get(&format!("{root}/checkpoint"));
        post(
            &server,
            &format!("{root}/schedules/commands"),
            "schedule",
            &json!({"action":"create","id":schedule,"snapshot":cp["data"]["snapshot"],"conditions":conditions,"posts_transaction":true}),
        );
        let cp = server.get(&format!("{root}/checkpoint"));
        let link = json!({"action":"link","id":schedule,"transaction_ids":[original],"snapshot":cp["data"]["snapshot"]});
        let catch_up = json!({"action":"catch-up","through":today});
        let barrier = Arc::new(Barrier::new(2));
        let path = format!("{}{root}/schedules/commands", server.url);
        let outcomes = std::thread::scope(|scope| {
            let handles = [
                ("racing-link", link.clone()),
                ("racing-catch-up", catch_up.clone()),
            ]
            .into_iter()
            .map(|(key, body)| {
                let barrier = Arc::clone(&barrier);
                let client = server.client.clone();
                let path = path.clone();
                let epoch = cp["data"]["epoch"].as_str().unwrap().to_owned();
                scope.spawn(move || {
                    barrier.wait();
                    let response = client
                        .post(path)
                        .header("Idempotency-Key", key)
                        .header("X-Actual-Budget-Epoch", epoch)
                        .json(&body)
                        .send()
                        .unwrap();
                    let status = response.status().as_u16();
                    (key, body, status, response.json::<Value>().unwrap())
                })
            })
            .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect::<Vec<_>>()
        });
        let (_, _, link_status, link_result) = &outcomes[0];
        let (_, _, catch_status, catch_result) = &outcomes[1];
        assert_eq!(*catch_status, 200, "{catch_result}");
        let generated = catch_result["data"]["posted_count"].as_u64().unwrap();
        assert!(generated <= 1);
        assert_eq!(
            catch_result["data"]["transaction_ids"]
                .as_array()
                .unwrap()
                .len(),
            generated as usize
        );
        assert!(matches!(*link_status, 200 | 409), "{link_result}");
        if *link_status == 200 {
            assert_eq!(generated, 0, "Link-first must acknowledge existing payment");
        } else {
            assert_eq!(
                generated, 1,
                "Only catch-up can stale the captured link snapshot"
            );
            // Explicit new intent after seeing the new snapshot, never rewrite a
            // captured/retry body. Source keeps both payments after a late link.
            let cp = server.get(&format!("{root}/checkpoint"));
            post(
                &server,
                &format!("{root}/schedules/commands"),
                "refreshed-link",
                &json!({"action":"link","id":schedule,"transaction_ids":[original],"snapshot":cp["data"]["snapshot"]}),
            );
        }
        let ledger = server.get(&format!("{root}/transactions"));
        let rows = ledger["data"].as_array().unwrap();
        assert_eq!(rows.len(), originals.len() * (1 + generated as usize));
        for original_row in &originals {
            assert_eq!(
                financial(rows.iter().find(|r| r["id"] == original_row["id"]).unwrap()),
                financial(original_row)
            );
        }
        assert!(rows.iter().all(|r| r["schedule_id"] == schedule));
        let sum = |id: &str| {
            rows.iter()
                .filter(|r| r["account_id"] == id)
                .map(|r| r["amount"].as_i64().unwrap())
                .sum::<i64>()
        };
        assert_eq!(sum(&account), -100 * (1 + generated as i64));
        assert_eq!(
            sum(&peer),
            if transfer {
                100 * (1 + generated as i64)
            } else {
                0
            }
        );
        if transfer {
            for row in rows {
                let other = rows.iter().find(|r| r["id"] == row["transfer_id"]).unwrap();
                assert_eq!(other["transfer_id"], row["id"]);
                assert_eq!(
                    other["amount"].as_i64().unwrap(),
                    -row["amount"].as_i64().unwrap()
                );
            }
        }
        for key in ["later-catch-up", "another-later-catch-up"] {
            assert_eq!(
                post(
                    &server,
                    &format!("{root}/schedules/commands"),
                    key,
                    &catch_up
                )["data"]["posted_count"],
                0
            );
        }
        let checkpoint = server.get(&format!("{root}/checkpoint"));
        let stable = server.get(&format!("{root}/transactions"));
        server = server.restart();
        for (key, body, status, receipt) in outcomes {
            if status == 200 {
                let response = server
                    .client
                    .post(format!("{}{root}/schedules/commands", server.url))
                    .header("Idempotency-Key", key)
                    .header(
                        "X-Actual-Budget-Epoch",
                        cp["data"]["epoch"].as_str().unwrap(),
                    )
                    .header("X-Actual-Receipt-Only", "true")
                    .json(&body)
                    .send()
                    .unwrap();
                assert_eq!(response.status(), 200);
                assert_eq!(response.json::<Value>().unwrap(), receipt);
            }
        }
        assert_eq!(server.get(&format!("{root}/checkpoint")), checkpoint);
        assert_eq!(server.get(&format!("{root}/transactions")), stable);
    }
}
