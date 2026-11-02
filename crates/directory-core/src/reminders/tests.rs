use super::memory::MemoryReminders;
use super::model::*;
use super::recurrence::{Freq, Rule};
use super::schedule::tests::at;
use super::service::*;
use super::zone::testing::FakeZones;
use crate::entropy::testing::CountingEntropy;
use crate::identity::{AuthMethod, Principal};
use crate::test_util::block_on;

fn user(id: &str) -> Principal {
    Principal {
        user_id: id.into(),
        method: AuthMethod::Session,
    }
}

struct World {
    repo: MemoryReminders,
    entropy: CountingEntropy,
}

impl World {
    fn new() -> Self {
        let repo = MemoryReminders::default();
        repo.categories
            .borrow_mut()
            .push(("u".into(), "c".repeat(32)));
        repo.categories
            .borrow_mut()
            .push(("other".into(), "d".repeat(32)));
        repo.tasks
            .borrow_mut()
            .push(("u".into(), "t".repeat(32).replace('t', "a")));
        Self {
            repo,
            entropy: CountingEntropy::default(),
        }
    }

    fn at(&self, now: i64) -> ReminderService<'_, MemoryReminders, CountingEntropy, FakeZones> {
        ReminderService::new(&self.repo, &self.entropy, &FakeZones, now)
    }
}

fn task_id() -> String {
    "a".repeat(32)
}

fn new(title: &str, f: impl FnOnce(&mut serde_json::Value)) -> NewReminder {
    let mut v = serde_json::json!({ "title": title, "timezone": "Test/Toronto" });
    f(&mut v);
    serde_json::from_value(v).unwrap()
}

fn daily(due: &str, time: &str, missed: &str) -> NewReminder {
    new("Daily", |v| {
        v["due_date"] = due.into();
        v["due_time"] = time.into();
        v["missed"] = missed.into();
        v["recurrence"] = serde_json::json!({ "freq": "daily" });
    })
}

#[test]
fn creates_with_defaults_and_validates() {
    let w = World::new();
    let s = w.at(at("2026-11-01", 600));
    let p = user("u");
    let plain = block_on(s.create(&p, NewReminder::titled("Buy milk"))).unwrap();
    assert_eq!(
        (plain.due_date.as_deref(), plain.alerts.len(), plain.wake_at),
        (None, 0, None)
    );
    assert_eq!(plain.timezone, "UTC");

    let timed = block_on(s.create(
        &p,
        new("Submit assignment", |v| {
            v["due_date"] = "2026-11-02".into();
            v["due_time"] = "20:00".into();
        }),
    ))
    .unwrap();
    assert_eq!(timed.alerts, [Alert::Before { minutes: 0 }]);
    assert_eq!(timed.wake_at, Some(at("2026-11-02", 1200)));
    assert_eq!(timed.due_at, timed.wake_at);

    let bad = |f: fn(&mut serde_json::Value)| block_on(s.create(&p, new("x", f))).unwrap_err();
    assert_eq!(
        bad(|v| v["due_time"] = "08:00".into()),
        ReminderError::Invalid("due_time", "a time needs a date")
    );
    assert!(matches!(
        bad(|v| v["timezone"] = "Nowhere/Zone".into()),
        ReminderError::Invalid("timezone", _)
    ));
    assert!(matches!(
        bad(|v| v["recurrence"] = serde_json::json!({ "freq": "daily" })),
        ReminderError::Invalid("recurrence", _)
    ));
    // Another owner's category, an unknown task, and an unknown list are refused.
    assert!(matches!(
        bad(|v| v["category_ids"] = serde_json::json!(["d".repeat(32)])),
        ReminderError::Invalid("category_ids", _)
    ));
    assert!(matches!(
        bad(|v| v["task_id"] = "b".repeat(32).into()),
        ReminderError::Invalid("task_id", _)
    ));
    assert!(matches!(
        bad(|v| v["list_id"] = "b".repeat(32).into()),
        ReminderError::Invalid("list_id", _)
    ));
    let with_cat = block_on(s.create(
        &p,
        new("Read", |v| {
            v["category_ids"] = serde_json::json!(["c".repeat(32), "c".repeat(32)])
        }),
    ))
    .unwrap();
    assert_eq!(with_cat.category_ids.len(), 1);

    // A replayed offline create with the same id is reported, not duplicated.
    let id = "e".repeat(32);
    let again = || block_on(s.create(&p, new("Offline", |v| v["id"] = id.clone().into())));
    again().unwrap();
    assert_eq!(again().unwrap_err(), ReminderError::Exists);
}

