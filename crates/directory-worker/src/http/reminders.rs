//! Reminder routes (D-049). Every route requires an authenticated owner.

use super::auth::principal;
use super::{error, error_with, json, no_content, read_json};
use crate::{
    bindings, d1::D1ReminderRepository, entropy::WebCryptoEntropy, try_or_respond, zone::IntlZones,
};
use directory_core::identity::Principal;
use directory_core::reminders::{
    ListPatch, NewList, NewReminder, OccurrenceRef, ReminderError, ReminderPatch, ReminderService,
    Snooze,
};
use serde_json::json as body;
use worker::{Date, Env, Request, Response, Result, RouteContext};

pub(crate) fn reminder_error(e: ReminderError) -> Result<Response> {
    match e {
        ReminderError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        ReminderError::Exists => error(409, "exists"),
        ReminderError::NotFound => error(404, "not_found"),
        // The current record comes back so the client can merge or retry.
        ReminderError::Conflict(reminder, fields) => json(
            409,
            &body!({ "error": "conflict", "fields": fields, "reminder": reminder }),
        ),
        ReminderError::Stale(reminder, why) => json(
            409,
            &body!({ "error": "stale", "message": why, "reminder": reminder }),
        ),
        ReminderError::Backend(detail) => {
            // Backend detail never carries reminder text.
            worker::console_error!("reminders backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

struct Ctx {
    repo: D1ReminderRepository,
    principal: Principal,
}

type Service<'a> = ReminderService<'a, D1ReminderRepository, WebCryptoEntropy, IntlZones>;

impl Ctx {
    fn service(&self) -> Service<'_> {
        ReminderService::new(
            &self.repo,
            &WebCryptoEntropy,
            &IntlZones,
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
        repo: D1ReminderRepository::new(rc.env.d1(bindings::DB)?),
        principal: p,
    }))
}

fn id_param(rc: &RouteContext<()>) -> String {
    rc.param("id").cloned().unwrap_or_default()
}

fn reply(result: std::result::Result<impl serde::Serialize, ReminderError>) -> Result<Response> {
    match result {
        Ok(v) => json(200, &v),
        Err(e) => reminder_error(e),
    }
}

/// `GET /api/v1/reminders`: every reminder, deleted ones included (the client filters).
pub async fn list(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    reply(c.service().list(&c.principal).await)
}

pub async fn create(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: NewReminder = try_or_respond!(read_json(&mut req).await?);
    match c.service().create(&c.principal, input).await {
        Ok(r) => json(201, &r),
        Err(e) => reminder_error(e),
    }
}

pub async fn get(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    reply(c.service().get(&c.principal, &id_param(&rc)).await)
}

pub async fn update(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let patch: ReminderPatch = try_or_respond!(read_json(&mut req).await?);
    reply(
        c.service()
            .update(&c.principal, &id_param(&rc), patch)
            .await,
    )
}

/// Soft delete; a second delete purges.
pub async fn delete(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().delete(&c.principal, &id_param(&rc)).await {
        Ok(()) => no_content(),
        Err(e) => reminder_error(e),
    }
}

pub async fn restore(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    reply(c.service().restore(&c.principal, &id_param(&rc)).await)
}

/// `POST /reminders/:id/complete {occurrence?}`: idempotent.
pub async fn complete(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: OccurrenceRef = try_or_respond!(read_json(&mut req).await?);
    reply(
        c.service()
            .complete(&c.principal, &id_param(&rc), input.occurrence.as_deref())
            .await,
    )
}

pub async fn uncomplete(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: OccurrenceRef = try_or_respond!(read_json(&mut req).await?);
    reply(
        c.service()
            .uncomplete(&c.principal, &id_param(&rc), input.occurrence.as_deref())
            .await,
    )
}

/// `POST /reminders/:id/snooze {minutes | until | tomorrow, occurrence?}`.
pub async fn snooze(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: Snooze = try_or_respond!(read_json(&mut req).await?);
    reply(
        c.service()
            .snooze(&c.principal, &id_param(&rc), input)
            .await,
    )
}

pub async fn history(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    reply(c.service().history_of(&c.principal, &id_param(&rc)).await)
}

/// `GET /api/v1/reminders/agenda?from=YYYY-MM-DD&to=YYYY-MM-DD`.
pub async fn agenda(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let url = req.url()?;
    let get = |name: &str| {
        url.query_pairs()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default()
    };
    reply(
        c.service()
            .agenda(&c.principal, &get("from"), &get("to"))
            .await,
    )
}

pub async fn lists(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    reply(c.service().lists(&c.principal).await)
}

pub async fn create_list(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: NewList = try_or_respond!(read_json(&mut req).await?);
    match c.service().create_list(&c.principal, input).await {
        Ok(l) => json(201, &l),
        Err(e) => reminder_error(e),
    }
}

pub async fn update_list(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let patch: ListPatch = try_or_respond!(read_json(&mut req).await?);
    reply(
        c.service()
            .update_list(&c.principal, &id_param(&rc), patch)
            .await,
    )
}

pub async fn delete_list(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().delete_list(&c.principal, &id_param(&rc)).await {
        Ok(()) => no_content(),
        Err(e) => reminder_error(e),
    }
}

/// A task was completed: complete the reminders linked to it that opted in
/// (one way; never the other direction, so no loops).
pub(crate) async fn task_completed(env: &Env, p: &Principal, task_id: &str) {
    let Ok(db) = env.d1(bindings::DB) else {
        return;
    };
    let repo = D1ReminderRepository::new(db);
    let service = ReminderService::new(
        &repo,
        &WebCryptoEntropy,
        &IntlZones,
        Date::now().as_millis() as i64,
    );
    // Log the kind only: other variants carry reminder text.
    match service.task_completed(p, task_id).await {
        Ok(_) => {}
        Err(ReminderError::Backend(detail)) => {
            worker::console_error!("reminders: task-linked completion failed: {detail}")
        }
        Err(_) => worker::console_error!("reminders: task-linked completion was refused"),
    }
}
