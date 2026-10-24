//! JARVIS routes: plan (preview), execute (approve), cancel, history.

use super::auth::{jarvis_principal, principal};
use super::{error, error_with, json, no_content, read_json};
use crate::d1::{
    D1EventRepository, D1HabitRepository, D1InboxRepository, D1JarvisRepository,
    D1JournalRepository, D1LinkRepository, D1NoteRepository, D1PresetRepository,
    D1ProjectRepository, D1SearchSource, D1TaskRepository,
};
use crate::{bindings, entropy::WebCryptoEntropy, try_or_respond};
use directory_core::habits::HabitRepository;
use directory_core::identity::Principal;
use directory_core::jarvis::parser::{ParseError, EXAMPLES};
use directory_core::jarvis::presets::{NewPreset, PresetError, PresetService};
use directory_core::jarvis::{
    voice, Command, CommandTargets, DeterministicParser, JarvisError, JarvisService, Plan,
    PlanRequest,
};
use directory_core::tasks::TaskRepository;
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
pub(crate) struct Targets {
    tasks: D1TaskRepository,
    notes: D1NoteRepository,
    journal: D1JournalRepository,
    inbox: D1InboxRepository,
    search: D1SearchSource,
    projects: D1ProjectRepository,
    events: D1EventRepository,
    habits: D1HabitRepository,
    links: D1LinkRepository,
}

impl Targets {
    pub(crate) fn from_env(env: &Env) -> Result<Self> {
        let db = || env.d1(bindings::DB);
        Ok(Self {
            tasks: D1TaskRepository::new(db()?),
            notes: D1NoteRepository::new(db()?),
            journal: D1JournalRepository::new(db()?),
            inbox: D1InboxRepository::new(db()?),
            search: D1SearchSource::new(db()?),
            projects: D1ProjectRepository::new(db()?),
            events: D1EventRepository::new(db()?),
            habits: D1HabitRepository::new(db()?),
            links: D1LinkRepository::new(db()?),
        })
    }
}

impl CommandTargets for Targets {
    type Tasks = D1TaskRepository;
    type Notes = D1NoteRepository;
    type Journal = D1JournalRepository;
    type Inbox = D1InboxRepository;
    type Search = D1SearchSource;
    type Projects = D1ProjectRepository;
    type Events = D1EventRepository;
    type Habits = D1HabitRepository;
    type Links = D1LinkRepository;
    fn links(&self) -> &D1LinkRepository {
        &self.links
    }
    fn events(&self) -> &D1EventRepository {
        &self.events
    }
    fn habits(&self) -> &D1HabitRepository {
        &self.habits
    }
    fn tasks(&self) -> &D1TaskRepository {
        &self.tasks
    }
    fn notes(&self) -> &D1NoteRepository {
        &self.notes
    }
    fn journal(&self) -> &D1JournalRepository {
        &self.journal
    }
    fn inbox(&self) -> &D1InboxRepository {
        &self.inbox
    }
    fn search(&self) -> &D1SearchSource {
        &self.search
    }
    fn projects(&self) -> &D1ProjectRepository {
        &self.projects
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
        .plan(&c.principal, &DeterministicParser, input, &targets)
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
        .execute(&c.principal, &id_param(&rc), &targets)
        .await
    {
        Ok(plan) => {
            fire_automations(&req, &rc.env, &c.principal, &plan, &targets).await;
            json(200, &plan)
        }
        Err(e) => jarvis_error(e),
    }
}

/// Completions and check-ins made through JARVIS fire automations like the
/// Tasks and Habits screens do (D-024). Automation actions never do.
async fn fire_automations(req: &Request, env: &Env, p: &Principal, plan: &Plan, t: &Targets) {
    for (action, result) in plan.actions.iter().zip(&plan.results) {
        if !result.changed {
            continue;
        }
        match &action.command {
            Command::CompleteTask {
                task_id: Some(id), ..
            } => {
                if let Ok(Some(task)) = t.tasks.get(&p.user_id, id).await {
                    super::tasks::completed(req, env, p, &task).await;
                }
            }
            Command::CheckHabit {
                habit_id: Some(id),
                date,
                ..
            } => {
                if let Ok(Some(habit)) = t.habits.get(&p.user_id, id).await {
                    super::habits::checked(env, p, &habit, date).await;
                }
            }
            _ => {}
        }
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

#[derive(serde::Deserialize)]
struct AskRequest {
    text: String,
    /// The phone's local date; the server's UTC date if missing.
    #[serde(default)]
    today: Option<String>,
    /// Answering "Say yes to confirm" for this pending plan.
    #[serde(default)]
    confirm: Option<String>,
}

/// `POST /api/v1/jarvis/ask`: one spoken turn for Apple Shortcuts (D-042,
/// docs/JARVIS_SHORTCUTS.md). Same parser, approval, and once-only execution
/// as the app; the reply carries a `speech` line. Accepts JARVIS-only tokens.
pub async fn ask(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(jarvis_principal(&req, &rc.env).await?);
    let body: AskRequest = try_or_respond!(read_json(&mut req).await?);
    let c = Ctx {
        repo: D1JarvisRepository::new(rc.env.d1(bindings::DB)?),
        principal: p,
    };
    let targets = Targets::from_env(&rc.env)?;
    let svc = c.service();
    let spoken = |r: std::result::Result<Plan, JarvisError>| match r {
        Ok(plan) => json(200, &voice::from_plan(&plan)),
        Err(e) => match voice::from_error(&e) {
            Some(answer) => json(200, &answer),
            None => jarvis_error(e),
        },
    };
    if let Some(id) = body.confirm {
        return match voice::yes_or_no(&body.text) {
            Some(true) => {
                let result = svc.execute(&c.principal, &id, &targets).await;
                if let Ok(plan) = &result {
                    fire_automations(&req, &rc.env, &c.principal, plan, &targets).await;
                }
                spoken(result)
            }
            Some(false) => spoken(svc.cancel(&c.principal, &id).await),
            None => json(200, &voice::ask_again(&id)),
        };
    }
    let today = body
        .today
        .unwrap_or_else(|| super::automations::local_date(&req));
    let input = PlanRequest {
        input: body.text,
        today,
    };
    spoken(
        svc.plan(&c.principal, &DeterministicParser, input, &targets)
            .await,
    )
}
