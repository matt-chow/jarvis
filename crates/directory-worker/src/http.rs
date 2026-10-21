//! HTTP handlers: translate requests into core calls and core results into
//! JSON responses. No business rules here.

use crate::{bindings, d1::D1SystemRepository, r2::R2ObjectStore};
use directory_core::auth::{authorize_bearer, AuthDecision};
use directory_core::health::{self, Check};
use directory_core::storage::{StorageError, StorageService};
use directory_core::system::database_check;
use serde::Serialize;
use worker::{Headers, Request, Response, Result, RouteContext};

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

/// API responses are private and never cached by browsers or intermediaries.
fn harden(headers: &mut Headers) -> Result<()> {
    headers.set("Cache-Control", "no-store")?;
    headers.set("X-Content-Type-Options", "nosniff")?;
    Ok(())
}

fn json<T: Serialize>(status: u16, body: &T) -> Result<Response> {
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

fn storage_error(e: StorageError) -> Result<Response> {
    let (status, code) = match &e {
        StorageError::InvalidKey(_) => (400, "invalid_key"),
        StorageError::TooLarge { .. } => (413, "too_large"),
        StorageError::Backend(detail) => {
            worker::console_error!("storage backend error: {detail}");
            (502, "storage_unavailable")
        }
    };
    json(
        status,
        &ErrorBody {
            error: code,
            message: Some(e.to_string()),
        },
    )
}

/// Returns `Some(response)` when the request must be rejected.
fn require_auth(req: &Request, ctx: &RouteContext<()>) -> Result<Option<Response>> {
    let header = req.headers().get("Authorization")?;
    let secret = ctx
        .env
        .secret(bindings::API_TOKEN)
        .ok()
        .map(|s| s.to_string());
    match authorize_bearer(header.as_deref(), secret.as_deref()) {
        AuthDecision::Allowed => Ok(None),
        AuthDecision::Missing | AuthDecision::Invalid => {
            let mut resp = error(401, "unauthorized")?;
            resp.headers_mut().set("WWW-Authenticate", "Bearer")?;
            Ok(Some(resp))
        }
        AuthDecision::NotConfigured => Ok(Some(error(503, "auth_not_configured")?)),
    }
}

fn storage(ctx: &RouteContext<()>) -> Result<StorageService<R2ObjectStore>> {
    Ok(StorageService::new(R2ObjectStore::new(
        ctx.env.bucket(bindings::FILES)?,
    )))
}

fn key_param(ctx: &RouteContext<()>) -> String {
    ctx.param("key").cloned().unwrap_or_default()
}

pub async fn health(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let db_check = match ctx.env.d1(bindings::DB) {
        Ok(db) => database_check(&D1SystemRepository::new(db)).await,
        Err(_) => Check::degraded("database", "binding missing"),
    };
    // Binding presence only: avoid spending R2 operations on every health probe.
    let storage_check = match ctx.env.bucket(bindings::FILES) {
        Ok(_) => Check::ok("storage", "bound"),
        Err(_) => Check::degraded("storage", "binding missing"),
    };
    let report = health::report(env!("CARGO_PKG_VERSION"), vec![db_check, storage_check]);
    let status = if report.status == health::Status::Ok {
        200
    } else {
        503
    };
    json(status, &report)
}

#[derive(Serialize)]
struct ListBody {
    objects: Vec<directory_core::storage::ObjectInfo>,
}

pub async fn list_objects(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if let Some(resp) = require_auth(&req, &ctx)? {
        return Ok(resp);
    }
    let url = req.url()?;
    let mut prefix = None;
    let mut limit = None;
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "prefix" => prefix = Some(v.into_owned()),
            "limit" => limit = v.parse().ok(),
            _ => {}
        }
    }
    match storage(&ctx)?.list(prefix.as_deref(), limit).await {
        Ok(objects) => json(200, &ListBody { objects }),
        Err(e) => storage_error(e),
    }
}

pub async fn put_object(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if let Some(resp) = require_auth(&req, &ctx)? {
        return Ok(resp);
    }
    let content_type = req.headers().get("Content-Type")?;
    let body = req.bytes().await?;
    match storage(&ctx)?
        .put(&key_param(&ctx), body, content_type.as_deref())
        .await
    {
        Ok(info) => json(201, &info),
        Err(e) => storage_error(e),
    }
}

pub async fn get_object(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if let Some(resp) = require_auth(&req, &ctx)? {
        return Ok(resp);
    }
    match storage(&ctx)?.get(&key_param(&ctx)).await {
        Ok(Some(obj)) => {
            let mut resp = Response::from_bytes(obj.body)?;
            let headers = resp.headers_mut();
            harden(headers)?;
            headers.set(
                "Content-Type",
                obj.info
                    .content_type
                    .as_deref()
                    .unwrap_or("application/octet-stream"),
            )?;
            // Never render user content on the app origin.
            headers.set("Content-Disposition", "attachment")?;
            headers.set("Content-Security-Policy", "default-src 'none'; sandbox")?;
            Ok(resp)
        }
        Ok(None) => error(404, "not_found"),
        Err(e) => storage_error(e),
    }
}

pub async fn delete_object(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if let Some(resp) = require_auth(&req, &ctx)? {
        return Ok(resp);
    }
    match storage(&ctx)?.delete(&key_param(&ctx)).await {
        Ok(()) => {
            let mut resp = Response::empty()?.with_status(204);
            harden(resp.headers_mut())?;
            Ok(resp)
        }
        Err(e) => storage_error(e),
    }
}
