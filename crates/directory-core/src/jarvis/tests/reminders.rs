//! Reminder and category commands (D-049): the §10 examples end to end,
//! through planning, approval, execution, and undo.

use super::*;
use crate::categories::CategoryService;
use crate::jarvis::ai::AiBudget;
use crate::jarvis::chat::{Assistant, ChatRequest};
use crate::jarvis::conversation::Screen;
use crate::reminders::zone::testing::FakeZones;
use crate::reminders::{NewReminder, ReminderService};

fn reminders(f: &F) -> ReminderService<'_, MemoryReminders, CountingEntropy, FakeZones> {
    ReminderService::new(&f.reminders, &f.e, &FakeZones, 1_000)
}

fn add_reminder(f: &F, p: &Principal, title: &str, due: Option<&str>) -> String {
    let mut input = NewReminder::titled(title);
    input.due_date = due.map(Into::into);
    input.timezone = Some("UTC".into());
    block_on(reminders(f).create(p, input)).unwrap().id
}

fn only(f: &F) -> crate::reminders::Reminder {
    let all = block_on(reminders(f).list(&me())).unwrap();
    assert_eq!(all.len(), 1, "{all:?}");
    all.into_iter().next().unwrap()
}

fn other() -> Principal {
    Principal {
        user_id: "u2".into(),
        method: AuthMethod::Session,
    }
}

#[test]
fn remind_me_creates_a_reminder_not_a_task_and_undo_removes_it() {
    let f = F::default();
    let plan = f
        .plan("Remind me to submit my assignment tomorrow at 8 PM")
        .unwrap();
    assert!(plan.requires_approval);
    assert_eq!(
        plan.actions[0].summary,
        "Remind you: “submit my assignment” on Thu 2026-10-22 at 20:00"
    );
    assert!(
        plan.actions[0].notes.iter().any(|n| n.contains("UTC")),
        "no zone sent, so it says the time is UTC: {:?}",
        plan.actions[0].notes
    );
    assert!(block_on(reminders(&f).list(&me())).unwrap().is_empty());

    let done = f.execute(&plan.id, 2_000).unwrap();
    assert!(done.results[0].ok, "{:?}", done.results);
    let r = only(&f);
    assert_eq!(
        (
            r.due_date.as_deref(),
            r.due_time.as_deref(),
            r.timezone.as_str()
        ),
        (Some("2026-10-22"), Some("20:00"), "UTC")
    );
    assert!(f.tasks.0.borrow().is_empty(), "not a task");
    assert_eq!(
        done.results[0].item.as_ref().map(|i| i.kind),
        Some(ItemKind::Reminder)
    );

    let undo = f.plan("undo").unwrap();
    assert_eq!(
        undo.actions[0].summary,
        "Delete the reminder “submit my assignment”"
    );
    f.execute(&undo.id, 3_000).unwrap();
    assert!(only(&f).deleted_at.is_some(), "recoverably deleted");
}

#[test]
fn repeating_reminders_keep_their_weekdays() {
    let f = F::default();
    let plan = f
        .plan("Remind me every Monday and Wednesday to review my notes")
        .unwrap();
    f.execute(&plan.id, 2_000).unwrap();
    let r = only(&f);
    assert_eq!(r.title, "review my notes");
    let rule = r.recurrence.expect("repeats");
    assert_eq!(rule.weekdays, vec![0, 2]);
    // TODAY (2026-10-21) is a Wednesday, so it starts today.
    assert_eq!(r.due_date.as_deref(), Some("2026-10-21"));
}

#[test]
fn overdue_list_runs_at_once_and_nudges_gently() {
    let f = F::default();
    add_reminder(&f, &me(), "Renew passport", Some("2026-10-01"));
    add_reminder(&f, &me(), "Call the bank", Some("2026-10-20"));
    add_reminder(&f, &me(), "Plan trip", Some("2026-10-30"));
    add_reminder(&f, &other(), "Someone else's", Some("2026-10-01"));

    let plan = f.plan("Show me my overdue reminders").unwrap();
    assert!(!plan.requires_approval, "reading needs no approval");
    let m = &plan.results[0].message;
    assert!(m.starts_with("2 overdue reminders:"), "{m}");
    assert!(m.contains("Renew passport") && m.contains("Call the bank"));
    assert!(!m.contains("Plan trip") && !m.contains("Someone else"));
    assert!(m.contains("has waited since"), "{m}");

    let empty = f.plan_as(&other(), "what reminders are due today", 1_000);
    let m = &empty.unwrap().results[0].message;
    assert!(
        m.contains("Someone else's") && !m.contains("passport"),
        "{m}"
    );
}

