//! `GET /api/v1/export`: the owner's complete data as one JSON download.

use super::auth::{identity_error, principal, Identity};
use super::{error, harden};
use crate::d1::{
    D1ExportSource, D1InboxRepository, D1JournalRepository, D1LinkRepository, D1NoteRepository,
    D1ProjectRepository, D1TaskRepository,
};
use crate::{bindings, r2::R2ObjectStore, try_or_respond};
use directory_core::export::export as build_export;
use directory_core::storage::{StorageService, MAX_LIST_LIMIT};
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
    };
    let storage = StorageService::new(
        R2ObjectStore::new(rc.env.bucket(bindings::FILES)?),
        &p.user_id,
    );
    let files = match storage.list(None, Some(MAX_LIST_LIMIT)).await {
        Ok(f) => f,
        Err(e) => {
            worker::console_error!("export: file list failed: {e}");
            return error(502, "storage_unavailable");
        }
    };
    let now = Date::now().as_millis() as i64;
    let doc = match build_export(&source, user, files, EXPECTED_SCHEMA_VERSION, now).await {
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
