//! Budget routes (D-050). Every route requires an authenticated owner.

use super::auth::principal;
use super::{error, error_with, json, no_content, read_json};
use crate::{bindings, d1::D1BudgetRepository, entropy::WebCryptoEntropy, try_or_respond};
use directory_core::budget::{
    AccountInput, BudgetError, BudgetService, CategoryInput, Entity, TemplateInput,
    TransactionInput,
};
use directory_core::identity::Principal;
use serde::Deserialize;
use serde_json::json as body;
use worker::{Date, Request, Response, Result, RouteContext};

fn budget_error(e: BudgetError) -> Result<Response> {
    match e {
        BudgetError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        BudgetError::Exists => error(409, "exists"),
        BudgetError::NotFound => error(404, "not_found"),
        BudgetError::Backend(detail) => {
            worker::console_error!("budget backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

struct Ctx {
    repo: D1BudgetRepository,
    principal: Principal,
}

impl Ctx {
    fn service(&self) -> BudgetService<'_, D1BudgetRepository, WebCryptoEntropy> {
        BudgetService::new(
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
        repo: D1BudgetRepository::new(rc.env.d1(bindings::DB)?),
        principal: p,
    }))
}

fn id_param(rc: &RouteContext<()>) -> String {
    rc.param("id").cloned().unwrap_or_default()
}

/// The `:kind` path segment: accounts, categories, transactions, or templates.
fn kind_param(rc: &RouteContext<()>) -> Option<Entity> {
    match rc.param("kind").map(String::as_str) {
        Some("accounts") => Some(Entity::Account),
        Some("categories") => Some(Entity::Category),
        Some("transactions") => Some(Entity::Transaction),
        Some("templates") => Some(Entity::Template),
        _ => None,
    }
}

/// `GET /api/v1/budget`: currency, accounts, categories, transactions, templates.
pub async fn load(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().load(&c.principal).await {
        Ok(b) => json(200, &b),
        Err(e) => budget_error(e),
    }
}

#[derive(Deserialize)]
struct Settings {
    currency: String,
}

/// `PUT /api/v1/budget/settings {currency}`.
pub async fn settings(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: Settings = try_or_respond!(read_json(&mut req).await?);
    match c
        .service()
        .set_currency(&c.principal, &input.currency)
        .await
    {
        Ok(currency) => json(200, &body!({ "currency": currency })),
        Err(e) => budget_error(e),
    }
}

/// `POST /api/v1/budget/:kind`: create (an optional client `id` makes retries idempotent).
pub async fn create(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let Some(kind) = kind_param(&rc) else {
        return error(404, "not_found");
    };
    let s = c.service();
    let p = &c.principal;
    match kind {
        Entity::Account => {
            let i: AccountInput = try_or_respond!(read_json(&mut req).await?);
            s.create_account(p, i).await.map(|v| json(201, &v))
        }
        Entity::Category => {
            let i: CategoryInput = try_or_respond!(read_json(&mut req).await?);
            s.create_category(p, i).await.map(|v| json(201, &v))
        }
        Entity::Transaction => {
            let i: TransactionInput = try_or_respond!(read_json(&mut req).await?);
            s.create_transaction(p, i).await.map(|v| json(201, &v))
        }
        Entity::Template => {
            let i: TemplateInput = try_or_respond!(read_json(&mut req).await?);
            s.create_template(p, i).await.map(|v| json(201, &v))
        }
    }
    .unwrap_or_else(budget_error)
}

/// `PUT /api/v1/budget/:kind/:id`: replace every editable field.
pub async fn replace(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let Some(kind) = kind_param(&rc) else {
        return error(404, "not_found");
    };
    let s = c.service();
    let p = &c.principal;
    let id = id_param(&rc);
    match kind {
        Entity::Account => {
            let i: AccountInput = try_or_respond!(read_json(&mut req).await?);
            s.replace_account(p, &id, i).await.map(|v| json(200, &v))
        }
        Entity::Category => {
            let i: CategoryInput = try_or_respond!(read_json(&mut req).await?);
            s.replace_category(p, &id, i).await.map(|v| json(200, &v))
        }
        Entity::Transaction => {
            let i: TransactionInput = try_or_respond!(read_json(&mut req).await?);
            s.replace_transaction(p, &id, i)
                .await
                .map(|v| json(200, &v))
        }
        Entity::Template => {
            let i: TemplateInput = try_or_respond!(read_json(&mut req).await?);
            s.replace_template(p, &id, i).await.map(|v| json(200, &v))
        }
    }
    .unwrap_or_else(budget_error)
}

/// `DELETE /api/v1/budget/:kind/:id`.
pub async fn delete(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let Some(kind) = kind_param(&rc) else {
        return error(404, "not_found");
    };
    match c.service().delete(&c.principal, kind, &id_param(&rc)).await {
        Ok(()) => no_content(),
        Err(e) => budget_error(e),
    }
}

#[derive(Deserialize)]
struct Post {
    id: String,
}

/// `POST /api/v1/budget/templates/:id/post {id}`: record the next occurrence
/// as a transaction with the client's `id`, and move the template on.
pub async fn post(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: Post = try_or_respond!(read_json(&mut req).await?);
    match c
        .service()
        .post(&c.principal, &id_param(&rc), &input.id)
        .await
    {
        Ok((transaction, template)) => json(
            201,
            &body!({ "transaction": transaction, "template": template }),
        ),
        Err(e) => budget_error(e),
    }
}

#[derive(Deserialize)]
struct Import {
    transactions: Vec<TransactionInput>,
}

/// `POST /api/v1/budget/import {transactions}`: all valid or none stored;
/// ids already present are skipped, so a retry is safe.
pub async fn import(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: Import = try_or_respond!(read_json(&mut req).await?);
    match c.service().import(&c.principal, input.transactions).await {
        Ok(r) => json(200, &r),
        Err(e) => budget_error(e),
    }
}
