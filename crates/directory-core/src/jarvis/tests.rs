use super::*;
use crate::calendar::memory::MemoryEvents;
use crate::calendar::{CalendarService, NewEvent};
use crate::entropy::testing::CountingEntropy;
use crate::habits::memory::MemoryHabits;
use crate::habits::{HabitService, NewHabit};
use crate::identity::AuthMethod;
use crate::inbox::memory::MemoryInbox;
use crate::journal::testing::MemoryJournal;
use crate::links::testing::MemoryLinks;
use crate::notes::testing::MemoryNotes;
use crate::projects::memory::MemoryProjects;
use crate::search::{Candidate, SearchKind, SearchSource};
use crate::tasks::memory::MemoryTasks;
use crate::test_util::block_on;
use std::cell::RefCell;

/// Test search: in-memory task titles only.
impl SearchSource for MemoryTasks {
    async fn candidates(
        &self,
        owner_id: &str,
        kind: SearchKind,
        pattern: &str,
        _: u32,
    ) -> Result<Vec<Candidate>, String> {
        if kind != SearchKind::Task {
            return Ok(vec![]);
        }
        let needle = pattern.trim_matches('%').to_lowercase();
        Ok(self
            .0
            .borrow()
            .iter()
            .filter(|t| t.owner_id == owner_id && t.title.to_lowercase().contains(&needle))
            .map(|t| Candidate {
                kind,
                id: t.id.clone(),
                title: t.title.clone(),
                body: t.notes.clone(),
                updated_at: t.updated_at,
                date: None,
            })
            .collect())
    }
}

#[derive(Default)]
struct MemoryPlans(RefCell<Vec<Plan>>);

impl JarvisRepository for MemoryPlans {
    async fn insert(&self, plan: &Plan) -> Result<(), String> {
        self.0.borrow_mut().push(plan.clone());
        Ok(())
    }
    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Plan>, String> {
        Ok(self
            .0
            .borrow()
            .iter()
            .find(|p| p.owner_id == owner_id && p.id == id)
            .cloned())
    }
    async fn claim(&self, owner_id: &str, id: &str, now: i64) -> Result<bool, String> {
        let mut all = self.0.borrow_mut();
        match all.iter_mut().find(|p| {
            p.owner_id == owner_id
                && p.id == id
                && p.status == PlanStatus::Pending
                && p.expires_at > now
        }) {
            Some(p) => {
                p.status = PlanStatus::Running;
                Ok(true)
            }
            None => Ok(false),
        }
    }
    async fn finish(
        &self,
        owner_id: &str,
        id: &str,
        status: PlanStatus,
        results: &[ActionResult],
    ) -> Result<(), String> {
        for p in self
            .0
            .borrow_mut()
            .iter_mut()
            .filter(|p| p.owner_id == owner_id && p.id == id)
        {
            p.status = status;
            p.results = results.to_vec();
        }
        Ok(())
    }
    async fn cancel(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        let mut all = self.0.borrow_mut();
        match all
            .iter_mut()
            .find(|p| p.owner_id == owner_id && p.id == id && p.status == PlanStatus::Pending)
        {
            Some(p) => {
                p.status = PlanStatus::Cancelled;
                Ok(true)
            }
            None => Ok(false),
        }
    }
    async fn history(&self, owner_id: &str, limit: u32) -> Result<Vec<Plan>, String> {
        let mut v: Vec<_> = self
            .0
            .borrow()
            .iter()
            .filter(|p| p.owner_id == owner_id)
            .cloned()
            .collect();
        v.reverse();
        v.truncate(limit as usize);
        Ok(v)
    }
}

const TODAY: &str = "2026-10-21";

fn me() -> Principal {
    Principal {
        user_id: "u1".into(),
        method: AuthMethod::Session,
    }
}

#[derive(Default)]
struct F {
    plans: MemoryPlans,
    tasks: MemoryTasks,
    notes: MemoryNotes,
    journal: MemoryJournal,
    inbox: MemoryInbox,
    projects: MemoryProjects,
    events: MemoryEvents,
    habits: MemoryHabits,
    links: MemoryLinks,
    e: CountingEntropy,
}

