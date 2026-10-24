//! OpenRouter adapter for optional AI intent parsing (DECISIONS D-043).
//!
//! Off unless `OPENROUTER_API_KEY` is set and `JARVIS_AI_MODEL` names a
//! `:free` model. One model, no fallbacks, no router: nothing can move a
//! request to a paid model. Each owner gets `JARVIS_AI_DAILY` calls a day.

use crate::bindings;
use crate::d1::D1RateLimitStore;
use directory_core::jarvis::ai::{is_free_model, AiBudget, TextModel};
use directory_core::rate_limit::{self, Decision, Policy};
use serde_json::{json, Value};
use worker::{Date, Env, Fetch, Headers, Method, Request, RequestInit};

const ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";
const DEFAULT_DAILY: u32 = 40;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

pub struct OpenRouter {
    key: String,
    model: String,
}

impl OpenRouter {
    /// The configured model, or `None` when AI is off (or misconfigured: logged).
    pub fn from_env(env: &Env) -> Option<Self> {
        let key = env.secret(bindings::OPENROUTER_API_KEY).ok()?.to_string();
        let model = env.var(bindings::AI_MODEL).ok()?.to_string();
        if key.trim().is_empty() || model.trim().is_empty() {
            return None;
        }
        if !is_free_model(&model) {
            worker::console_warn!(
                "JARVIS_AI_MODEL is not a :free model; AI intent parsing stays off"
            );
            return None;
        }
        Some(Self {
            key: key.trim().to_owned(),
            model: model.trim().to_owned(),
        })
    }
}

impl TextModel for OpenRouter {
    async fn complete(&self, system: &str, user: &str) -> Result<String, String> {
        let body = json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user },
            ],
            "temperature": 0,
            "max_tokens": 400,
        });
        let headers = Headers::new();
        let set = |k: &str, v: &str| headers.set(k, v).map_err(|e| e.to_string());
        set("Authorization", &format!("Bearer {}", self.key))?;
        set("Content-Type", "application/json")?;
        set("X-Title", "Directory")?;
        let mut init = RequestInit::new();
        init.with_method(Method::Post)
            .with_headers(headers.clone())
            .with_body(Some(body.to_string().into()));
        let req = Request::new_with_init(ENDPOINT, &init).map_err(|e| e.to_string())?;
        let mut resp = Fetch::Request(req)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if resp.status_code() != 200 {
            return Err(format!("openrouter status {}", resp.status_code()));
        }
        let v: Value = resp.json().await.map_err(|e| e.to_string())?;
        v.pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| "openrouter reply without content".into())
    }
}

/// Per-owner daily cap, counted in D1 (the rate-limit table). Fails closed:
/// if the counter can't be read, no AI call is made.
pub struct DailyBudget {
    store: D1RateLimitStore,
    key: String,
    max: u32,
}

impl DailyBudget {
    pub fn new(env: &Env, owner_id: &str) -> worker::Result<Self> {
        let max = env
            .var(bindings::AI_DAILY)
            .ok()
            .and_then(|v| v.to_string().trim().parse().ok())
            .unwrap_or(DEFAULT_DAILY);
        Ok(Self {
            store: D1RateLimitStore::new(env.d1(bindings::DB)?),
            key: format!("ai:{owner_id}"),
            max,
        })
    }
}

impl AiBudget for DailyBudget {
    async fn allow(&self) -> bool {
        let policy = Policy {
            max: self.max,
            window_ms: DAY_MS,
        };
        let now = Date::now().as_millis() as i64;
        match rate_limit::check(&self.store, policy, &self.key, now).await {
            Ok(Decision::Allowed) => true,
            Ok(Decision::Limited { .. }) => false,
            Err(e) => {
                worker::console_error!("ai budget store error: {e}");
                false
            }
        }
    }
}
