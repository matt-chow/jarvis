//! Automation routes, and `fire`, which the task, habit, and JARVIS routes call
//! after a change succeeds (DECISIONS D-024).

use super::auth::principal;
use super::jarvis::Targets;
use super::{error, error_with, json, no_content, read_json};
use crate::d1::{D1AutomationRepository, D1JarvisRepository};
use crate::{bindings, entropy::WebCryptoEntropy, try_or_respond};
use directory_core::automations::{AutomationError, AutomationService, Event, NewRule, RulePatch};
use directory_core::identity::Principal;
use directory_core::jarvis::JarvisService;
use directory_core::validate;
use serde_json::json as body;
use worker::{Date, Env, Request, Response, Result, RouteContext};

fn automation_error(e: AutomationError) -> Result<Response> {
    match e {
        AutomationError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        AutomationError::NotFound => error(404, "not_found"),
        AutomationError::Backend(detail) => {
            worker::console_error!("automations backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

fn now() -> i64 {
    Date::now().as_millis() as i64
}

/// The caller's local date from `?today=YYYY-MM-DD`, else the UTC date.
pub(crate) fn local_date(req: &Request) -> String {
    req.url()
        .ok()
        .and_then(|u| {
            u.query_pairs()
                .find(|(k, _)| k == "today")
                .map(|(_, v)| v.into_owned())
        })
        .and_then(|d| validate::date(&d).ok())
        .unwrap_or_else(|| validate::civil_from_days(now().div_euclid(86_400_000)))
}

/// Run the owner's rules for `event`. Never fails the caller's request.
pub(crate) async fn fire(env: &Env, p: &Principal, event: Event) {
    let run = async {
        let rules = D1AutomationRepository::new(env.d1(bindings::DB)?);
        let plans = D1JarvisRepository::new(env.d1(bindings::DB)?);
        let targets = Targets::from_env(env)?;
        let jarvis = JarvisService::new(&plans, &WebCryptoEntropy, now());
        AutomationService::new(&rules, &WebCryptoEntropy, now())
            .fire(p, &event, &jarvis, &targets)
            .await;
        Ok::<(), worker::Error>(())
    };
    if let Err(e) = run.await {
        worker::console_error!("automations failed to run: {e}");
    }
}

pub async fn list(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &rc.env).await?);
    let repo = D1AutomationRepository::new(rc.env.d1(bindings::DB)?);
    match AutomationService::new(&repo, &WebCryptoEntropy, now())
        .list(&p)
        .await
    {
        Ok(rules) => json(200, &body!({ "automations": rules })),
        Err(e) => automation_error(e),
    }
}

pub async fn create(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &rc.env).await?);
    let input: NewRule = try_or_respond!(read_json(&mut req).await?);
    let repo = D1AutomationRepository::new(rc.env.d1(bindings::DB)?);
    match AutomationService::new(&repo, &WebCryptoEntropy, now())
        .create(&p, input)
        .await
    {
        Ok(r) => json(201, &r),
        Err(e) => automation_error(e),
    }
}

pub async fn update(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &rc.env).await?);
    let patch: RulePatch = try_or_respond!(read_json(&mut req).await?);
    let id = rc.param("id").cloned().unwrap_or_default();
    let repo = D1AutomationRepository::new(rc.env.d1(bindings::DB)?);
    match AutomationService::new(&repo, &WebCryptoEntropy, now())
        .update(&p, &id, patch)
        .await
    {
        Ok(r) => json(200, &r),
        Err(e) => automation_error(e),
    }
}

pub async fn delete(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &rc.env).await?);
    let id = rc.param("id").cloned().unwrap_or_default();
    let repo = D1AutomationRepository::new(rc.env.d1(bindings::DB)?);
    match AutomationService::new(&repo, &WebCryptoEntropy, now())
        .delete(&p, &id)
        .await
    {
        Ok(()) => no_content(),
        Err(e) => automation_error(e),
    }
}
