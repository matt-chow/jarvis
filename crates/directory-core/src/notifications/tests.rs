use super::memory::{FakePush, MemoryNotifications};
use super::vapid::Vapid;
use super::*;
use crate::entropy::Entropy;
use crate::identity::{AuthMethod, Principal};
use crate::reminders::memory::MemoryReminders;
use crate::reminders::schedule::tests::at;
use crate::reminders::zone::testing::FakeZones;
use crate::reminders::{run_scheduler, NewReminder, ReminderService, Snooze};
use crate::test_util::block_on;
use std::cell::Cell;

const KEY: &str = "AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA";
const PHONE: &str = "https://web.push.apple.com/phone";
const LAPTOP: &str = "https://fcm.googleapis.com/fcm/send/laptop";
const THEIRS: &str = "https://fcm.googleapis.com/fcm/send/theirs";

/// Distinct ids however many a test makes.
#[derive(Default)]
struct Seq(Cell<u64>);

impl Entropy for Seq {
    fn fill(&self, buf: &mut [u8]) {
        let n = self.0.get() + 1;
        self.0.set(n);
        for (i, b) in buf.iter_mut().enumerate() {
            *b = n.to_le_bytes().get(i).copied().unwrap_or(0);
        }
    }
}

fn user(id: &str) -> Principal {
    Principal {
        user_id: id.into(),
        method: AuthMethod::Session,
    }
}

fn new(f: impl FnOnce(&mut serde_json::Value)) -> NewReminder {
    let mut v = serde_json::json!({ "title": "Submit assignment", "timezone": "Test/Toronto" });
    f(&mut v);
    serde_json::from_value(v).unwrap()
}

struct World {
    reminders: MemoryReminders,
    entropy: Seq,
    push: FakePush,
    vapid: Vapid,
}

impl World {
    fn new() -> Self {
        Self {
            reminders: MemoryReminders::default(),
            entropy: Seq::default(),
            push: FakePush::default(),
            vapid: Vapid::new(KEY, "mailto:owner@example.com").unwrap(),
        }
    }
    fn repo(&self) -> MemoryNotifications<'_> {
        MemoryNotifications::new(&self.reminders.notices)
    }
    fn rem(&self, now: i64) -> ReminderService<'_, MemoryReminders, Seq, FakeZones> {
        ReminderService::new(&self.reminders, &self.entropy, &FakeZones, now)
    }
}

fn svc<'a>(
    w: &'a World,
    n: &'a MemoryNotifications<'a>,
    now: i64,
) -> NotificationService<'a, MemoryNotifications<'a>, MemoryReminders, Seq> {
    NotificationService::new(n, &w.reminders, &w.entropy, now, None)
}

fn sub(endpoint: &str, label: &str) -> NewSubscription {
    NewSubscription {
        endpoint: endpoint.into(),
        label: label.into(),
    }
}

/// One scheduler minute: scan, fan out, send.
fn tick(w: &World, n: &MemoryNotifications, now: i64) -> (DispatchReport, SendReport) {
    block_on(run_scheduler(&w.reminders, &w.entropy, &FakeZones, now, 50)).unwrap();
    let d = block_on(dispatch_pending(n, &w.reminders, &w.entropy, true, now, 50)).unwrap();
    let s = block_on(send_due(n, &w.push, &w.vapid, None, now, 50)).unwrap();
    (d, s)
}

fn states(n: &MemoryNotifications) -> Vec<String> {
    n.deliveries
        .borrow()
        .iter()
        .map(|d| d.state.clone())
        .collect()
}

