//! External URLs never cross the public error boundary. Each hop is resolved,
//! checked and pinned before connecting; redirects are handled by the adapter.
use crate::error::Error;
use axum::http::StatusCode;
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::{Url, blocking::Client, redirect::Policy};
use std::{
    io::Read,
    net::{IpAddr, ToSocketAddrs},
    time::Duration,
};
pub(super) fn failure(code: &'static str, message: &'static str) -> Error {
    Error::new(StatusCode::BAD_GATEWAY, code, message)
}
fn test_url(url: &Url) -> bool {
    std::env::var("ACTUALBUDGET_BANK_TEST_ORIGIN")
        .ok()
        .and_then(|v| Url::parse(&v).ok())
        .is_some_and(|origin| {
            origin.origin() == url.origin()
                && matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "::1"))
        })
}
fn public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_unspecified()
                && !ip.is_broadcast()
                && !ip.is_multicast()
                && !ip.is_documentation()
                && ip.octets()[0] != 0
                && ip.octets()[0] < 224
                && !(ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1]))
                && !(ip.octets()[0] == 198 && (18..=19).contains(&ip.octets()[1]))
        }
        IpAddr::V6(ip) => ip
            .to_ipv4_mapped()
            .map(|v| public(IpAddr::V4(v)))
            .unwrap_or_else(|| {
                ip.segments()[0] & 0xe000 == 0x2000 && ip.segments()[0..2] != [0x2001, 0xdb8]
            }),
    }
}
pub(super) fn url(raw: &str, credentials: bool) -> Result<Url, Error> {
    let url =
        Url::parse(raw).map_err(|_| Error::invalid("The provider supplied an invalid URL"))?;
    if raw.len() > 8192
        || url.fragment().is_some()
        || url.host_str().is_none()
        || (!test_url(&url) && url.scheme() != "https")
        || (!credentials && (!url.username().is_empty() || url.password().is_some()))
        || (credentials && (url.username().is_empty() || url.password().is_none()))
    {
        return Err(Error::invalid(
            "SimpleFIN requires a valid HTTPS URL and access credentials",
        ));
    }
    Ok(url)
}
pub(super) fn setup(token: &str) -> Result<Url, Error> {
    let bytes = STANDARD
        .decode(token)
        .map_err(|_| Error::invalid("The SimpleFIN setup token is not valid base64"))?;
    let raw = std::str::from_utf8(&bytes)
        .map_err(|_| Error::invalid("The SimpleFIN setup token is not a URL"))?;
    url(raw, false)
}
fn client(url: &Url) -> Result<Client, Error> {
    let host = url
        .host_str()
        .ok_or_else(|| Error::invalid("Missing provider host"))?;
    let addresses = (host, url.port_or_known_default().unwrap_or(443))
        .to_socket_addrs()
        .map_err(|_| {
            failure(
                "provider_unavailable",
                "The provider could not be reached. Retry later",
            )
        })?
        .collect::<Vec<_>>();
    if addresses.is_empty()
        || addresses.len() > 32
        || (!test_url(url) && addresses.iter().any(|a| !public(a.ip())))
    {
        return Err(Error::invalid(
            "Provider URLs must resolve to public HTTPS servers",
        ));
    }
    Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(8))
        .resolve_to_addrs(host, &addresses)
        .no_proxy()
        .build()
        .map_err(|_| failure("provider_unavailable", "Provider transport could not start"))
}
pub(super) fn claim(token: &str) -> Result<String, Error> {
    let claim_url = setup(token)?;
    let response = client(&claim_url)?.post(claim_url).send().map_err(|_| {
        failure(
            "claim_uncertain",
            "The claim response was lost. Obtain a replacement setup token before trying again",
        )
    })?;
    match response.status().as_u16() {
        200..=299 => {}
        403 => {
            return Err(failure(
                "claim_rejected",
                "The setup token is expired or already used. Obtain a new token",
            ));
        }
        _ => {
            return Err(failure(
                "claim_uncertain",
                "The provider did not complete the claim. Obtain a replacement setup token",
            ));
        }
    }
    let mut bytes = Vec::new();
    response.take(8193).read_to_end(&mut bytes).map_err(|_| {
        failure(
            "claim_uncertain",
            "The claim response was interrupted. Obtain a replacement setup token",
        )
    })?;
    if bytes.len() > 8192 {
        return Err(failure(
            "claim_uncertain",
            "The provider claim response exceeds the supported limit",
        ));
    }
    let access = std::str::from_utf8(&bytes)
        .map_err(|_| {
            failure(
                "claim_uncertain",
                "The provider returned an invalid access URL",
            )
        })?
        .trim();
    url(access, true).map_err(|_| {
        failure(
            "claim_uncertain",
            "The provider returned an unsafe access URL. Obtain a replacement setup token",
        )
    })?;
    Ok(access.to_owned())
}

