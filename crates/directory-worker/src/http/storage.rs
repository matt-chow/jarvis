//! Private file storage routes (R2). Every route requires an authenticated owner.

use super::auth::principal;
use super::{error, error_with, harden, json, no_content};
use crate::{bindings, r2::R2ObjectStore, try_or_respond};
use directory_core::identity::Principal;
use directory_core::storage::{ObjectInfo, StorageError, StorageService};
use serde::Serialize;
use worker::{Request, Response, Result, RouteContext};

fn storage_error(e: StorageError) -> Result<Response> {
    match &e {
        StorageError::InvalidKey(_) => error_with(400, "invalid_key", e.to_string()),
        StorageError::TooLarge { .. } => error_with(413, "too_large", e.to_string()),
        StorageError::Backend(detail) => {
            worker::console_error!("storage backend error: {detail}");
            error(502, "storage_unavailable")
        }
    }
}

fn storage(ctx: &RouteContext<()>, p: &Principal) -> Result<StorageService<R2ObjectStore>> {
    Ok(StorageService::new(
        R2ObjectStore::new(ctx.env.bucket(bindings::FILES)?),
        &p.user_id,
    ))
}

fn key_param(ctx: &RouteContext<()>) -> String {
    ctx.param("key").cloned().unwrap_or_default()
}

#[derive(Serialize)]
struct ListBody {
    objects: Vec<ObjectInfo>,
}

pub async fn list_objects(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &ctx.env).await?);
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
    match storage(&ctx, &p)?.list(prefix.as_deref(), limit).await {
        Ok(objects) => json(200, &ListBody { objects }),
        Err(e) => storage_error(e),
    }
}

pub async fn put_object(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &ctx.env).await?);
    let content_type = req.headers().get("Content-Type")?;
    let body = req.bytes().await?;
    match storage(&ctx, &p)?
        .put(&key_param(&ctx), body, content_type.as_deref())
        .await
    {
        Ok(info) => json(201, &info),
        Err(e) => storage_error(e),
    }
}

pub async fn get_object(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &ctx.env).await?);
    match storage(&ctx, &p)?.get(&key_param(&ctx)).await {
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
    let p = try_or_respond!(principal(&req, &ctx.env).await?);
    match storage(&ctx, &p)?.delete(&key_param(&ctx)).await {
        Ok(()) => no_content(),
        Err(e) => storage_error(e),
    }
}
