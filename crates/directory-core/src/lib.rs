//! Directory core: platform-independent domain logic.
//!
//! Infrastructure (Cloudflare D1, R2, HTTP) is reached only through the traits
//! defined here, so this crate builds and tests natively and can be reused by
//! future Windows or iOS clients.

// Workers is single-threaded, so the async traits below intentionally do not
// require `Send` futures.
#![allow(async_fn_in_trait)]

pub mod automations;
pub mod backup;
pub mod calendar;
pub mod encoding;
pub mod entropy;
pub mod export;
pub mod files;
pub mod habits;
pub mod health;
pub mod identity;
pub mod inbox;
pub mod jarvis;
pub mod journal;
pub mod library;
pub mod links;
pub mod music;
pub mod notes;
pub mod preferences;
pub mod projects;
pub mod rate_limit;
pub mod search;
pub mod system;
pub mod tasks;
pub mod validate;

#[cfg(test)]
pub(crate) mod test_util;