pub(super) fn permit() -> Result<tokio::sync::OwnedSemaphorePermit, Error> {
    static SLOTS: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
        std::sync::OnceLock::new();
    SLOTS
        .get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(4)))
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            Error::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "provider_busy",
                "Other provider requests are finishing. Retry later",
            )
        })
}
#[derive(Clone)]
pub(super) enum Download {
    Balances,
    Transactions {
        accounts: Vec<String>,
        start: i64,
        end: i64,
    },
}
pub(super) fn accounts(access: &str, download: Download) -> Result<serde_json::Value, Error> {
    let mut current = url(access, true)?;
    let username = percent_encoding::percent_decode_str(current.username())
        .decode_utf8()
        .map_err(|_| Error::invalid("Invalid provider access credentials"))?
        .into_owned();
    let password = percent_encoding::percent_decode_str(current.password().unwrap())
        .decode_utf8()
        .map_err(|_| Error::invalid("Invalid provider access credentials"))?
        .into_owned();
    current
        .set_username("")
        .map_err(|_| Error::invalid("Invalid provider URL"))?;
    current
        .set_password(None)
        .map_err(|_| Error::invalid("Invalid provider URL"))?;
    current.set_path(&format!(
        "{}/accounts",
        current.path().trim_end_matches('/')
    ));
    {
        let mut query = current.query_pairs_mut();
        query.append_pair("version", "2");
        match download {
            Download::Balances => {
                query.append_pair("balances-only", "1");
            }
            Download::Transactions {
                accounts,
                start,
                end,
            } => {
                query
                    .append_pair("pending", "1")
                    .append_pair("start-date", &start.to_string())
                    .append_pair("end-date", &end.to_string());
                for account in accounts {
                    query.append_pair("account", &account);
                }
            }
        }
    }
    let mut authorization = Some(format!(
        "Basic {}",
        STANDARD.encode(format!("{username}:{password}"))
    ));
    for hop in 0..=5 {
        let mut request = client(&current)?.get(current.clone());
        if let Some(auth) = &authorization {
            request = request.header(reqwest::header::AUTHORIZATION, auth);
        }
        let response = request.send().map_err(|_| {
            failure(
                "provider_unavailable",
                "The provider response was interrupted. Retry later",
            )
        })?;
        let status = response.status().as_u16();
        if (300..400).contains(&status) {
            if hop == 5 {
                return Err(failure(
                    "provider_redirect_limit",
                    "The provider redirected too many times",
                ));
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| {
                    failure(
                        "provider_invalid_response",
                        "Provider redirect is missing its destination",
                    )
                })?;
            let next = current.join(location).map_err(|_| {
                failure(
                    "provider_invalid_response",
                    "The provider returned an invalid redirect",
                )
            })?;
            url(next.as_str(), false)?;
            if next.origin() != current.origin() {
                authorization = None;
            }
            current = next;
            continue;
        }
        match status {
            200..=299 => {}
            402 => {
                return Err(failure(
                    "provider_payment_required",
                    "The provider requires payment. Review your SimpleFIN subscription",
                ));
            }
            403 => {
                return Err(failure(
                    "provider_revoked",
                    "Provider access was revoked. Replace the setup token",
                ));
            }
            429 => {
                return Err(failure(
                    "provider_rate_limited",
                    "The provider rate limit was reached. Wait before refreshing again",
                ));
            }
            _ => {
                return Err(failure(
                    "provider_unavailable",
                    "The provider is unavailable. Retry later",
                ));
            }
        }
        let mut bytes = Vec::new();
        response
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| {
                failure(
                    "provider_unavailable",
                    "The provider response was interrupted",
                )
            })?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(failure(
                "provider_response_limit",
                "The provider response exceeds 16 MiB. No data was applied",
            ));
        }
        return serde_json::from_slice(&bytes).map_err(|_| {
            failure(
                "provider_invalid_response",
                "The provider returned invalid JSON. No data was applied",
            )
        });
    }
    unreachable!()
}
