use super::*;
use crate::entropy::testing::CountingEntropy;
use crate::identity::AuthMethod;
use crate::inbox::memory::MemoryInbox;
use crate::journal::testing::MemoryJournal;
use crate::notes::testing::MemoryNotes;
use crate::tasks::memory::MemoryTasks;
use crate::test_util::block_on;
use std::cell::RefCell;

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
    e: CountingEntropy,
}

impl F {
    fn targets(&self) -> CommandTargets<'_, MemoryTasks, MemoryNotes, MemoryJournal, MemoryInbox> {
        CommandTargets {
            tasks: &self.tasks,
            notes: &self.notes,
            journal: &self.journal,
            inbox: &self.inbox,
        }
    }
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
        block_on(
            self.svc(now)
                .plan(p, &DeterministicParser, req, self.targets()),
        )
    }
    fn execute(&self, id: &str, now: i64) -> Result<Plan, JarvisError> {
        block_on(self.svc(now).execute(&me(), id, self.targets()))
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
        Err(JarvisError::Unresolved(msg)) => assert!(msg.contains("matches 2 tasks"), "{msg}"),
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
        block_on(f.svc(1_500).execute(&other, &c.id, f.targets())).unwrap_err(),
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
        block_on(
            f.svc(1)
                .plan(&me(), &DeterministicParser, bad_today, f.targets())
        ),
        Err(JarvisError::Invalid("today", _))
    ));
}