#[test]
fn owners_never_see_each_others_reminders() {
    let w = World::new();
    let s = w.at(1);
    let mine = block_on(s.create(&user("u"), NewReminder::titled("Mine"))).unwrap();
    let them = user("other");
    assert!(block_on(s.list(&them)).unwrap().is_empty());
    assert_eq!(
        block_on(s.get(&them, &mine.id)).unwrap_err(),
        ReminderError::NotFound
    );
    assert_eq!(
        block_on(s.complete(&them, &mine.id, None)).unwrap_err(),
        ReminderError::NotFound
    );
    assert_eq!(
        block_on(s.delete(&them, &mine.id)).unwrap_err(),
        ReminderError::NotFound
    );
    let patch = ReminderPatch {
        title: Some("Hijacked".into()),
        ..Default::default()
    };
    assert_eq!(
        block_on(s.update(&them, &mine.id, patch)).unwrap_err(),
        ReminderError::NotFound
    );
}

#[test]
fn field_level_merge_keeps_both_sides_or_reports_the_clash() {
    let w = World::new();
    let p = user("u");
    let r = block_on(w.at(1).create(&p, NewReminder::titled("Dentist"))).unwrap();
    // Device A renames it (version 2).
    let a = ReminderPatch {
        base_version: Some(1),
        title: Some("Dentist at 3".into()),
        ..Default::default()
    };
    block_on(w.at(2).update(&p, &r.id, a)).unwrap();
    // Device B, still on version 1, flags it: no overlap, both kept.
    let b = ReminderPatch {
        base_version: Some(1),
        flagged: Some(true),
        ..Default::default()
    };
    let merged = block_on(w.at(3).update(&p, &r.id, b)).unwrap();
    assert_eq!(
        (merged.title.as_str(), merged.flagged, merged.version),
        ("Dentist at 3", true, 3)
    );
    // Device B also renamed it from version 1: that clashes with A's title.
    let c = ReminderPatch {
        base_version: Some(1),
        title: Some("Dentist Monday".into()),
        ..Default::default()
    };
    match block_on(w.at(4).update(&p, &r.id, c)).unwrap_err() {
        ReminderError::Conflict(current, fields) => {
            assert_eq!(fields, ["title"]);
            assert_eq!(current.title, "Dentist at 3");
        }
        e => panic!("{e:?}"),
    }
    // The same value as the server's isn't a conflict (a replay).
    let same = ReminderPatch {
        base_version: Some(1),
        title: Some("Dentist at 3".into()),
        ..Default::default()
    };
    assert_eq!(
        block_on(w.at(5).update(&p, &r.id, same)).unwrap().version,
        3
    );
}

