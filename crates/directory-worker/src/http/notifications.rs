//! Notification routes (D-049): devices, test, claim, ack, history. Every
//! route requires an authenticated owner and touches only their rows.

use super::auth::principal;
use super::{error, error_with, json, no_content, read_json};
use crate::d1::{D1NotificationRepository, D1ReminderRepository};
use crate::{bindings, entropy::WebCryptoEntropy, push, try_or_respond};
use directory_core::identity::Principal;
use directory_core::notifications::{
    Ack, NewSubscription, NotificationService, NotifyError, MAX_HISTORY,
};
use serde::Deserialize;
use serde_json::json as body;
use worker::{Date, Request, Response, Result, RouteContext};

fn notify_error(e: NotifyError) -> Result<Response> {
    match e {
        NotifyError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        NotifyError::NotFound => error(404, "not_found"),
        NotifyError::TooMany => error_with(409, "too_many", "too many devices; remove one first"),
        NotifyError::Busy => error_with(429, "busy", "a test notification was just sent"),
        NotifyError::Backend(detail) => {
            worker::console_error!("notifications backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

struct Ctx {
    repo: D1NotificationRepository,
    reminders: D1ReminderRepository,
    principal: Principal,
    test_origin: Option<String>,
    now: i64,
}

type Service<'a> =
    NotificationService<'a, D1NotificationRepository, D1ReminderRepository, WebCryptoEntropy>;

impl Ctx {
    fn service(&self) -> Service<'_> {
        NotificationService::new(
            &self.repo,
            &self.reminders,
            &WebCryptoEntropy,
            self.now,
            self.test_origin.as_deref(),
        )
    }
}

async fn ctx(req: &Request, rc: &RouteContext<()>) -> Result<std::result::Result<Ctx, Response>> {
    let p = match principal(req, &rc.env).await? {
        Ok(p) => p,
        Err(resp) => return Ok(Err(resp)),
    };
    Ok(Ok(Ctx {
        repo: D1NotificationRepository::new(rc.env.d1(bindings::DB)?),
        reminders: D1ReminderRepository::new(rc.env.d1(bindings::DB)?),
        principal: p,
        test_origin: push::test_origin(&rc.env),
        now: Date::now().as_millis() as i64,
    }))
}

fn reply(result: std::result::Result<impl serde::Serialize, NotifyError>) -> Result<Response> {
    match result {
        Ok(v) => json(200, &v),
        Err(e) => notify_error(e),
    }
}

/// `GET /api/v1/notifications/config`: whether push is set up, and the
/// public key a browser subscribes with.
pub async fn config(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let _ = try_or_respond!(ctx(&req, &rc).await?);
    let key = push::vapid(&rc.env).map(|v| v.public_key().to_owned());
    json(200, &body!({ "enabled": key.is_some(), "public_key": key }))
}

pub async fn devices(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    reply(c.service().devices(&c.principal).await)
}

/// Register or renew this browser's subscription.
pub async fn register(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: NewSubscription = try_or_respond!(read_json(&mut req).await?);
    reply(c.service().register(&c.principal, input).await)
}

pub async fn remove(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let id = rc.param("id").cloned().unwrap_or_default();
    match c.service().remove(&c.principal, &id).await {
        Ok(()) => no_content(),
        Err(e) => notify_error(e),
    }
}

/// Queue a test notification and send it now (best effort). The reply
/// says "accepted" at most; the device confirms by claiming it.
pub async fn test(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let notice = match c.service().test(&c.principal).await {
        Ok(n) => n,
        Err(e) => return notify_error(e),
    };
    if let Err(e) = push::deliver(&rc.env, c.now).await {
        // The cron retries; the notice stays pending.
        worker::console_error!("notifications: immediate send failed: {e}");
    }
    json(202, &body!({ "notice_id": notice.id }))
}

#[derive(Deserialize)]
struct ClaimBody {
    endpoint: String,
}

/// The service worker asks what to show after a push.
pub async fn claim(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: ClaimBody = try_or_respond!(read_json(&mut req).await?);
    reply(c.service().claim(&c.principal, &input.endpoint).await)
}

#[derive(Deserialize)]
struct AckBody {
    action: Ack,
}

pub async fn ack(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: AckBody = try_or_respond!(read_json(&mut req).await?);
    let id = rc.param("id").cloned().unwrap_or_default();
    match c.service().ack(&c.principal, &id, input.action).await {
        Ok(()) => no_content(),
        Err(e) => notify_error(e),
    }
}

/// `GET /api/v1/notifications/history?limit=N` (newest first).
pub async fn history(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let limit = req
        .url()?
        .query_pairs()
        .find(|(k, _)| k == "limit")
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(50)
        .min(MAX_HISTORY);
    reply(c.service().history(&c.principal, limit).await)
}