impl CommandTargets for F {
    type Tasks = MemoryTasks;
    type Notes = MemoryNotes;
    type Journal = MemoryJournal;
    type Inbox = MemoryInbox;
    type Search = MemoryTasks;
    type Projects = MemoryProjects;
    type Events = MemoryEvents;
    type Habits = MemoryHabits;
    type Links = MemoryLinks;
    fn links(&self) -> &MemoryLinks {
        &self.links
    }
    fn events(&self) -> &MemoryEvents {
        &self.events
    }
    fn habits(&self) -> &MemoryHabits {
        &self.habits
    }
    fn tasks(&self) -> &MemoryTasks {
        &self.tasks
    }
    fn notes(&self) -> &MemoryNotes {
        &self.notes
    }
    fn journal(&self) -> &MemoryJournal {
        &self.journal
    }
    fn inbox(&self) -> &MemoryInbox {
        &self.inbox
    }
    fn search(&self) -> &MemoryTasks {
        &self.tasks
    }
    fn projects(&self) -> &MemoryProjects {
        &self.projects
    }
}

impl F {
    fn svc(&self, now: i64) -> JarvisService<'_, MemoryPlans, CountingEntropy> {
        JarvisService::new(&self.plans, &self.e, now)
    }
    fn plan(&self, input: &str) -> Result<Plan, JarvisError> {
        self.plan_as(&me(), input, 1_000)
    }
    fn plan_as(&self, p: &Principal, input: &str, now: i64) -> Result<Plan, JarvisError> {
        let req = PlanRequest {
            input: input.into(),
            today: TODAY.into(),
        };
        block_on(self.svc(now).plan(p, &DeterministicParser, req, self))
    }
    fn execute(&self, id: &str, now: i64) -> Result<Plan, JarvisError> {
        block_on(self.svc(now).execute(&me(), id, self))
    }
}

#[test]
fn write_plans_wait_for_approval_and_run_once() {
    let f = F::default();
    let plan = f
        .plan("add task Call the dentist due friday; journal booked the dentist")
        .unwrap();
    assert!(plan.requires_approval);
    assert_eq!(plan.status, PlanStatus::Pending);
    assert_eq!(
        plan.actions[0].summary,
        "Create task “Call the dentist” due 2026-10-23"
    );
    assert!(
        f.tasks.0.borrow().is_empty(),
        "nothing happens before approval"
    );

    let done = f.execute(&plan.id, 2_000).unwrap();
    assert_eq!(done.status, PlanStatus::Executed);
    assert!(done.results.iter().all(|r| r.ok));
    assert_eq!(
        done.results[0].entity.as_ref().unwrap().kind,
        EntityKind::Task
    );
    assert_eq!(
        f.tasks.0.borrow()[0].due_date.as_deref(),
        Some("2026-10-23")
    );

    assert_eq!(
        f.execute(&plan.id, 3_000).unwrap_err(),
        JarvisError::NotPending(PlanStatus::Executed)
    );
    assert_eq!(f.tasks.0.borrow().len(), 1, "no double execution");
}

#[test]
fn read_only_plans_run_immediately() {
    let f = F::default();
    let empty = f.plan("what's open?").unwrap();
    assert!(!empty.requires_approval);
    assert_eq!(
        (empty.status, empty.results[0].message.as_str()),
        (PlanStatus::Executed, "Nothing open. Nice.")
    );

    f.execute(
        &f.plan("add task A due tomorrow; add task B").unwrap().id,
        1_500,
    )
    .unwrap();
    let list = f.plan("list tasks").unwrap();
    assert_eq!(list.results[0].message, "2 open: A (due 2026-10-22); B.");
    assert_eq!(block_on(f.svc(2_000).history(&me())).unwrap().len(), 3);
}

