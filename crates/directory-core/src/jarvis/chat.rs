//! One conversational turn with JARVIS (DECISIONS D-048).
//!
//! The deterministic parser goes first. What it doesn't understand goes to
//! the optional model ([`agent`]), which reads through lookups and proposes
//! actions. Either way the result is an ordinary plan: read-only plans run at
//! once, changes wait for approval. The conversation adds follow-ups ("it",
//! "the second one", "this task" on screen), answers to clarifying
//! questions, corrections to the plan awaiting approval (which replace it),
//! and a typed "yes"/"no" for that plan.

use super::agent::{self, Converted, Edit};
use super::ai::{AiBudget, TextModel};
use super::conversation::{
    self, Clarify, Conversation, ConversationRepository, RefKind, Screen, State,
};
use super::parser::ParseError;
use super::*;
use crate::blocks::BlockService;

#[derive(Debug, Clone, Deserialize)]
pub struct ChatRequest {
    pub text: String,
    /// The user's local date and time (see [`PlanRequest`]).
    pub today: String,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub screen: Option<Screen>,
    /// Steps left out if this turn approves the pending plan ("yes").
    #[serde(default)]
    pub skip: Vec<usize>,
    /// Propose reversing this executed plan (the app's Undo button); `text`
    /// is what the conversation records. The plan must be the caller's.
    #[serde(default)]
    pub undo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChatReply {
    pub conversation_id: String,
    /// `plan` (see `plan.status`), `ask`, `answer`, `cancelled`, `reset`,
    /// or an error status from [`voice::from_error`] (`not_understood`,
    /// `unresolved`, `invalid`, `not_found`, `expired`, `executed`).
    pub status: &'static str,
    pub say: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<Plan>,
    /// Answers to offer for `ask`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<Choice>,
    /// `parser` or `model`: who understood the request.
    pub via: &'static str,
    /// Whether the AI model is configured (otherwise only direct commands work).
    pub ai: bool,
    /// The pending plan this one replaced (it was cancelled).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaces: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<&'static str>,
}

impl ChatReply {
    fn new(status: &'static str, say: impl Into<String>) -> Self {
        Self {
            conversation_id: String::new(),
            status,
            say: say.into(),
            plan: None,
            choices: vec![],
            via: "parser",
            ai: false,
            replaces: None,
            examples: vec![],
        }
    }

    fn plan(plan: Plan) -> Self {
        let say = if plan.status == PlanStatus::Pending {
            "Here's what I'd do. Check it, then confirm.".to_owned()
        } else {
            voice::from_plan(&plan).speech
        };
        Self {
            plan: Some(plan),
            ..Self::new("plan", say)
        }
    }

    fn ask(question: String, choices: Vec<Choice>) -> Self {
        Self {
            choices,
            ..Self::new("ask", question)
        }
    }

    fn from_error(e: &JarvisError) -> Option<Self> {
        let a = voice::from_error(e)?;
        let mut r = Self::new(a.status, a.speech);
        if let JarvisError::Ambiguous { choices, .. } = e {
            r.status = "ask";
            r.choices = choices.clone();
        }
        if r.status == "not_understood" {
            r.examples = parser::EXAMPLES.iter().take(4).copied().collect();
        }
        Some(r)
    }
}

/// The optional model and its daily budget.
pub struct Assistant<'a, M, B> {
    pub model: Option<&'a M>,
    pub budget: &'a B,
}

const MAX_OPTIONS: usize = 5;

fn clean_route(r: &str) -> Option<String> {
    let ok = !r.is_empty()
        && r.len() <= 60
        && r.chars()
            .all(|c| c.is_ascii_alphanumeric() || "/#-_=&?.".contains(c));
    ok.then(|| r.to_owned())
}

