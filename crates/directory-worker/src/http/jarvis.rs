//! JARVIS routes: plan (preview), execute (approve), cancel, history.

use super::auth::principal;
use super::{error, error_with, json, no_content, read_json};
use crate::d1::{
    D1InboxRepository, D1JarvisRepository, D1JournalRepository, D1NoteRepository,
    D1PresetRepository, D1ProjectRepository, D1SearchSource, D1TaskRepository,
};
use crate::{bindings, entropy::WebCryptoEntropy, try_or_respond};
use directory_core::identity::Principal;
use directory_core::jarvis::parser::{ParseError, EXAMPLES};
use directory_core::jarvis::presets::{NewPreset, PresetError, PresetService};
use directory_core::jarvis::{
    CommandTargets, DeterministicParser, JarvisError, JarvisService, PlanRequest,
};
use serde_json::json as body;
use worker::{Date, Env, Request, Response, Result, RouteContext};

fn jarvis_error(e: JarvisError) -> Result<Response> {
    match e {
        JarvisError::Parse(p) => {
            let message = match p {
                ParseError::Empty => "Type a command.".to_owned(),
                ParseError::TooManySteps => "Too many steps in one request (max 5).".to_owned(),
                ParseError::Unknown(step) => format!("I don't know how to “{step}” yet."),
                ParseError::MissingText(what) => format!("Missing {what}."),
                ParseError::BadDate(d) => format!("I don't understand the date “{d}”."),
            };
            json(
                400,
                &body!({ "error": "not_understood", "message": message, "examples": EXAMPLES }),
            )
        }
        JarvisError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        JarvisError::Unresolved(message) => error_with(422, "unresolved", message),
        JarvisError::NotFound => error(404, "not_found"),
        JarvisError::NotPending(status) => error_with(409, "not_pending", status.as_str()),
        JarvisError::Backend(detail) => {
            worker::console_error!("jarvis backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

/// Repositories commands execute through (one D1 handle each).
struct Targets {
    tasks: D1TaskRepository,
    notes: D1NoteRepository,
    journal: D1JournalRepository,
    inbox: D1InboxRepository,
    search: D1SearchSource,
    projects: D1ProjectRepository,
}

type Borrowed<'a> = CommandTargets<
    'a,
    D1TaskRepository,
    D1NoteRepository,
    D1JournalRepository,
    D1InboxRepository,
    D1SearchSource,
    D1ProjectRepository,
>;

impl Targets {
    fn from_env(env: &Env) -> Result<Self> {
        let db = || env.d1(bindings::DB);
        Ok(Self {
            tasks: D1TaskRepository::new(db()?),
            notes: D1NoteRepository::new(db()?),
            journal: D1JournalRepository::new(db()?),
            inbox: D1InboxRepository::new(db()?),
            search: D1SearchSource::new(db()?),
            projects: D1ProjectRepository::new(db()?),
        })
    }

    fn borrow(&self) -> Borrowed<'_> {
        CommandTargets {
            tasks: &self.tasks,
            notes: &self.notes,
            journal: &self.journal,
            inbox: &self.inbox,
            search: &self.search,
            projects: &self.projects,
        }
    }
}

struct Ctx {
    repo: D1JarvisRepository,
    principal: Principal,
}

impl Ctx {
    fn service(&self) -> JarvisService<'_, D1JarvisRepository, WebCryptoEntropy> {
        JarvisService::new(
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
        repo: D1JarvisRepository::new(rc.env.d1(bindings::DB)?),
        principal: p,
    }))
}

fn id_param(rc: &RouteContext<()>) -> String {
    rc.param("id").cloned().unwrap_or_default()
}

pub async fn plan(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: PlanRequest = try_or_respond!(read_json(&mut req).await?);
    let targets = Targets::from_env(&rc.env)?;
    match c
        .service()
        .plan(&c.principal, &DeterministicParser, input, targets.borrow())
        .await
    {
        Ok(plan) => json(200, &plan),
        Err(e) => jarvis_error(e),
    }
}

pub async fn execute(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let targets = Targets::from_env(&rc.env)?;
    match c
        .service()
        .execute(&c.principal, &id_param(&rc), targets.borrow())
        .await
    {
        Ok(plan) => json(200, &plan),
        Err(e) => jarvis_error(e),
    }
}

pub async fn cancel(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().cancel(&c.principal, &id_param(&rc)).await {
        Ok(plan) => json(200, &plan),
        Err(e) => jarvis_error(e),
    }
}

pub async fn history(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().history(&c.principal).await {
        Ok(plans) => json(200, &body!({ "plans": plans })),
        Err(e) => jarvis_error(e),
    }
}

fn preset_error(e: PresetError) -> Result<Response> {
    match e {
        PresetError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        PresetError::Parse(p) => jarvis_error(JarvisError::Parse(p)),
        PresetError::Limit => error_with(409, "limit_reached", "at most 50 presets"),
        PresetError::NotFound => error(404, "not_found"),
        PresetError::Backend(detail) => {
            worker::console_error!("presets backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

async fn presets(
    req: &Request,
    rc: &RouteContext<()>,
) -> Result<std::result::Result<(D1PresetRepository, Principal), Response>> {
    let p = match principal(req, &rc.env).await? {
        Ok(p) => p,
        Err(resp) => return Ok(Err(resp)),
    };
    Ok(Ok((D1PresetRepository::new(rc.env.d1(bindings::DB)?), p)))
}

fn now() -> i64 {
    Date::now().as_millis() as i64
}

pub async fn list_presets(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let (repo, p) = try_or_respond!(presets(&req, &rc).await?);
    match PresetService::new(&repo, &WebCryptoEntropy, now())
        .list(&p)
        .await
    {
        Ok(list) => json(200, &body!({ "presets": list })),
        Err(e) => preset_error(e),
    }
}

pub async fn create_preset(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let (repo, p) = try_or_respond!(presets(&req, &rc).await?);
    let input: NewPreset = try_or_respond!(read_json(&mut req).await?);
    match PresetService::new(&repo, &WebCryptoEntropy, now())
        .create(&p, input)
        .await
    {
        Ok(preset) => json(201, &preset),
        Err(e) => preset_error(e),
    }
}

pub async fn delete_preset(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let (repo, p) = try_or_respond!(presets(&req, &rc).await?);
    match PresetService::new(&repo, &WebCryptoEntropy, now())
        .delete(&p, &id_param(&rc))
        .await
    {
        Ok(()) => no_content(),
        Err(e) => preset_error(e),
    }
}