#[test]
fn an_alert_goes_once_to_each_of_the_owners_devices_and_counts_as_delivered_only_when_claimed() {
    let w = World::new();
    let n = w.repo();
    let (u, other) = (user("u"), user("other"));
    let s = svc(&w, &n, 1);
    let phone = block_on(s.register(&u, sub(PHONE, "iPhone"))).unwrap();
    block_on(s.register(&u, sub(LAPTOP, "Laptop"))).unwrap();
    block_on(s.register(&other, sub(THEIRS, ""))).unwrap();
    let r = block_on(w.rem(at("2026-11-01", 0)).create(
        &u,
        new(|v| {
            v["due_date"] = "2026-11-02".into();
            v["due_time"] = "08:00".into();
        }),
    ))
    .unwrap();
    // The other owner's reminder at the same time never reaches u's devices.
    block_on(w.rem(at("2026-11-01", 0)).create(
        &other,
        new(|v| {
            v["due_date"] = "2026-11-02".into();
            v["due_time"] = "08:00".into();
        }),
    ))
    .unwrap();

    // Not yet due.
    let (d, _) = tick(&w, &n, at("2026-11-02", 7 * 60 + 59));
    assert_eq!(d.dispatched, 0);
    assert!(w.push.sent.borrow().is_empty());

    let (d, sent) = tick(&w, &n, at("2026-11-02", 8 * 60 + 1));
    assert_eq!((d.dispatched, sent.accepted), (2, 3));
    let pushes = w.push.sent.borrow().clone();
    let mine: Vec<&str> = pushes
        .iter()
        .map(|(e, _)| e.as_str())
        .filter(|e| *e != THEIRS)
        .collect();
    assert_eq!(mine.len(), 2);
    let (_, headers) = pushes.iter().find(|(e, _)| e == PHONE).unwrap();
    let h = |k: &str| {
        headers
            .iter()
            .find(|(x, _)| *x == k)
            .map(|(_, v)| v.clone())
            .unwrap()
    };
    assert!(h("Authorization").starts_with("vapid t="));
    assert_eq!(
        (h("TTL"), h("Urgency"), h("Content-Length")),
        ("43200".into(), "high".into(), "0".into())
    );
    let notice_id = w
        .reminders
        .notices
        .borrow()
        .iter()
        .find(|x| x.owner_id == "u")
        .unwrap()
        .id
        .clone();
    assert_eq!(h("Topic"), notice_id);

    // Later minutes don't log or send the same alert again.
    tick(&w, &n, at("2026-11-02", 8 * 60 + 2));
    tick(&w, &n, at("2026-11-02", 9 * 60));
    assert_eq!(w.push.sent.borrow().len(), 3);
    assert_eq!(
        w.reminders
            .notices
            .borrow()
            .iter()
            .filter(|x| x.owner_id == "u")
            .count(),
        1
    );

    // Accepted by the push service is not delivered; the device's claim is.
    let now = at("2026-11-02", 8 * 60 + 3);
    let s = svc(&w, &n, now);
    let shown = block_on(s.claim(&u, PHONE)).unwrap();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].title, "Submit assignment");
    assert_eq!(shown[0].body, "Due 2026-11-02 08:00");
    assert_eq!(shown[0].url, format!("/#/reminders?reminder={}", r.id));
    assert_eq!(shown[0].occurrence.as_deref(), Some("2026-11-02"));
    assert!(
        block_on(s.claim(&u, PHONE)).unwrap().is_empty(),
        "claimed once"
    );
    // Someone else can't claim (or learn about) u's device.
    assert!(block_on(s.claim(&other, PHONE)).unwrap().is_empty());

    let hist = block_on(s.history(&u, 10)).unwrap();
    assert_eq!(hist.len(), 1);
    assert_eq!(hist[0].title.as_deref(), Some("Submit assignment"));
    assert!(hist[0].open);
    let mut ds: Vec<(String, String)> = hist[0]
        .deliveries
        .iter()
        .map(|d| (d.subscription_id.clone(), d.state.clone()))
        .collect();
    ds.sort_by_key(|(id, _)| (id != &phone.id, id.clone()));
    assert_eq!(ds[0], (phone.id.clone(), delivery::SHOWN.into()));
    assert_eq!(ds[1].1, delivery::ACCEPTED);

    block_on(s.ack(&u, &shown[0].delivery_id, Ack::Completed)).unwrap();
    assert_eq!(
        block_on(s.ack(&other, &shown[0].delivery_id, Ack::Opened)),
        Err(NotifyError::NotFound)
    );
    assert!(states(&n).contains(&delivery::COMPLETED.to_owned()));
}

