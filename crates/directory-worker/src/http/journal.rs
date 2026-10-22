//! Journal routes. Every route requires an authenticated owner.

use super::auth::principal;
use super::{error, error_with, json, no_content, read_json_limited};
use crate::{bindings, d1::D1JournalRepository, try_or_respond};
use directory_core::identity::Principal;
use directory_core::journal::{JournalError, JournalService, PutEntry};
use serde_json::json as body;
use worker::{Date, Request, Response, Result, RouteContext};

/// 50k characters of body, up to 4 bytes each, plus JSON escaping.
const ENTRY_JSON_BYTES: usize = 256 * 1024;

fn journal_error(e: JournalError) -> Result<Response> {
    match e {
        JournalError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        JournalError::NotFound => error(404, "not_found"),
        JournalError::Conflict(current) => {
            json(409, &body!({ "error": "conflict", "entry": current }))
        }
        JournalError::Backend(detail) => {
            worker::console_error!("journal backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

struct Ctx {
    repo: D1JournalRepository,
    principal: Principal,
}

impl Ctx {
    fn service(&self) -> JournalService<'_, D1JournalRepository> {
        JournalService::new(&self.repo, Date::now().as_millis() as i64)
    }
}

async fn ctx(req: &Request, rc: &RouteContext<()>) -> Result<std::result::Result<Ctx, Response>> {
    let p = match principal(req, &rc.env).await? {
        Ok(p) => p,
        Err(resp) => return Ok(Err(resp)),
    };
    Ok(Ok(Ctx {
        repo: D1JournalRepository::new(rc.env.d1(bindings::DB)?),
        principal: p,
    }))
}

fn date_param(rc: &RouteContext<()>) -> String {
    rc.param("date").cloned().unwrap_or_default()
}

pub async fn list(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let url = req.url()?;
    let param = |name: &str| {
        url.query_pairs()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default()
    };
    match c
        .service()
        .list(&c.principal, &param("from"), &param("to"))
        .await
    {
        Ok(entries) => json(200, &body!({ "entries": entries })),
        Err(e) => journal_error(e),
    }
}

pub async fn get(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().get(&c.principal, &date_param(&rc)).await {
        Ok(e) => json(200, &e),
        Err(e) => journal_error(e),
    }
}

pub async fn put(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: PutEntry = try_or_respond!(read_json_limited(&mut req, ENTRY_JSON_BYTES).await?);
    match c.service().put(&c.principal, &date_param(&rc), input).await {
        Ok(e) => json(200, &e),
        Err(e) => journal_error(e),
    }
}

pub async fn delete(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().delete(&c.principal, &date_param(&rc)).await {
        Ok(()) => no_content(),
        Err(e) => journal_error(e),
    }
}
