//! Web Push (D-049): the transport, the VAPID config, and the cron job
//! that turns due reminders into notifications.
//!
//! Pushes carry no payload; the device's service worker claims what to show
//! (`POST /api/v1/notifications/claim`). A 2xx from the push service only
//! means "accepted"; "shown" is recorded by that claim.

use crate::ai::within;
use crate::d1::{D1NotificationRepository, D1ReminderRepository};
use crate::{bindings, entropy::WebCryptoEntropy, zone::IntlZones};
use directory_core::notifications::vapid::Vapid;
use directory_core::notifications::{
    dispatch_pending, send_due, NotificationRepository, PushTransport, KEEP_MS,
};
use directory_core::reminders::run_scheduler;
use worker::{AbortController, Date, Env, Fetch, Headers, Method, Request, RequestInit};

/// Most reminders visited, notices fanned out, and pushes sent per run;
/// the rest wait for the next minute (Workers Free CPU limits).
const SCAN_LIMIT: u32 = 50;
const DISPATCH_LIMIT: u32 = 50;
const SEND_LIMIT: u32 = 25;
const SEND_TIMEOUT_MS: u64 = 8_000;
const MINUTE_MS: i64 = 60_000;

pub struct WebPush;

impl PushTransport for WebPush {
    async fn send(
        &self,
        endpoint: &str,
        headers: &[(&'static str, String)],
    ) -> Result<u16, String> {
        let h = Headers::new();
        for (k, v) in headers {
            h.set(k, v).map_err(|e| e.to_string())?;
        }
        let mut init = RequestInit::new();
        init.with_method(Method::Post).with_headers(h);
        let req = Request::new_with_init(endpoint, &init).map_err(|e| e.to_string())?;
        let abort = AbortController::default();
        let signal = abort.signal();
        let call = async {
            Fetch::Request(req)
                .send_with_signal(&signal)
                .await
                .map(|r| r.status_code())
                .map_err(|e| e.to_string())
        };
        match within(call, SEND_TIMEOUT_MS).await {
            Some(r) => r,
            None => {
                abort.abort();
                Err("push timed out".into())
            }
        }
    }
}

/// The VAPID signer, or `None` while the key is unset or invalid
/// (notifications then report "not configured").
pub fn vapid(env: &Env) -> Option<Vapid> {
    let key = env.secret(bindings::VAPID_PRIVATE_KEY).ok()?.to_string();
    let subject = env
        .var(bindings::VAPID_SUBJECT)
        .ok()
        .map(|v| v.to_string())
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            env.var(bindings::ORIGINS)
                .ok()?
                .to_string()
                .split(',')
                .map(str::trim)
                .find(|o| o.starts_with("https://"))
                .map(str::to_owned)
        })?;
    match Vapid::new(key.trim(), subject.trim()) {
        Ok(v) => Some(v),
        Err(e) => {
            worker::console_error!("push: VAPID config invalid ({e:?})");
            None
        }
    }
}

/// Extra push origin accepted by local smoke tests only. Never set it on a
/// deployed Worker.
pub fn test_origin(env: &Env) -> Option<String> {
    env.var(bindings::PUSH_TEST_ORIGIN)
        .ok()
        .map(|v| v.to_string())
        .filter(|s| !s.trim().is_empty())
}

/// Fan out pending notices and send what is due. Also used right after a
/// test notification is queued.
pub async fn deliver(env: &Env, now: i64) -> Result<(), String> {
    let repo = D1NotificationRepository::new(env.d1(bindings::DB).map_err(|e| e.to_string())?);
    let reminders = D1ReminderRepository::new(env.d1(bindings::DB).map_err(|e| e.to_string())?);
    let signer = vapid(env);
    let origin = test_origin(env);
    let d = dispatch_pending(
        &repo,
        &reminders,
        &WebCryptoEntropy,
        signer.is_some(),
        now,
        DISPATCH_LIMIT,
    )
    .await?;
    let s = match &signer {
        Some(v) => send_due(&repo, &WebPush, v, origin.as_deref(), now, SEND_LIMIT).await?,
        None => Default::default(),
    };
    if d.dispatched + d.skipped + s.accepted + s.retrying + s.failed + s.expired > 0 {
        // Counts only: never reminder text or endpoints.
        worker::console_log!(
            "push: dispatched {} skipped {} accepted {} retrying {} failed {} expired {}",
            d.dispatched,
            d.skipped,
            s.accepted,
            s.retrying,
            s.failed,
            s.expired
        );
    }
    Ok(())
}

/// The cron step (every minute): advance due reminders, then deliver.
pub async fn scheduled(env: &Env) -> Result<(), String> {
    let now = Date::now().as_millis() as i64;
    let reminders = D1ReminderRepository::new(env.d1(bindings::DB).map_err(|e| e.to_string())?);
    let scanned = run_scheduler(&reminders, &WebCryptoEntropy, &IntlZones, now, SCAN_LIMIT).await?;
    if !scanned.is_empty() {
        worker::console_log!("push: scanned {} reminder(s)", scanned.len());
    }
    deliver(env, now).await?;
    // Old history goes once an hour.
    if (now / MINUTE_MS) % 60 == 0 {
        D1NotificationRepository::new(env.d1(bindings::DB).map_err(|e| e.to_string())?)
            .prune(now - KEEP_MS)
            .await?;
    }
    Ok(())
}
