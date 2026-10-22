//! `GET /api/v1/search?q=…` across the owner's tasks, notes, projects, journal.

use super::auth::principal;
use super::{error, error_with, json};
use crate::{bindings, d1::D1SearchSource, try_or_respond};
use directory_core::search::{search as run_search, SearchError};
use serde_json::json as body;
use worker::{Request, Response, Result, RouteContext};

pub async fn search(req: Request, rc: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &rc.env).await?);
    let q = req
        .url()?
        .query_pairs()
        .find(|(k, _)| k == "q")
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default();
    let source = D1SearchSource::new(rc.env.d1(bindings::DB)?);
    match run_search(&source, &p, &q).await {
        Ok(hits) => json(200, &body!({ "hits": hits })),
        Err(SearchError::Invalid(why)) => error_with(400, "invalid_input", format!("q: {why}")),
        Err(SearchError::Backend(detail)) => {
            worker::console_error!("search backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}