#[test]
fn completing_is_idempotent_and_an_offline_completion_of_a_missed_occurrence_counts() {
    let w = World::new();
    let p = user("u");
    let r = block_on(
        w.at(at("2026-11-01", 0))
            .create(&p, daily("2026-11-02", "08:00", "one")),
    )
    .unwrap();
    // The scheduler moves past Monday's and Tuesday's while the phone is offline.
    let ran = block_on(run_scheduler(
        &w.repo,
        &w.entropy,
        &FakeZones,
        at("2026-11-04", 9 * 60),
        50,
    ))
    .unwrap();
    assert_eq!(ran[0].scan.missed, ["2026-11-02", "2026-11-03"]);
    assert_eq!(
        block_on(w.at(0).get(&p, &r.id))
            .unwrap()
            .due_date
            .as_deref(),
        Some("2026-11-04")
    );
    // The phone's queued "complete Monday" arrives: Monday is recorded completed, the cursor stays.
    let s = w.at(at("2026-11-04", 10 * 60));
    let after = block_on(s.complete(&p, &r.id, Some("2026-11-02"))).unwrap();
    assert_eq!(after.due_date.as_deref(), Some("2026-11-04"));
    let again = block_on(s.complete(&p, &r.id, Some("2026-11-02"))).unwrap();
    assert_eq!(again.version, after.version);
    let rows = block_on(
        w.repo
            .occurrences("u", Some(&r.id), "2026-11-01", "2026-11-30"),
    )
    .unwrap();
    let states: Vec<(&str, &str)> = rows
        .iter()
        .map(|x| (x.occurrence.as_str(), x.state.as_str()))
        .collect();
    assert_eq!(
        states,
        [("2026-11-02", "completed"), ("2026-11-03", "missed")]
    );
    // Completing the current one twice advances once.
    let done = block_on(s.complete(&p, &r.id, Some("2026-11-04"))).unwrap();
    assert_eq!(done.due_date.as_deref(), Some("2026-11-05"));
    let replay = block_on(s.complete(&p, &r.id, Some("2026-11-04"))).unwrap();
    assert_eq!(replay.due_date.as_deref(), Some("2026-11-05"));
    // And it can be undone, which brings Wednesday back.
    let undone = block_on(s.uncomplete(&p, &r.id, Some("2026-11-04"))).unwrap();
    assert_eq!(undone.due_date.as_deref(), Some("2026-11-04"));
    assert!(block_on(s.uncomplete(&p, &r.id, Some("2026-11-02"))).is_err());
    // With no occurrence named, "undo" means the latest completion.
    block_on(s.complete(&p, &r.id, None)).unwrap();
    let undone = block_on(s.uncomplete(&p, &r.id, None)).unwrap();
    assert_eq!(undone.due_date.as_deref(), Some("2026-11-04"));
    let history = block_on(s.history_of(&p, &r.id)).unwrap();
    assert_eq!(history[0].action, "uncompleted");
    assert_eq!(history.iter().filter(|h| h.action == "missed").count(), 2);
}

#[test]
fn snoozing_applies_to_the_current_occurrence_only() {
    let w = World::new();
    let p = user("u");
    let now = at("2026-11-02", 8 * 60 + 1);
    let r = block_on(
        w.at(at("2026-11-01", 0))
            .create(&p, daily("2026-11-02", "08:00", "one")),
    )
    .unwrap();
    let s = w.at(now);
    let snoozed = block_on(s.snooze(
        &p,
        &r.id,
        Snooze {
            minutes: Some(10),
            ..Default::default()
        },
    ))
    .unwrap();
    assert_eq!(snoozed.snoozed_until, Some(now + 600_000));
    assert_eq!(snoozed.wake_at, snoozed.snoozed_until);
    let tomorrow = block_on(s.snooze(
        &p,
        &r.id,
        Snooze {
            tomorrow: true,
            ..Default::default()
        },
    ))
    .unwrap();
    assert_eq!(tomorrow.snoozed_until, Some(at("2026-11-03", 540)));
    // A notification's action for an occurrence that has passed is refused.
    let stale = Snooze {
        minutes: Some(60),
        occurrence: Some("2026-11-01".into()),
        ..Default::default()
    };
    assert!(matches!(
        block_on(s.snooze(&p, &r.id, stale)).unwrap_err(),
        ReminderError::Stale(..)
    ));
    assert!(block_on(s.snooze(&p, &r.id, Snooze::default())).is_err());
    assert!(block_on(s.snooze(
        &p,
        &r.id,
        Snooze {
            until: Some(now - 1),
            ..Default::default()
        }
    ))
    .is_err());
    // Completing clears the snooze; the next occurrence isn't snoozed.
    let done = block_on(s.complete(&p, &r.id, None)).unwrap();
    assert_eq!(
        (done.due_date.as_deref(), done.snoozed_until),
        (Some("2026-11-03"), None)
    );
    let undated = block_on(s.create(&p, NewReminder::titled("Someday"))).unwrap();
    assert!(block_on(s.snooze(
        &p,
        &undated.id,
        Snooze {
            minutes: Some(10),
            ..Default::default()
        }
    ))
    .is_err());
}

