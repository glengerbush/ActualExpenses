//! Fixed-origin, bounded GoCardless APIv2 transport. Never publish raw errors.
use super::*;
use reqwest::{Method, Url};
use std::io::Read;

pub(super) struct Client {
    pub tokens: Credentials,
    deadline: std::time::Instant,
    remaining_bytes: usize,
}
impl Client {
    pub fn new(tokens: Credentials) -> Self {
        Self {
            tokens,
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(28),
            remaining_bytes: 16 * 1024 * 1024,
        }
    }
    fn base() -> Result<Url, Error> {
        let origin = std::env::var("ACTUALBUDGET_GOCARDLESS_TEST_ORIGIN").ok();
        let raw = origin
            .as_deref()
            .unwrap_or("https://bankaccountdata.gocardless.com");
        let url = Url::parse(raw).map_err(|_| Error::invalid("Invalid provider test origin"))?;
        if origin.is_some()
            && (std::env::var("ACTUALBUDGET_BANK_TEST_ORIGIN")
                .ok()
                .as_deref()
                != Some(raw)
                || !matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "::1")))
        {
            return Err(Error::invalid(
                "Provider test transport must use the explicitly allowed loopback origin",
            ));
        }
        if url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || !matches!(url.scheme(), "http" | "https")
        {
            return Err(Error::invalid("Invalid provider origin"));
        }
        Ok(url)
    }
    pub(super) fn request(
        &mut self,
        method: Method,
        path: &str,
        body: Option<Value>,
        authenticated: bool,
    ) -> Result<Value, Error> {
        if !path.starts_with("/api/v2/") {
            return Err(Error::internal("Invalid provider endpoint"));
        }
        let url = Self::base()?
            .join(path)
            .map_err(|_| Error::internal("Invalid provider endpoint"))?;
        let now = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).timestamp();
        self.tokens.quota.retain(|_, end| *end > now);
        let scope = url.path().to_owned();
        let reset = self
            .tokens
            .quota
            .get("global")
            .copied()
            .unwrap_or(0)
            .max(self.tokens.quota.get(&scope).copied().unwrap_or(0));
        if reset > now {
            return Err(Error::new(
                StatusCode::TOO_MANY_REQUESTS,
                "provider_rate_limited",
                format!(
                    "The provider request window is exhausted. Wait {} seconds before retrying",
                    reset - now
                ),
            ));
        }
        let remaining = self
            .deadline
            .checked_duration_since(std::time::Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| {
                transport::failure(
                    "provider_unavailable",
                    "The bounded provider request timed out. Retry a smaller account selection",
                )
            })?;
        let mut request = transport::client(&url)?
            .request(method, url)
            .timeout(remaining.min(std::time::Duration::from_secs(20)));
        request = request.header(reqwest::header::ACCEPT, "application/json");
        if authenticated {
            request = request.bearer_auth(
                self.tokens
                    .access_token
                    .as_deref()
                    .ok_or_else(|| Error::conflict("Provider access token is missing"))?,
            );
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().map_err(|_| {
            transport::failure(
                "provider_unavailable",
                "GoCardless did not complete the request. Retry or review the saved consent",
            )
        })?;
        let status = response.status().as_u16();
        let seconds = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<i64>().ok())
                .filter(|v| *v >= 0 && *v <= 31 * 86400)
        };
        let hint = |part: &str| {
            seconds(&format!("x-ratelimit-{part}"))
                .or_else(|| seconds(&format!("http_x_ratelimit_{}", part.replace('-', "_"))))
        };
        let global = hint("remaining") == Some(0);
        let account = hint("account-success-remaining") == Some(0);
        if global && let Some(wait) = hint("reset").filter(|v| *v > 0) {
            self.tokens.quota.insert("global".to_owned(), now + wait);
        }
        if account && let Some(wait) = hint("account-success-reset").filter(|v| *v > 0) {
            self.tokens.quota.insert(scope.clone(), now + wait);
        }
        if status == 429 {
            let wait = hint("account-success-reset")
                .or_else(|| hint("reset"))
                .or_else(|| seconds("retry-after"))
                .unwrap_or(60)
                .max(1);
            self.tokens
                .quota
                .insert(if global { "global".to_owned() } else { scope }, now + wait);
        }
        if self.tokens.quota.len() > 4096 {
            return Err(transport::failure(
                "provider_response_limit",
                "Too many provider quota windows. Wait before requesting more accounts",
            ));
        }
        if (300..400).contains(&status) {
            return Err(transport::failure(
                "provider_redirect_refused",
                "GoCardless returned an unexpected redirect. No data was applied",
            ));
        }
        let mut bytes = Vec::new();
        response
            .take(self.remaining_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| {
                transport::failure(
                    "provider_unavailable",
                    "The GoCardless response was interrupted",
                )
            })?;
        if bytes.len() > self.remaining_bytes {
            return Err(transport::failure(
                "provider_response_limit",
                "The provider response exceeds16MiB. No data was applied",
            ));
        }
        self.remaining_bytes -= bytes.len();
        let data: Value = serde_json::from_slice(&bytes).map_err(|_| {
            transport::failure(
                "provider_invalid_response",
                "GoCardless returned invalid JSON. No data was applied",
            )
        })?;
        if !(200..300).contains(&status) {
            let summary = data["summary"].as_str().unwrap_or("").to_ascii_lowercase();
            let (code, message) = match status {
                401 if summary.contains("end user agreement") && summary.contains("expired") => (
                    "provider_consent_expired",
                    "Bank consent expired. Authorize this connection again",
                ),
                401 => (
                    "provider_credentials_invalid",
                    "GoCardless user secrets or access credentials are invalid. Review credentials",
                ),
                403 => (
                    "provider_access_denied",
                    "GoCardless denied access. Review provider permissions or IP restrictions",
                ),
                404 => (
                    "provider_resource_missing",
                    "GoCardless no longer has this resource. Review the connection",
                ),
                409 => (
                    "provider_consent_suspended",
                    "Bank access is suspended. Create a new consent",
                ),
                429 => (
                    "provider_rate_limited",
                    "The institution request limit was reached. Wait before retrying",
                ),
                400 => (
                    "provider_request_rejected",
                    "GoCardless rejected this request. Review the selected institution and consent",
                ),
                _ => (
                    "provider_unavailable",
                    "GoCardless or the institution is unavailable. Retry later",
                ),
            };
            return Err(transport::failure(code, message));
        }
        Ok(data)
    }
    pub(super) fn authenticate(&mut self) -> Result<(), Error> {
        let now = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).timestamp();
        if self.tokens.access_token.is_some()
            && self.tokens.access_expires.is_some_and(|e| e > now + 30)
        {
            return Ok(());
        }
        if self.tokens.refresh_token.is_none() {
            let token = self.request(
                Method::POST,
                "/api/v2/token/new/",
                Some(
                    json!({"secret_id":self.tokens.secret_id,"secret_key":self.tokens.secret_key}),
                ),
                false,
            )?;
            self.accept_token(&token)?;
            if self.tokens.access_token.is_some()
                && self.tokens.access_expires.is_some_and(|e| e > now + 30)
            {
                return Ok(());
            }
        }
        let refresh = self.tokens.refresh_token.as_deref().ok_or_else(|| {
            transport::failure(
                "provider_invalid_response",
                "GoCardless did not return a usable refresh token",
            )
        })?;
        let token = self.request(
            Method::POST,
            "/api/v2/token/refresh/",
            Some(json!({"refresh":refresh})),
            false,
        )?;
        self.accept_token(&token)?;
        if self.tokens.access_token.is_none()
            || !self.tokens.access_expires.is_some_and(|e| e > now + 30)
        {
            return Err(transport::failure(
                "provider_invalid_response",
                "GoCardless did not return a usable access token",
            ));
        }
        Ok(())
    }
    fn accept_token(&mut self, data: &Value) -> Result<(), Error> {
        for key in ["access", "refresh"] {
            if let Some(value) = data.get(key) {
                let token = value
                    .as_str()
                    .filter(|v| {
                        !v.is_empty() && v.len() <= 16384 && !v.chars().any(char::is_control)
                    })
                    .ok_or_else(|| {
                        transport::failure(
                            "provider_invalid_response",
                            "GoCardless returned invalid token data",
                        )
                    })?;
                if key == "access" {
                    let duration = data["access_expires"]
                        .as_i64()
                        .filter(|s| *s > 30 && *s <= 366 * 86400)
                        .ok_or_else(|| {
                            transport::failure(
                                "provider_invalid_response",
                                "GoCardless returned invalid token expiry",
                            )
                        })?;
                    self.tokens.access_token = Some(token.to_owned());
                    self.tokens.access_expires = Some(
                        chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
                            .timestamp()
                            + duration,
                    );
                } else {
                    self.tokens.refresh_token = Some(token.to_owned());
                }
            }
        }
        Ok(())
    }
    pub fn institutions(&mut self, country: &str) -> Result<Value, Error> {
        self.authenticate()?;
        let raw = self.request(
            Method::GET,
            &format!("/api/v2/institutions/?country={country}"),
            None,
            true,
        )?;
        let rows = raw.as_array().filter(|r| r.len() <= 1000).ok_or_else(|| {
            transport::failure(
                "provider_response_limit",
                "Institution listing exceeds1000 entries or has an invalid shape",
            )
        })?;
        let mut output = Vec::new();
        for row in rows {
            let id = identifier(&row["id"])?;
            let name = row["name"]
                .as_str()
                .filter(|v| !v.is_empty() && v.len() <= 1000)
                .ok_or_else(|| {
                    transport::failure(
                        "provider_invalid_response",
                        "Institution details are invalid",
                    )
                })?;
            let countries = row["countries"]
                .as_array()
                .filter(|r| {
                    r.len() <= 300
                        && r.iter().all(|c| {
                            c.as_str().is_some_and(|v| {
                                v.len() == 2 && v.bytes().all(|c| c.is_ascii_uppercase())
                            })
                        })
                })
                .ok_or_else(|| {
                    transport::failure(
                        "provider_invalid_response",
                        "Institution countries are invalid",
                    )
                })?;
            output.push(json!({"id":id,"name":name,"countries":countries,"transaction_total_days":days(&row["transaction_total_days"])? ,"max_access_valid_for_days":days(&row["max_access_valid_for_days"])? ,"account_selection":row["supported_features"].as_array().is_some_and(|f|f.iter().any(|v|v=="account_selection"))}));
        }
        Ok(Value::Array(output))
    }
}
pub(super) fn identifier(value: &Value) -> Result<String, Error> {
    value
        .as_str()
        .filter(|v| {
            !v.is_empty()
                && v.len() <= 200
                && v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        })
        .map(str::to_owned)
        .ok_or_else(|| {
            transport::failure(
                "provider_invalid_response",
                "A provider resource identity is invalid",
            )
        })
}
pub(super) fn days(value: &Value) -> Result<u64, Error> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|v| v.parse().ok()))
        .filter(|v| *v > 0 && *v <= 3650)
        .ok_or_else(|| {
            transport::failure(
                "provider_invalid_response",
                "Institution consent limits are invalid",
            )
        })
}
