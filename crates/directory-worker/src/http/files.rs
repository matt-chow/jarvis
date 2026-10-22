//! Private file routes: metadata in D1, content streamed to/from R2.

use super::auth::principal;
use super::{error, error_with, harden, json, read_json};
use crate::{
    bindings, d1::D1FileRepository, entropy::WebCryptoEntropy, r2::R2Blobs, try_or_respond,
};
use directory_core::files::{
    blob_key, content_disposition, FileError, FilePatch, FileService, DEFAULT_QUOTA_BYTES,
};
use directory_core::identity::Principal;
use worker::{Data, Date, Env, Request, Response, ResponseBody, Result, RouteContext};

/// Optional var overriding the per-owner quota (bytes).
const QUOTA_VAR: &str = "DIRECTORY_QUOTA_BYTES";

fn file_error(e: FileError) -> Result<Response> {
    match e {
        FileError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        FileError::Exists => error_with(
            409,
            "exists",
            "a file with that name is already in the folder",
        ),
        FileError::TooLarge { max } => error_with(413, "too_large", format!("max {max} bytes")),
        FileError::QuotaExceeded { used, quota } => error_with(
            507,
            "quota_exceeded",
            format!("{used} of {quota} bytes used"),
        ),
        FileError::NotFound => error(404, "not_found"),
        FileError::Backend(detail) => {
            worker::console_error!("files backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

fn quota(env: &Env) -> u64 {
    env.var(QUOTA_VAR)
        .ok()
        .and_then(|v| v.to_string().parse().ok())
        .unwrap_or(DEFAULT_QUOTA_BYTES)
}

struct Ctx {
    repo: D1FileRepository,
    blobs: R2Blobs,
    principal: Principal,
    quota: u64,
}

impl Ctx {
    fn service(&self) -> FileService<'_, D1FileRepository, WebCryptoEntropy> {
        FileService::new(
            &self.repo,
            &WebCryptoEntropy,
            Date::now().as_millis() as i64,
            self.quota,
        )
    }
}

async fn ctx(req: &Request, rc: &RouteContext<()>) -> Result<std::result::Result<Ctx, Response>> {
    let p = match principal(req, &rc.env).await? {
        Ok(p) => p,
        Err(resp) => return Ok(Err(resp)),
    };
    Ok(Ok(Ctx {
        repo: D1FileRepository::new(rc.env.d1(bindings::DB)?),
        blobs: R2Blobs::new(rc.env.bucket(bindings::FILES)?),
        principal: p,
        quota: quota(&rc.env),
    }))
}

fn query(req: &Request, name: &str) -> Result<String> {
    Ok(req
        .url()?
        .query_pairs()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default())
}

fn id_param(rc: &RouteContext<()>) -> String {
    rc.param("id").cloned().unwrap_or_default()
}

pub async fn list(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c
        .service()
        .list(&c.principal, &query(&req, "folder")?)
        .await
    {
        Ok(listing) => json(200, &listing),
        Err(e) => file_error(e),
    }
}

pub async fn usage(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().usage(&c.principal).await {
        Ok(u) => json(200, &u),
        Err(e) => file_error(e),
    }
}

/// `POST /api/v1/files?folder=…&name=…` with the raw content as the body.
pub async fn upload(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let Some(size) = req
        .headers()
        .get("Content-Length")?
        .and_then(|v| v.parse::<u64>().ok())
    else {
        return error_with(411, "length_required", "Content-Length is required");
    };
    let content_type = req.headers().get("Content-Type")?;
    let svc = c.service();
    let entry = match svc
        .prepare_upload(
            &c.principal,
            &query(&req, "folder")?,
            &query(&req, "name")?,
            size,
            content_type.as_deref(),
        )
        .await
    {
        Ok(e) => e,
        Err(e) => return file_error(e),
    };
    let key = blob_key(&c.principal.user_id, &entry.id);
    // Stream straight from the request into R2; nothing is buffered here.
    let body = match req.inner().body() {
        Some(stream) => Data::ReadableStream(stream),
        None => Data::Empty,
    };
    if let Err(e) = c.blobs.put(&key, body, &entry.content_type).await {
        worker::console_error!("files: R2 put failed: {e}");
        return error(502, "storage_unavailable");
    }
    if let Err(e) = svc.commit_upload(&entry).await {
        // Don't leave an orphaned blob behind.
        if let Err(del) = c.blobs.delete(&key).await {
            worker::console_error!("files: cleanup of {key} failed: {del}");
        }
        return file_error(e);
    }
    json(201, &entry)
}

pub async fn content(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let entry = match c.service().get(&c.principal, &id_param(&rc)).await {
        Ok(e) => e,
        Err(e) => return file_error(e),
    };
    let object = match c
        .blobs
        .get(&blob_key(&c.principal.user_id, &entry.id))
        .await
    {
        Ok(Some(o)) => o,
        Ok(None) => {
            worker::console_error!("files: metadata without content for {}", entry.id);
            return error(404, "not_found");
        }
        Err(e) => {
            worker::console_error!("files: R2 get failed: {e}");
            return error(502, "storage_unavailable");
        }
    };
    // Hand the R2 stream to the runtime: no Worker CPU spent while streaming.
    let body = match object.body() {
        Some(b) => b.response_body()?,
        None => ResponseBody::Empty,
    };
    let mut resp = Response::from_body(body)?;
    let headers = resp.headers_mut();
    harden(headers)?;
    headers.set("Content-Type", &entry.content_type)?;
    headers.set("Content-Length", &entry.size.to_string())?;
    // Never render user content on the app origin.
    headers.set("Content-Disposition", &content_disposition(&entry.name))?;
    headers.set("Content-Security-Policy", "default-src 'none'; sandbox")?;
    Ok(resp)
}

pub async fn update(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let patch: FilePatch = try_or_respond!(read_json(&mut req).await?);
    match c
        .service()
        .update(&c.principal, &id_param(&rc), patch)
        .await
    {
        Ok(e) => json(200, &e),
        Err(e) => file_error(e),
    }
}

pub async fn delete(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().delete(&c.principal, &id_param(&rc)).await {
        Ok(entry) => {
            // Metadata is gone first, so a failed blob delete only wastes space.
            if let Err(e) = c
                .blobs
                .delete(&blob_key(&c.principal.user_id, &entry.id))
                .await
            {
                worker::console_error!("files: R2 delete failed for {}: {e}", entry.id);
            }
            super::no_content()
        }
        Err(e) => file_error(e),
    }
}
