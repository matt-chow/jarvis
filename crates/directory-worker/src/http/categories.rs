//! Category routes (D-049). Every route requires an authenticated owner.

use super::auth::principal;
use super::{error, error_with, json, no_content, read_json};
use crate::{bindings, d1::D1CategoryRepository, entropy::WebCryptoEntropy, try_or_respond};
use directory_core::categories::{CategoryError, CategoryPatch, CategoryService, NewCategory};
use directory_core::identity::Principal;
use serde::Deserialize;
use serde_json::json as body;
use worker::{Date, Request, Response, Result, RouteContext};

pub(crate) fn category_error(e: CategoryError) -> Result<Response> {
    match e {
        CategoryError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        CategoryError::Exists => error(409, "exists"),
        CategoryError::NotFound => error(404, "not_found"),
        CategoryError::Backend(detail) => {
            worker::console_error!("categories backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

struct Ctx {
    repo: D1CategoryRepository,
    principal: Principal,
}

impl Ctx {
    fn service(&self) -> CategoryService<'_, D1CategoryRepository, WebCryptoEntropy> {
        CategoryService::new(
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
        repo: D1CategoryRepository::new(rc.env.d1(bindings::DB)?),
        principal: p,
    }))
}

fn id_param(rc: &RouteContext<()>) -> String {
    rc.param("id").cloned().unwrap_or_default()
}

/// `GET /api/v1/categories`: the categories and every task assignment.
pub async fn list(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().list(&c.principal).await {
        Ok(l) => json(200, &l),
        Err(e) => category_error(e),
    }
}

pub async fn create(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: NewCategory = try_or_respond!(read_json(&mut req).await?);
    match c.service().create(&c.principal, input).await {
        Ok(cat) => json(201, &cat),
        Err(e) => category_error(e),
    }
}

pub async fn update(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let patch: CategoryPatch = try_or_respond!(read_json(&mut req).await?);
    match c
        .service()
        .update(&c.principal, &id_param(&rc), patch)
        .await
    {
        Ok(cat) => json(200, &cat),
        Err(e) => category_error(e),
    }
}

pub async fn delete(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().delete(&c.principal, &id_param(&rc)).await {
        Ok(()) => no_content(),
        Err(e) => category_error(e),
    }
}

#[derive(Deserialize)]
struct SetCategories {
    category_ids: Vec<String>,
}

/// `PUT /api/v1/tasks/:id/categories {category_ids}`: replace the task's set.
pub async fn set_task(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: SetCategories = try_or_respond!(read_json(&mut req).await?);
    let task_id = id_param(&rc);
    match c
        .service()
        .set_task(&c.principal, &task_id, &input.category_ids)
        .await
    {
        Ok(ids) => json(200, &body!({ "task_id": task_id, "category_ids": ids })),
        Err(e) => category_error(e),
    }
}
