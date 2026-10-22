use super::json;
use crate::{bindings, d1::D1SystemRepository};
use directory_core::health::{self, Check};
use directory_core::system::database_check;
use worker::{Request, Response, Result, RouteContext};

pub async fn health(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let db_check = match ctx.env.d1(bindings::DB) {
        Ok(db) => database_check(&D1SystemRepository::new(db)).await,
        Err(_) => Check::degraded("database", "binding missing"),
    };
    // Binding presence only: avoid spending R2 operations on every health probe.
    let storage_check = match ctx.env.bucket(bindings::FILES) {
        Ok(_) => Check::ok("storage", "bound"),
        Err(_) => Check::degraded("storage", "binding missing"),
    };
    let report = health::report(env!("CARGO_PKG_VERSION"), vec![db_check, storage_check]);
    let status = if report.status == health::Status::Ok {
        200
    } else {
        503
    };
    json(status, &report)
}
