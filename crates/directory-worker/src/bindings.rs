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
/// Secret (optional): OpenRouter API key for AI intent parsing (D-043). Unset = AI off.
pub const OPENROUTER_API_KEY: &str = "OPENROUTER_API_KEY";
/// Var (optional): the OpenRouter model id; must end in `:free` or AI stays off.
pub const AI_MODEL: &str = "JARVIS_AI_MODEL";
/// Var (optional): most AI calls per owner per UTC day (default 40).
pub const AI_DAILY: &str = "JARVIS_AI_DAILY";
