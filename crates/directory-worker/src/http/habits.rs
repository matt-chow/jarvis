//! Habit routes. Every route requires an authenticated owner.

use super::auth::principal;
use super::automations;
use super::{error, error_with, json, no_content, read_json};
use crate::{bindings, d1::D1HabitRepository, entropy::WebCryptoEntropy, try_or_respond};
use directory_core::automations::Event;
use directory_core::habits::{
    Habit, HabitError, HabitPatch, HabitService, NewHabit, DEFAULT_WINDOW_DAYS,
};
use directory_core::identity::Principal;
use serde_json::json as body;
use worker::{Date, Env, Request, Response, Result, RouteContext};

fn habit_error(e: HabitError) -> Result<Response> {
    match e {
        HabitError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        HabitError::Exists => error(409, "exists"),
        HabitError::NotFound => error(404, "not_found"),
        HabitError::Backend(detail) => {
            worker::console_error!("habits backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

struct Ctx {
    repo: D1HabitRepository,
    principal: Principal,
}

impl Ctx {
    fn service(&self) -> HabitService<'_, D1HabitRepository, WebCryptoEntropy> {
        HabitService::new(
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
        repo: D1HabitRepository::new(rc.env.d1(bindings::DB)?),
        principal: p,
    }))
}

fn param(rc: &RouteContext<()>, name: &str) -> String {
    rc.param(name).cloned().unwrap_or_default()
}

/// `?today=YYYY-MM-DD` (the caller's local date, required) and `&days=N` (history window).
pub async fn list(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let url = req.url()?;
    let (mut today, mut days) = (String::new(), DEFAULT_WINDOW_DAYS);
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "today" => today = v.into_owned(),
            "days" => days = v.parse().unwrap_or(-1),
            _ => {}
        }
    }
    match c.service().list(&c.principal, &today, days).await {
        Ok(habits) => json(200, &body!({ "habits": habits })),
        Err(e) => habit_error(e),
    }
}

pub async fn create(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: NewHabit = try_or_respond!(read_json(&mut req).await?);
    match c.service().create(&c.principal, input).await {
        Ok(h) => json(201, &h),
        Err(e) => habit_error(e),
    }
}

pub async fn update(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let patch: HabitPatch = try_or_respond!(read_json(&mut req).await?);
    match c
        .service()
        .update(&c.principal, &param(&rc, "id"), patch)
        .await
    {
        Ok(h) => json(200, &h),
        Err(e) => habit_error(e),
    }
}

pub async fn delete(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().delete(&c.principal, &param(&rc, "id")).await {
        Ok(()) => no_content(),
        Err(e) => habit_error(e),
    }
}

async fn set_check(req: Request, rc: RouteContext<()>, done: bool) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let date = param(&rc, "date");
    match c
        .service()
        .set_check(&c.principal, &param(&rc, "id"), &date, done)
        .await
    {
        Ok((habit, changed)) => {
            // Only a new check-in fires automations, so replays don't repeat them.
            if done && changed {
                checked(&rc.env, &c.principal, &habit, &date).await;
            }
            no_content()
        }
        Err(e) => habit_error(e),
    }
}

/// Fire "habit checked in" automations (D-024).
pub(crate) async fn checked(env: &Env, p: &Principal, habit: &Habit, date: &str) {
    let event = Event::HabitChecked {
        habit_id: habit.id.clone(),
        name: habit.name.clone(),
        date: date.to_owned(),
    };
    automations::fire(env, p, event).await;
}

/// Mark a day done (idempotent).
pub async fn check(req: Request, rc: RouteContext<()>) -> Result<Response> {
    set_check(req, rc, true).await
}

/// Clear a day (idempotent).
pub async fn uncheck(req: Request, rc: RouteContext<()>) -> Result<Response> {
    set_check(req, rc, false).await
}
