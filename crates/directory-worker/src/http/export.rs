//! `GET /api/v1/export`: the owner's complete data as one JSON download.

use super::auth::{identity_error, principal, Identity};
use super::{error, harden};
use crate::d1::{
    D1EventRepository, D1ExportSource, D1FileRepository, D1HabitRepository, D1InboxRepository,
    D1JournalRepository, D1LibraryRepository, D1LinkRepository, D1NoteRepository,
    D1ProjectRepository, D1TaskRepository,
};
use crate::{bindings, try_or_respond};
use directory_core::export::export as build_export;
use directory_core::system::EXPECTED_SCHEMA_VERSION;
use worker::{js_sys, Date, Request, Response, Result, RouteContext};

pub async fn export(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &rc.env).await?);
    let identity = Identity::from_env(&rc.env)?;
    let user = match identity.service().current_user(&p).await {
        Ok(u) => u,
        Err(e) => return identity_error(e),
    };
    let db = || rc.env.d1(bindings::DB);
    let source = D1ExportSource {
        tasks: D1TaskRepository::new(db()?),
        projects: D1ProjectRepository::new(db()?),
        notes: D1NoteRepository::new(db()?),
        journal: D1JournalRepository::new(db()?),
        inbox: D1InboxRepository::new(db()?),
        links: D1LinkRepository::new(db()?),
        files: D1FileRepository::new(db()?),
        library: D1LibraryRepository::new(db()?, D1FileRepository::new(db()?)),
        habits: D1HabitRepository::new(db()?),
        events: D1EventRepository::new(db()?),
    };
    let now = Date::now().as_millis() as i64;
    let doc = match build_export(&source, user, EXPECTED_SCHEMA_VERSION, now).await {
        Ok(d) => d,
        Err(e) => {
            worker::console_error!("export failed: {e}");
            return error(502, "backend_unavailable");
        }
    };
    let day: String = String::from(js_sys::Date::new_0().to_iso_string())
        .chars()
        .take(10)
        .collect();
    let mut resp = Response::from_json(&doc)?;
    let headers = resp.headers_mut();
    harden(headers)?;
    headers.set(
        "Content-Disposition",
        &format!("attachment; filename=\"directory-export-{day}.json\""),
    )?;
    Ok(resp)
}