#[test]
fn snooze_and_complete_by_name_with_undo() {
    let f = F::default();
    let assignment = add_reminder(&f, &me(), "Assignment", Some("2026-10-21"));
    let dentist = add_reminder(&f, &me(), "Dentist", Some("2026-10-22"));

    let plan = f
        .plan("Snooze my assignment reminder for one hour")
        .unwrap();
    assert_eq!(plan.actions[0].summary, "Snooze “Assignment” for 1 h");
    f.execute(&plan.id, 2_000).unwrap();
    let r = block_on(reminders(&f).get(&me(), &assignment)).unwrap();
    assert_eq!(r.snoozed_until, Some(2_000 + 60 * 60_000));

    let plan = f.plan("Mark my dentist reminder as complete").unwrap();
    assert_eq!(plan.actions[0].summary, "Complete the reminder “Dentist”");
    f.execute(&plan.id, 3_000).unwrap();
    let r = block_on(reminders(&f).get(&me(), &dentist)).unwrap();
    assert!(r.completed_at.is_some());

    let undo = f.plan("undo").unwrap();
    f.execute(&undo.id, 4_000).unwrap();
    let r = block_on(reminders(&f).get(&me(), &dentist)).unwrap();
    assert!(r.completed_at.is_none(), "undo reopens it");

    // Another user can't reach either reminder.
    assert!(matches!(
        f.plan_as(&other(), "complete the dentist reminder", 5_000),
        Err(JarvisError::Unresolved(_))
    ));
}

#[test]
fn categories_are_created_and_assigned_by_name_and_undo_restores_them() {
    let f = F::default();
    let task = add_task(&f, &me(), "Assignment", None, None);

    let plan = f
        .plan("Add School and Important to my assignment task")
        .unwrap();
    assert_eq!(
        plan.actions[0].summary,
        "Add “School”, “Important” to the task “Assignment”"
    );
    assert!(plan.actions[0].notes[0].starts_with("New categories:"));
    let done = f.execute(&plan.id, 2_000).unwrap();
    assert!(done.results[0].ok, "{:?}", done.results);

    let listing = block_on(CategoryService::new(&f.cats, &f.e, 3_000).list(&me())).unwrap();
    let mut names: Vec<&str> = listing.categories.iter().map(|c| c.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["Important", "School"]);
    assert_eq!(
        listing.tasks.iter().filter(|a| a.task_id == task).count(),
        2
    );

    let undo = f.plan("undo").unwrap();
    f.execute(&undo.id, 4_000).unwrap();
    let listing = block_on(CategoryService::new(&f.cats, &f.e, 5_000).list(&me())).unwrap();
    assert!(listing.tasks.iter().all(|a| a.task_id != task));
    assert_eq!(listing.categories.len(), 2, "categories themselves stay");

    // Removing a category that doesn't exist is a question, not a guess.
    assert!(matches!(
        f.plan("remove Errands from my assignment task"),
        Err(JarvisError::Unresolved(_))
    ));
}

#[test]
fn past_times_are_refused_and_undated_reminders_say_they_wont_notify() {
    let f = F::default();
    let c = ChatRequest {
        text: "remind me to call mom today at 7 am".into(),
        today: TODAY.into(),
        time: Some("08:00".into()),
        zone: Some("Test/Toronto".into()),
        conversation_id: None,
        screen: None,
        skip: vec![],
        undo: None,
    };
    let convs = MemoryChats::default();
    let ai: Assistant<'_, NoModel, NoBudget> = Assistant {
        model: None,
        budget: &NoBudget,
    };
    let r = block_on(f.svc(1_000).chat(&me(), &ai, &convs, c, &f)).unwrap();
    assert_eq!(r.status, "unresolved", "{r:?}");

    let plan = f.plan("remind me to water the plants").unwrap();
    assert!(plan.actions[0].notes[0].contains("won't notify"));
}