#[test]
fn retries_busy_push_services_drops_expired_devices_and_reports_refusals() {
    let w = World::new();
    let n = w.repo();
    let u = user("u");
    let s = svc(&w, &n, 1);
    let phone = block_on(s.register(&u, sub(PHONE, "iPhone"))).unwrap();
    block_on(w.rem(at("2026-11-01", 0)).create(
        &u,
        new(|v| {
            v["due_date"] = "2026-11-02".into();
            v["due_time"] = "08:00".into();
        }),
    ))
    .unwrap();
    *w.push.replies.borrow_mut() = vec![Err("dns".into()), Ok(503), Ok(201)];
    let t0 = at("2026-11-02", 8 * 60);
    let (_, r) = tick(&w, &n, t0);
    assert_eq!(r.retrying, 1);
    let d = n.deliveries.borrow()[0].clone();
    assert_eq!(
        (d.state.as_str(), d.attempts, d.next_try),
        (delivery::RETRY, 1, Some(t0 + 60_000))
    );
    assert_eq!(
        n.subscriptions.borrow()[0].last_error.as_deref(),
        Some("push service unreachable")
    );
    // Not before its time.
    assert_eq!(
        block_on(send_due(&n, &w.push, &w.vapid, None, t0 + 30_000, 50)).unwrap(),
        SendReport::default()
    );
    let r = block_on(send_due(&n, &w.push, &w.vapid, None, t0 + 60_000, 50)).unwrap();
    assert_eq!(r.retrying, 1);
    assert_eq!(
        n.deliveries.borrow()[0].next_try,
        Some(t0 + 60_000 + 120_000)
    );
    let r = block_on(send_due(&n, &w.push, &w.vapid, None, t0 + 180_000, 50)).unwrap();
    assert_eq!(r.accepted, 1);
    let healthy = n.subscriptions.borrow()[0].clone();
    assert_eq!(
        (healthy.failures, healthy.last_error, healthy.last_ok_at),
        (0, None, Some(t0 + 180_000))
    );
    assert_eq!(w.push.sent.borrow().len(), 3, "one push per attempt");

    // Five failures in a row: failed, not retried forever.
    let t = svc(&w, &n, t0 + 200_000);
    block_on(t.test(&u)).unwrap();
    *w.push.replies.borrow_mut() = vec![Ok(500); 5];
    let mut now = t0 + 200_000;
    block_on(dispatch_pending(
        &n,
        &w.reminders,
        &w.entropy,
        true,
        now,
        50,
    ))
    .unwrap();
    for _ in 0..5 {
        block_on(send_due(&n, &w.push, &w.vapid, None, now, 50)).unwrap();
        now += 3_600_000;
    }
    assert_eq!(states(&n), [delivery::ACCEPTED, delivery::FAILED]);
    assert_eq!(
        block_on(send_due(&n, &w.push, &w.vapid, None, now, 50)).unwrap(),
        SendReport::default()
    );

    // A refused server key is reported on the device; the device stays.
    block_on(svc(&w, &n, now).test(&u)).unwrap();
    *w.push.replies.borrow_mut() = vec![Ok(403)];
    block_on(dispatch_pending(
        &n,
        &w.reminders,
        &w.entropy,
        true,
        now,
        50,
    ))
    .unwrap();
    assert_eq!(
        block_on(send_due(&n, &w.push, &w.vapid, None, now, 50))
            .unwrap()
            .failed,
        1
    );
    assert!(n.subscriptions.borrow()[0]
        .last_error
        .as_deref()
        .unwrap()
        .contains("server key"));

    // 410 Gone: the device is removed.
    now += TEST_WINDOW_MS;
    block_on(svc(&w, &n, now).test(&u)).unwrap();
    *w.push.replies.borrow_mut() = vec![Ok(410)];
    block_on(dispatch_pending(
        &n,
        &w.reminders,
        &w.entropy,
        true,
        now,
        50,
    ))
    .unwrap();
    assert_eq!(
        block_on(send_due(&n, &w.push, &w.vapid, None, now, 50))
            .unwrap()
            .expired,
        1
    );
    assert!(n.subscriptions.borrow().is_empty());
    assert_eq!(
        states(&n).last().map(String::as_str),
        Some(delivery::EXPIRED)
    );
    assert_eq!(
        block_on(svc(&w, &n, now).remove(&u, &phone.id)),
        Err(NotifyError::NotFound)
    );
}

