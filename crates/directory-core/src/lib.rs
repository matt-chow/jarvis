//! Directory core: platform-independent domain logic.
//!
//! Infrastructure (Cloudflare D1, R2, HTTP) is reached only through the traits
//! defined here, so this crate builds and tests natively and can be reused by
//! future Windows or iOS clients.

// Workers is single-threaded, so the async traits below intentionally do not
// require `Send` futures.
#![allow(async_fn_in_trait)]

pub mod encoding;
pub mod health;
pub mod identity;
pub mod storage;
pub mod system;

#[cfg(test)]
pub(crate) mod test_util;
