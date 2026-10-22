//! Music routes: the audio library and playlists. Every route requires an
//! authenticated owner; audio itself streams from `/files/{id}/content`.

use super::auth::principal;
use super::{error, error_with, json, no_content, read_json};
use crate::d1::{D1FileRepository, D1MusicRepository};
use crate::{bindings, entropy::WebCryptoEntropy, try_or_respond};
use directory_core::music::{MusicError, MusicService, NewPlaylist, PlaylistPatch};
use worker::{Date, Env, Request, Response, Result, RouteContext};

fn music_error(e: MusicError) -> Result<Response> {
    match e {
        MusicError::Invalid(field, why) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        MusicError::NotFound => error(404, "not_found"),
        MusicError::Backend(detail) => {
            worker::console_error!("music backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

fn repo(env: &Env) -> Result<D1MusicRepository> {
    Ok(D1MusicRepository::new(
        env.d1(bindings::DB)?,
        D1FileRepository::new(env.d1(bindings::DB)?),
    ))
}

fn now() -> i64 {
    Date::now().as_millis() as i64
}

pub async fn library(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &rc.env).await?);
    let repo = repo(&rc.env)?;
    match MusicService::new(&repo, &WebCryptoEntropy, now())
        .library(&p)
        .await
    {
        Ok(lib) => json(200, &lib),
        Err(e) => music_error(e),
    }
}

pub async fn create(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &rc.env).await?);
    let input: NewPlaylist = try_or_respond!(read_json(&mut req).await?);
    let repo = repo(&rc.env)?;
    match MusicService::new(&repo, &WebCryptoEntropy, now())
        .create(&p, input)
        .await
    {
        Ok(pl) => json(201, &pl),
        Err(e) => music_error(e),
    }
}

pub async fn update(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &rc.env).await?);
    let patch: PlaylistPatch = try_or_respond!(read_json(&mut req).await?);
    let id = rc.param("id").cloned().unwrap_or_default();
    let repo = repo(&rc.env)?;
    match MusicService::new(&repo, &WebCryptoEntropy, now())
        .update(&p, &id, patch)
        .await
    {
        Ok(pl) => json(200, &pl),
        Err(e) => music_error(e),
    }
}

pub async fn delete(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &rc.env).await?);
    let id = rc.param("id").cloned().unwrap_or_default();
    let repo = repo(&rc.env)?;
    match MusicService::new(&repo, &WebCryptoEntropy, now())
        .delete(&p, &id)
        .await
    {
        Ok(()) => no_content(),
        Err(e) => music_error(e),
    }
}
