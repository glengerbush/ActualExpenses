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
const B: &str = "30000000-0000-4000-8000-000000000001";
const A: &str = "30000000-0000-4000-8000-000000000002";
const T: &str = "30000000-0000-4000-8000-000000000003";
const C0: &str = "2026-01-01T00:00:00.000Z-0000-0000000000000001";
const C1: &str = "2026-01-02T00:00:00.000Z-0000-0000000000000001";
const C2: &str = "2026-01-03T00:00:00.000Z-0000-0000000000000001";
const C3: &str = "2026-01-04T00:00:00.000Z-0000-0000000000000001";
#[test]
fn receipt_only_reconciles_original_outcomes_without_applying_unknown_work() {
    let s = Server::start();
    let path = setup(&s);
    let base = format!("/api/v1/budgets/{B}");
    let probe = |method: &str, path: &str, key: &str, clock: &str, body: Value| {
        let request = s
            .client
            .request(method.parse().unwrap(), format!("{}{path}", s.url))
            .header("Idempotency-Key", key)
            .header("X-Actual-Edit-Clock", clock)
            .header("X-Actual-Receipt-Only", "true");
        (if body.is_null() {
            request
        } else {
            request.json(&body)
        })
        .send()
        .unwrap()
    };
    let checkpoint = s.get(&format!("{base}/checkpoint"));
    let unknown_body = json!({"id":"30000000-0000-4000-8000-000000000004","account_id":A,"amount":-200,"date":"2026-01-01"});
    let unknown = probe(
        "POST",
        &format!("{base}/transactions"),
        "unknown",
        C1,
        unknown_body.clone(),
    );
    assert_eq!(unknown.status(), 404);
    assert_eq!(
        unknown.json::<Value>().unwrap()["error"]["code"],
        "operation_uncommitted"
    );
    assert_eq!(s.get(&format!("{base}/checkpoint")), checkpoint);
    // A read-only miss must not cancel, claim or otherwise consume the identity.
    assert_eq!(
        s.write(
            "POST",
            &format!("{base}/transactions"),
            "unknown",
            Some(C1),
            unknown_body
        )
        .status(),
        201
    );
    let backup = s
        .write(
            "POST",
            &format!("{base}/backups"),
            "backup",
            None,
            json!({"label":"Before edit"}),
        )
        .json::<Value>()
        .unwrap();
    let patch = json!({"date":"2026-02-02"});
    let edited = s
        .write("PATCH", &path, "edit", Some(C1), patch.clone())
        .json::<Value>()
        .unwrap();
    let deleted = s
        .write("DELETE", &path, "delete", Some(C2), Value::Null)
        .json::<Value>()
        .unwrap();
    assert_eq!(
        s.write(
            "POST",
            &format!(
                "{base}/backups/{}/restore",
                backup["data"]["id"].as_str().unwrap()
            ),
            "restore",
            None,
            json!({"confirm_name":"Concurrent"})
        )
        .status(),
        200
    );
    // Restore changes the cursor, but retained receipts still replay before epoch gates.
    assert_eq!(
        probe("PATCH", &path, "edit", C1, patch.clone())
            .json::<Value>()
            .unwrap(),
        edited
    );
    assert_eq!(
        probe("DELETE", &path, "delete", C2, Value::Null)
            .json::<Value>()
            .unwrap(),
        deleted
    );
    let epoch = s.get(&format!("{base}/checkpoint"))["data"]["epoch"]
        .as_str()
        .unwrap()
        .to_owned();
    let response = s
        .client
        .delete(format!("{}{base}/accounts/{A}", s.url))
        .header("Idempotency-Key", "remove-account")
        .header("X-Actual-Budget-Epoch", &epoch)
        .json(&json!({"confirm_name":"Spending"}))
        .send()
        .unwrap();
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    assert_eq!(
        probe("PATCH", &path, "edit", C1, patch.clone())
            .json::<Value>()
            .unwrap(),
        edited
    );
    assert_eq!(
        probe("PATCH", &path, "edit", C1, json!({"date":"2026-02-03"})).status(),
        409
    );
    let original = probe(
        "POST",
        &format!("{base}/transactions"),
        "create",
        C0,
        json!({"id":T,"account_id":A,"amount":-100,"date":"2026-01-01"}),
    );
    assert_eq!(original.status(), 201);
    assert_eq!(original.json::<Value>().unwrap()["data"]["amount"], -100);
    let rejected = s
        .client
        .patch(format!("{}{path}", s.url))
        .header("Idempotency-Key", "bad-flags")
        .header("X-Actual-Receipt-Only", "true")
        .header("X-Actual-Cancel-If-Uncommitted", "true")
        .json(&patch)
        .send()
        .unwrap();
    assert_eq!(rejected.status(), 400);
    assert_eq!(
        probe(
            "POST",
            "/api/v1/budgets",
            "unsupported",
            C0,
            json!({"id":B,"name":"Forbidden probe"})
        )
        .status(),
        400
    );
}
fn setup(s: &Server) -> String {
    assert_eq!(
        s.write(
            "POST",
            "/api/v1/budgets",
            "budget",
            None,
            json!({"id":B,"name":"Concurrent"})
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
    let path = format!("/api/v1/budgets/{B}/transactions");
    assert_eq!(
        s.write(
            "POST",
            &path,
            "create",
            Some(C0),
            json!({"id":T,"account_id":A,"amount":-100,"date":"2026-01-01"})
        )
        .status(),
        201
    );
    format!("{path}/{T}")
}
#[test]
fn field_ordering_merges_independent_edits_and_replays_original_clock() {
    let s = Server::start();
    let path = setup(&s);
    assert_eq!(
        s.write(
            "PATCH",
            &path,
            "new-date",
            Some(C2),
            json!({"date":"2026-02-10"})
        )
        .status(),
        200
    );
    let result: Value = s
        .write(
            "PATCH",
            &path,
            "late-old",
            Some(C1),
            json!({"date":"2026-02-11","amount":-250}),
        )
        .json()
        .unwrap();
    assert_eq!(result["data"]["date"], "2026-02-10");
    assert_eq!(result["data"]["amount"], -250);
    assert_eq!(result["conflicts"][0]["field"], "date");
    assert_eq!(result["data"]["field_versions"]["amount"], C1);
    let s = s.restart();
    assert_eq!(
        s.write(
            "PATCH",
            &path,
            "late-old",
            Some(C1),
            json!({"date":"2026-02-11","amount":-250})
        )
        .json::<Value>()
        .unwrap(),
        result
    );
    assert_eq!(
        s.write(
            "PATCH",
            &path,
            "late-old",
            Some(C3),
            json!({"date":"2026-02-11","amount":-250})
        )
        .status(),
        409
    );
    let current = s.get(&path);
    assert_eq!(current["data"]["date"], "2026-02-10");
    assert_eq!(current["data"]["amount"], -250);
}
#[test]
fn unpublished_delete_has_durable_rejection_but_published_tombstone_never_resurrects() {
    let s = Server::start();
    let path = setup(&s);
    assert_eq!(
        s.write(
            "PATCH",
            &path,
            "new-edit",
            Some(C2),
            json!({"date":"2026-02-14"})
        )
        .status(),
        200
    );
    let rejection = s.write("DELETE", &path, "old-delete", Some(C1), Value::Null);
    assert_eq!(rejection.status(), 409);
    let rejected: Value = rejection.json().unwrap();
    assert_eq!(rejected["error"]["code"], "deletion_superseded");
    assert_eq!(rejected["data"]["date"], "2026-02-14");
    let deletion = s.write("DELETE", &path, "published", Some(C3), Value::Null);
    assert_eq!(deletion.status(), 200);
    let published: Value = deletion.json().unwrap();
    let s = s.restart();
    assert_eq!(
        s.write("DELETE", &path, "old-delete", Some(C1), Value::Null)
            .json::<Value>()
            .unwrap(),
        rejected
    );
    assert_eq!(
        s.write("DELETE", &path, "published", Some(C3), Value::Null)
            .json::<Value>()
            .unwrap(),
        published
    );
    let later = s.write(
        "PATCH",
        &path,
        "after-deletion",
        Some("2026-01-05T00:00:00.000Z-0000-0000000000000001"),
        json!({"date":"2026-02-09"}),
    );
    assert_eq!(later.status(), 200);
    let hidden: Value = later.json().unwrap();
    assert_eq!(hidden["data"]["deleted"], true);
    assert_eq!(hidden["data"]["date"], "2026-02-09");
    assert!(
        s.get(&format!("/api/v1/budgets/{B}/transactions"))["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn change_cursor_finds_late_old_edits_and_empty_refresh_does_not_advance() {
    let s = Server::start();
    let path = setup(&s);
    let initial = s.get(&format!("/api/v1/budgets/{B}/transactions"));
    let cursor = initial["snapshot"].as_str().unwrap();
    let url = format!("/api/v1/budgets/{B}/changes?cursor={cursor}");
    let empty = s.get(&url);
    assert!(empty["data"].as_array().unwrap().is_empty());
    assert_eq!(empty["next_cursor"], cursor);
    assert_eq!(
        s.write(
            "PATCH",
            &path,
            "new-date",
            Some(C3),
            json!({"date":"2026-02-13"})
        )
        .status(),
        200
    );
    let fetched = s.get(&url);
    let checkpoint = fetched["next_cursor"].as_str().unwrap();
    assert_eq!(
        s.write(
            "PATCH",
            &path,
            "old-amount",
            Some(C1),
            json!({"amount":-300})
        )
        .status(),
        200
    );
    let late = s.get(&format!("/api/v1/budgets/{B}/changes?cursor={checkpoint}"));
    let entries = late["data"].as_array().unwrap();
    assert!(
        entries
            .iter()
            .any(|entry| entry["entity"] == "transactions" && entry["data"]["amount"] == -300)
    );
    let last = late["next_cursor"].as_str().unwrap();
    let s = s.restart();
    let empty = s.get(&format!("/api/v1/budgets/{B}/changes?cursor={last}"));
    assert!(empty["data"].as_array().unwrap().is_empty());
    assert_eq!(empty["next_cursor"], last);
}
#[test]
fn pinned_actual_clock_ordering_and_published_hidden_fields_match() {
    let mut fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/concurrency.json")).unwrap();
    // Notes are retired. Replay the pinned field-clock workload on dates,
    // preserving every clock, amount, deletion policy and expected winner.
    let mut values = std::collections::BTreeSet::new();
    for scenario in fixture["scenarios"].as_array().unwrap() {
        values.insert(scenario["initial"]["notes"].as_str().unwrap().to_owned());
        values.insert(scenario["expected"]["notes"].as_str().unwrap().to_owned());
        for mutation in scenario["mutations"].as_array().unwrap() {
            if let Some(value) = mutation["fields"]["notes"].as_str() {
                values.insert(value.to_owned());
            }
        }
    }
    let dates: std::collections::BTreeMap<_, _> = values
        .into_iter()
        .enumerate()
        .map(|(i, value)| (value, format!("2026-02-{:02}", i + 1)))
        .collect();
    for scenario in fixture["scenarios"].as_array_mut().unwrap() {
        for key in ["initial", "expected"] {
            let row = scenario[key].as_object_mut().unwrap();
            let note = row.remove("notes").unwrap();
            row.insert("date".into(), json!(dates[note.as_str().unwrap()]));
        }
        let versions = scenario["expected"]["field_versions"]
            .as_object_mut()
            .unwrap();
        if let Some(clock) = versions.remove("notes") {
            versions.insert("date".into(), clock);
        }
        for mutation in scenario["mutations"].as_array_mut().unwrap() {
            let fields = mutation["fields"].as_object_mut().unwrap();
            if let Some(note) = fields.remove("notes") {
                fields.insert("date".into(), json!(dates[note.as_str().unwrap()]));
            }
        }
    }
    let mut s = Server::start();
    setup(&s);
    let off = "30000000-0000-4000-8000-000000000020";
    let category = "30000000-0000-4000-8000-000000000021";
    assert_eq!(s.write("POST", &format!("/api/v1/budgets/{B}/accounts"), "oracle-off", None, json!({"id":off,"name":"Off budget","off_budget":true,"opening_balance":0,"opening_date":"2026-01-01"})).status(),201);
    assert_eq!(
        s.write(
            "POST",
            &format!("/api/v1/budgets/{B}/categories"),
            "oracle-category",
            None,
            json!({"id":category,"name":"Food","is_income":false})
        )
        .status(),
        201
    );
    let base = format!("/api/v1/budgets/{B}/transactions");
    for scenario in fixture["scenarios"].as_array().unwrap() {
        let id = scenario["id"].as_str().unwrap();
        let initial = &scenario["initial"];
        let response=s.write("POST",&base,id,initial["clock"].as_str(),json!({"id":id,"account_id":A,"amount":initial["amount"],"date":initial["date"],"cleared":initial["cleared"]}));
        assert_eq!(response.status(), 201);
        let path = format!("{base}/{id}");
        let mut last: Value = Value::Null;
        for (index, mutation) in scenario["mutations"].as_array().unwrap().iter().enumerate() {
            let deletion = mutation["fields"].get("tombstone").is_some();
            let body = if deletion {
                Value::Null
            } else {
                let mut fields = serde_json::Map::new();
                for (field, value) in mutation["fields"].as_object().unwrap() {
                    let target = match field.as_str() {
                        "acct" => "account_id",
                        "category" => "category_id",
                        other => other,
                    };
                    fields.insert(
                        target.to_owned(),
                        match value.as_str() {
                            Some("$off") => json!(off),
                            Some("$category") => json!(category),
                            _ => value.clone(),
                        },
                    );
                }
                Value::Object(fields)
            };
            let response = s.write(
                if deletion { "DELETE" } else { "PATCH" },
                &path,
                &format!("{id}-{index}"),
                mutation["clock"].as_str(),
                body,
            );
            assert_eq!(
                response.status().as_u16(),
                if deletion && scenario.get("policy").is_some() {
                    409
                } else {
                    200
                }
            );
            last = response.json().unwrap();
        }
        let expected = &scenario["expected"];
        assert_eq!(last["data"]["amount"], expected["amount"]);
        assert_eq!(last["data"]["date"], expected["date"]);
        if expected.get("account").is_some() {
            assert_eq!(last["data"]["account_id"], off);
            assert_eq!(last["data"]["category_id"], category);
            assert_eq!(last["data"]["category_name"], expected["category"]);
        }
        for (field, clock) in expected["field_versions"].as_object().unwrap() {
            let field = match field.as_str() {
                "acct" => "account_id",
                "category" => "category_id",
                other => other,
            };
            if field == "tombstone" && scenario.get("policy").is_some() {
                assert_eq!(last["data"]["field_versions"][field], initial["clock"]);
            } else {
                assert_eq!(&last["data"]["field_versions"][field], clock);
            }
        }
        s = s.restart();
        let visible = s.get(&base)["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == id);
        assert_eq!(
            visible,
            scenario.get("policy").is_some() || expected["visible"] == true
        );
    }
}
#[test]
fn skew_and_malformed_clocks_fail_without_consuming_identity_or_advancing_cursor() {
    let s = Server::start();
    let path = setup(&s);
    let before = s.get(&format!("/api/v1/budgets/{B}/transactions"))["snapshot"].clone();
    for clock in [
        "9999-01-01T00:00:00.000Z-0000-0000000000000001",
        "not-a-clock",
        "2026-02-30T00:00:00.000Z-0000-0000000000000001",
        "2026-01-01T00:00:00.000Z-ffff-0000000000000001",
    ] {
        let r = s.write(
            "PATCH",
            &path,
            "bad-clock",
            Some(clock),
            json!({"date":"2026-02-17"}),
        );
        assert_eq!(r.status(), 400);
        assert_eq!(
            s.get(&format!("/api/v1/budgets/{B}/transactions"))["snapshot"],
            before
        );
    }
    assert_eq!(
        s.write(
            "PATCH",
            &path,
            "bad-clock",
            Some(C2),
            json!({"date":"2026-02-17"})
        )
        .status(),
        200
    );
}
#[test]
fn cursor_coalescing_during_pagination_retains_every_latest_entity() {
    let s = Server::start();
    let path = setup(&s);
    let base = format!("/api/v1/budgets/{B}/transactions");
    let ids = [
        T,
        "30000000-0000-4000-8000-000000000004",
        "30000000-0000-4000-8000-000000000005",
    ];
    for id in &ids[1..] {
        assert_eq!(
            s.write(
                "POST",
                &base,
                id,
                Some(C0),
                json!({"id":id,"account_id":A,"amount":-10,"date":"2026-01-01"})
            )
            .status(),
            201
        );
    }
    let initial = s.get(&base);
    let mut cursor = initial["snapshot"].as_str().unwrap().to_owned();
    for id in ids {
        assert_eq!(
            s.write(
                "PATCH",
                &format!("{base}/{id}"),
                &format!("patch-{id}"),
                Some(C1),
                json!({"date":"2026-02-12"})
            )
            .status(),
            200
        );
    }
    let page = s.get(&format!(
        "/api/v1/budgets/{B}/changes?cursor={cursor}&limit=1"
    ));
    assert_eq!(page["has_more"], true);
    cursor = page["next_cursor"].as_str().unwrap().to_owned();
    let mut seen = std::collections::BTreeMap::new();
    for entry in page["data"].as_array().unwrap() {
        seen.insert(
            entry["id"].as_str().unwrap().to_owned(),
            entry["data"]["date"].clone(),
        );
    }
    assert_eq!(
        s.write(
            "PATCH",
            &format!("{base}/{}", ids[1]),
            "coalesced-unseen",
            Some(C2),
            json!({"date":"2026-02-16"})
        )
        .status(),
        200
    );
    assert_eq!(
        s.write(
            "PATCH",
            &path,
            "coalesced-seen",
            Some(C2),
            json!({"date":"2026-02-16"})
        )
        .status(),
        200
    );
    for _ in 0..10 {
        let page = s.get(&format!(
            "/api/v1/budgets/{B}/changes?cursor={cursor}&limit=1"
        ));
        for entry in page["data"].as_array().unwrap() {
            seen.insert(
                entry["id"].as_str().unwrap().to_owned(),
                entry["data"]["date"].clone(),
            );
        }
        cursor = page["next_cursor"].as_str().unwrap().to_owned();
        if page["has_more"] == false {
            break;
        }
    }
    assert_eq!(seen[T], "2026-02-16");
    assert_eq!(seen[ids[1]], "2026-02-16");
    assert_eq!(seen[ids[2]], "2026-02-12");
    assert!(
        s.get(&format!("/api/v1/budgets/{B}/changes?cursor={cursor}"))["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let other = "30000000-0000-4000-8000-000000000099";
    assert_eq!(
        s.write(
            "POST",
            "/api/v1/budgets",
            "other",
            None,
            json!({"id":other,"name":"Other"})
        )
        .status(),
        409
    );
    assert_eq!(s.get(&base)["snapshot"], cursor);
    assert_eq!(
        s.client
            .get(format!(
                "{}/api/v1/budgets/{other}/changes?cursor={cursor}",
                s.url
            ))
            .send()
            .unwrap()
            .status(),
        404
    );
}
#[test]
fn genuine_schema_three_receipts_replay_exactly_without_clock_reinterpretation() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/server-v3.json")).unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::write(
        data.path().join("actualbudget.sqlite3"),
        include_bytes!("../../tests/fixtures/server-v3.sqlite3"),
    )
    .unwrap();
    let s = Server::open(data);
    for receipt in fixture["receipts"].as_array().unwrap() {
        let response = s.write(
            receipt["method"].as_str().unwrap(),
            &format!("/api/v1{}", receipt["path"].as_str().unwrap()),
            receipt["key"].as_str().unwrap(),
            None,
            receipt["body"].clone(),
        );
        assert_eq!(
            response.status().as_u16() as u64,
            receipt["status"].as_u64().unwrap()
        );
        assert_eq!(response.json::<Value>().unwrap(), receipt["response"]);
    }
    let budget = fixture["budget_id"].as_str().unwrap();
    let path = format!("/api/v1/budgets/{budget}/transactions");
    let rows = s.get(&path);
    assert_eq!(rows["data"].as_array().unwrap().len(), 1);
    assert_eq!(rows["balances"]["total"], -234);
    for clock in rows["data"][0]["field_versions"]
        .as_object()
        .unwrap()
        .values()
    {
        assert_eq!(clock, "1970-01-01T00:00:00.000Z-0000-0000000000000000");
    }
    let s = s.restart();
    assert_eq!(s.get(&path)["snapshot"], rows["snapshot"]);
}
#[test]
fn bounded_future_counter_overflow_is_recoverable_without_consuming_operation() {
    let s = Server::start();
    let path = setup(&s);
    let before = s.get(&format!("/api/v1/budgets/{B}/transactions"))["snapshot"].clone();
    let future = chrono::DateTime::<chrono::Utc>::from(
        std::time::SystemTime::now() + Duration::from_secs(60),
    )
    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let clock = format!("{future}-FFFF-0000000000000001");
    let response = s.write(
        "PATCH",
        &path,
        "overflow",
        Some(&clock),
        json!({"date":"2026-02-05"}),
    );
    assert_eq!(response.status(), 400);
    assert_eq!(
        response.json::<Value>().unwrap()["error"]["code"],
        "clock_overflow"
    );
    assert_eq!(
        s.get(&format!("/api/v1/budgets/{B}/transactions"))["snapshot"],
        before
    );
    assert_eq!(
        s.write(
            "PATCH",
            &path,
            "overflow",
            Some(C3),
            json!({"date":"2026-02-05"})
        )
        .status(),
        200
    );
}
#[test]
fn new_opening_balance_has_creation_order_instead_of_historical_zero_floor() {
    let s = Server::start();
    assert_eq!(
        s.write(
            "POST",
            "/api/v1/budgets",
            "budget",
            None,
            json!({"id":B,"name":"Opening ordering"})
        )
        .status(),
        201
    );
    let account_path = format!("/api/v1/budgets/{B}/accounts");
    let body = json!({"id":A,"name":"Spending","opening_balance":100,"opening_date":"2026-01-01"});
    let created: Value = s
        .write("POST", &account_path, "opening", None, body.clone())
        .json()
        .unwrap();
    let register = s.get(&format!("/api/v1/budgets/{B}/transactions"));
    let opening = &register["data"][0];
    assert!(opening["field_versions"]["amount"].as_str().unwrap() > C3);
    let path = format!(
        "/api/v1/budgets/{B}/transactions/{}",
        opening["id"].as_str().unwrap()
    );
    let old = s.write(
        "PATCH",
        &path,
        "older-than-opening",
        Some(C0),
        json!({"amount":200}),
    );
    assert_eq!(old.status(), 200);
    let result: Value = old.json().unwrap();
    assert_eq!(result["data"]["amount"], 100);
    assert_eq!(result["conflicts"][0]["field"], "amount");
    let s = s.restart();
    assert_eq!(
        s.write("POST", &account_path, "opening", None, body)
            .json::<Value>()
            .unwrap(),
        created
    );
    assert_eq!(
        s.get(&path)["data"]["field_versions"],
        opening["field_versions"]
    );
}

#[test]
fn concurrent_account_and_category_edits_converge_in_both_orders() {
    const OFF: &str = "30000000-0000-4000-8000-000000000020";
    const CATEGORY: &str = "30000000-0000-4000-8000-000000000021";
    const COPY: &str = "30000000-0000-4000-8000-000000000022";
    for newer_first in [false, true] {
        let s = Server::start();
        let path = setup(&s);
        assert_eq!(s.write("POST", &format!("/api/v1/budgets/{B}/accounts"), "off", None, json!({"id":OFF,"name":"Off budget","off_budget":true,"opening_balance":0,"opening_date":"2026-01-01"})).status(), 201);
        assert_eq!(
            s.write(
                "POST",
                &format!("/api/v1/budgets/{B}/categories"),
                "category",
                None,
                json!({"id":CATEGORY,"name":"Food","is_income":false})
            )
            .status(),
            201
        );
        let older = json!({"account_id":OFF,"category_id":null,"date":"2026-02-04"});
        let newer = json!({"category_id":CATEGORY});
        let operations = if newer_first {
            vec![
                ("newer", C2, newer.clone()),
                ("2026-02-11", C1, older.clone()),
            ]
        } else {
            vec![
                ("2026-02-11", C1, older.clone()),
                ("newer", C2, newer.clone()),
            ]
        };
        let mut outcomes = vec![];
        for (key, clock, body) in &operations {
            let response = s.write("PATCH", &path, key, Some(clock), body.clone());
            assert_eq!(
                response.status(),
                200,
                "order={newer_first}, key={key}: {}",
                response.text().unwrap()
            );
            // Replay is checked independently from the current merged row.
            outcomes.push(
                s.write("PATCH", &path, key, Some(clock), body.clone())
                    .json::<Value>()
                    .unwrap(),
            );
        }
        let row = s.get(&path)["data"].clone();
        assert_eq!(row["account_id"], OFF);
        assert_eq!(row["category_id"], CATEGORY);
        assert_eq!(row["category_name"], "Food");
        assert_eq!(row["date"], "2026-02-04");
        assert_eq!(row["field_versions"]["account_id"], C1);
        assert_eq!(row["field_versions"]["category_id"], C2);
        assert_eq!(row["field_versions"]["date"], C1);
        for (account, total) in [(A, 0), (OFF, -100)] {
            let page = s.get(&format!(
                "/api/v1/budgets/{B}/transactions?account_id={account}"
            ));
            assert_eq!(
                page["balances"],
                json!({"total":total,"cleared":total,"uncleared":0})
            );
        }
        let s = s.restart();
        assert_eq!(s.get(&path)["data"], row);
        for ((key, clock, body), outcome) in operations.iter().zip(outcomes) {
            assert_eq!(
                s.write("PATCH", &path, key, Some(clock), body.clone())
                    .json::<Value>()
                    .unwrap(),
                outcome
            );
        }
        assert_eq!(s.write("POST", &format!("/api/v1/budgets/{B}/transactions"), "new-off-category", Some(C3), json!({"id":COPY,"account_id":OFF,"category_id":CATEGORY,"amount":-100,"date":"2026-01-01"})).status(),400);
        let duplicate = s.write(
            "POST",
            &format!("{path}/duplicate"),
            "duplicate",
            Some(C3),
            json!({"id":COPY}),
        );
        assert_eq!(duplicate.status(), 201, "{}", duplicate.text().unwrap());
        let copied = s.get(&format!("/api/v1/budgets/{B}/transactions/{COPY}"))["data"].clone();
        assert_eq!(copied["account_id"], OFF);
        assert_eq!(copied["category_id"], CATEGORY);
        assert_eq!(copied["field_versions"]["category_id"], C3);
        assert_eq!(
            s.get(&format!(
                "/api/v1/budgets/{B}/transactions?account_id={OFF}"
            ))["balances"]["total"],
            -200
        );
    }
}

#[test]
fn cancellation_atomically_fences_late_originals_and_replays_committed_outcomes() {
    let s = Server::start();
    let path = setup(&s);
    let create_path = format!("/api/v1/budgets/{B}/transactions");
    let created_id = "30000000-0000-4000-8000-000000000030";
    let duplicate_id = "30000000-0000-4000-8000-000000000031";
    let operations = [
        (
            "POST",
            create_path.clone(),
            "cancel-create",
            json!({"id":created_id,"account_id":A,"amount":-200,"date":"2026-01-01"}),
        ),
        (
            "PATCH",
            path.clone(),
            "cancel-patch",
            json!({"date":"2026-02-08"}),
        ),
        ("DELETE", path.clone(), "cancel-delete", Value::Null),
        (
            "POST",
            format!("{path}/duplicate"),
            "cancel-duplicate",
            json!({"id":duplicate_id}),
        ),
    ];
    let mut outcomes = vec![];
    for (method, url, key, body) in &operations {
        let mut request = s
            .client
            .request(method.parse().unwrap(), format!("{}{url}", s.url))
            .header("Idempotency-Key", *key)
            .header("X-Actual-Edit-Clock", C1)
            .header("X-Actual-Cancel-If-Uncommitted", "true");
        if !body.is_null() {
            request = request.json(body);
        }
        let response = request.send().unwrap();
        assert_eq!(response.status(), 409, "{key}");
        let response: Value = response.json().unwrap();
        assert_eq!(response["error"]["code"], "operation_canceled");
        outcomes.push(response);
    }
    let s = s.restart();
    for ((method, url, key, body), outcome) in operations.iter().zip(outcomes) {
        let response = s.write(method, url, key, Some(C1), body.clone());
        assert_eq!(response.status(), 409);
        assert_eq!(response.json::<Value>().unwrap(), outcome);
    }
    assert_eq!(s.get(&create_path)["data"].as_array().unwrap().len(), 1);
    assert_eq!(s.get(&path)["data"]["date"], "2026-01-01");
    let original = s
        .write(
            "PATCH",
            &path,
            "already-saved",
            Some(C2),
            json!({"date":"2026-02-15"}),
        )
        .json::<Value>()
        .unwrap();
    let canceled = s
        .client
        .patch(format!("{}{path}", s.url))
        .header("Idempotency-Key", "already-saved")
        .header("X-Actual-Edit-Clock", C2)
        .header("X-Actual-Cancel-If-Uncommitted", "true")
        .json(&json!({"date":"2026-02-15"}))
        .send()
        .unwrap();
    assert_eq!(canceled.status(), 200);
    assert_eq!(canceled.json::<Value>().unwrap(), original);
    let invalid = s
        .client
        .patch(format!("{}{path}", s.url))
        .header("Idempotency-Key", "bad-cancel-header")
        .header("X-Actual-Cancel-If-Uncommitted", "false")
        .json(&json!({"date":"2026-02-07"}))
        .send()
        .unwrap();
    assert_eq!(invalid.status(), 400);
    let unsupported = s
        .client
        .get(format!("{}/api/v1/info", s.url))
        .header("X-Actual-Cancel-If-Uncommitted", "true")
        .send()
        .unwrap();
    assert_eq!(unsupported.status(), 400);
    assert!(
        s.get("/api/v1/info")["data"]["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("transaction_cancellation"))
    );
    let changed = s
        .client
        .patch(format!("{}{path}", s.url))
        .header("Idempotency-Key", "already-saved")
        .header("X-Actual-Edit-Clock", C2)
        .header("X-Actual-Cancel-If-Uncommitted", "true")
        .json(&json!({"date":"2026-02-06"}))
        .send()
        .unwrap();
    assert_eq!(changed.status(), 409);
    assert_eq!(
        changed.json::<Value>().unwrap()["error"]["code"],
        "idempotency_conflict"
    );
    assert_eq!(s.get(&path)["data"]["date"], "2026-02-15");
}
