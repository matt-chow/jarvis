//! Inbox routes. Every route requires an authenticated owner.

use super::auth::principal;
use super::{error, error_with, json, no_content, read_json};
use crate::d1::{D1InboxRepository, D1JournalRepository, D1NoteRepository, D1TaskRepository};
use crate::{bindings, entropy::WebCryptoEntropy, try_or_respond};
use directory_core::identity::Principal;
use directory_core::inbox::{Capture, InboxError, InboxService, Targets, Triage};
use serde_json::json as body;
use worker::{Date, Request, Response, Result, RouteContext};

fn inbox_error(e: InboxError) -> Result<Response> {
    match e {
        InboxError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        InboxError::Exists => error(409, "exists"),
        InboxError::NotFound => error(404, "not_found"),
        InboxError::Backend(detail) => {
            worker::console_error!("inbox backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

struct Ctx {
    repo: D1InboxRepository,
    principal: Principal,
}

impl Ctx {
    fn service(&self) -> InboxService<'_, D1InboxRepository, WebCryptoEntropy> {
        InboxService::new(
            &self.repo,
            &WebCryptoEntropy,
            Date::now().as_millis() as i64,
        )
    }
}

async fn ctx(req: &Request, rc: &RouteContext<()>) -> Result<std::result::Result<Ctx, Response>> {
    let p = match principal(req, &rc.env).await? {
        Ok(p) => p,
        Err(resp) => return Ok(Err(resp)),
    };
    Ok(Ok(Ctx {
        repo: D1InboxRepository::new(rc.env.d1(bindings::DB)?),
        principal: p,
    }))
}

fn id_param(rc: &RouteContext<()>) -> String {
    rc.param("id").cloned().unwrap_or_default()
}

pub async fn list(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().list(&c.principal).await {
        Ok(items) => json(200, &body!({ "items": items })),
        Err(e) => inbox_error(e),
    }
}

pub async fn capture(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: Capture = try_or_respond!(read_json(&mut req).await?);
    match c.service().capture(&c.principal, input).await {
        Ok(item) => json(201, &item),
        Err(e) => inbox_error(e),
    }
}

pub async fn delete(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().delete(&c.principal, &id_param(&rc)).await {
        Ok(()) => no_content(),
        Err(e) => inbox_error(e),
    }
}

pub async fn triage(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let how: Triage = try_or_respond!(read_json(&mut req).await?);
    // Each repository gets its own handle to the same D1 binding.
    let tasks = D1TaskRepository::new(rc.env.d1(bindings::DB)?);
    let notes = D1NoteRepository::new(rc.env.d1(bindings::DB)?);
    let journal = D1JournalRepository::new(rc.env.d1(bindings::DB)?);
    let targets = Targets {
        tasks: &tasks,
        notes: &notes,
        journal: &journal,
    };
    match c
        .service()
        .triage(&c.principal, &id_param(&rc), how, targets)
        .await
    {
        Ok(result) => json(200, &result),
        Err(e) => inbox_error(e),
    }
}
