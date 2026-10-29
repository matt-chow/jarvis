//! OpenRouter adapter for optional AI intent parsing (DECISIONS D-043).
//!
//! Off unless `OPENROUTER_API_KEY` is set and `JARVIS_AI_MODEL` names a
//! `:free` model. One model, no fallbacks, no router: nothing can move a
//! request to a paid model. Each owner gets `JARVIS_AI_DAILY` calls a day.

use crate::bindings;
use crate::d1::D1RateLimitStore;
use directory_core::jarvis::ai::{is_free_model, openrouter_body, AiBudget, TextModel};
use directory_core::rate_limit::{self, Decision, Policy};
use serde_json::Value;
use std::future::{poll_fn, Future};
use std::pin::pin;
use std::task::Poll;
use std::time::Duration;
use worker::{AbortController, Date, Delay, Env, Fetch, Headers, Method, Request, RequestInit};

const ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";
const DEFAULT_DAILY: u32 = 40;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const DEFAULT_TIMEOUT_MS: u64 = 12_000;
/// Room for a chat turn's JSON (several actions), still small.
const MAX_TOKENS: u32 = 1_000;

pub struct OpenRouter {
    key: String,
    model: String,
    timeout_ms: u64,
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
        let timeout_ms = env
            .var(bindings::AI_TIMEOUT_MS)
            .ok()
            .and_then(|v| v.to_string().trim().parse().ok())
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .clamp(2_000, 25_000);
        Some(Self {
            key: key.trim().to_owned(),
            model: model.trim().to_owned(),
            timeout_ms,
        })
    }
}

impl TextModel for OpenRouter {
    async fn complete(&self, system: &str, user: &str) -> Result<String, String> {
        let body = openrouter_body(&self.model, system, user, MAX_TOKENS);
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
        let abort = AbortController::default();
        let signal = abort.signal();
        let call = async {
            let mut resp = Fetch::Request(req)
                .send_with_signal(&signal)
                .await
                .map_err(|e| e.to_string())?;
            if resp.status_code() != 200 {
                return Err(format!("openrouter status {}", resp.status_code()));
            }
            resp.json::<Value>().await.map_err(|e| e.to_string())
        };
        let Some(v) = within(call, self.timeout_ms).await else {
            abort.abort();
            return Err("openrouter timed out".into());
        };
        let v = v?;
        v.pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| "openrouter reply without content".into())
    }
}

/// `f`'s output, or `None` if it takes longer than `ms`.
async fn within<F: Future>(f: F, ms: u64) -> Option<F::Output> {
    let mut f = pin!(f);
    let mut timer = pin!(Delay::from(Duration::from_millis(ms)));
    poll_fn(|cx| {
        if let Poll::Ready(v) = f.as_mut().poll(cx) {
            return Poll::Ready(Some(v));
        }
        if timer.as_mut().poll(cx).is_ready() {
            return Poll::Ready(None);
        }
        Poll::Pending
    })
    .await
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