#[test]
fn resolved_occurrences_are_not_sent_or_shown() {
    let w = World::new();
    let n = w.repo();
    let u = user("u");
    block_on(svc(&w, &n, 1).register(&u, sub(PHONE, ""))).unwrap();
    let r = block_on(w.rem(at("2026-11-01", 0)).create(
        &u,
        new(|v| {
            v["due_date"] = "2026-11-02".into();
            v["due_time"] = "08:00".into();
            v["recurrence"] = serde_json::json!({ "freq": "daily" });
        }),
    ))
    .unwrap();
    let t = at("2026-11-02", 8 * 60);
    // Logged by the scan, then completed on another device before it was sent.
    block_on(run_scheduler(&w.reminders, &w.entropy, &FakeZones, t, 50)).unwrap();
    block_on(w.rem(t + 1).complete(&u, &r.id, None)).unwrap();
    let d = block_on(dispatch_pending(
        &n,
        &w.reminders,
        &w.entropy,
        true,
        t + 2,
        50,
    ))
    .unwrap();
    assert_eq!((d.dispatched, d.skipped), (0, 1));
    assert_eq!(w.reminders.notices.borrow()[0].state, notice::RESOLVED);

    // Sent, then completed offline-and-synced before the phone fetched it: cancelled, not shown.
    let t = at("2026-11-03", 8 * 60);
    tick(&w, &n, t);
    block_on(w.rem(t + 1).complete(&u, &r.id, Some("2026-11-03"))).unwrap();
    assert!(block_on(svc(&w, &n, t + 2).claim(&u, PHONE))
        .unwrap()
        .is_empty());
    assert_eq!(states(&n), [delivery::CANCELLED]);
    let hist = block_on(svc(&w, &n, t + 2).history(&u, 10)).unwrap();
    assert!(hist.iter().all(|h| !h.open));
}

#[test]
fn snoozes_hold_back_alerts_and_alert_again_when_they_end() {
    let w = World::new();
    let n = w.repo();
    let u = user("u");
    block_on(svc(&w, &n, 1).register(&u, sub(PHONE, ""))).unwrap();
    let r = block_on(w.rem(at("2026-11-01", 0)).create(&u, new(|v| {
        v["due_date"] = "2026-11-02".into();
        v["due_time"] = "08:00".into();
        v["alerts"] = serde_json::json!([{ "kind": "before", "minutes": 15 }, { "kind": "before", "minutes": 0 }]);
    })))
    .unwrap();
    tick(&w, &n, at("2026-11-02", 7 * 60 + 45));
    // Snoozed an hour from the first alert: the 08:00 alert is held back.
    let until = at("2026-11-02", 8 * 60 + 45);
    block_on(w.rem(at("2026-11-02", 7 * 60 + 46)).snooze(
        &u,
        &r.id,
        Snooze {
            minutes: None,
            until: Some(until),
            tomorrow: false,
            occurrence: None,
        },
    ))
    .unwrap();
    tick(&w, &n, at("2026-11-02", 8 * 60));
    let log = w.reminders.notices.borrow().clone();
    assert_eq!(log.len(), 2);
    assert_eq!(
        (log[1].slot.as_str(), log[1].state.as_str()),
        ("b0", notice::RESOLVED)
    );
    // The snooze's own alert.
    tick(&w, &n, until);
    let log = w.reminders.notices.borrow().clone();
    assert_eq!(log[2].slot, format!("s{until}"));
    assert_eq!(log[2].state, notice::DISPATCHED);
    let shown = block_on(svc(&w, &n, until + 1).claim(&u, PHONE)).unwrap();
    // The snoozed 07:45 alert, never fetched meanwhile, is cancelled; the snooze's shows.
    assert_eq!(shown.len(), 1);
    assert!(shown[0].body.starts_with("Snoozed"));
    assert_eq!(
        shown[0].tag,
        format!("r-{}-2026-11-02", r.id),
        "one notification per occurrence"
    );
    assert_eq!(states(&n), [delivery::CANCELLED, delivery::SHOWN]);
}

