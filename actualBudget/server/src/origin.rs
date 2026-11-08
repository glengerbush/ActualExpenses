use crate::error::Error;
use axum::{
    extract::{Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};

#[derive(Clone)]
pub struct OriginPolicy {
    pub public_origin: Option<String>,
}
impl OriginPolicy {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let public_origin = std::env::var("ACTUALBUDGET_PUBLIC_ORIGIN").ok();
        if let Some(origin) = &public_origin {
            let uri: axum::http::Uri = origin.parse()?;
            if !matches!(uri.scheme_str(), Some("http" | "https"))
                || uri.authority().is_none()
                || origin.ends_with('/')
                || uri.path_and_query().is_some_and(|v| v.as_str() != "/")
            {
                return Err("ACTUALBUDGET_PUBLIC_ORIGIN must be an http(s) origin without a trailing slash or path".into());
            }
        }
        Ok(Self { public_origin })
    }
}
pub async fn protect(
    State(policy): State<OriginPolicy>,
    request: Request,
    next: Next,
) -> Result<Response, Error> {
    let is_api = request.uri().path() == "/api" || request.uri().path().starts_with("/api/");
    if is_api {
        let reject = || {
            Error::new(
                StatusCode::FORBIDDEN,
                "origin_rejected",
                "Cross-origin API requests are not allowed",
            )
        };
        if request
            .headers()
            .get("Sec-Fetch-Site")
            .is_some_and(|v| v == "cross-site")
        {
            return Err(reject());
        }
        if let Some(origin) = request.headers().get("Origin") {
            let origin = origin.to_str().map_err(|_| reject())?;
            let host = request
                .headers()
                .get("Host")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(reject)?;
            let expected = policy
                .public_origin
                .unwrap_or_else(|| format!("http://{host}"));
            // Never trust X-Forwarded-* supplied by callers. TLS proxies use an explicitly configured origin.
            if origin != expected {
                return Err(reject());
            }
        }
    }
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        "X-Content-Type-Options",
        HeaderValue::from_static("nosniff"),
    );
    response
        .headers_mut()
        .insert("Referrer-Policy", HeaderValue::from_static("no-referrer"));
    response
        .headers_mut()
        .insert("X-Frame-Options", HeaderValue::from_static("DENY"));
    if is_api {
        response
            .headers_mut()
            .insert("Cache-Control", HeaderValue::from_static("no-store"));
    }
    Ok(response)
}
