//! Calendar event routes. Every route requires an authenticated owner.

use super::auth::principal;
use super::{error, error_with, json, no_content, read_json};
use crate::{bindings, d1::D1EventRepository, entropy::WebCryptoEntropy, try_or_respond};
use directory_core::calendar::{CalendarError, CalendarService, EventPatch, NewEvent, Split};
use directory_core::identity::Principal;
use serde_json::json as body;
use worker::{Date, Request, Response, Result, RouteContext};

fn calendar_error(e: CalendarError) -> Result<Response> {
    match e {
        CalendarError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        CalendarError::Exists => error(409, "exists"),
        CalendarError::NotFound => error(404, "not_found"),
        CalendarError::Backend(detail) => {
            worker::console_error!("calendar backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

struct Ctx {
    repo: D1EventRepository,
    principal: Principal,
}

impl Ctx {
    fn service(&self) -> CalendarService<'_, D1EventRepository, WebCryptoEntropy> {
        CalendarService::new(
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
        repo: D1EventRepository::new(rc.env.d1(bindings::DB)?),
        principal: p,
    }))
}

fn id_param(rc: &RouteContext<()>) -> String {
    rc.param("id").cloned().unwrap_or_default()
}

/// Every event (not expanded); clients expand repeats for any range offline.
pub async fn list(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().list(&c.principal).await {
        Ok(events) => json(200, &body!({ "events": events })),
        Err(e) => calendar_error(e),
    }
}

/// `?from=&to=` (inclusive dates): occurrences with repeats expanded.
pub async fn agenda(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let url = req.url()?;
    let get = |name: &str| {
        url.query_pairs()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default()
    };
    match c
        .service()
        .agenda(&c.principal, &get("from"), &get("to"))
        .await
    {
        Ok(occurrences) => json(200, &body!({ "occurrences": occurrences })),
        Err(e) => calendar_error(e),
    }
}

pub async fn create(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: NewEvent = try_or_respond!(read_json(&mut req).await?);
    match c.service().create(&c.principal, input).await {
        Ok(e) => json(201, &e),
        Err(e) => calendar_error(e),
    }
}

pub async fn update(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let patch: EventPatch = try_or_respond!(read_json(&mut req).await?);
    match c
        .service()
        .update(&c.principal, &id_param(&rc), patch)
        .await
    {
        Ok(e) => json(200, &e),
        Err(e) => calendar_error(e),
    }
}

/// Cancel, move, or end part of a repeating series in one transaction.
pub async fn split(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: Split = try_or_respond!(read_json(&mut req).await?);
    match c.service().split(&c.principal, &id_param(&rc), input).await {
        Ok(r) => json(200, &r),
        Err(e) => calendar_error(e),
    }
}

pub async fn delete(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().delete(&c.principal, &id_param(&rc)).await {
        Ok(()) => no_content(),
        Err(e) => calendar_error(e),
    }
}
