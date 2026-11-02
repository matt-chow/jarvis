//! Reminder notifications (D-049): per-device Web Push subscriptions, the
//! notice log (one row per logical alert), and per-device deliveries.
//!
//! The scheduler step writes a [`Notice`] in the same batch that moves a
//! reminder's high-water mark, so an alert is recorded exactly once.
//! [`dispatch_pending`] fans each notice out to the owner's devices, and
//! [`send_due`] sends payload-free pushes with retries. A device shows a
//! notification only after its service worker claims it
//! ([`NotificationService::claim`]); that claim, not the push service's
//! `201`, is what counts as delivered.

pub mod vapid;

#[cfg(test)]
pub(crate) mod memory;
#[cfg(test)]
mod tests;

use crate::entropy::{is_id, new_id, Entropy};
use crate::identity::Principal;
use crate::reminders::{fire_label, Reminder, ReminderRepository};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use vapid::{audience, endpoint_allowed, Vapid};

const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 60 * MINUTE_MS;
const DAY_MS: i64 = 24 * HOUR_MS;

pub const MAX_DEVICES: usize = 20;
pub const MAX_LABEL: usize = 60;
/// Sends per delivery before it is marked failed.
pub const MAX_ATTEMPTS: u32 = 5;
/// A notice this late is not sent (the app still shows the reminder overdue).
pub const STALE_MS: i64 = 12 * HOUR_MS;
/// How long a push service holds an undelivered push (seconds).
pub const PUSH_TTL_S: i64 = 12 * 3_600;
/// A device may claim notices accepted this recently.
pub const CLAIM_WINDOW_MS: i64 = DAY_MS;
/// Notices and deliveries older than this are pruned.
pub const KEEP_MS: i64 = 30 * DAY_MS;
/// One test notification per owner per window.
pub const TEST_WINDOW_MS: i64 = 30_000;
pub const MAX_HISTORY: u32 = 100;

/// Notice states.
pub mod notice {
    /// Waiting for [`super::dispatch_pending`].
    pub const PENDING: &str = "pending";
    /// Fanned out to the owner's devices.
    pub const DISPATCHED: &str = "dispatched";
    /// The owner has no devices subscribed; the app shows it in-app only.
    pub const NO_DEVICES: &str = "no_devices";
    /// Web Push isn't configured on this server.
    pub const NOT_CONFIGURED: &str = "not_configured";
    /// Completed, snoozed past, deleted, or moved on before it was sent.
    pub const RESOLVED: &str = "resolved";
    /// Came due with a newer alert of the same reminder; only the newer one is sent.
    pub const SUPERSEDED: &str = "superseded";
    /// Not sent: it was more than [`super::STALE_MS`] late.
    pub const STALE: &str = "stale";
}

/// Delivery states (one row per notice per device).
pub mod delivery {
    pub const QUEUED: &str = "queued";
    /// Will be sent again at `next_try`.
    pub const RETRY: &str = "retry";
    /// The push service took it. Not proof the device got it.
    pub const ACCEPTED: &str = "accepted";
    /// The device's service worker fetched it to display: delivered.
    pub const SHOWN: &str = "shown";
    pub const OPENED: &str = "opened";
    pub const COMPLETED: &str = "completed";
    pub const SNOOZED: &str = "snoozed";
    pub const DISMISSED: &str = "dismissed";
    /// Resolved before the device fetched it; nothing was shown.
    pub const CANCELLED: &str = "cancelled";
    pub const FAILED: &str = "failed";
    /// The push service says the subscription is gone; the device was removed.
    pub const EXPIRED: &str = "expired";
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotifyError {
    Invalid(&'static str, &'static str),
    NotFound,
    TooMany,
    /// A test notification was sent moments ago.
    Busy,
    Backend(String),
}

/// One device's push subscription.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Subscription {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    /// A capability URL: never returned to clients.
    #[serde(skip)]
    pub endpoint: String,
    pub label: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_ok_at: Option<i64>,
    pub last_error: Option<String>,
    pub failures: u32,
}

/// One logical alert. `reminder_id` is empty for a test notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub reminder_id: String,
    /// The occurrence date (`-` for a test).
    pub occurrence: String,
    /// The alert slot (see `schedule::Fire`), or `t<window>` for a test.
    pub slot: String,
    pub due_at: i64,
    pub state: String,
    pub created_at: i64,
}

