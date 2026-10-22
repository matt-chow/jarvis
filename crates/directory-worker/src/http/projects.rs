//! Project routes. Every route requires an authenticated owner.

use super::auth::principal;
use super::{error, error_with, json, no_content, read_json};
use crate::{bindings, d1::D1ProjectRepository, entropy::WebCryptoEntropy, try_or_respond};
use directory_core::identity::Principal;
use directory_core::projects::{NewProject, ProjectError, ProjectPatch, ProjectService};
use serde_json::json as body;
use worker::{Date, Request, Response, Result, RouteContext};

fn project_error(e: ProjectError) -> Result<Response> {
    match e {
        ProjectError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        ProjectError::Exists => error(409, "exists"),
        ProjectError::NotFound => error(404, "not_found"),
        ProjectError::Backend(detail) => {
            worker::console_error!("projects backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

struct Ctx {
    repo: D1ProjectRepository,
    principal: Principal,
}

impl Ctx {
    fn service(&self) -> ProjectService<'_, D1ProjectRepository, WebCryptoEntropy> {
        ProjectService::new(
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
        repo: D1ProjectRepository::new(rc.env.d1(bindings::DB)?),
        principal: p,
    }))
}

fn id_param(rc: &RouteContext<()>) -> String {
    rc.param("id").cloned().unwrap_or_default()
}

pub async fn list(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let include_archived = req
        .url()?
        .query_pairs()
        .any(|(k, v)| k == "include" && v == "archived");
    match c.service().list(&c.principal, include_archived).await {
        Ok(projects) => json(200, &body!({ "projects": projects })),
        Err(e) => project_error(e),
    }
}

pub async fn create(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: NewProject = try_or_respond!(read_json(&mut req).await?);
    match c.service().create(&c.principal, input).await {
        Ok(p) => json(201, &p),
        Err(e) => project_error(e),
    }
}

pub async fn get(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().get(&c.principal, &id_param(&rc)).await {
        Ok(p) => json(200, &p),
        Err(e) => project_error(e),
    }
}

pub async fn update(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let patch: ProjectPatch = try_or_respond!(read_json(&mut req).await?);
    match c
        .service()
        .update(&c.principal, &id_param(&rc), patch)
        .await
    {
        Ok(p) => json(200, &p),
        Err(e) => project_error(e),
    }
}

pub async fn delete(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().delete(&c.principal, &id_param(&rc)).await {
        Ok(()) => no_content(),
        Err(e) => project_error(e),
    }
}
