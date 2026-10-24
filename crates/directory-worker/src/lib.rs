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

use http::{
    auth, automations, backup, diagnostics, events, export, files, habits, health, inbox, jarvis,
    journal, library, links, music, notes, preferences, projects, search, tasks,
};

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
        // JARVIS
        .post_async("/api/v1/jarvis/ask", jarvis::ask)
        .post_async("/api/v1/jarvis/plan", jarvis::plan)
        .post_async("/api/v1/jarvis/plans/:id/execute", jarvis::execute)
        .post_async("/api/v1/jarvis/plans/:id/cancel", jarvis::cancel)
        .get_async("/api/v1/jarvis/history", jarvis::history)
        .get_async("/api/v1/jarvis/presets", jarvis::list_presets)
        .post_async("/api/v1/jarvis/presets", jarvis::create_preset)
        .delete_async("/api/v1/jarvis/presets/:id", jarvis::delete_preset)
        // Library (reading progress for PDF/ePub files)
        .get_async("/api/v1/library", library::list)
        .put_async("/api/v1/library/:file_id/progress", library::update)
        .delete_async("/api/v1/library/:file_id/progress", library::clear)
        // Search
        .get_async("/api/v1/search", search::search)
        // Data export
        .get_async("/api/v1/export", export::export)
        // Links
        .get_async("/api/v1/links", links::list)
        .post_async("/api/v1/links", links::create)
        .delete_async("/api/v1/links/:id", links::delete)
        // Automations
        .get_async("/api/v1/automations", automations::list)
        .post_async("/api/v1/automations", automations::create)
        .patch_async("/api/v1/automations/:id", automations::update)
        .delete_async("/api/v1/automations/:id", automations::delete)
        // Restore from an export (D-025)
        .post_async("/api/v1/import/:kind", backup::import)
        // Client error reports (D-027)
        .get_async("/api/v1/diagnostics/errors", diagnostics::list)
        .post_async("/api/v1/diagnostics/errors", diagnostics::report)
        .delete_async("/api/v1/diagnostics/errors", diagnostics::clear)
        // Calendar
        .get_async("/api/v1/events", events::list)
        .post_async("/api/v1/events", events::create)
        .get_async("/api/v1/events/agenda", events::agenda)
        .patch_async("/api/v1/events/:id", events::update)
        .delete_async("/api/v1/events/:id", events::delete)
        // Habits
        .get_async("/api/v1/habits", habits::list)
        .post_async("/api/v1/habits", habits::create)
        .patch_async("/api/v1/habits/:id", habits::update)
        .delete_async("/api/v1/habits/:id", habits::delete)
        .put_async("/api/v1/habits/:id/checkins/:date", habits::check)
        .delete_async("/api/v1/habits/:id/checkins/:date", habits::uncheck)
        // Inbox
        .get_async("/api/v1/inbox", inbox::list)
        .post_async("/api/v1/inbox", inbox::capture)
        .delete_async("/api/v1/inbox/:id", inbox::delete)
        .post_async("/api/v1/inbox/:id/triage", inbox::triage)
        // Journal
        .get_async("/api/v1/journal", journal::list)
        .get_async("/api/v1/journal/:date", journal::get)
        .put_async("/api/v1/journal/:date", journal::put)
        .delete_async("/api/v1/journal/:date", journal::delete)
        // Music (audio streams from /files/:id/content)
        .get_async("/api/v1/music", music::library)
        .post_async("/api/v1/music/playlists", music::create)
        .patch_async("/api/v1/music/playlists/:id", music::update)
        .delete_async("/api/v1/music/playlists/:id", music::delete)
        // Notes
        .get_async("/api/v1/notes", notes::list)
        .post_async("/api/v1/notes", notes::create)
        .get_async("/api/v1/notes/:id", notes::get)
        .get_async("/api/v1/notes/:id/backlinks", notes::backlinks)
        .patch_async("/api/v1/notes/:id", notes::update)
        .delete_async("/api/v1/notes/:id", notes::delete)
        // Preferences
        .get_async("/api/v1/preferences", preferences::get)
        .patch_async("/api/v1/preferences", preferences::update)
        // Projects
        .get_async("/api/v1/projects", projects::list)
        .post_async("/api/v1/projects", projects::create)
        .get_async("/api/v1/projects/:id", projects::get)
        .patch_async("/api/v1/projects/:id", projects::update)
        .delete_async("/api/v1/projects/:id", projects::delete)
        // Tasks
        .get_async("/api/v1/tasks", tasks::list)
        .post_async("/api/v1/tasks", tasks::create)
        .get_async("/api/v1/tasks/plan", tasks::plan)
        .get_async("/api/v1/tasks/:id", tasks::get)
        .patch_async("/api/v1/tasks/:id", tasks::update)
        .delete_async("/api/v1/tasks/:id", tasks::delete)
        // Files: metadata in D1, content streamed via R2
        .get_async("/api/v1/files", files::list)
        .post_async("/api/v1/files", files::upload)
        .get_async("/api/v1/files/usage", files::usage)
        .get_async("/api/v1/files/:id/content", files::content)
        .patch_async("/api/v1/files/:id", files::update)
        .delete_async("/api/v1/files/:id", files::delete)
        .or_else_any_method_async("/api/*rest", |_, _| async { http::error(404, "not_found") })
        .run(req, env)
        .await
}
