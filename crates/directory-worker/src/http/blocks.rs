//! Time block routes (work sessions, D-047). Every route requires an
//! authenticated owner; a linked task must be the owner's own.

use super::auth::principal;
use super::{error, error_with, json, no_content, read_json};
use crate::{bindings, d1::D1BlockRepository, entropy::WebCryptoEntropy, try_or_respond};
use directory_core::blocks::{BlockError, BlockPatch, BlockService, NewBlock};
use directory_core::identity::Principal;
use serde_json::json as body;
use worker::{Date, Request, Response, Result, RouteContext};

fn block_error(e: BlockError) -> Result<Response> {
    match e {
        BlockError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        BlockError::Exists => error(409, "exists"),
        BlockError::NotFound => error(404, "not_found"),
        BlockError::Backend(detail) => {
            worker::console_error!("blocks backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

struct Ctx {
    repo: D1BlockRepository,
    principal: Principal,
}

impl Ctx {
    fn service(&self) -> BlockService<'_, D1BlockRepository, WebCryptoEntropy> {
        BlockService::new(
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
        repo: D1BlockRepository::new(rc.env.d1(bindings::DB)?),
        principal: p,
    }))
}

fn id_param(rc: &RouteContext<()>) -> String {
    rc.param("id").cloned().unwrap_or_default()
}

/// Every block; clients compute allocation, conflicts, and layout offline.
pub async fn list(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().list(&c.principal).await {
        Ok(blocks) => json(200, &body!({ "blocks": blocks })),
        Err(e) => block_error(e),
    }
}

pub async fn create(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: NewBlock = try_or_respond!(read_json(&mut req).await?);
    match c.service().create(&c.principal, input).await {
        Ok(b) => json(201, &b),
        Err(e) => block_error(e),
    }
}

pub async fn update(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let patch: BlockPatch = try_or_respond!(read_json(&mut req).await?);
    match c
        .service()
        .update(&c.principal, &id_param(&rc), patch)
        .await
    {
        Ok(b) => json(200, &b),
        Err(e) => block_error(e),
    }
}

pub async fn delete(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().delete(&c.principal, &id_param(&rc)).await {
        Ok(()) => no_content(),
        Err(e) => block_error(e),
    }
}
