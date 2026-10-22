//! Library routes: PDF/ePub documents with reading progress.

use super::auth::principal;
use super::{error, error_with, json, no_content, read_json};
use crate::d1::{D1FileRepository, D1LibraryRepository};
use crate::{bindings, try_or_respond};
use directory_core::identity::Principal;
use directory_core::library::{LibraryError, LibraryService, ProgressPatch};
use serde_json::json as body;
use worker::{Date, Request, Response, Result, RouteContext};

fn library_error(e: LibraryError) -> Result<Response> {
    match e {
        LibraryError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        LibraryError::NotFound => error(404, "not_found"),
        LibraryError::Backend(detail) => {
            worker::console_error!("library backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

async fn ctx(
    req: &Request,
    rc: &RouteContext<()>,
) -> Result<std::result::Result<(D1LibraryRepository, Principal), Response>> {
    let p = match principal(req, &rc.env).await? {
        Ok(p) => p,
        Err(resp) => return Ok(Err(resp)),
    };
    let files = D1FileRepository::new(rc.env.d1(bindings::DB)?);
    Ok(Ok((
        D1LibraryRepository::new(rc.env.d1(bindings::DB)?, files),
        p,
    )))
}

fn now() -> i64 {
    Date::now().as_millis() as i64
}

fn file_param(rc: &RouteContext<()>) -> String {
    rc.param("file_id").cloned().unwrap_or_default()
}

pub async fn list(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let (repo, p) = try_or_respond!(ctx(&req, &rc).await?);
    match LibraryService::new(&repo, now()).list(&p).await {
        Ok(items) => json(200, &body!({ "items": items })),
        Err(e) => library_error(e),
    }
}

pub async fn update(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let (repo, p) = try_or_respond!(ctx(&req, &rc).await?);
    let patch: ProgressPatch = try_or_respond!(read_json(&mut req).await?);
    match LibraryService::new(&repo, now())
        .update(&p, &file_param(&rc), patch)
        .await
    {
        Ok(progress) => json(200, &progress),
        Err(e) => library_error(e),
    }
}

pub async fn clear(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let (repo, p) = try_or_respond!(ctx(&req, &rc).await?);
    match LibraryService::new(&repo, now())
        .clear(&p, &file_param(&rc))
        .await
    {
        Ok(()) => no_content(),
        Err(e) => library_error(e),
    }
}