#[test]
fn complete_resolves_by_title() {
    let f = F::default();
    f.execute(
        &f.plan("add task Pay rent; add task Pay phone bill; add task Water plants")
            .unwrap()
            .id,
        1_100,
    )
    .unwrap();

    let plan = f.plan("done water").unwrap();
    assert_eq!(plan.actions[0].summary, "Complete task “Water plants”");
    f.execute(&plan.id, 1_200).unwrap();
    assert!(f
        .tasks
        .0
        .borrow()
        .iter()
        .any(|t| t.title == "Water plants" && t.status == Status::Done));

    match f.plan("complete pay") {
        Err(JarvisError::Unresolved(msg)) => assert!(msg.contains("matches 2 open tasks"), "{msg}"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        f.plan("complete pay rent").unwrap().actions[0].command,
        Command::CompleteTask {
            task_id: Some(_),
            ..
        }
    ));
    assert!(matches!(
        f.plan("complete taxes"),
        Err(JarvisError::Unresolved(_))
    ));
}

#[test]
fn projects_create_list_and_receive_tasks() {
    let f = F::default();
    let created = f.plan("new project Garden; new project Garage").unwrap();
    assert_eq!(created.actions[0].summary, "Create project “Garden”");
    f.execute(&created.id, 1_100).unwrap();
    // The task repository checks project ownership through its own table.
    for pr in f.projects.0.borrow().iter() {
        f.tasks
            .1
            .borrow_mut()
            .push((pr.owner_id.clone(), pr.id.clone()));
    }

    let plan = f
        .plan("add task plant tomatoes in project garden due friday")
        .unwrap();
    assert_eq!(
        plan.actions[0].summary,
        "Create task “plant tomatoes” in project “Garden” due 2026-10-23"
    );
    let done = f.execute(&plan.id, 1_200).unwrap();
    assert_eq!(
        done.results[0].message,
        "Created task “plant tomatoes” in “Garden”."
    );
    let garden_id = f.projects.0.borrow()[0].id.clone();
    assert!(f.tasks.0.borrow().iter().any(
        |t| t.title == "plant tomatoes" && t.project_id.as_deref() == Some(garden_id.as_str())
    ));

    // Read-only: runs at once.
    let list = f.plan("list projects").unwrap();
    assert_eq!(list.status, PlanStatus::Executed);
    assert_eq!(
        list.results[0].message,
        "2 active: Garden (0/1 done); Garage (0/0 done)."
    );

    match f.plan("add task x in project ga") {
        Err(JarvisError::Unresolved(msg)) => {
            assert!(msg.contains("matches 2 active projects"), "{msg}")
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        f.plan("add task x to project shed"),
        Err(JarvisError::Unresolved(m)) if m == "No active project matches “shed”."
    ));

    // A project created earlier in the same plan is resolved when the plan runs
    // (verified end to end against D1 by the smoke test).
    let forward = f
        .plan("new project Shed; add task paint door in project shed")
        .unwrap();
    assert_eq!(
        forward.actions[1].summary,
        "Create task “paint door” in project “Shed”"
    );
    assert!(matches!(
        &forward.actions[1].command,
        Command::CreateTask { project: Some(n), project_id: None, .. } if n == "Shed"
    ));
}

#[test]
fn failures_stop_the_plan() {
    let f = F::default();
    let long = "x".repeat(tasks::MAX_TITLE + 1);
    let plan = f
        .plan(&format!("add task {long}; note never written"))
        .unwrap();
    let ran = f.execute(&plan.id, 1_500).unwrap();
    assert_eq!(ran.status, PlanStatus::Failed);
    assert_eq!(ran.results.len(), 1, "later steps skipped");
    assert!(!ran.results[0].ok && ran.results[0].message.contains("title"));
    assert!(f.notes.0.borrow().is_empty());
}

#[test]
fn cancel_expiry_and_ownership() {
    let f = F::default();
    let a = f.plan("capture solar panels").unwrap();
    assert_eq!(
        block_on(f.svc(1_500).cancel(&me(), &a.id)).unwrap().status,
        PlanStatus::Cancelled
    );
    assert_eq!(
        f.execute(&a.id, 1_600).unwrap_err(),
        JarvisError::NotPending(PlanStatus::Cancelled)
    );

    let b = f.plan("capture heat pump").unwrap();
    assert_eq!(
        f.execute(&b.id, 1_000 + PLAN_TTL_MS).unwrap_err(),
        JarvisError::NotPending(PlanStatus::Expired)
    );
    assert!(f.inbox.0.borrow().is_empty());

    let c = f.plan("capture insulation").unwrap();
    let other = Principal {
        user_id: "u2".into(),
        method: AuthMethod::Session,
    };
    assert_eq!(
        block_on(f.svc(1_500).execute(&other, &c.id, &f)).unwrap_err(),
        JarvisError::NotFound
    );
    assert!(f.plan_as(&other, "list tasks", 1_500).unwrap().results[0]
        .message
        .starts_with("Nothing open"));
}

