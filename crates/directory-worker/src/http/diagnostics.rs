//! Client error reports (DECISIONS D-027). Every route requires an
//! authenticated owner; reports are bounded per owner.

use super::auth::principal;
use super::{error, error_with, json, no_content, read_json_limited};
use crate::{bindings, d1::D1DiagnosticsRepository, entropy::WebCryptoEntropy, try_or_respond};
use directory_core::diagnostics::{DiagnosticsError, DiagnosticsService, NewClientError};
use serde_json::json as body;
use worker::{Date, Request, Response, Result, RouteContext};

/// Message plus stack, with room for JSON escaping.
const REPORT_JSON_BYTES: usize = 32 * 1024;

fn diagnostics_error(e: DiagnosticsError) -> Result<Response> {
    match e {
        DiagnosticsError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        DiagnosticsError::Backend(detail) => {
            worker::console_error!("diagnostics backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

fn now() -> i64 {
    Date::now().as_millis() as i64
}

pub async fn report(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &rc.env).await?);
    let input: NewClientError =
        try_or_respond!(read_json_limited(&mut req, REPORT_JSON_BYTES).await?);
    let ua = req.headers().get("User-Agent")?.unwrap_or_default();
    let repo = D1DiagnosticsRepository::new(rc.env.d1(bindings::DB)?);
    match DiagnosticsService::new(&repo, &WebCryptoEntropy, now())
        .record(&p, input, &ua)
        .await
    {
        Ok(e) => {
            // Also visible in Workers logs (`wrangler tail`).
            worker::console_warn!("client error [{}]: {}", e.module, e.message);
            json(201, &body!({ "id": e.id }))
        }
        Err(e) => diagnostics_error(e),
    }
}

pub async fn list(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &rc.env).await?);
    let repo = D1DiagnosticsRepository::new(rc.env.d1(bindings::DB)?);
    match DiagnosticsService::new(&repo, &WebCryptoEntropy, now())
        .list(&p)
        .await
    {
        Ok(errors) => json(200, &body!({ "errors": errors })),
        Err(e) => diagnostics_error(e),
    }
}

pub async fn clear(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &rc.env).await?);
    let repo = D1DiagnosticsRepository::new(rc.env.d1(bindings::DB)?);
    match DiagnosticsService::new(&repo, &WebCryptoEntropy, now())
        .clear(&p)
        .await
    {
        Ok(()) => no_content(),
        Err(e) => diagnostics_error(e),
    }
}
