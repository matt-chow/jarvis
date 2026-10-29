//! Short-lived conversation state for JARVIS chat (DECISIONS D-048).
//!
//! A conversation remembers just enough for follow-ups: the items recently
//! mentioned (under short handles such as `t1` or `b2`, so the model never
//! sees ids), the last few turns' text, the plan awaiting approval, and an
//! open clarification. It is bounded, owner-scoped, and expires after
//! [`TTL_MS`] without use. Ids in it are hints: every command built from them
//! is resolved again through the owner's services.

use super::{Choice, Command, ItemKind, Plan, PlanStatus};
use serde::{Deserialize, Serialize};

/// Idle time after which a conversation starts fresh.
pub const TTL_MS: i64 = 30 * 60 * 1000;
pub const MAX_REFS: usize = 24;
pub const MAX_TURNS: usize = 6;
pub const TURN_CHARS: usize = 280;

/// One stored conversation (table `jarvis_conversations`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversation {
    pub id: String,
    pub owner_id: String,
    pub state: State,
    pub updated_at: i64,
    pub expires_at: i64,
}

pub trait ConversationRepository {
    /// The owner's conversation `id`, expired or not.
    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Conversation>, String>;
    /// Insert or replace (only ever the owner's own row).
    async fn put(&self, c: &Conversation) -> Result<(), String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefKind {
    Task,
    Event,
    Block,
    Project,
    Note,
}

impl RefKind {
    fn letter(self) -> char {
        match self {
            Self::Task => 't',
            Self::Event => 'e',
            Self::Block => 'b',
            Self::Project => 'p',
            Self::Note => 'n',
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Event => "event",
            Self::Block => "block",
            Self::Project => "project",
            Self::Note => "note",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "task" | "open task" => Self::Task,
            "event" => Self::Event,
            "block" | "session" => Self::Block,
            "project" | "active project" => Self::Project,
            "note" => Self::Note,
            _ => return None,
        })
    }
}

/// An item mentioned in the conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ref {
    pub handle: String,
    pub kind: RefKind,
    pub id: String,
    pub title: String,
    /// When it happens, for events and sessions (`YYYY-MM-DD HH:MM–HH:MM`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    /// Only proposed by a plan that hasn't run (it may never exist).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub proposed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Said {
    /// `you` or `jarvis`.
    pub who: String,
    pub text: String,
}

/// A question JARVIS asked: the commands waiting on the answer (if any) and
/// the choices offered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clarify {
    pub question: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<Command>,
    #[serde(default)]
    pub choices: Vec<Choice>,
    /// The request it came from (the model sees it with the answer).
    #[serde(default)]
    pub input: String,
}

/// What the app is showing, sent with a request ("this task"). Ids are
/// checked against the caller's data before use.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Screen {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_id: Option<String>,
    /// The day the calendar or planner shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    /// Most recent first.
    #[serde(default)]
    pub refs: Vec<Ref>,
    /// Handles of the items the last answer listed, in order ("the second one").
    #[serde(default)]
    pub listed: Vec<String>,
    /// Handles of the verified items on screen.
    #[serde(default)]
    pub selected: Vec<String>,
    #[serde(default)]
    pub screen_route: Option<String>,
    #[serde(default)]
    pub screen_date: Option<String>,
    #[serde(default)]
    pub turns: Vec<Said>,
    #[serde(default)]
    pub pending_plan: Option<String>,
    /// The commands the pending plan was made from (before expansion), so a
    /// correction ("leave Thursday evening free") can plan them again.
    #[serde(default)]
    pub pending_commands: Vec<Command>,
    #[serde(default)]
    pub last_plan: Option<String>,
    #[serde(default)]
    pub clarify: Option<Clarify>,
    #[serde(default)]
    next_handle: u32,
}