impl Notice {
    pub fn is_test(&self) -> bool {
        self.reminder_id.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivery {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub notice_id: String,
    pub subscription_id: String,
    pub state: String,
    pub attempts: u32,
    pub next_try: Option<i64>,
    /// The push service's last HTTP status.
    pub status: Option<u16>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub trait NotificationRepository {
    async fn subscriptions(&self, owner_id: &str) -> Result<Vec<Subscription>, String>;
    /// By endpoint, whoever owns it.
    async fn subscription_by_endpoint(
        &self,
        endpoint: &str,
    ) -> Result<Option<Subscription>, String>;
    /// Insert or replace by id.
    async fn put_subscription(&self, s: &Subscription) -> Result<(), String>;
    /// Remove the device; its unsent deliveries become `expired`.
    async fn delete_subscription(&self, owner_id: &str, id: &str) -> Result<bool, String>;
    /// Insert unless a notice with the same owner, reminder, occurrence,
    /// and slot exists; `false` means it did.
    async fn insert_notice(&self, n: &Notice) -> Result<bool, String>;
    /// Pending notices of every owner, oldest first (scheduler only).
    async fn pending_notices(&self, limit: u32) -> Result<Vec<Notice>, String>;
    /// Set the notice's state and add its deliveries (ignoring any that exist), in one batch.
    async fn dispatch(&self, n: &Notice, deliveries: &[Delivery]) -> Result<(), String>;
    /// Queued or retrying deliveries whose time has come, each with its
    /// device (same owner), oldest first (scheduler only).
    async fn due_deliveries(
        &self,
        now: i64,
        limit: u32,
    ) -> Result<Vec<(Delivery, Subscription)>, String>;
    /// Save a delivery and, when given, its device's health, in one batch.
    async fn save_delivery(&self, d: &Delivery, s: Option<&Subscription>) -> Result<(), String>;
    /// The device's deliveries in `states`, updated since `since`, with their notices.
    async fn deliveries_for(
        &self,
        owner_id: &str,
        subscription_id: &str,
        states: &[&str],
        since: i64,
    ) -> Result<Vec<(Delivery, Notice)>, String>;
    async fn delivery(&self, owner_id: &str, id: &str) -> Result<Option<Delivery>, String>;
    /// Newest first, with each notice's deliveries.
    async fn history(
        &self,
        owner_id: &str,
        limit: u32,
    ) -> Result<Vec<(Notice, Vec<Delivery>)>, String>;
    /// Delete notices (and their deliveries) created before `before`.
    async fn prune(&self, before: i64) -> Result<(), String>;
}

/// Sends one payload-free push. `Err` is a network failure.
pub trait PushTransport {
    async fn send(&self, endpoint: &str, headers: &[(&'static str, String)])
        -> Result<u16, String>;
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewSubscription {
    pub endpoint: String,
    #[serde(default)]
    pub label: String,
}

/// What a device shows for a claimed delivery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Shown {
    pub delivery_id: String,
    pub notice_id: String,
    pub reminder_id: Option<String>,
    pub occurrence: Option<String>,
    pub title: String,
    pub body: String,
    /// In-app path to open.
    pub url: String,
    /// Replaces an earlier notification of the same occurrence.
    pub tag: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HistoryItem {
    #[serde(flatten)]
    pub notice: Notice,
    /// The reminder's current title, if it still exists.
    pub title: Option<String>,
    /// The occurrence is still open (in-app fallback shows these).
    pub open: bool,
    pub deliveries: Vec<Delivery>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ack {
    Opened,
    Completed,
    Snoozed,
    Dismissed,
}

/// Whether `notice` still describes an open occurrence of `r` that should alert.
pub fn still_due(r: &Reminder, n: &Notice) -> bool {
    if r.deleted_at.is_some() || r.completed_at.is_some() {
        return false;
    }
    let Some(cursor) = r.due_date.as_deref() else {
        return false;
    };
    let occ = n.occurrence.as_str();
    if occ < cursor || r.completed_ahead.iter().any(|d| d == occ) {
        return false;
    }
    match n.slot.strip_prefix('s') {
        // A snooze alert counts only while that snooze stands.
        Some(until) => {
            occ == cursor && r.snoozed_until.map(|u| u.to_string()).as_deref() == Some(until)
        }
        // An ordinary alert is held back while its occurrence is snoozed past it.
        None => !(occ == cursor && r.snoozed_until.is_some_and(|u| u > n.due_at)),
    }
}

fn clean_label(raw: &str) -> String {
    let s: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let s: String = s.chars().take(MAX_LABEL).collect();
    if s.is_empty() {
        "This device".into()
    } else {
        s
    }
}

fn backend(e: String) -> NotifyError {
    NotifyError::Backend(e)
}

/// Owner-scoped operations.
pub struct NotificationService<'a, N, R, E: ?Sized> {
    repo: &'a N,
    reminders: &'a R,
    entropy: &'a E,
    now: i64,
    /// Accept endpoints under this origin too (local tests only).
    test_origin: Option<&'a str>,
}

impl<'a, N: NotificationRepository, R: ReminderRepository, E: Entropy + ?Sized>
    NotificationService<'a, N, R, E>
{
    pub fn new(
        repo: &'a N,
        reminders: &'a R,
        entropy: &'a E,
        now: i64,
        test_origin: Option<&'a str>,
    ) -> Self {
        Self {
            repo,
            reminders,
            entropy,
            now,
            test_origin,
        }
    }

    pub async fn devices(&self, p: &Principal) -> Result<Vec<Subscription>, NotifyError> {
        let mut all = self.repo.subscriptions(&p.user_id).await.map_err(backend)?;
        all.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
        Ok(all)
    }

    /// Register this device, or renew it (same endpoint). An endpoint
    /// registered under another account moves to this one: the browser now
    /// belongs to whoever signed in on it.
    pub async fn register(
        &self,
        p: &Principal,
        new: NewSubscription,
    ) -> Result<Subscription, NotifyError> {
        let endpoint = new.endpoint.trim();
        if !endpoint_allowed(endpoint, self.test_origin) {
            return Err(NotifyError::Invalid(
                "endpoint",
                "must be a push service URL Directory supports",
            ));
        }
        let label = clean_label(&new.label);
        let existing = self
            .repo
            .subscription_by_endpoint(endpoint)
            .await
            .map_err(backend)?;
        if let Some(mut s) = existing.clone().filter(|s| s.owner_id == p.user_id) {
            s.label = label;
            s.updated_at = self.now;
            self.repo.put_subscription(&s).await.map_err(backend)?;
            return Ok(s);
        }
        if self
            .repo
            .subscriptions(&p.user_id)
            .await
            .map_err(backend)?
            .len()
            >= MAX_DEVICES
        {
            return Err(NotifyError::TooMany);
        }
        if let Some(other) = existing {
            self.repo
                .delete_subscription(&other.owner_id, &other.id)
                .await
                .map_err(backend)?;
        }
        let s = Subscription {
            id: new_id(self.entropy),
            owner_id: p.user_id.clone(),
            endpoint: endpoint.to_owned(),
            label,
            created_at: self.now,
            updated_at: self.now,
            last_ok_at: None,
            last_error: None,
            failures: 0,
        };
        self.repo.put_subscription(&s).await.map_err(backend)?;
        Ok(s)
    }

    pub async fn remove(&self, p: &Principal, id: &str) -> Result<(), NotifyError> {
        if !is_id(id) {
            return Err(NotifyError::NotFound);
        }
        match self.repo.delete_subscription(&p.user_id, id).await {
            Ok(true) => Ok(()),
            Ok(false) => Err(NotifyError::NotFound),
            Err(e) => Err(backend(e)),
        }
    }

    /// Queue a test notification to all of the owner's devices (sent by the
    /// next scheduler run, or at once by the caller).
    pub async fn test(&self, p: &Principal) -> Result<Notice, NotifyError> {
        let n = Notice {
            id: new_id(self.entropy),
            owner_id: p.user_id.clone(),
            reminder_id: String::new(),
            occurrence: "-".into(),
            slot: format!("t{}", self.now / TEST_WINDOW_MS),
            due_at: self.now,
            state: notice::PENDING.into(),
            created_at: self.now,
        };
        match self.repo.insert_notice(&n).await {
            Ok(true) => Ok(n),
            Ok(false) => Err(NotifyError::Busy),
            Err(e) => Err(backend(e)),
        }
    }

    /// The device's service worker fetches what to show after a push.
    /// Deliveries whose occurrence was resolved meanwhile are cancelled, not shown.
    pub async fn claim(&self, p: &Principal, endpoint: &str) -> Result<Vec<Shown>, NotifyError> {
        let Some(sub) = self
            .repo
            .subscription_by_endpoint(endpoint.trim())
            .await
            .map_err(backend)?
            .filter(|s| s.owner_id == p.user_id)
        else {
            return Ok(Vec::new());
        };
        let states = [delivery::ACCEPTED, delivery::RETRY, delivery::QUEUED];
        let rows = self
            .repo
            .deliveries_for(&p.user_id, &sub.id, &states, self.now - CLAIM_WINDOW_MS)
            .await
            .map_err(backend)?;
        let mut out = Vec::new();
        for (mut d, n) in rows {
            let shown = if n.is_test() {
                Some(Shown {
                    delivery_id: d.id.clone(),
                    notice_id: n.id.clone(),
                    reminder_id: None,
                    occurrence: None,
                    title: "Directory test notification".into(),
                    body: "Notifications work on this device.".into(),
                    url: "/#/settings".into(),
                    tag: format!("test-{}", n.id),
                })
            } else {
                self.reminders
                    .get(&p.user_id, &n.reminder_id)
                    .await
                    .map_err(backend)?
                    .filter(|r| still_due(r, &n))
                    .map(|r| shown(&r, &n, &d))
            };
            d.state = if shown.is_some() {
                delivery::SHOWN
            } else {
                delivery::CANCELLED
            }
            .into();
            d.next_try = None;
            d.updated_at = self.now;
            self.repo.save_delivery(&d, None).await.map_err(backend)?;
            out.extend(shown);
        }
        Ok(out)
    }

    /// Record what the person did with a shown notification.
    pub async fn ack(&self, p: &Principal, id: &str, action: Ack) -> Result<(), NotifyError> {
        if !is_id(id) {
            return Err(NotifyError::NotFound);
        }
        let mut d = self
            .repo
            .delivery(&p.user_id, id)
            .await
            .map_err(backend)?
            .ok_or(NotifyError::NotFound)?;
        d.state = match action {
            Ack::Opened => delivery::OPENED,
            Ack::Completed => delivery::COMPLETED,
            Ack::Snoozed => delivery::SNOOZED,
            Ack::Dismissed => delivery::DISMISSED,
        }
        .into();
        d.updated_at = self.now;
        self.repo.save_delivery(&d, None).await.map_err(backend)
    }

    pub async fn history(
        &self,
        p: &Principal,
        limit: u32,
    ) -> Result<Vec<HistoryItem>, NotifyError> {
        let rows = self
            .repo
            .history(&p.user_id, limit.clamp(1, MAX_HISTORY))
            .await
            .map_err(backend)?;
        let reminders: HashMap<String, Reminder> = if rows.iter().any(|(n, _)| !n.is_test()) {
            self.reminders
                .list(&p.user_id)
                .await
                .map_err(backend)?
                .into_iter()
                .map(|r| (r.id.clone(), r))
                .collect()
        } else {
            HashMap::new()
        };
        Ok(rows
            .into_iter()
            .map(|(n, deliveries)| {
                let r = reminders.get(&n.reminder_id);
                HistoryItem {
                    title: r.map(|r| r.title.clone()),
                    open: r.is_some_and(|r| still_due(r, &n)),
                    notice: n,
                    deliveries,
                }
            })
            .collect())
    }
}

fn shown(r: &Reminder, n: &Notice, d: &Delivery) -> Shown {
    let fire = crate::reminders::schedule::Fire {
        occurrence: n.occurrence.clone(),
        slot: n.slot.clone(),
        at: n.due_at,
    };
    let when = fire_label(r, &fire);
    // The time only: notes stay off the lock screen.
    let body = if n.slot.starts_with('s') {
        format!("Snoozed · due {when}")
    } else {
        format!("Due {when}")
    };
    Shown {
        delivery_id: d.id.clone(),
        notice_id: n.id.clone(),
        reminder_id: Some(r.id.clone()),
        occurrence: Some(n.occurrence.clone()),
        title: r.title.clone(),
        body,
        url: format!("/#/reminders?reminder={}", r.id),
        tag: format!("r-{}-{}", r.id, n.occurrence),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DispatchReport {
    pub dispatched: u32,
    pub skipped: u32,
}

/// Fan pending notices out to their owners' devices (scheduler only). Each
/// notice is handled under its own owner: only that owner's reminder and
/// devices are read.
pub async fn dispatch_pending<
    N: NotificationRepository,
    R: ReminderRepository,
    E: Entropy + ?Sized,
>(
    repo: &N,
    reminders: &R,
    entropy: &E,
    configured: bool,
    now: i64,
    limit: u32,
) -> Result<DispatchReport, String> {
    let mut report = DispatchReport::default();
    for mut n in repo.pending_notices(limit).await? {
        let open = n.is_test()
            || reminders
                .get(&n.owner_id, &n.reminder_id)
                .await?
                .is_some_and(|r| still_due(&r, &n));
        let mut deliveries = Vec::new();
        n.state = if !open {
            notice::RESOLVED
        } else if now - n.due_at > STALE_MS {
            notice::STALE
        } else if !configured {
            notice::NOT_CONFIGURED
        } else {
            let subs = repo.subscriptions(&n.owner_id).await?;
            deliveries = subs
                .iter()
                .filter(|s| s.owner_id == n.owner_id)
                .map(|s| Delivery {
                    id: new_id(entropy),
                    owner_id: n.owner_id.clone(),
                    notice_id: n.id.clone(),
                    subscription_id: s.id.clone(),
                    state: delivery::QUEUED.into(),
                    attempts: 0,
                    next_try: Some(now),
                    status: None,
                    created_at: now,
                    updated_at: now,
                })
                .collect();
            if deliveries.is_empty() {
                notice::NO_DEVICES
            } else {
                notice::DISPATCHED
            }
        }
        .into();
        if deliveries.is_empty() {
            report.skipped += 1;
        } else {
            report.dispatched += 1;
        }
        repo.dispatch(&n, &deliveries).await?;
    }
    Ok(report)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SendReport {
    pub accepted: u32,
    pub retrying: u32,
    pub failed: u32,
    pub expired: u32,
}

/// The headers of a payload-free push, given its `Authorization` value.
pub fn push_headers(authorization: String, notice_id: &str) -> Vec<(&'static str, String)> {
    vec![
        ("Authorization", authorization),
        ("TTL", PUSH_TTL_S.to_string()),
        ("Urgency", "high".into()),
        // Lets the push service replace an undelivered retry of the same notice.
        ("Topic", notice_id.chars().take(32).collect()),
        ("Content-Length", "0".into()),
    ]
}

fn backoff(attempts: u32) -> i64 {
    MINUTE_MS << attempts.saturating_sub(1).min(6)
}

/// Send queued and retrying deliveries (scheduler only).
pub async fn send_due<N: NotificationRepository, T: PushTransport>(
    repo: &N,
    transport: &T,
    vapid: &Vapid,
    test_origin: Option<&str>,
    now: i64,
    limit: u32,
) -> Result<SendReport, String> {
    let mut report = SendReport::default();
    // One signature per push service per run.
    let mut auth: HashMap<String, String> = HashMap::new();
    for (mut d, mut s) in repo.due_deliveries(now, limit).await? {
        d.attempts += 1;
        d.updated_at = now;
        d.next_try = None;
        if s.owner_id != d.owner_id || !endpoint_allowed(&s.endpoint, test_origin) {
            d.state = delivery::FAILED.into();
            report.failed += 1;
            repo.save_delivery(&d, None).await?;
            continue;
        }
        let aud = audience(&s.endpoint).unwrap_or_default();
        let authorization = auth
            .entry(aud)
            .or_insert_with_key(|aud| vapid.authorization(aud, now))
            .clone();
        let headers = push_headers(authorization, &d.notice_id);
        let status = transport.send(&s.endpoint, &headers).await;
        d.status = status.as_ref().ok().copied();
        let retry =
            |d: &mut Delivery, s: &mut Subscription, why: String, report: &mut SendReport| {
                s.failures += 1;
                s.last_error = Some(why);
                if d.attempts >= MAX_ATTEMPTS {
                    d.state = delivery::FAILED.into();
                    report.failed += 1;
                } else {
                    d.state = delivery::RETRY.into();
                    d.next_try = Some(now + backoff(d.attempts));
                    report.retrying += 1;
                }
            };
        match status {
            Ok(200..=299) => {
                d.state = delivery::ACCEPTED.into();
                s.last_ok_at = Some(now);
                s.last_error = None;
                s.failures = 0;
                report.accepted += 1;
            }
            Ok(404 | 410) => {
                d.state = delivery::EXPIRED.into();
                report.expired += 1;
                repo.save_delivery(&d, None).await?;
                repo.delete_subscription(&s.owner_id, &s.id).await?;
                continue;
            }
            Ok(code @ (429 | 500..=599)) => retry(
                &mut d,
                &mut s,
                format!("push service busy ({code})"),
                &mut report,
            ),
            Err(_) => retry(
                &mut d,
                &mut s,
                "push service unreachable".into(),
                &mut report,
            ),
            Ok(code) => {
                d.state = delivery::FAILED.into();
                s.failures += 1;
                s.last_error = Some(if matches!(code, 401 | 403) {
                    format!("push service refused the server key ({code}); turn notifications off and on again on this device")
                } else {
                    format!("push service rejected the push ({code})")
                });
                report.failed += 1;
            }
        }
        repo.save_delivery(&d, Some(&s)).await?;
    }
    Ok(report)
}
