//! Note routes. Every route requires an authenticated owner.

use super::auth::principal;
use super::{error, error_with, json, no_content, read_json_limited};
use crate::{bindings, d1::D1NoteRepository, entropy::WebCryptoEntropy, try_or_respond};
use directory_core::identity::Principal;
use directory_core::notes::{NewNote, NoteError, NotePatch, NoteService};
use serde_json::json as body;
use worker::{Date, Request, Response, Result, RouteContext};

/// 100k characters of body, up to 4 bytes each in UTF-8, plus JSON escaping.
const NOTE_JSON_BYTES: usize = 512 * 1024;

fn note_error(e: NoteError) -> Result<Response> {
    match e {
        NoteError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        NoteError::Exists => error(409, "exists"),
        NoteError::NotFound => error(404, "not_found"),
        // The client needs the winning version to resolve the conflict.
        NoteError::Conflict(current) => json(409, &body!({ "error": "conflict", "note": current })),
        NoteError::Backend(detail) => {
            worker::console_error!("notes backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

struct Ctx {
    repo: D1NoteRepository,
    principal: Principal,
}

impl Ctx {
    fn service(&self) -> NoteService<'_, D1NoteRepository, WebCryptoEntropy> {
        NoteService::new(
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
        repo: D1NoteRepository::new(rc.env.d1(bindings::DB)?),
        principal: p,
    }))
}

fn id_param(rc: &RouteContext<()>) -> String {
    rc.param("id").cloned().unwrap_or_default()
}

pub async fn list(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().list(&c.principal).await {
        Ok(notes) => json(200, &body!({ "notes": notes })),
        Err(e) => note_error(e),
    }
}

pub async fn create(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: NewNote = try_or_respond!(read_json_limited(&mut req, NOTE_JSON_BYTES).await?);
    match c.service().create(&c.principal, input).await {
        Ok(n) => json(201, &n),
        Err(e) => note_error(e),
    }
}

pub async fn get(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().get(&c.principal, &id_param(&rc)).await {
        Ok(n) => json(200, &n),
        Err(e) => note_error(e),
    }
}

/// Notes that wiki-link to this one.
pub async fn backlinks(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().backlinks(&c.principal, &id_param(&rc)).await {
        Ok(notes) => json(200, &body!({ "notes": notes })),
        Err(e) => note_error(e),
    }
}

pub async fn update(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let patch: NotePatch = try_or_respond!(read_json_limited(&mut req, NOTE_JSON_BYTES).await?);
    match c
        .service()
        .update(&c.principal, &id_param(&rc), patch)
        .await
    {
        Ok(n) => json(200, &n),
        Err(e) => note_error(e),
    }
}

pub async fn delete(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().delete(&c.principal, &id_param(&rc)).await {
        Ok(()) => no_content(),
        Err(e) => note_error(e),
    }
}