#[test]
fn delete_is_recoverable_until_deleted_again() {
    let w = World::new();
    let p = user("u");
    let s = w.at(at("2026-11-01", 0));
    let r = block_on(s.create(&p, daily("2026-11-02", "08:00", "one"))).unwrap();
    block_on(s.delete(&p, &r.id)).unwrap();
    let gone = block_on(s.get(&p, &r.id)).unwrap();
    assert!(gone.deleted_at.is_some() && gone.wake_at.is_none());
    assert!(block_on(s.update(
        &p,
        &r.id,
        ReminderPatch {
            flagged: Some(true),
            ..Default::default()
        }
    ))
    .is_err());
    let back = block_on(s.restore(&p, &r.id)).unwrap();
    assert!(back.deleted_at.is_none() && back.wake_at.is_some());
    block_on(s.delete(&p, &r.id)).unwrap();
    block_on(s.delete(&p, &r.id)).unwrap();
    assert_eq!(
        block_on(s.get(&p, &r.id)).unwrap_err(),
        ReminderError::NotFound
    );
}

#[test]
fn rescheduling_doesnt_resend_past_alerts_and_re_anchors_the_rule() {
    let w = World::new();
    let p = user("u");
    let r = block_on(
        w.at(at("2026-11-01", 0))
            .create(&p, daily("2026-11-02", "08:00", "one")),
    )
    .unwrap();
    let s = w.at(at("2026-11-03", 12 * 60));
    let patch = ReminderPatch {
        due_date: Some(Some("2026-11-04".into())),
        recurrence: Some(Some(Rule {
            freq: Freq::Weekly,
            interval: 1,
            weekdays: vec![],
            until: None,
            anchor: "1999-01-01".into(),
        })),
        ..Default::default()
    };
    let moved = block_on(s.update(&p, &r.id, patch)).unwrap();
    assert_eq!(moved.recurrence.as_ref().unwrap().anchor, "2026-11-04");
    assert_eq!(moved.wake_at, Some(at("2026-11-04", 480)));
    let turned_off = block_on(s.update(
        &p,
        &r.id,
        ReminderPatch {
            recurrence: Some(None),
            ..Default::default()
        },
    ))
    .unwrap();
    assert!(turned_off.recurrence.is_none());
    let cleared = block_on(s.update(
        &p,
        &r.id,
        ReminderPatch {
            due_date: Some(None),
            ..Default::default()
        },
    ))
    .unwrap();
    assert_eq!((cleared.due_time, cleared.wake_at), (None, None));
}

#[test]
fn agenda_shows_occurrences_and_their_states() {
    let w = World::new();
    let p = user("u");
    let r = block_on(
        w.at(at("2026-11-01", 0))
            .create(&p, daily("2026-11-02", "08:00", "each")),
    )
    .unwrap();
    block_on(
        w.at(at("2026-11-01", 0))
            .create(&p, NewReminder::titled("Undated")),
    )
    .unwrap();
    let s = w.at(at("2026-11-03", 9 * 60));
    block_on(s.complete(&p, &r.id, None)).unwrap();
    let items = block_on(s.agenda(&p, "2026-11-01", "2026-11-05")).unwrap();
    let got: Vec<(&str, &str)> = items.iter().map(|i| (i.date.as_str(), i.state)).collect();
    assert_eq!(
        got,
        [
            ("2026-11-02", "completed"),
            ("2026-11-03", "overdue"),
            ("2026-11-04", "open"),
            ("2026-11-05", "open")
        ]
    );
    assert_eq!(items[1].occurrence_id, format!("{}:2026-11-03", r.id));
    assert!(block_on(s.agenda(&p, "2026-01-01", "2026-12-31")).is_err());
}

