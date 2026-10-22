//! Link routes. Every route requires an authenticated owner.

use super::auth::principal;
use super::{error, error_with, json, no_content, read_json};
use crate::{bindings, d1::D1LinkRepository, entropy::WebCryptoEntropy, try_or_respond};
use directory_core::identity::Principal;
use directory_core::links::{EntityKind, EntityRef, LinkError, LinkService, NewLink};
use serde_json::json as body;
use worker::{Date, Request, Response, Result, RouteContext};

fn link_error(e: LinkError) -> Result<Response> {
    match e {
        LinkError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        LinkError::NotFound => error(404, "not_found"),
        LinkError::Backend(detail) => {
            worker::console_error!("links backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

struct Ctx {
    repo: D1LinkRepository,
    principal: Principal,
}

impl Ctx {
    fn service(&self) -> LinkService<'_, D1LinkRepository, WebCryptoEntropy> {
        LinkService::new(
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
        repo: D1LinkRepository::new(rc.env.d1(bindings::DB)?),
        principal: p,
    }))
}

/// `GET /api/v1/links?kind=task&id=…`
pub async fn list(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let url = req.url()?;
    let param = |name: &str| {
        url.query_pairs()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
    };
    let Some(kind) = param("kind").as_deref().and_then(EntityKind::parse) else {
        return error_with(
            400,
            "invalid_input",
            "kind: expected task, note, project, or journal",
        );
    };
    let entity = EntityRef {
        kind,
        id: param("id").unwrap_or_default(),
    };
    match c.service().list(&c.principal, entity).await {
        Ok(links) => json(200, &body!({ "links": links })),
        Err(e) => link_error(e),
    }
}

pub async fn create(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: NewLink = try_or_respond!(read_json(&mut req).await?);
    match c.service().link(&c.principal, input).await {
        Ok(linked) => json(201, &linked),
        Err(e) => link_error(e),
    }
}

pub async fn delete(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let id = rc.param("id").cloned().unwrap_or_default();
    match c.service().unlink(&c.principal, &id).await {
        Ok(()) => no_content(),
        Err(e) => link_error(e),
    }
}
