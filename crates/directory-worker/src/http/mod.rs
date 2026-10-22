//! HTTP layer: translate requests into core calls and core results into JSON.
//! No business rules here.

pub mod auth;
pub mod health;
pub mod projects;
pub mod storage;
pub mod tasks;

use serde::de::DeserializeOwned;
use serde::Serialize;
use worker::{Headers, Request, Response, Result};

/// JSON request bodies larger than this are rejected before parsing.
const MAX_JSON_BYTES: usize = 64 * 1024;

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

/// API responses are private and never cached by browsers or intermediaries.
pub fn harden(headers: &mut Headers) -> Result<()> {
    headers.set("Cache-Control", "no-store")?;
    headers.set("X-Content-Type-Options", "nosniff")?;
    Ok(())
}

pub fn json<T: Serialize>(status: u16, body: &T) -> Result<Response> {
    let mut resp = Response::from_json(body)?.with_status(status);
    harden(resp.headers_mut())?;
    Ok(resp)
}

pub fn error(status: u16, code: &str) -> Result<Response> {
    json(
        status,
        &ErrorBody {
            error: code,
            message: None,
        },
    )
}

pub fn error_with(status: u16, code: &str, message: impl Into<String>) -> Result<Response> {
    json(
        status,
        &ErrorBody {
            error: code,
            message: Some(message.into()),
        },
    )
}

pub fn no_content() -> Result<Response> {
    let mut resp = Response::empty()?.with_status(204);
    harden(resp.headers_mut())?;
    Ok(resp)
}

/// Parse a bounded JSON body; `Err(response)` is a ready 4xx.
pub async fn read_json<T: DeserializeOwned>(
    req: &mut Request,
) -> Result<std::result::Result<T, Response>> {
    let declared = req
        .headers()
        .get("Content-Length")?
        .and_then(|v| v.parse::<usize>().ok());
    if declared.is_some_and(|n| n > MAX_JSON_BYTES) {
        return Ok(Err(error(413, "too_large")?));
    }
    let bytes = req.bytes().await?;
    if bytes.len() > MAX_JSON_BYTES {
        return Ok(Err(error(413, "too_large")?));
    }
    match serde_json::from_slice(&bytes) {
        Ok(v) => Ok(Ok(v)),
        Err(e) => Ok(Err(error_with(400, "invalid_json", e.to_string())?)),
    }
}

/// Early-return a ready response from a `Result<Result<T, Response>>`.
#[macro_export]
macro_rules! try_or_respond {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(resp) => return Ok(resp),
        }
    };
}
