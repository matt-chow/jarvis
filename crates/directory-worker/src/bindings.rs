//! Binding and variable names; must match wrangler.toml / .dev.vars.

pub const ASSETS: &str = "ASSETS";
pub const DB: &str = "DB";
pub const FILES: &str = "FILES";
/// Secret: one-time owner setup token (≥ 32 chars).
pub const SETUP_TOKEN: &str = "DIRECTORY_SETUP_TOKEN";
/// Var: WebAuthn relying party id (the app's hostname, e.g. `localhost`).
pub const RP_ID: &str = "DIRECTORY_RP_ID";
/// Var: comma-separated exact origins allowed to sign in and send cookie-authenticated writes.
pub const ORIGINS: &str = "DIRECTORY_ORIGINS";
