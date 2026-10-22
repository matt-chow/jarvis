//! Preference routes. Every route requires an authenticated owner.

use super::auth::principal;
use super::{error, error_with, json, read_json};
use crate::{bindings, d1::D1PreferencesRepository, try_or_respond};
use directory_core::preferences::{PreferencesError, PreferencesPatch, PreferencesService};
use worker::{Date, Request, Response, Result, RouteContext};

fn preferences_error(e: PreferencesError) -> Result<Response> {
    match e {
        PreferencesError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        PreferencesError::Backend(detail) => {
            worker::console_error!("preferences backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

pub async fn get(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &rc.env).await?);
    let repo = D1PreferencesRepository::new(rc.env.d1(bindings::DB)?);
    let svc = PreferencesService::new(&repo, Date::now().as_millis() as i64);
    match svc.get(&p).await {
        Ok(prefs) => json(200, &prefs),
        Err(e) => preferences_error(e),
    }
}

/// Partial update: omitted fields are unchanged, `null` resets one to its default.
pub async fn update(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &rc.env).await?);
    let patch: PreferencesPatch = try_or_respond!(read_json(&mut req).await?);
    let repo = D1PreferencesRepository::new(rc.env.d1(bindings::DB)?);
    let svc = PreferencesService::new(&repo, Date::now().as_millis() as i64);
    match svc.update(&p, patch).await {
        Ok(prefs) => json(200, &prefs),
        Err(e) => preferences_error(e),
    }
}
