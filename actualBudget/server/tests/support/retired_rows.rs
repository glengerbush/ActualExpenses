use serde_json::{Value, json};

/// Compare the retained financial oracle under the explicit notes/status retirement.
/// Original fixtures stay immutable; identities, money, dates, links and receipts remain exact.
pub fn current(mut value: Value) -> Value {
    fn visit(value: &mut Value) {
        match value {
            Value::Array(rows) => rows.iter_mut().for_each(visit),
            Value::Object(row) => {
                if row.get("amount").is_some_and(Value::is_number)
                    && row.get("date").is_some_and(Value::is_string)
                {
                    if row.contains_key("notes") {
                        row.insert("notes".into(), json!(""));
                    }
                    if row.get("cleared").is_some_and(Value::is_boolean) {
                        row.insert("cleared".into(), json!(true));
                    }
                    if row.get("reconciled").is_some_and(Value::is_boolean) {
                        row.insert("reconciled".into(), json!(false));
                    }
                }
                if let Some(actions) = row.get_mut("actions").and_then(Value::as_array_mut) {
                    actions.retain(|a| {
                        !(a["op"] == "set" && a["field"] == "notes"
                            || matches!(a["op"].as_str(), Some("append-notes" | "prepend-notes")))
                    });
                }
                row.values_mut().for_each(visit);
            }
            _ => {}
        }
    }
    visit(&mut value);
    value
}