#[test]
fn input_validation() {
    let f = F::default();
    assert!(matches!(
        f.plan("dance"),
        Err(JarvisError::Parse(parser::ParseError::Unknown(_)))
    ));
    assert!(matches!(
        f.plan(&"a".repeat(MAX_INPUT + 1)),
        Err(JarvisError::Invalid("input", _))
    ));
    let bad_today = PlanRequest {
        input: "list tasks".into(),
        today: "yesterday".into(),
    };
    assert!(matches!(
        block_on(f.svc(1).plan(&me(), &DeterministicParser, bad_today, &f)),
        Err(JarvisError::Invalid("today", _))
    ));
}

#[test]
fn find_is_read_only_and_links_the_top_hit() {
    let f = F::default();
    f.execute(
        &f.plan("add task Renew passport; add task Pack bags")
            .unwrap()
            .id,
        1_100,
    )
    .unwrap();
    let plan = f.plan("find passport").unwrap();
    assert!(!plan.requires_approval);
    assert_eq!(plan.results[0].message, "1 found: Renew passport (task).");
    assert_eq!(
        plan.results[0].entity.as_ref().unwrap().kind,
        EntityKind::Task
    );
    assert_eq!(
        f.plan("find xylophone").unwrap().results[0].message,
        "Nothing matches “xylophone”."
    );
}

#[test]
fn agenda_and_habit_check_ins() {
    // Noon UTC on TODAY: habit check-ins can't be in the server's future.
    const NOW: i64 = 1_792_584_000_000;
    let f = F::default();
    let habit = block_on(HabitService::new(&f.habits, &f.e, NOW).create(
        &me(),
        NewHabit {
            name: "Stretch".into(),
            ..Default::default()
        },
    ))
    .unwrap();
    block_on(HabitService::new(&f.habits, &f.e, NOW).set_check(
        &me(),
        &habit.id,
        "2026-10-20",
        true,
    ))
    .unwrap();
    let cal = CalendarService::new(&f.events, &f.e, NOW);
    for (title, time) in [("Standup", Some("09:30")), ("Trip", None)] {
        block_on(cal.create(
            &me(),
            NewEvent {
                title: title.into(),
                start_date: TODAY.into(),
                start_time: time.map(Into::into),
                ..Default::default()
            },
        ))
        .unwrap();
    }
    let add = f
        .plan_as(&me(), "add task pay rent due today", NOW)
        .unwrap();
    f.execute(&add.id, NOW).unwrap();

    let agenda = f.plan_as(&me(), "agenda", NOW).unwrap();
    assert_eq!(agenda.status, PlanStatus::Executed);
    assert_eq!(
        agenda.results[0].message,
        "Wednesday 2026-10-21 — Events: all day Trip; 09:30 Standup. Due: pay rent. Habits left: Stretch."
    );

    let check = f.plan_as(&me(), "check in STRETCH", NOW).unwrap();
    assert_eq!(
        check.actions[0].summary,
        "Check in “Stretch” for 2026-10-21"
    );
    let done = f.execute(&check.id, NOW).unwrap();
    assert_eq!(
        done.results[0].message,
        "Checked in “Stretch” for 2026-10-21 (2-day streak)."
    );
    let after = f.plan_as(&me(), "what's on today", NOW).unwrap();
    assert!(!after.results[0].message.contains("Habits left"));

    assert!(matches!(
        f.plan_as(&me(), "check in nope", NOW),
        Err(JarvisError::Unresolved(m)) if m == "No habit matches “nope”."
    ));
    // Still-open tasks due earlier count as overdue relative to the day asked about.
    let later = f.plan_as(&me(), "agenda 2026-12-25", NOW).unwrap();
    assert_eq!(
        later.results[0].message,
        "Friday 2026-12-25 — Overdue: 1 task. Habits left: Stretch."
    );
}