#[test]
fn completing_a_linked_task_completes_reminders_that_follow_it() {
    let w = World::new();
    let p = user("u");
    let s = w.at(at("2026-11-01", 0));
    let follows = block_on(s.create(
        &p,
        new("Hand in", |v| {
            v["task_id"] = task_id().into();
            v["complete_with_task"] = true.into();
        }),
    ))
    .unwrap();
    let independent =
        block_on(s.create(&p, new("Celebrate", |v| v["task_id"] = task_id().into()))).unwrap();
    assert_eq!(block_on(s.task_completed(&p, &task_id())).unwrap(), 1);
    assert!(block_on(s.get(&p, &follows.id))
        .unwrap()
        .completed_at
        .is_some());
    assert!(block_on(s.get(&p, &independent.id))
        .unwrap()
        .completed_at
        .is_none());
    // Another owner's task id does nothing to these.
    assert_eq!(
        block_on(s.task_completed(&user("other"), &task_id())).unwrap(),
        0
    );
    // Clearing the link turns auto-complete off.
    let unlinked = block_on(s.update(
        &p,
        &follows.id,
        ReminderPatch {
            task_id: Some(None),
            ..Default::default()
        },
    ))
    .unwrap();
    assert!(!unlinked.complete_with_task);
}

#[test]
fn lists_are_named_uniquely_and_deleting_one_keeps_its_reminders() {
    let w = World::new();
    let p = user("u");
    let s = w.at(1);
    let school = block_on(s.create_list(
        &p,
        NewList {
            name: "School".into(),
            ..Default::default()
        },
    ))
    .unwrap();
    assert!(block_on(s.create_list(
        &p,
        NewList {
            name: "school".into(),
            ..Default::default()
        }
    ))
    .is_err());
    let r = block_on(s.create(
        &p,
        new("Essay", |v| v["list_id"] = school.id.clone().into()),
    ))
    .unwrap();
    assert!(block_on(s.update_list(&user("other"), &school.id, ListPatch::default())).is_err());
    block_on(s.delete_list(&p, &school.id)).unwrap();
    assert_eq!(block_on(s.get(&p, &r.id)).unwrap().list_id, None);
}

#[test]
fn the_scheduler_sends_each_alert_once_per_owner_record() {
    let w = World::new();
    let p = user("u");
    let start = at("2026-11-01", 0);
    block_on(w.at(start).create(&p, new("Call", |v| {
        v["due_date"] = "2026-11-02".into();
        v["due_time"] = "08:00".into();
        v["alerts"] = serde_json::json!([{ "kind": "before", "minutes": 15 }, { "kind": "before", "minutes": 0 }]);
    })))
    .unwrap();
    block_on(
        w.at(start)
            .create(&user("other"), NewReminder::titled("Theirs")),
    )
    .unwrap();
    let run = |m: u32| {
        block_on(run_scheduler(
            &w.repo,
            &w.entropy,
            &FakeZones,
            at("2026-11-02", m),
            50,
        ))
        .unwrap()
    };
    assert!(run(7 * 60).is_empty());
    let first = run(7 * 60 + 46);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].owner_id, "u");
    assert_eq!(first[0].scan.send.as_ref().unwrap().slot, "b15");
    assert!(run(7 * 60 + 47).is_empty());
    assert_eq!(run(8 * 60).pop().unwrap().scan.send.unwrap().slot, "b0");
    assert!(run(9 * 60).is_empty());
}
