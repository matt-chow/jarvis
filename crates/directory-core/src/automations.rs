//! Automations: owner rules that run a JARVIS command when something happens
//! (DECISIONS D-024). Saving a rule approves its command in advance, so only
//! additive commands are allowed, and actions never trigger further rules.

use crate::entropy::{is_id, new_id, Entropy};
use crate::identity::Principal;
use crate::jarvis::{
    parser, ActionResult, Command, CommandTargets, JarvisRepository, JarvisService, PlanStatus,
};
use crate::validate;
use serde::{Deserialize, Serialize};

pub const MAX_RULES: u32 = 20;
pub const MAX_NAME: usize = 80;
pub const MAX_TEMPLATE: usize = 500;
pub const MAX_STEPS: usize = 3;
const MAX_MESSAGE: usize = 300;
/// Placeholders a template may use.
pub const PLACEHOLDERS: &[&str] = &["{title}", "{project}", "{date}"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    /// A task moved to done (optionally only in one project).
    TaskCompleted {
        #[serde(default)]
        project_id: Option<String>,
    },
    /// A habit was checked in (optionally only one habit).
    HabitChecked {
        #[serde(default)]
        habit_id: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rule {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub name: String,
    pub trigger: Trigger,
    /// JARVIS command text; may contain placeholders.
    pub command: String,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_run_at: Option<i64>,
    pub last_ok: Option<bool>,
    pub last_message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewRule {
    pub name: String,
    pub trigger: Trigger,
    pub command: String,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RulePatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub trigger: Option<Trigger>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// Something that happened, with the owner's local date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    TaskCompleted {
        title: String,
        project_id: Option<String>,
        project_name: Option<String>,
        date: String,
    },
    HabitChecked {
        habit_id: String,
        name: String,
        date: String,
    },
}

impl Event {
    fn date(&self) -> &str {
        match self {
            Self::TaskCompleted { date, .. } | Self::HabitChecked { date, .. } => date,
        }
    }

    fn values(&self) -> [(&'static str, String); 3] {
        match self {
            Self::TaskCompleted {
                title,
                project_name,
                date,
                ..
            } => [
                ("{title}", title.clone()),
                ("{project}", project_name.clone().unwrap_or_default()),
                ("{date}", date.clone()),
            ],
            Self::HabitChecked { name, date, .. } => [
                ("{title}", name.clone()),
                ("{project}", String::new()),
                ("{date}", date.clone()),
            ],
        }
    }

    fn matches(&self, trigger: &Trigger) -> bool {
        match (self, trigger) {
            (
                Self::TaskCompleted { project_id, .. },
                Trigger::TaskCompleted { project_id: want },
            ) => want.is_none() || want == project_id,
            (Self::HabitChecked { habit_id, .. }, Trigger::HabitChecked { habit_id: want }) => {
                want.as_ref().is_none_or(|w| w == habit_id)
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutomationError {
    Invalid(&'static str, String),
    NotFound,
    Backend(String),
}

pub trait AutomationRepository {
    async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<Rule>, String>;
    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Rule>, String>;
    async fn insert(&self, rule: &Rule) -> Result<(), String>;
    async fn update(&self, rule: &Rule) -> Result<bool, String>;
    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String>;
    async fn count(&self, owner_id: &str) -> Result<u32, String>;
    async fn record_run(
        &self,
        owner_id: &str,
        id: &str,
        at: i64,
        ok: bool,
        message: &str,
    ) -> Result<(), String>;
}

fn backend(e: String) -> AutomationError {
    AutomationError::Backend(e)
}

fn invalid(field: &'static str, why: impl Into<String>) -> AutomationError {
    AutomationError::Invalid(field, why.into())
}

/// Replace placeholders in one already-parsed text field (never re-parsed,
/// so a value like "x; capture y" can't add steps).
fn fill(text: &mut String, values: &[(&'static str, String)]) {
    for (k, v) in values {
        if text.contains(k) {
            *text = text.replace(k, v);
        }
    }
}

/// Parse a rule's template for `date` and fill in an event's values.
pub fn commands_for(
    template: &str,
    date: &str,
    values: &[(&'static str, String)],
) -> Result<Vec<Command>, AutomationError> {
    let template = template.trim();
    if template.is_empty() {
        return Err(invalid("command", "required"));
    }
    if template.chars().count() > MAX_TEMPLATE {
        return Err(invalid("command", "too long"));
    }
    // Every `{…}` must be a known placeholder.
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let end = rest[start..].find('}').map(|e| start + e + 1);
        let token = end.map(|e| &rest[start..e]).unwrap_or(&rest[start..]);
        if !PLACEHOLDERS.contains(&token) {
            return Err(invalid(
                "command",
                format!("unknown placeholder {token} (use {{title}}, {{project}}, or {{date}})"),
            ));
        }
        rest = &rest[end.unwrap_or(rest.len())..];
    }
    let mut commands = parser::parse(template, date)
        .map_err(|_| invalid("command", "not a command JARVIS understands"))?;
    if commands.len() > MAX_STEPS {
        return Err(invalid("command", "at most 3 steps"));
    }
    for c in &mut commands {
        match c {
            Command::CreateTask { title, project, .. } => {
                fill(title, values);
                if let Some(p) = project {
                    fill(p, values);
                }
            }
            Command::CreateNote { body } => fill(body, values),
            Command::AppendJournal { text, .. } | Command::Capture { text } => fill(text, values),
            _ => {
                return Err(invalid(
                    "command",
                    "only add task, note, journal, and capture can run automatically",
                ))
            }
        }
    }
    Ok(commands)
}

fn check_trigger(t: Trigger) -> Result<Trigger, AutomationError> {
    let id = match &t {
        Trigger::TaskCompleted { project_id } => project_id,
        Trigger::HabitChecked { habit_id } => habit_id,
    };
    if id.as_deref().is_some_and(|i| !is_id(i)) {
        return Err(invalid("trigger", "malformed id"));
    }
    Ok(t)
}

fn sample_values() -> [(&'static str, String); 3] {
    [
        ("{title}", "Sample".into()),
        ("{project}", "Sample".into()),
        ("{date}", "2000-01-01".into()),
    ]
}

pub struct AutomationService<'a, R, E: ?Sized> {
    repo: &'a R,
    entropy: &'a E,
    now: i64,
}

impl<'a, R: AutomationRepository, E: Entropy + ?Sized> AutomationService<'a, R, E> {
    pub fn new(repo: &'a R, entropy: &'a E, now: i64) -> Self {
        Self { repo, entropy, now }
    }

    pub async fn list(&self, p: &Principal) -> Result<Vec<Rule>, AutomationError> {
        self.repo.list(&p.user_id, MAX_RULES).await.map_err(backend)
    }

    async fn get(&self, p: &Principal, id: &str) -> Result<Rule, AutomationError> {
        if !is_id(id) {
            return Err(AutomationError::NotFound);
        }
        self.repo
            .get(&p.user_id, id)
            .await
            .map_err(backend)?
            .ok_or(AutomationError::NotFound)
    }

    fn check(&self, mut r: Rule) -> Result<Rule, AutomationError> {
        r.name = validate::line(&r.name, MAX_NAME).map_err(|w| invalid("name", w))?;
        r.trigger = check_trigger(r.trigger)?;
        r.command = r.command.trim().to_owned();
        commands_for(&r.command, "2000-01-01", &sample_values())?;
        Ok(r)
    }

    pub async fn create(&self, p: &Principal, input: NewRule) -> Result<Rule, AutomationError> {
        let rule = self.check(Rule {
            id: new_id(self.entropy),
            owner_id: p.user_id.clone(),
            name: input.name,
            trigger: input.trigger,
            command: input.command,
            enabled: input.enabled.unwrap_or(true),
            created_at: self.now,
            updated_at: self.now,
            last_run_at: None,
            last_ok: None,
            last_message: None,
        })?;
        if self.repo.count(&p.user_id).await.map_err(backend)? >= MAX_RULES {
            return Err(invalid("automations", "limit of 20 automations reached"));
        }
        self.repo.insert(&rule).await.map_err(backend)?;
        Ok(rule)
    }

    pub async fn update(
        &self,
        p: &Principal,
        id: &str,
        patch: RulePatch,
    ) -> Result<Rule, AutomationError> {
        let mut r = self.get(p, id).await?;
        if let Some(v) = patch.name {
            r.name = v;
        }
        if let Some(v) = patch.trigger {
            r.trigger = v;
        }
        if let Some(v) = patch.command {
            r.command = v;
        }
        if let Some(v) = patch.enabled {
            r.enabled = v;
        }
        r.updated_at = self.now;
        let r = self.check(r)?;
        if !self.repo.update(&r).await.map_err(backend)? {
            return Err(AutomationError::NotFound);
        }
        Ok(r)
    }

    pub async fn delete(&self, p: &Principal, id: &str) -> Result<(), AutomationError> {
        if is_id(id) && self.repo.delete(&p.user_id, id).await.map_err(backend)? {
            Ok(())
        } else {
            Err(AutomationError::NotFound)
        }
    }

    /// Run every enabled rule matching `event`. Failures are recorded on the
    /// rule, never returned: what triggered the event already succeeded.
    /// Returns how many rules ran.
    pub async fn fire<J: JarvisRepository, C: CommandTargets>(
        &self,
        p: &Principal,
        event: &Event,
        jarvis: &JarvisService<'_, J, E>,
        targets: &C,
    ) -> usize {
        let Ok(rules) = self.list(p).await else {
            return 0;
        };
        let mut ran = 0;
        for rule in rules
            .iter()
            .filter(|r| r.enabled && event.matches(&r.trigger))
        {
            ran += 1;
            let (ok, message) = match commands_for(&rule.command, event.date(), &event.values()) {
                Err(AutomationError::Invalid(_, why)) => (false, why),
                Err(_) => (false, "The rule couldn't be prepared.".into()),
                Ok(commands) => {
                    let (status, results) = jarvis.run_unattended(p, commands, targets).await;
                    let text: Vec<String> = results
                        .into_iter()
                        .map(|r: ActionResult| r.message)
                        .collect();
                    (status == PlanStatus::Executed, text.join(" "))
                }
            };
            let message: String = message.chars().take(MAX_MESSAGE).collect();
            // Best effort: a failed write here loses only the status line.
            let _ = self
                .repo
                .record_run(&p.user_id, &rule.id, self.now, ok, &message)
                .await;
        }
        ran
    }
}

#[cfg(test)]
pub(crate) mod memory {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct MemoryAutomations(pub RefCell<Vec<Rule>>);

    impl AutomationRepository for MemoryAutomations {
        async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<Rule>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .filter(|r| r.owner_id == owner_id)
                .take(limit as usize)
                .cloned()
                .collect())
        }
        async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Rule>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .find(|r| r.owner_id == owner_id && r.id == id)
                .cloned())
        }
        async fn insert(&self, rule: &Rule) -> Result<(), String> {
            self.0.borrow_mut().push(rule.clone());
            Ok(())
        }
        async fn update(&self, rule: &Rule) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            match all
                .iter_mut()
                .find(|r| r.owner_id == rule.owner_id && r.id == rule.id)
            {
                Some(r) => {
                    *r = rule.clone();
                    Ok(true)
                }
                None => Ok(false),
            }
        }
        async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            let before = all.len();
            all.retain(|r| !(r.owner_id == owner_id && r.id == id));
            Ok(all.len() < before)
        }
        async fn count(&self, owner_id: &str) -> Result<u32, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .filter(|r| r.owner_id == owner_id)
                .count() as u32)
        }
        async fn record_run(
            &self,
            owner_id: &str,
            id: &str,
            at: i64,
            ok: bool,
            message: &str,
        ) -> Result<(), String> {
            if let Some(r) = self
                .0
                .borrow_mut()
                .iter_mut()
                .find(|r| r.owner_id == owner_id && r.id == id)
            {
                r.last_run_at = Some(at);
                r.last_ok = Some(ok);
                r.last_message = Some(message.into());
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(title: &str) -> [(&'static str, String); 3] {
        [
            ("{title}", title.into()),
            ("{project}", "Garden".into()),
            ("{date}", "2026-10-22".into()),
        ]
    }

    #[test]
    fn templates_are_parsed_then_filled_without_injection() {
        let cmds = commands_for(
            "journal Finished {title} ({project}); capture review {title} on {date}",
            "2026-10-22",
            &values("x; add task evil"),
        )
        .unwrap();
        assert_eq!(cmds.len(), 2, "the value's `;` did not add a step");
        assert_eq!(
            cmds[0],
            Command::AppendJournal {
                date: "2026-10-22".into(),
                text: "Finished x; add task evil (Garden)".into()
            }
        );
        assert_eq!(
            cmds[1],
            Command::Capture {
                text: "review x; add task evil on 2026-10-22".into()
            }
        );
        // Relative dates resolve against the event's date.
        let due = commands_for(
            "add task water {title} due tomorrow",
            "2026-10-22",
            &values("plants"),
        )
        .unwrap();
        assert!(
            matches!(&due[0], Command::CreateTask { title, due_date: Some(d), .. } if title == "water plants" && d == "2026-10-23")
        );
    }

    #[test]
    fn templates_are_restricted() {
        let err = |t: &str| commands_for(t, "2026-10-22", &values("t")).unwrap_err();
        let why = |e: AutomationError| match e {
            AutomationError::Invalid(_, w) => w,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            why(err("complete {title}")),
            "only add task, note, journal, and capture can run automatically"
        );
        assert_eq!(
            why(err("capture a; capture b; capture c; capture d")),
            "at most 3 steps"
        );
        assert!(why(err("capture {owner}")).starts_with("unknown placeholder {owner}"));
        assert!(why(err("capture {title")).starts_with("unknown placeholder {title"));
        assert_eq!(why(err("dance")), "not a command JARVIS understands");
        assert_eq!(why(err("  ")), "required");
    }

    #[test]
    fn events_match_triggers_and_filters() {
        let done = |project: Option<&str>| Event::TaskCompleted {
            title: "t".into(),
            project_id: project.map(Into::into),
            project_name: None,
            date: "2026-10-22".into(),
        };
        let any = Trigger::TaskCompleted { project_id: None };
        let garden = Trigger::TaskCompleted {
            project_id: Some("g".into()),
        };
        assert!(done(None).matches(&any) && done(Some("g")).matches(&any));
        assert!(done(Some("g")).matches(&garden) && !done(None).matches(&garden));
        let habit = Event::HabitChecked {
            habit_id: "h".into(),
            name: "Read".into(),
            date: "2026-10-22".into(),
        };
        assert!(!habit.matches(&any));
        assert!(habit.matches(&Trigger::HabitChecked { habit_id: None }));
        assert!(!habit.matches(&Trigger::HabitChecked {
            habit_id: Some("other".into())
        }));
    }
}