#[test]
fn registers_renews_moves_and_limits_devices() {
    let w = World::new();
    let n = w.repo();
    let (u, other) = (user("u"), user("other"));
    let s = svc(&w, &n, 5);
    for bad in [
        "http://fcm.googleapis.com/x",
        "https://example.com/push",
        "https://169.254.169.254/x",
        "",
    ] {
        assert!(
            matches!(
                block_on(s.register(&u, sub(bad, ""))),
                Err(NotifyError::Invalid("endpoint", _))
            ),
            "{bad}"
        );
    }
    let a = block_on(s.register(&u, sub(PHONE, "  my\u{7}  phone "))).unwrap();
    assert_eq!(a.label, "my phone");
    // Renewal: the same endpoint keeps its id.
    let again = block_on(svc(&w, &n, 9).register(&u, sub(PHONE, ""))).unwrap();
    assert_eq!(
        (again.id.as_str(), again.label.as_str(), again.updated_at),
        (a.id.as_str(), "This device", 9)
    );
    // Another account signs in on that browser: the device moves.
    let moved = block_on(s.register(&other, sub(PHONE, ""))).unwrap();
    assert_ne!(moved.id, a.id);
    assert!(block_on(s.devices(&u)).unwrap().is_empty());
    assert_eq!(
        block_on(s.remove(&u, &moved.id)),
        Err(NotifyError::NotFound)
    );
    for i in 0..MAX_DEVICES {
        block_on(s.register(&u, sub(&format!("{LAPTOP}{i}"), ""))).unwrap();
    }
    assert_eq!(
        block_on(s.register(&u, sub(&format!("{LAPTOP}x"), ""))),
        Err(NotifyError::TooMany)
    );
    // A test origin admits only endpoints beneath it.
    let local = NotificationService::new(
        &n,
        &w.reminders,
        &w.entropy,
        5,
        Some("http://127.0.0.1:8799"),
    );
    assert!(block_on(local.register(&other, sub("http://127.0.0.1:8799/push/1", ""))).is_ok());
    // And the device's JSON never includes its endpoint or owner.
    let json = serde_json::to_string(&a).unwrap();
    assert!(!json.contains("apple") && !json.contains("owner"));
}

#[test]
fn test_notifications_are_rate_limited_and_honest_without_devices_or_keys() {
    let w = World::new();
    let n = w.repo();
    let u = user("u");
    let s = svc(&w, &n, 60_000);
    block_on(s.test(&u)).unwrap();
    assert_eq!(block_on(s.test(&u)), Err(NotifyError::Busy));
    // No devices: logged as such, nothing sent.
    block_on(dispatch_pending(
        &n,
        &w.reminders,
        &w.entropy,
        true,
        60_000,
        50,
    ))
    .unwrap();
    assert_eq!(w.reminders.notices.borrow()[0].state, notice::NO_DEVICES);
    // Not configured: logged as such, even with a device.
    block_on(s.register(&u, sub(PHONE, ""))).unwrap();
    block_on(svc(&w, &n, 90_000).test(&u)).unwrap();
    block_on(dispatch_pending(
        &n,
        &w.reminders,
        &w.entropy,
        false,
        90_000,
        50,
    ))
    .unwrap();
    assert_eq!(
        w.reminders.notices.borrow()[1].state,
        notice::NOT_CONFIGURED
    );
    assert!(n.deliveries.borrow().is_empty());
    // Far too late (the scheduler was down): stale, not sent.
    block_on(svc(&w, &n, 120_000).test(&u)).unwrap();
    block_on(dispatch_pending(
        &n,
        &w.reminders,
        &w.entropy,
        true,
        120_000 + STALE_MS + 1,
        50,
    ))
    .unwrap();
    assert_eq!(w.reminders.notices.borrow()[2].state, notice::STALE);
    // A test that does go out shows as one.
    block_on(svc(&w, &n, 150_000).test(&u)).unwrap();
    block_on(dispatch_pending(
        &n,
        &w.reminders,
        &w.entropy,
        true,
        150_000,
        50,
    ))
    .unwrap();
    block_on(send_due(&n, &w.push, &w.vapid, None, 150_000, 50)).unwrap();
    let shown = block_on(svc(&w, &n, 150_001).claim(&u, PHONE)).unwrap();
    assert_eq!(shown[0].title, "Directory test notification");
    assert_eq!(shown[0].reminder_id, None);
    // Old rows are pruned.
    block_on(n.prune(150_000)).unwrap();
    assert_eq!(w.reminders.notices.borrow().len(), 1);
}

#[test]
fn purging_a_reminder_removes_its_notices() {
    let w = World::new();
    let n = w.repo();
    let u = user("u");
    let r = block_on(w.rem(at("2026-11-01", 0)).create(
        &u,
        new(|v| {
            v["due_date"] = "2026-11-02".into();
            v["due_time"] = "08:00".into();
        }),
    ))
    .unwrap();
    tick(&w, &n, at("2026-11-02", 8 * 60));
    assert_eq!(w.reminders.notices.borrow().len(), 1);
    let s = w.rem(at("2026-11-02", 9 * 60));
    block_on(s.delete(&u, &r.id)).unwrap();
    block_on(s.delete(&u, &r.id)).unwrap();
    assert!(w.reminders.notices.borrow().is_empty());
}