fn clip(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

impl State {
    /// Add (or refresh) an item and return its handle. Handles stay stable
    /// while the item is remembered.
    pub fn remember(
        &mut self,
        kind: RefKind,
        id: &str,
        title: &str,
        when: Option<String>,
        proposed: bool,
    ) -> String {
        if let Some(i) = self.refs.iter().position(|r| r.id == id && r.kind == kind) {
            let mut r = self.refs.remove(i);
            r.title = clip(title, 120);
            r.when = when.or(r.when);
            r.proposed = proposed;
            let handle = r.handle.clone();
            self.refs.insert(0, r);
            return handle;
        }
        self.next_handle += 1;
        let handle = format!("{}{}", kind.letter(), self.next_handle);
        self.refs.insert(
            0,
            Ref {
                handle: handle.clone(),
                kind,
                id: id.to_owned(),
                title: clip(title, 120),
                when,
                proposed,
            },
        );
        self.refs.truncate(MAX_REFS);
        handle
    }

    pub fn by_handle(&self, handle: &str) -> Option<&Ref> {
        let h = handle.trim().trim_start_matches('#').to_ascii_lowercase();
        self.refs.iter().find(|r| r.handle == h)
    }

    pub fn said(&mut self, who: &str, text: &str) {
        self.turns.push(Said {
            who: who.to_owned(),
            text: clip(text, TURN_CHARS),
        });
        let extra = self.turns.len().saturating_sub(MAX_TURNS);
        self.turns.drain(..extra);
    }

    /// The `kind` item a pronoun means: on screen for "this", else the most
    /// recently mentioned one.
    pub fn pronoun(&self, kind: RefKind, query: &str) -> Option<&Ref> {
        let q = normalize(query);
        let ordinal = ORDINALS.iter().position(|o| {
            q == format!("the {o} one")
                || q == format!("{o} one")
                || q == *o
                || q == format!("the {o}")
                || q.starts_with(&format!("the {o} "))
        });
        if let Some(n) = ordinal {
            return self
                .listed
                .iter()
                .filter_map(|h| self.by_handle(h))
                .filter(|r| r.kind == kind)
                .nth(n);
        }
        let on_screen = || {
            self.selected
                .iter()
                .filter_map(|h| self.by_handle(h))
                .find(|r| r.kind == kind)
        };
        let latest = || self.refs.iter().find(|r| r.kind == kind && !r.proposed);
        let noun = kind.as_str();
        let this = [
            "this".to_owned(),
            format!("this {noun}"),
            format!("the selected {noun}"),
            format!("selected {noun}"),
            format!("the current {noun}"),
            format!("current {noun}"),
            format!("the open {noun}"),
        ];
        let that = [
            "it".to_owned(),
            "that".to_owned(),
            "that one".to_owned(),
            format!("that {noun}"),
            format!("the {noun}"),
            format!("the same {noun}"),
            format!("the last {noun}"),
        ];
        if this.contains(&q) {
            return on_screen().or_else(latest);
        }
        if that.contains(&q) {
            return latest().or_else(on_screen);
        }
        None
    }

    /// Remember what a plan proposed or did, and what it listed.
    pub fn note_plan(&mut self, plan: &Plan) {
        let proposed = plan.status == PlanStatus::Pending;
        let mut listed = Vec::new();
        for (i, a) in plan.actions.iter().enumerate() {
            if plan.results.get(i).is_some_and(|r| r.skipped || !r.ok) {
                continue;
            }
            let (kind, id, title, when) = match &a.command {
                Command::CreateTask {
                    id: Some(id),
                    title,
                    ..
                } => (RefKind::Task, id, title, None),
                Command::CompleteTask {
                    task_id: Some(id),
                    title: Some(title),
                    ..
                }
                | Command::UpdateTask {
                    task_id: Some(id),
                    title: Some(title),
                    ..
                } => (RefKind::Task, id, title, None),
                Command::AddSubtask { id, title, .. } => (RefKind::Task, id, title, None),
                Command::CreateBlock {
                    id,
                    title,
                    date,
                    start_time,
                    end_time,
                    ..
                }
                | Command::MoveBlock {
                    block_id: id,
                    title,
                    date,
                    start_time,
                    end_time,
                    ..
                } => (
                    RefKind::Block,
                    id,
                    title,
                    Some(format!("{date} {start_time}–{end_time}")),
                ),
                Command::CreateEvent {
                    id,
                    title,
                    date,
                    start_time,
                    ..
                }
                | Command::MoveEvent {
                    event_id: id,
                    title,
                    date,
                    start_time,
                    ..
                } => (
                    RefKind::Event,
                    id,
                    title,
                    Some(match start_time {
                        Some(s) => format!("{date} {s}"),
                        None => date.clone(),
                    }),
                ),
                _ => continue,
            };
            // A moved or completed item exists either way.
            let new = matches!(
                a.command,
                Command::CreateTask { .. }
                    | Command::AddSubtask { .. }
                    | Command::CreateBlock { .. }
                    | Command::CreateEvent { .. }
            );
            listed.push(self.remember(kind, id, title, when, proposed && new));
        }
        // Items a read answer named (sessions and events it listed).
        for r in &plan.results {
            if let Some(item) = &r.item {
                let kind = match item.kind {
                    ItemKind::Block => RefKind::Block,
                    ItemKind::Event => RefKind::Event,
                };
                let title = r
                    .message
                    .split('“')
                    .nth(1)
                    .and_then(|s| s.split('”').next());
                listed.push(self.remember(
                    kind,
                    &item.id,
                    title.unwrap_or(&r.message),
                    Some(item.date.clone()),
                    false,
                ));
            }
        }
        if !listed.is_empty() {
            self.listed = listed;
        }
        if proposed && plan.requires_approval {
            self.pending_plan = Some(plan.id.clone());
        } else {
            if self.pending_plan.as_deref() == Some(&plan.id) {
                self.pending_plan = None;
                self.pending_commands.clear();
            }
            if plan.status != PlanStatus::Cancelled {
                self.last_plan = Some(plan.id.clone());
            }
        }
    }

    /// The pick a reply to a clarification makes: a number, an ordinal, or
    /// (part of) one choice's title.
    pub fn pick(&self, text: &str) -> Option<&Choice> {
        let c = self.clarify.as_ref()?;
        let q = normalize(text);
        let q = q
            .trim_start_matches("i mean ")
            .trim_start_matches("i meant ")
            .trim_start_matches("use ")
            .trim_start_matches("pick ")
            .trim_start_matches("number ")
            .trim_start_matches("option ")
            .trim_start_matches("the ")
            .trim_end_matches(" one")
            .trim();
        if let Ok(n) = q.parse::<usize>() {
            return n.checked_sub(1).and_then(|i| c.choices.get(i));
        }
        if let Some(n) = ORDINALS.iter().position(|o| q == *o) {
            return c.choices.get(n);
        }
        if q.is_empty() {
            return None;
        }
        let exact: Vec<&Choice> = c
            .choices
            .iter()
            .filter(|x| normalize(&x.title) == q)
            .collect();
        if let [one] = exact.as_slice() {
            return Some(one);
        }
        let partial: Vec<&Choice> = c
            .choices
            .iter()
            .filter(|x| normalize(&x.title).contains(q))
            .collect();
        match partial.as_slice() {
            [one] => Some(one),
            _ => None,
        }
    }
}

const ORDINALS: [&str; 5] = ["first", "second", "third", "fourth", "fifth"];

fn normalize(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '’' => '\'',
            c if c.is_alphanumeric() || c == '\'' || c == ' ' || c == '-' => c,
            _ => ' ',
        })
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// "reset", "start over", "new conversation": forget the conversation.
pub fn is_reset(text: &str) -> bool {
    matches!(
        normalize(text).as_str(),
        "reset" | "start over" | "new conversation" | "forget this conversation" | "clear"
    )
}

