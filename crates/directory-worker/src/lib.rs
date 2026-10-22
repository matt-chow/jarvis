//! Directory Worker entry point.
//!
//! Only `/api/*` reaches this code (see `run_worker_first` in wrangler.toml);
//! everything else is served by Workers Static Assets. This crate is a thin
//! adapter: domain rules live in `directory-core`.

use worker::*;

mod bindings;
mod d1;
mod entropy;
mod http;
mod r2;

use http::{auth, health, storage, tasks};

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    if !req.path().starts_with("/api/") {
        // Defensive fallback if routing config changes: hand off to assets.
        return env.assets(bindings::ASSETS)?.fetch_request(req).await;
    }

    Router::new()
        .get_async("/api/v1/health", health::health)
        // Identity
        .get_async("/api/v1/auth/session", auth::session)
        .post_async("/api/v1/auth/setup/options", auth::setup_options)
        .post_async("/api/v1/auth/setup/verify", auth::setup_verify)
        .post_async("/api/v1/auth/login/options", auth::login_options)
        .post_async("/api/v1/auth/login/verify", auth::login_verify)
        .post_async("/api/v1/auth/logout", auth::logout)
        .get_async("/api/v1/auth/passkeys", auth::list_passkeys)
        .post_async("/api/v1/auth/passkeys/options", auth::add_passkey_options)
        .post_async("/api/v1/auth/passkeys/verify", auth::add_passkey_verify)
        .delete_async("/api/v1/auth/passkeys/:id", auth::delete_passkey)
        .get_async("/api/v1/auth/tokens", auth::list_tokens)
        .post_async("/api/v1/auth/tokens", auth::create_token)
        .delete_async("/api/v1/auth/tokens/:id", auth::delete_token)
        // Tasks
        .get_async("/api/v1/tasks", tasks::list)
        .post_async("/api/v1/tasks", tasks::create)
        .get_async("/api/v1/tasks/:id", tasks::get)
        .patch_async("/api/v1/tasks/:id", tasks::update)
        .delete_async("/api/v1/tasks/:id", tasks::delete)
        // Files (R2)
        .get_async("/api/v1/storage/objects", storage::list_objects)
        .put_async("/api/v1/storage/objects/*key", storage::put_object)
        .get_async("/api/v1/storage/objects/*key", storage::get_object)
        .delete_async("/api/v1/storage/objects/*key", storage::delete_object)
        .or_else_any_method_async("/api/*rest", |_, _| async { http::error(404, "not_found") })
        .run(req, env)
        .await
}