#[test]
fn automations_run_approved_commands_when_events_happen() {
    use crate::automations::memory::MemoryAutomations;
    use crate::automations::{AutomationService, Event, NewRule, Trigger};
    const NOW: i64 = 1_792_584_000_000;
    let f = F::default();
    let rules = MemoryAutomations::default();
    let auto = AutomationService::new(&rules, &f.e, NOW);
    let rule = |name: &str, trigger: Trigger, command: &str| {
        block_on(auto.create(
            &me(),
            NewRule {
                name: name.into(),
                trigger,
                command: command.into(),
                enabled: None,
            },
        ))
        .unwrap()
    };
    let log = rule(
        "Log wins",
        Trigger::TaskCompleted { project_id: None },
        "journal Finished {title}",
    );
    let broken = rule(
        "Follow-up",
        Trigger::TaskCompleted { project_id: None },
        "add task tidy up after {title} in project nowhere",
    );
    let off = rule(
        "Off",
        Trigger::TaskCompleted { project_id: None },
        "capture should not run",
    );
    block_on(auto.update(
        &me(),
        &off.id,
        crate::automations::RulePatch {
            enabled: Some(false),
            ..Default::default()
        },
    ))
    .unwrap();
    rule(
        "Habit only",
        Trigger::HabitChecked { habit_id: None },
        "capture habit {title}",
    );

    let event = Event::TaskCompleted {
        title: "Pay rent".into(),
        project_id: None,
        project_name: None,
        date: TODAY.into(),
    };
    let ran = block_on(auto.fire(&me(), &event, &f.svc(NOW), &f));
    assert_eq!(ran, 2, "enabled task rules only");

    let entry = block_on(f.journal.get("u1", TODAY)).unwrap().unwrap();
    assert_eq!(entry.body, "Finished Pay rent");
    assert!(
        f.inbox.0.borrow().is_empty(),
        "disabled and habit rules didn't run"
    );
    let stored = |id: &str| {
        rules
            .0
            .borrow()
            .iter()
            .find(|r| r.id == id)
            .cloned()
            .unwrap()
    };
    assert_eq!(stored(&log.id).last_ok, Some(true));
    assert_eq!(
        stored(&broken.id).last_message.as_deref(),
        Some("No active project matches “nowhere”.")
    );
    assert_eq!(stored(&broken.id).last_ok, Some(false));
    assert_eq!(stored(&off.id).last_run_at, None);
}

#[test]
fn link_commands_resolve_names_and_link_through_the_links_module() {
    let f = F::default();
    let made = f.plan("add task book flights; note trip ideas").unwrap();
    f.execute(&made.id, 1_100).unwrap();
    let task = f.tasks.0.borrow()[0].clone();
    let note = f.notes.0.borrow()[0].clone();
    // The links fake knows which entities exist through its own map.
    for (kind, id, title) in [
        (EntityKind::Task, &task.id, "book flights"),
        (EntityKind::Note, &note.id, "trip ideas"),
    ] {
        f.links.entities.borrow_mut().insert(
            (
                "u1".into(),
                EntityRef {
                    kind,
                    id: id.clone(),
                },
            ),
            title.into(),
        );
    }

    let plan = f.plan("link note trip to task book flights").unwrap();
    assert_eq!(
        plan.actions[0].summary,
        "Link note “trip ideas” with task “book flights”"
    );
    let done = f.execute(&plan.id, 1_200).unwrap();
    assert!(done.results[0].ok, "{:?}", done.results);
    assert_eq!(f.links.links.borrow().len(), 1);

    assert!(matches!(
        f.plan("link note nothing like it to task book flights"),
        Err(JarvisError::Unresolved(m)) if m.starts_with("No note matches")
    ));
}

