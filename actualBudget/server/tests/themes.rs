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
    _directory: tempfile::TempDir,
}
impl Server {
    fn start() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_actualbudget-server"))
            .env("ACTUALBUDGET_DATA_DIR", directory.path())
            .env("ACTUALBUDGET_WEB_DIR", directory.path())
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
            _directory: directory,
        }
    }
    fn post(&self, path: &str, body: Value) -> Value {
        let response = self
            .client
            .post(format!("{}{path}", self.url))
            .json(&body)
            .send()
            .unwrap();
        assert_eq!(response.status(), 200);
        response.json::<Value>().unwrap()["data"].clone()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
#[test]
fn pinned_theme_validation_and_catalog_match_public_contract() {
    let server = Server::start();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../web/tests/fixtures/custom-themes-26.10.0.json"
    ))
    .unwrap();
    let response = server
        .client
        .get(format!("{}/api/v1/themes/catalog", server.url))
        .send()
        .unwrap();
    assert_eq!(response.status(), 200);
    let catalog: Value = response.json().unwrap();
    assert_eq!(catalog["data"]["entries"], fixture["catalog"]["entries"]);
    for case in fixture["validation"].as_array().unwrap() {
        let result = server.post(
            "/api/v1/themes/compile",
            json!({"css":case["css"],"base_theme":"light"}),
        );
        assert_eq!(result["valid"], case["valid"], "{}", case["name"]);
        assert_eq!(result["source"], case["css"]);
    }
    let compiled=server.post("/api/v1/themes/compile",json!({"css":":root { --my-accent: #123456; --color-pageText: var(--my-accent); --color-futureWidget: #fff; }","base_theme":"dark"}));
    assert_eq!(compiled["variables"]["--text"], "#123456");
    assert_eq!(compiled["version"], 2);
    assert_eq!(compiled["variables"]["--color-pageText"], "#123456");
    let formula = server.post(
        "/api/v1/themes/compile",
        json!({"css":":root { --color-warningText: var(--color-pageBackground); --color-chartQual1: #123456; }"}),
    );
    assert_ne!(
        formula["variables_by_base"]["light"]["--color-warningText"],
        formula["variables_by_base"]["dark"]["--color-warningText"]
    );
    assert_eq!(formula["variables"]["--color-chartQual1"], "#123456");
    assert!(
        compiled["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["key"] == "--color-futureWidget")
    );
}

#[test]
fn conversion_preserves_all_raw_sources_and_legacy_precedence() {
    let server = Server::start();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../web/tests/fixtures/custom-themes-26.10.0.json"
    ))
    .unwrap();
    for case in fixture["migrations"].as_array().unwrap() {
        let mut global = json!({});
        for (source, key) in [
            ("lightJson", "installed-custom-theme"),
            ("darkJson", "installed-custom-dark-theme"),
            ("existingOverride", "custom-css-override"),
        ] {
            if let Some(value) = case["input"].get(source) {
                global[key] = value.clone();
            }
        }
        let result = server.post("/api/v1/themes/convert-actual", json!({"global":global}));
        assert_eq!(result["original"], global);
        if !case["result"].is_null() {
            assert_eq!(result["override"]["source"], case["result"]["override"]);
        }
    }
    let global = json!({"installed-custom-theme":"{broken","installed-custom-dark-theme":serde_json::to_string(&json!({"id":"bad","name":"Bad selector","repo":"","cssContent":"body { color: red; }","unknown":"preserved"})).unwrap(),"custom-css-override":17});
    let result = server.post("/api/v1/themes/convert-actual", json!({"global":global}));
    assert_eq!(result["original"], global);
    assert!(result["light"].is_null());
    assert_eq!(result["dark"]["valid"], false);
    assert_eq!(result["warnings"].as_array().unwrap().len(), 2);
}

#[test]
fn compiler_bounds_fonts_retains_cycles_and_resolves_override_per_base() {
    let server = Server::start();
    let result = server.post(
        "/api/v1/themes/compile",
        json!({"css":":root { --color-pageText: var(--color-pageBackground); }"}),
    );
    assert_ne!(
        result["variables_by_base"]["light"]["--text"],
        result["variables_by_base"]["dark"]["--text"]
    );
    let cycle = server.post(
        "/api/v1/themes/compile",
        json!({"css":":root { --x: var(--y); --y: var(--x); --color-pageText: var(--x); }"}),
    );
    assert!(cycle["variables"].as_object().unwrap().is_empty());
    assert!(!cycle["warnings"].as_array().unwrap().is_empty());
    let large = format!(
        "/* {} */ :root {{ --color-pageText: #123456; }}",
        "x".repeat(70000)
    );
    let compiled = server.post("/api/v1/themes/compile", json!({"css":large}));
    assert_eq!(compiled["valid"], true);
    assert_eq!(compiled["source"], large);
    let font = format!(
        "@font-face {{font-family: Big; src: url(data:font/woff2;base64,{});}} :root {{ --font-family: Big; }}",
        "A".repeat(3 * 1024 * 1024)
    );
    let compiled = server.post("/api/v1/themes/compile", json!({"css":font}));
    assert_eq!(compiled["valid"], false);
    assert_eq!(compiled["source"], font);
}

#[test]
fn contextual_dark_base_converts_raw_and_parsed_preferences_without_source_loss() {
    let server = Server::start();
    let installed = json!({"id":"contextual","name":"Contextual","repo":"","cssContent":":root { --color-pageText: var(--color-pageBackground); }"}).to_string();
    for (theme_key, preference_key) in [
        ("installed-custom-dark-theme", "preferred-dark-theme"),
        ("installedCustomDarkTheme", "preferredDarkTheme"),
    ] {
        let global = json!({theme_key:installed, preference_key:"midnight", "theme":"system"});
        let converted = server.post("/api/v1/themes/convert-actual", json!({"global":global}));
        assert_eq!(converted["dark"]["base_theme"], "midnight");
        assert_eq!(converted["original"], global);
        assert!(converted["warnings"].as_array().unwrap().is_empty());
    }
    let global = json!({"installedCustomDarkTheme":installed, "preferred-dark-theme":"dark", "preferredDarkTheme":"midnight"});
    let converted = server.post("/api/v1/themes/convert-actual", json!({"global":global}));
    assert_eq!(converted["dark"]["base_theme"], "dark");
    assert_eq!(converted["original"], global);
    assert!(
        converted["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["key"] == "preferred-dark-theme")
    );
}
