//! JARVIS routes: chat (conversation), plan (preview), execute (approve),
//! cancel, history.

use super::auth::{jarvis_principal, principal};
use super::{error, error_with, json, no_content, read_json, MAX_JSON_BYTES};
use crate::ai::{DailyBudget, OpenRouter};
use crate::d1::{
    D1BlockRepository, D1ConversationRepository, D1EventRepository, D1HabitRepository,
    D1InboxRepository, D1JarvisRepository, D1JournalRepository, D1LinkRepository, D1NoteRepository,
    D1PreferencesRepository, D1PresetRepository, D1ProjectRepository, D1SearchSource,
    D1TaskRepository,
};
use crate::{bindings, entropy::WebCryptoEntropy, try_or_respond};
use directory_core::habits::HabitRepository;
use directory_core::identity::Principal;
use directory_core::jarvis::chat::{Assistant, ChatRequest};
use directory_core::jarvis::parser::{ParseError, EXAMPLES};
use directory_core::jarvis::presets::{NewPreset, PresetError, PresetService};
use directory_core::jarvis::{
    ai::ParserThenModel, voice, Command, CommandTargets, JarvisError, JarvisService, Plan,
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
        JarvisError::Ambiguous { message, choices } => json(
            422,
            &body!({ "error": "ambiguous", "message": message, "choices": choices }),
        ),
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
    blocks: D1BlockRepository,
    preferences: D1PreferencesRepository,
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
            blocks: D1BlockRepository::new(db()?),
            preferences: D1PreferencesRepository::new(db()?),
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
    type Blocks = D1BlockRepository;
    type Preferences = D1PreferencesRepository;
    fn blocks(&self) -> &D1BlockRepository {
        &self.blocks
    }
    fn preferences(&self) -> &D1PreferencesRepository {
        &self.preferences
    }
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
    let (model, budget) = ai_parts(&rc.env, &c.principal)?;
    let provider = ParserThenModel {
        model: model.as_ref(),
        budget: &budget,
    };
    match c
        .service()
        .plan(&c.principal, &provider, input, &targets)
        .await
    {
        Ok(plan) => json(200, &plan),
        Err(e) => jarvis_error(e),
    }
}

/// Optional body of `execute`: steps to leave out, and the conversation
/// to tell about the result (so "undo that" and "it" work afterwards).
#[derive(Default, serde::Deserialize)]
struct Approval {
    #[serde(default)]
    skip: Vec<usize>,
    #[serde(default)]
    conversation_id: Option<String>,
}

pub async fn execute(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let bytes = req.bytes().await?;
    if bytes.len() > MAX_JSON_BYTES {
        return error(413, "too_large");
    }
    let approval: Approval = if bytes.iter().all(u8::is_ascii_whitespace) {
        Approval::default()
    } else {
        match serde_json::from_slice(&bytes) {
            Ok(a) => a,
            Err(e) => return error_with(400, "invalid_json", e.to_string()),
        }
    };
    let targets = Targets::from_env(&rc.env)?;
    let svc = c.service();
    match svc
        .execute_with(&c.principal, &id_param(&rc), &approval.skip, &targets)
        .await
    {
        Ok(plan) => {
            let today = super::automations::local_date(&req);
            fire_automations(&today, &rc.env, &c.principal, &plan, &targets).await;
            if let Some(id) = &approval.conversation_id {
                let convs = D1ConversationRepository::new(rc.env.d1(bindings::DB)?);
                // The plan ran; failing to note it only weakens follow-ups.
                if let Err(e) = svc.remember_plan(&c.principal, &convs, id, &plan).await {
                    worker::console_error!("jarvis conversation note failed: {e:?}");
                }
            }
            json(200, &plan)
        }
        Err(e) => jarvis_error(e),
    }
}

/// `POST /api/v1/jarvis/chat`: one conversational turn (D-048). Replies are
/// always 200 with a `status`; only a backend failure is an error.
pub async fn chat(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: ChatRequest = try_or_respond!(read_json(&mut req).await?);
    let today = input.today.clone();
    let targets = Targets::from_env(&rc.env)?;
    let convs = D1ConversationRepository::new(rc.env.d1(bindings::DB)?);
    let (model, budget) = ai_parts(&rc.env, &c.principal)?;
    let ai = Assistant {
        model: model.as_ref(),
        budget: &budget,
    };
    match c
        .service()
        .chat(&c.principal, &ai, &convs, input, &targets)
        .await
    {
        Ok(reply) => {
            if let Some(plan) = &reply.plan {
                fire_automations(&today, &rc.env, &c.principal, plan, &targets).await;
            }
            json(200, &reply)
        }
        Err(e) => jarvis_error(e),
    }
}

/// Completions and check-ins made through JARVIS fire automations like the
/// Tasks and Habits screens do (D-024). Automation actions never do.
/// `today` is the caller's local date.
async fn fire_automations(today: &str, env: &Env, p: &Principal, plan: &Plan, t: &Targets) {
    for (action, result) in plan.actions.iter().zip(&plan.results) {
        if !result.changed {
            continue;
        }
        match &action.command {
            Command::CompleteTask {
                task_id: Some(id), ..
            } => {
                if let Ok(Some(task)) = t.tasks.get(&p.user_id, id).await {
                    super::tasks::completed(today, env, p, &task).await;
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
    // The phone's date, for "today" in commands and for automations it fires.
    let today = body
        .today
        .clone()
        .unwrap_or_else(|| super::automations::local_date(&req));
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
                    fire_automations(&today, &rc.env, &c.principal, plan, &targets).await;
                }
                spoken(result)
            }
            Some(false) => spoken(svc.cancel(&c.principal, &id).await),
            None => json(200, &voice::ask_again(&id)),
        };
    }
    let input = PlanRequest {
        input: body.text,
        today,
        time: None,
    };
    let (model, budget) = ai_parts(&rc.env, &c.principal)?;
    let provider = ParserThenModel {
        model: model.as_ref(),
        budget: &budget,
    };
    spoken(svc.plan(&c.principal, &provider, input, &targets).await)
}

/// The optional AI model (D-043) and its per-owner daily budget.
fn ai_parts(env: &Env, p: &Principal) -> Result<(Option<OpenRouter>, DailyBudget)> {
    Ok((
        OpenRouter::from_env(env),
        DailyBudget::new(env, &p.user_id)?,
    ))
}