#[test]
fn task_details_updates_and_planner_commands() {
    let f = F::default();
    let plan = f
        .plan("add task file taxes high priority due friday; add task stretch takes 10 minutes; add task read low priority takes 1 hour")
        .unwrap();
    assert_eq!(
        plan.actions[0].summary,
        "Create task “file taxes” due 2026-10-23, high priority"
    );
    assert_eq!(plan.actions[1].summary, "Create task “stretch”, 10 min");
    f.execute(&plan.id, 1_100).unwrap();
    {
        let tasks = f.tasks.0.borrow();
        assert_eq!(tasks[0].priority, Priority::High);
        assert_eq!(tasks[1].estimate_minutes, Some(10));
        assert_eq!(
            (tasks[2].priority, tasks[2].estimate_minutes),
            (Priority::Low, Some(60))
        );
    }

    // Priorities and suggestions are read-only: they run at once, change nothing.
    let top = f.plan("What should I do today?").unwrap();
    assert!(!top.requires_approval);
    assert_eq!(
        top.results[0].message,
        "Top 3 for Wednesday 2026-10-21: 1. file taxes (due in 2 days, high priority). 2. read (low priority). 3. stretch."
    );
    let fit = f.plan("I have 45 minutes. What should I do?").unwrap();
    assert_eq!(
        fit.results[0].message,
        "For 45 min on Wednesday 2026-10-21: 1. file taxes (about 30 min, no estimate, due in 2 days, high priority). 2. stretch (10 min). That's 40 min in all."
    );
    assert_eq!(
        f.plan("what can I do in 5 min").unwrap().results[0].message,
        "Nothing open fits in 5 min; the shortest takes longer."
    );

    // Updates resolve the task by name and wait for approval.
    let up = f.plan("move taxes to monday").unwrap();
    assert!(up.requires_approval);
    assert_eq!(
        up.actions[0].summary,
        "Update task “file taxes”: due 2026-10-26"
    );
    let done = f.execute(&up.id, 1_200).unwrap();
    assert_eq!(
        done.results[0].message,
        "Updated “file taxes”: due 2026-10-26."
    );
    let id = f.plan("set priority of stretch to medium").unwrap().id;
    f.execute(&id, 1_300).unwrap();
    let id = f.plan("estimate taxes at 2 hours").unwrap().id;
    f.execute(&id, 1_400).unwrap();
    let id = f.plan("clear the due date of taxes").unwrap().id;
    f.execute(&id, 1_500).unwrap();
    let tasks = f.tasks.0.borrow();
    assert_eq!(tasks[1].priority, Priority::Medium);
    assert_eq!(
        (tasks[0].estimate_minutes, tasks[0].due_date.as_deref()),
        (Some(120), None)
    );
    drop(tasks);
    assert!(matches!(
        f.plan("move nothing-like-this to friday"),
        Err(JarvisError::Unresolved(_))
    ));
}

#[test]
fn planning_can_be_limited_to_one_project() {
    let f = F::default();
    f.execute(&f.plan("new project Garden").unwrap().id, 1_050)
        .unwrap();
    for pr in f.projects.0.borrow().iter() {
        f.tasks
            .1
            .borrow_mut()
            .push((pr.owner_id.clone(), pr.id.clone()));
    }
    let setup = f
        .plan("add task plant tomatoes in project garden takes 20 minutes; add task file taxes high priority")
        .unwrap();
    f.execute(&setup.id, 1_100).unwrap();

    let top = f.plan("What should I do for project garden?").unwrap();
    assert_eq!(
        top.actions[0].summary,
        "Show top priorities in “Garden” for 2026-10-21"
    );
    assert_eq!(
        top.results[0].message,
        "Top 1 in “Garden” for Wednesday 2026-10-21: 1. plant tomatoes."
    );
    let fit = f.plan("I have 30 minutes in project Garden").unwrap();
    assert_eq!(
        fit.results[0].message,
        "For 30 min in “Garden” on Wednesday 2026-10-21: 1. plant tomatoes (20 min). That's 20 min in all."
    );
    match f.plan("priorities for project shed") {
        Err(JarvisError::Unresolved(m)) => assert!(m.contains("No active project matches"), "{m}"),
        other => panic!("{other:?}"),
    }
}