#[test]
fn this_project_follows_the_screen_and_the_device_zone_is_kept() {
    let f = F::default();
    f.execute(&f.plan("new project Garden").unwrap().id, 1_100)
        .unwrap();
    let project = f.projects.0.borrow()[0].id.clone();
    f.sync();
    let req = ChatRequest {
        text: "Remind me to follow up on this project in three days".into(),
        today: TODAY.into(),
        time: Some("08:00".into()),
        zone: Some("Test/Toronto".into()),
        conversation_id: None,
        screen: Some(Screen {
            project_id: Some(project.clone()),
            ..Default::default()
        }),
        skip: vec![],
        undo: None,
    };
    let convs = MemoryChats::default();
    let ai: Assistant<'_, NoModel, NoBudget> = Assistant {
        model: None,
        budget: &NoBudget,
    };
    let r = block_on(f.svc(2_000).chat(&me(), &ai, &convs, req, &f)).unwrap();
    assert_eq!(r.status, "plan", "{r:?}");
    let plan = r.plan.unwrap();
    f.execute(&plan.id, 3_000).unwrap();
    let rem = only(&f);
    assert_eq!(rem.project_id.as_deref(), Some(project.as_str()));
    assert_eq!(rem.due_date.as_deref(), Some("2026-10-24"));
    assert_eq!(rem.timezone, "Test/Toronto");

    // Without a project on screen, "this project" is asked about.
    assert!(matches!(
        f.plan("remind me to follow up on this project tomorrow"),
        Err(JarvisError::Unresolved(_))
    ));
}

#[test]
fn deadline_reminders_are_proposed_and_complete_with_their_task() {
    let f = F::default();
    let essay = add_task(&f, &me(), "Essay", None, Some("2026-10-23"));
    add_task(&f, &me(), "Old report", None, Some("2026-10-10"));
    add_task(&f, &me(), "Someday", None, Some("2026-12-01"));
    let covered = add_task(&f, &me(), "Covered", None, Some("2026-10-22"));
    f.sync();
    let mut input = NewReminder::titled("Covered");
    input.due_date = Some("2026-10-22".into());
    input.task_id = Some(covered);
    block_on(reminders(&f).create(&me(), input)).unwrap();

    let plan = f.plan("remind me about my deadlines this week").unwrap();
    assert!(plan.requires_approval, "suggestions wait for approval");
    let titles: Vec<&str> = plan
        .actions
        .iter()
        .map(|a| match &a.command {
            Command::CreateReminder { title, .. } => title.as_str(),
            _ => "?",
        })
        .collect();
    assert_eq!(titles, ["Old report", "Essay"]);
    assert!(plan.actions[0].notes[0].contains("overdue"));
    f.execute(&plan.id, 2_000).unwrap();
    let linked = block_on(reminders(&f).list(&me()))
        .unwrap()
        .into_iter()
        .find(|r| r.task_id.as_deref() == Some(essay.as_str()))
        .unwrap();
    assert!(linked.complete_with_task);
    assert_eq!(linked.due_date.as_deref(), Some("2026-10-23"));
}

#[derive(Default)]
struct MemoryChats(RefCell<Vec<crate::jarvis::conversation::Conversation>>);

impl crate::jarvis::conversation::ConversationRepository for MemoryChats {
    async fn get(
        &self,
        owner_id: &str,
        id: &str,
    ) -> Result<Option<crate::jarvis::conversation::Conversation>, String> {
        Ok(self
            .0
            .borrow()
            .iter()
            .find(|c| c.owner_id == owner_id && c.id == id)
            .cloned())
    }
    async fn put(&self, c: &crate::jarvis::conversation::Conversation) -> Result<(), String> {
        self.0.borrow_mut().retain(|x| x.id != c.id);
        self.0.borrow_mut().push(c.clone());
        Ok(())
    }
}

struct NoBudget;

impl AiBudget for NoBudget {
    async fn allow(&self) -> bool {
        false
    }
}

struct NoModel;

impl crate::jarvis::ai::TextModel for NoModel {
    async fn complete(&self, _: &str, _: &str) -> Result<String, String> {
        Err("no model in these tests".into())
    }
}
