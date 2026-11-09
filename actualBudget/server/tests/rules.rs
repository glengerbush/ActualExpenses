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
    for case in f["cases"].as_array().unwrap() {
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
        assert_eq!(response.status(), 200, "{}", response.text().unwrap());
        let preview = response.json::<Value>().unwrap();
        let after = &preview["data"]["transactions"][0]["after"];
        for (native, source) in [
            ("amount", "amount"),
            ("date", "date"),
            ("notes", "notes"),
            ("cleared", "cleared"),
            ("payee_id", "payee"),
            ("category_id", "category"),
        ] {
            if native=="payee_id"&&case["new_payee_name"].is_string(){continue;}
            assert_eq!(
                after[native], case["after"][source],
                "{} {native}",
                case["name"]
            );
        }
        if case["new_payee_name"]=="Generated shop"{assert_eq!(after["new_payee"]["name"],"Generated shop");}
        if case["after"]["tombstone"]==1 {assert_eq!(after["deleted"],true);}
        if let Some(children)=case["after"]["subtransactions"].as_array(){
            let actual=after["children"].as_array().unwrap();assert_eq!(actual.len(),children.len());
            for (a,c) in actual.iter().zip(children){for (native,source) in [("amount","amount"),("notes","notes"),("payee_id","payee"),("category_id","category")] {assert_eq!(a[native],c[source],"{} child {native}",case["name"]);}}
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
