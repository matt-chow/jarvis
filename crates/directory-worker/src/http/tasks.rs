//! Task routes. Every route requires an authenticated owner.

use super::auth::principal;
use super::automations;
use super::{error, error_with, json, no_content, read_json};
use crate::d1::{D1ProjectRepository, D1TaskRepository};
use crate::{bindings, entropy::WebCryptoEntropy, try_or_respond};
use directory_core::automations::Event;
use directory_core::identity::Principal;
use directory_core::projects::ProjectRepository;
use directory_core::tasks::{Filter, NewTask, Status, Task, TaskError, TaskPatch, TaskService};
use serde::Serialize;
use worker::{Date, Env, Request, Response, Result, RouteContext};

fn task_error(e: TaskError) -> Result<Response> {
    match e {
        TaskError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        TaskError::Exists => error(409, "exists"),
        TaskError::NotFound => error(404, "not_found"),
        TaskError::Backend(detail) => {
            worker::console_error!("tasks backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

struct Ctx {
    repo: D1TaskRepository,
    principal: Principal,
}

impl Ctx {
    fn service(&self) -> TaskService<'_, D1TaskRepository, WebCryptoEntropy> {
        TaskService::new(
            &self.repo,
            &WebCryptoEntropy,
            Date::now().as_millis() as i64,
        )
    }
}

async fn ctx(req: &Request, rc: &RouteContext<()>) -> Result<std::result::Result<Ctx, Response>> {
    let principal = match principal(req, &rc.env).await? {
        Ok(p) => p,
        Err(resp) => return Ok(Err(resp)),
    };
    Ok(Ok(Ctx {
        repo: D1TaskRepository::new(rc.env.d1(bindings::DB)?),
        principal,
    }))
}

fn id_param(rc: &RouteContext<()>) -> String {
    rc.param("id").cloned().unwrap_or_default()
}

#[derive(Serialize)]
struct ListBody<T: Serialize> {
    tasks: Vec<T>,
}

pub async fn list(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let url = req.url()?;
    let filter = match url.query_pairs().find(|(k, _)| k == "status") {
        None => Filter::All,
        Some((_, v)) => match Filter::parse(&v) {
            Some(f) => f,
            None => {
                return error_with(
                    400,
                    "invalid_input",
                    "status: expected all, open, or closed",
                )
            }
        },
    };
    match c.service().list(&c.principal, filter).await {
        Ok(tasks) => json(200, &ListBody { tasks }),
        Err(e) => task_error(e),
    }
}

pub async fn create(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let input: NewTask = try_or_respond!(read_json(&mut req).await?);
    match c.service().create(&c.principal, input).await {
        Ok(task) => json(201, &task),
        Err(e) => task_error(e),
    }
}

pub async fn get(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().get(&c.principal, &id_param(&rc)).await {
        Ok(task) => json(200, &task),
        Err(e) => task_error(e),
    }
}

pub async fn update(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    let patch: TaskPatch = try_or_respond!(read_json(&mut req).await?);
    let id = id_param(&rc);
    let was_open = c
        .service()
        .get(&c.principal, &id)
        .await
        .is_ok_and(|t| t.status.is_open());
    match c.service().update(&c.principal, &id, patch).await {
        Ok(task) => {
            if was_open && task.status == Status::Done {
                completed(&req, &rc.env, &c.principal, &task).await;
            }
            json(200, &task)
        }
        Err(e) => task_error(e),
    }
}

/// Fire "task completed" automations (D-024).
pub(crate) async fn completed(req: &Request, env: &Env, p: &Principal, task: &Task) {
    let project_name = match (&task.project_id, env.d1(bindings::DB)) {
        (Some(id), Ok(db)) => D1ProjectRepository::new(db)
            .get(&p.user_id, id)
            .await
            .ok()
            .flatten()
            .map(|pr| pr.name),
        _ => None,
    };
    let event = Event::TaskCompleted {
        title: task.title.clone(),
        project_id: task.project_id.clone(),
        project_name,
        date: automations::local_date(req),
    };
    automations::fire(env, p, event).await;
}

pub async fn delete(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let c = try_or_respond!(ctx(&req, &rc).await?);
    match c.service().delete(&c.principal, &id_param(&rc)).await {
        Ok(()) => no_content(),
        Err(e) => task_error(e),
    }
}