impl<'a, R: JarvisRepository, E: Entropy + ?Sized> JarvisService<'a, R, E> {
    /// Handle one message in a conversation. Errors are only backend
    /// failures; everything else is a reply.
    pub async fn chat<M, B, V, C>(
        &self,
        p: &Principal,
        ai: &Assistant<'_, M, B>,
        convs: &V,
        req: ChatRequest,
        t: &C,
    ) -> Result<ChatReply, JarvisError>
    where
        M: TextModel,
        B: AiBudget,
        V: ConversationRepository,
        C: CommandTargets,
    {
        let input = req.text.trim().to_owned();
        let mut conv = self
            .open_conversation(p, convs, req.conversation_id.as_deref())
            .await?;
        let outcome = match Clock::new(&req.today, req.time.as_deref()) {
            Err(e) => Err(e),
            Ok(_) if input.chars().count() > MAX_INPUT => {
                Err(JarvisError::Invalid("text", "too long"))
            }
            Ok(clock) => {
                self.see_screen(p, &mut conv.state, req.screen.as_ref(), t)
                    .await;
                let time = req.time.as_deref();
                let undo = req.undo.as_deref();
                self.turn(
                    p,
                    ai,
                    &mut conv.state,
                    &input,
                    &clock,
                    time,
                    &req.skip,
                    undo,
                    t,
                )
                .await
            }
        };
        let mut reply = match outcome {
            Ok(r) => r,
            Err(e) => ChatReply::from_error(&e).ok_or(e)?,
        };
        if !input.is_empty() && reply.status != "reset" {
            conv.state.said("you", &input);
            conv.state.said("jarvis", &reply.say);
        }
        conv.updated_at = self.now;
        conv.expires_at = self.now + conversation::TTL_MS;
        convs.put(&conv).await.map_err(backend)?;
        reply.conversation_id = conv.id;
        reply.ai = ai.model.is_some();
        Ok(reply)
    }

    /// Record a plan approved outside the chat (the app's Confirm button) so
    /// follow-ups can refer to what it did.
    pub async fn remember_plan<V: ConversationRepository>(
        &self,
        p: &Principal,
        convs: &V,
        conversation_id: &str,
        plan: &Plan,
    ) -> Result<(), JarvisError> {
        if !is_id(conversation_id) {
            return Ok(());
        }
        let Some(mut conv) = convs
            .get(&p.user_id, conversation_id)
            .await
            .map_err(backend)?
        else {
            return Ok(());
        };
        if conv.owner_id != p.user_id || conv.expires_at <= self.now {
            return Ok(());
        }
        conv.state.note_plan(plan);
        conv.updated_at = self.now;
        conv.expires_at = self.now + conversation::TTL_MS;
        convs.put(&conv).await.map_err(backend)
    }

    async fn open_conversation<V: ConversationRepository>(
        &self,
        p: &Principal,
        convs: &V,
        id: Option<&str>,
    ) -> Result<Conversation, JarvisError> {
        if let Some(id) = id.filter(|id| is_id(id)) {
            if let Some(c) = convs.get(&p.user_id, id).await.map_err(backend)? {
                if c.owner_id == p.user_id && c.expires_at > self.now {
                    return Ok(c);
                }
            }
        }
        Ok(Conversation {
            id: new_id(self.entropy),
            owner_id: p.user_id.clone(),
            state: State::default(),
            updated_at: self.now,
            expires_at: self.now + conversation::TTL_MS,
        })
    }