/// Fill a choice the owner picked into the commands that were waiting on it.
/// Returns `false` when none of them takes that kind of item.
pub fn apply_choice(commands: &mut [Command], choice: &Choice) -> bool {
    let kind = RefKind::parse(&choice.kind);
    for c in commands.iter_mut() {
        let slot = match (kind, c) {
            (
                Some(RefKind::Task),
                Command::CompleteTask { task_id, .. }
                | Command::UpdateTask { task_id, .. }
                | Command::ScheduleTask { task_id, .. },
            ) if task_id.is_none() => task_id,
            _ => continue,
        };
        *slot = Some(choice.id.clone());
        return true;
    }
    false
}

/// Fill pronouns ("it", "this task", "the second one") in task commands with
/// the item they mean. Unknown pronouns are left for name resolution.
pub fn bind_pronouns(state: &State, commands: &mut [Command]) {
    for c in commands.iter_mut() {
        if let Command::CompleteTask { query, task_id, .. }
        | Command::UpdateTask { query, task_id, .. }
        | Command::ScheduleTask { query, task_id, .. } = c
        {
            if task_id.is_none() {
                if let Some(r) = state.pronoun(RefKind::Task, query) {
                    *task_id = Some(r.id.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(id: &str, title: &str) -> Choice {
        Choice {
            kind: "open task".into(),
            id: id.into(),
            title: title.into(),
        }
    }

    #[test]
    fn handles_are_stable_and_bounded() {
        let mut s = State::default();
        let a = s.remember(RefKind::Task, "a", "Essay", None, false);
        let b = s.remember(RefKind::Block, "b", "Essay", Some("x".into()), true);
        assert_eq!((a.as_str(), b.as_str()), ("t1", "b2"));
        assert_eq!(
            s.remember(RefKind::Task, "a", "Essay v2", None, false),
            "t1"
        );
        assert_eq!(s.refs[0].title, "Essay v2");
        assert_eq!(s.by_handle("T1").unwrap().id, "a");
        for i in 0..40 {
            s.remember(RefKind::Note, &format!("n{i}"), "x", None, false);
        }
        assert_eq!(s.refs.len(), MAX_REFS);
        assert!(s.by_handle("t1").is_none(), "oldest dropped");
        for i in 0..10 {
            s.said("you", &"x".repeat(400 + i));
        }
        assert_eq!(s.turns.len(), MAX_TURNS);
        assert!(s.turns[0].text.chars().count() <= TURN_CHARS);
    }

    #[test]
    fn pronouns_prefer_screen_for_this_and_history_for_it() {
        let mut s = State::default();
        let shown = s.remember(RefKind::Task, "shown", "Read chapter", None, false);
        s.selected = vec![shown];
        s.remember(RefKind::Task, "said", "Essay", None, false);
        s.remember(RefKind::Task, "maybe", "Proposed", None, true);
        assert_eq!(s.pronoun(RefKind::Task, "this task").unwrap().id, "shown");
        assert_eq!(
            s.pronoun(RefKind::Task, "the selected task").unwrap().id,
            "shown"
        );
        assert_eq!(
            s.pronoun(RefKind::Task, "it").unwrap().id,
            "said",
            "not proposed"
        );
        assert!(s.pronoun(RefKind::Task, "essay").is_none());
        s.listed = vec!["t2".into(), "t1".into()];
        assert_eq!(
            s.pronoun(RefKind::Task, "the second one").unwrap().id,
            "shown"
        );
        assert!(s.pronoun(RefKind::Task, "the third one").is_none());
        assert!(s.pronoun(RefKind::Block, "it").is_none());
    }

    #[test]
    fn clarification_picks() {
        let mut s = State::default();
        assert!(s.pick("1").is_none());
        s.clarify = Some(Clarify {
            question: "Which?".into(),
            commands: vec![],
            choices: vec![choice("a", "Essay draft"), choice("b", "Essay final")],
            input: "finish essay".into(),
        });
        assert_eq!(s.pick("2").unwrap().id, "b");
        assert_eq!(s.pick("the first one").unwrap().id, "a");
        assert_eq!(s.pick("Final.").unwrap().id, "b");
        assert_eq!(s.pick("I meant essay draft").unwrap().id, "a");
        assert!(s.pick("essay").is_none(), "still ambiguous");
        assert!(s.pick("3").is_none());
        assert!(s.pick("0").is_none());
        let mut cmds = vec![Command::CompleteTask {
            query: "essay".into(),
            task_id: None,
            title: None,
        }];
        assert!(apply_choice(&mut cmds, &choice("b", "Essay final")));
        assert!(matches!(&cmds[0], Command::CompleteTask { task_id: Some(id), .. } if id == "b"));
        assert!(
            !apply_choice(&mut cmds, &choice("c", "x")),
            "already filled"
        );
    }

    #[test]
    fn state_survives_json_and_reset_words() {
        let mut s = State::default();
        s.remember(
            RefKind::Event,
            "e",
            "Class",
            Some("2026-10-30 09:00".into()),
            false,
        );
        s.said("you", "hi");
        let back: State = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.remember_count(), 1);
        assert!(is_reset("Start over."));
        assert!(!is_reset("start over my essay"));
    }

    impl State {
        fn remember_count(&self) -> u32 {
            self.next_handle
        }
    }
}
