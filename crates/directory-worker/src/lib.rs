//! Directory Worker entry point.
//!
//! Only `/api/*` reaches this code (see `run_worker_first` in wrangler.toml);
//! everything else is served by Workers Static Assets. This crate is a thin
//! adapter: domain rules live in `directory-core`.

use worker::*;

mod bindings;
mod d1;
mod http;
mod r2;

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    if !req.path().starts_with("/api/") {
        // Defensive fallback if routing config changes: hand off to assets.
        return env.assets(bindings::ASSETS)?.fetch_request(req).await;
    }

    Router::new()
        .get_async("/api/v1/health", http::health)
        .get_async("/api/v1/storage/objects", http::list_objects)
        .put_async("/api/v1/storage/objects/*key", http::put_object)
        .get_async("/api/v1/storage/objects/*key", http::get_object)
        .delete_async("/api/v1/storage/objects/*key", http::delete_object)
        .or_else_any_method_async("/api/*rest", |_, _| async { http::error(404, "not_found") })
        .run(req, env)
        .await
}