    /// What's on screen, keeping only items that are the caller's.
    async fn see_screen<C: CommandTargets>(
        &self,
        p: &Principal,
        st: &mut State,
        screen: Option<&Screen>,
        t: &C,
    ) {
        st.selected.clear();
        st.screen_route = None;
        st.screen_date = None;
        let Some(s) = screen else { return };
        st.screen_route = s.route.as_deref().and_then(clean_route);
        st.screen_date = s.date.as_deref().and_then(|d| validate::date(d).ok());
        let ok = |id: &Option<String>| id.clone().filter(|id| is_id(id));
        let mut seen: Vec<(RefKind, String, String, Option<String>)> = vec![];
        if let Some(id) = ok(&s.task_id) {
            if let Ok(x) = TaskService::new(t.tasks(), self.entropy, self.now)
                .get(p, &id)
                .await
            {
                seen.push((RefKind::Task, x.id, x.title, None));
            }
        }
        if let Some(id) = ok(&s.event_id) {
            if let Ok(x) = CalendarService::new(t.events(), self.entropy, self.now)
                .get(p, &id)
                .await
            {
                let when = match &x.start_time {
                    Some(s) => format!("{} {s}", x.start_date),
                    None => x.start_date.clone(),
                };
                seen.push((RefKind::Event, x.id, x.title, Some(when)));
            }
        }
        if let Some(id) = ok(&s.block_id) {
            if let Ok(x) = BlockService::new(t.blocks(), self.entropy, self.now)
                .get(p, &id)
                .await
            {
                let when = format!("{} {}–{}", x.date, x.start_time, x.end_time);
                seen.push((RefKind::Block, x.id, x.title, Some(when)));
            }
        }
        if let Some(id) = ok(&s.project_id) {
            if let Ok(x) = ProjectService::new(t.projects(), self.entropy, self.now)
                .get(p, &id)
                .await
            {
                seen.push((RefKind::Project, x.id, x.name, None));
            }
        }
        if let Some(id) = ok(&s.note_id) {
            if let Ok(x) = NoteService::new(t.notes(), self.entropy, self.now)
                .get(p, &id)
                .await
            {
                seen.push((RefKind::Note, x.id, x.title, None));
            }
        }
        for (kind, id, title, when) in seen {
            let h = st.remember(kind, &id, &title, when, false);
            st.selected.push(h);
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn turn<M, B, C>(
        &self,
        p: &Principal,
        ai: &Assistant<'_, M, B>,
        st: &mut State,
        input: &str,
        clock: &Clock,
        time: Option<&str>,
        skip: &[usize],
        undo: Option<&str>,
        t: &C,
    ) -> Result<ChatReply, JarvisError>
    where
        M: TextModel,
        B: AiBudget,
        C: CommandTargets,
    {
        if input.is_empty() {
            return Err(JarvisError::Parse(ParseError::Empty));
        }
        if conversation::is_reset(input) {
            if let Some(id) = st.pending_plan.take() {
                let _ = self.cancel(p, &id).await;
            }
            *st = State::default();
            return Ok(ChatReply::new("reset", "Okay, starting over."));
        }
        if let Some(id) = undo {
            if !is_id(id) {
                return Err(JarvisError::Invalid("undo", "must be a plan id"));
            }
            st.clarify = None;
            let commands = vec![Command::Undo {
                plan_id: Some(id.to_owned()),
            }];
            return self.propose(p, st, input, commands, clock, None, t).await;
        }
        // A typed answer to the plan awaiting approval.
        if let Some(id) = st.pending_plan.clone() {
            match voice::yes_or_no(input) {
                Some(true) => {
                    let ran = self.execute_with(p, &id, skip, t).await;
                    if let Err(JarvisError::NotPending(_) | JarvisError::NotFound) = &ran {
                        // Already decided or expired: nothing waits any more.
                        st.pending_plan = None;
                        st.pending_commands.clear();
                    }
                    let plan = ran?;
                    st.note_plan(&plan);
                    return Ok(ChatReply::plan(plan));
                }
                Some(false) => {
                    st.pending_plan = None;
                    st.pending_commands.clear();
                    let plan = self.cancel(p, &id).await?;
                    return Ok(ChatReply {
                        plan: Some(plan),
                        ..ChatReply::new("cancelled", "Okay, I won't do that.")
                    });
                }
                None => {}
            }
        }
        // An answer to a question JARVIS asked.
        let mut model_input = None;
        if let Some(choice) = st.pick(input).cloned() {
            // Kept for the model's context (YOU ASKED) until it answers.
            let c = st.clarify.clone().unwrap_or_else(|| unreachable!());
            let mut commands = c.commands;
            if !commands.is_empty() && conversation::apply_choice(&mut commands, &choice) {
                st.clarify = None;
                return self
                    .propose(p, st, &c.input, commands, clock, None, t)
                    .await;
            }
            model_input = Some(format!(
                "{} (my answer to your question “{}”: {})",
                c.input, c.question, choice.title
            ));
        }
        let parsed = match model_input {
            Some(_) => Err(ParseError::Unknown(input.to_owned())),
            None => parser::parse(input, &clock.today),
        };
        match parsed {
            Ok(mut commands) => {
                st.clarify = None;
                conversation::bind_pronouns(st, &mut commands);
                self.propose(p, st, input, commands, clock, None, t).await
            }
            Err(ParseError::Unknown(step)) => match ai.model {
                Some(model) => {
                    let asked = model_input.as_deref().unwrap_or(input);
                    let mut reply = self
                        .think(p, model, ai.budget, st, asked, clock, time, t)
                        .await?;
                    reply.via = "model";
                    Ok(reply)
                }
                None => Err(JarvisError::Parse(ParseError::Unknown(step))),
            },
            Err(e) => Err(JarvisError::Parse(e)),
        }
    }

    /// Make a plan from `commands` in this conversation: it replaces any
    /// plan still awaiting approval, and an ambiguous name becomes a question.
    #[allow(clippy::too_many_arguments)]
    async fn propose<C: CommandTargets>(
        &self,
        p: &Principal,
        st: &mut State,
        input: &str,
        commands: Vec<Command>,
        clock: &Clock,
        revises: Option<String>,
        t: &C,
    ) -> Result<ChatReply, JarvisError> {
        let kept = commands.clone();
        match self
            .plan_commands(p, input, commands, clock, revises.clone(), t)
            .await
        {
            Ok(plan) => {
                let mut replaced = None;
                if plan.requires_approval {
                    if let Some(old) = st.pending_plan.take() {
                        if old != plan.id && self.cancel(p, &old).await.is_ok() {
                            replaced = Some(old);
                        }
                    }
                    st.pending_commands = kept;
                }
                st.note_plan(&plan);
                Ok(ChatReply {
                    replaces: revises.or(replaced),
                    ..ChatReply::plan(plan)
                })
            }
            Err(JarvisError::Ambiguous { message, choices }) => {
                st.clarify = Some(Clarify {
                    question: message.clone(),
                    commands: kept,
                    choices: choices.clone(),
                    input: input.to_owned(),
                });
                Ok(ChatReply::ask(message, choices))
            }
            Err(e) => Err(e),
        }
    }

    /// The model loop: up to [`agent::MAX_ROUNDS`] calls, lookups between
    /// them, then actions, a question, or a short answer.
    #[allow(clippy::too_many_arguments)]
    async fn think<M: TextModel, B: AiBudget, C: CommandTargets>(
        &self,
        p: &Principal,
        model: &M,
        budget: &B,
        st: &mut State,
        input: &str,
        clock: &Clock,
        time: Option<&str>,
        t: &C,
    ) -> Result<ChatReply, JarvisError> {
        let fallback = |say: &str| ChatReply {
            examples: parser::EXAMPLES.iter().take(4).copied().collect(),
            ..ChatReply::new("not_understood", say)
        };
        let pending = match st.pending_plan.clone() {
            Some(id) => self
                .load(p, &id)
                .await
                .ok()
                .filter(|x| x.status == PlanStatus::Pending),
            None => None,
        };
        if pending.is_none() {
            st.pending_plan = None;
            st.pending_commands.clear();
        }
        let mut data = String::new();
        let mut rejected: Option<String> = None;
        let mut turn = None;
        for round in 0..agent::MAX_ROUNDS {
            if !budget.allow().await {
                return Ok(fallback(
                    "I've used today's AI requests, so I can only follow direct commands until tomorrow.",
                ));
            }
            let last = round + 1 == agent::MAX_ROUNDS;
            let mut user = agent::context(st, clock, time, pending.as_ref(), input, &data);
            if let Some(why) = &rejected {
                user.push_str(&format!(
                    "\nYOUR LAST REPLY WAS REJECTED ({}). Reply with one JSON object as described.",
                    agent::quote(why, 200)
                ));
            }
            if last {
                user.push_str("\nNO MORE LOOKUPS: reply with actions, ask, or say.");
            }
            let reply = match model.complete(agent::SYSTEM_PROMPT, &user).await {
                Ok(r) => r,
                Err(_) => return Ok(fallback(
                    "The AI model isn't answering right now, so I can only follow direct commands.",
                )),
            };
            match agent::parse_turn(&reply) {
                Err(why) => rejected = Some(why),
                Ok(x) if !x.lookup.is_empty() && !last => {
                    rejected = None;
                    let found = self.lookups(p, st, clock, &x.lookup, t).await;
                    data.push_str(&found);
                    agent::trim_data(&mut data);
                }
                Ok(x) => {
                    turn = Some(x);
                    break;
                }
            }
        }
        let Some(turn) = turn else {
            return Ok(fallback(
                "I couldn't work that out. Try saying it another way, or use a direct command.",
            ));
        };
        let say = agent::clean_say(&turn.say);
        if let (Some(q), true) = (&turn.ask, turn.actions.is_empty()) {
            let question = agent::clean_say(&q.question);
            let choices: Vec<Choice> = q
                .options
                .iter()
                .map(|o| agent::quote(o, 120))
                .filter(|o| !o.is_empty())
                .take(MAX_OPTIONS)
                .enumerate()
                .map(|(i, o)| Choice {
                    kind: "option".into(),
                    id: (i + 1).to_string(),
                    title: o,
                })
                .collect();
            let original = st
                .clarify
                .as_ref()
                .map(|c| c.input.clone())
                .unwrap_or_else(|| input.to_owned());
            st.clarify = Some(Clarify {
                question: question.clone(),
                commands: vec![],
                choices: choices.clone(),
                input: original,
            });
            let question = if question.is_empty() {
                "Could you say a bit more?".to_owned()
            } else {
                question
            };
            return Ok(ChatReply::ask(question, choices));
        }
        st.clarify = None;
        if turn.actions.is_empty() {
            let say = if say.is_empty() {
                "I'm not sure how to help with that. I can plan your time and manage tasks, events, notes, and projects.".to_owned()
            } else {
                say
            };
            return Ok(ChatReply::new("answer", say));
        }
        let converted = self.convert(p, st, clock, turn.actions, t).await?;
        let replanning = !converted.keep.is_empty();
        let mut reply = if converted.edits.is_empty() && !replanning {
            self.propose(p, st, input, converted.commands, clock, None, t)
                .await?
        } else {
            self.revise(p, st, input, converted, clock, pending, t)
                .await?
        };
        let pending_now = reply
            .plan
            .as_ref()
            .is_some_and(|x| x.status == PlanStatus::Pending);
        if !say.is_empty() && !replanning && (pending_now || reply.status == "ask") {
            reply.say = say;
        }
        Ok(reply)
    }

    /// Change the plan awaiting approval: move or drop proposed items, or
    /// plan it again keeping some time free. The new plan replaces it.
    #[allow(clippy::too_many_arguments)]
    async fn revise<C: CommandTargets>(
        &self,
        p: &Principal,
        st: &mut State,
        input: &str,
        change: Converted,
        clock: &Clock,
        pending: Option<Plan>,
        t: &C,
    ) -> Result<ChatReply, JarvisError> {
        let Some(old) = pending else {
            return Err(JarvisError::Unresolved(
                "There's no proposal waiting for approval to change. Ask again with the new details."
                    .into(),
            ));
        };
        let replanning = !change.keep.is_empty();
        let request = st.pending_commands.clone();
        let mut commands = if replanning {
            if !change.edits.is_empty() {
                return Err(JarvisError::Unresolved(
                    "Please make one kind of change at a time.".into(),
                ));
            }
            let mut base = st.pending_commands.clone();
            let mut took = false;
            for c in base.iter_mut() {
                if let Command::ScheduleTask { keep, .. }
                | Command::ScheduleDue { keep, .. }
                | Command::Rearrange { keep, .. } = c
                {
                    keep.extend(change.keep.iter().cloned());
                    took = true;
                }
            }
            if !took {
                return Err(JarvisError::Unresolved(
                    "That proposal doesn't look for free time, so there's nothing to keep free. Say which item to move instead."
                        .into(),
                ));
            }
            base
        } else {
            edit_steps(&old, change.edits)?
        };
        commands.extend(change.commands);
        if commands.is_empty() {
            let _ = self.cancel(p, &old.id).await;
            st.pending_plan = None;
            st.pending_commands.clear();
            return Ok(ChatReply {
                replaces: Some(old.id),
                ..ChatReply::new(
                    "cancelled",
                    "That leaves nothing to do, so I dropped the proposal.",
                )
            });
        }
        let mut reply = self
            .propose(p, st, input, commands, clock, Some(old.id.clone()), t)
            .await?;
        // `propose` cancelled it if the new plan needs approval; a read-only
        // result replaces it too.
        let _ = self.cancel(p, &old.id).await;
        if st.pending_plan.as_deref() == Some(old.id.as_str()) {
            st.pending_plan = None;
        }
        if st.pending_plan.is_some() {
            if replanning {
                reply.say = "I planned it again from your request, keeping that time free. Check it, then confirm.".into();
            } else {
                // Keep the original request so "keep … free" can plan it
                // again (which redoes any moves; the reply says so).
                st.pending_commands = request;
            }
        }
        Ok(reply)
    }
}

/// The pending plan's commands with `edits` applied. Dropping a step drops
/// the steps that depend on it.
fn edit_steps(old: &Plan, edits: Vec<Edit>) -> Result<Vec<Command>, JarvisError> {
    if old
        .actions
        .iter()
        .any(|a| matches!(a.command, Command::Revert { .. }))
    {
        return Err(JarvisError::Unresolved(
            "An undo can't be edited; leave steps out when you confirm instead.".into(),
        ));
    }
    let mut steps: Vec<Option<Command>> = old
        .actions
        .iter()
        .map(|a| Some(a.command.clone()))
        .collect();
    let find = |steps: &[Option<Command>], id: &str| {
        steps
            .iter()
            .position(|s| s.as_ref().and_then(agent::step_item) == Some(id))
            .ok_or_else(|| JarvisError::Unresolved("That item isn't in the proposal.".into()))
    };
    for e in edits {
        match e {
            Edit::Drop { id } => {
                let i = find(&steps, &id)?;
                steps[i] = None;
                for j in i + 1..steps.len() {
                    let gone = old.actions[j].after.iter().any(|&k| steps[k].is_none());
                    if gone {
                        steps[j] = None;
                    }
                }
            }
            Edit::Move {
                id,
                date,
                start,
                end,
            } => {
                let i = find(&steps, &id)?;
                let Some(step) = steps[i].as_mut() else {
                    continue;
                };
                move_step(step, date, start, end)?;
            }
        }
    }
    Ok(steps.into_iter().flatten().collect())
}

fn move_step(
    step: &mut Command,
    new_date: String,
    start: String,
    end: Option<String>,
) -> Result<(), JarvisError> {
    match step {
        Command::CreateBlock {
            date,
            start_time,
            end_time,
            ..
        }
        | Command::MoveBlock {
            date,
            start_time,
            end_time,
            ..
        } => {
            let new_end = match end {
                Some(e) => e,
                None => agent::same_length(&start, start_time, end_time)?,
            };
            *date = new_date;
            *start_time = start;
            *end_time = new_end;
        }
        Command::CreateEvent {
            date,
            start_time,
            end_time,
            ..
        }
        | Command::MoveEvent {
            date,
            start_time,
            end_time,
            ..
        } => {
            *date = new_date;
            if !start.is_empty() {
                let new_end = match (end, &start_time, &end_time) {
                    (Some(e), _, _) => Some(e),
                    (None, Some(a), Some(b)) => Some(agent::same_length(&start, a, b)?),
                    (None, _, _) => None,
                };
                *start_time = Some(start);
                *end_time = new_end;
            }
        }
        _ => {
            return Err(JarvisError::Unresolved(
                "I can only move sessions and events in a proposal.".into(),
            ))
        }
    }
    Ok(())
}
