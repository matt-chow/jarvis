//! Optional AI intent parsing (DECISIONS D-043).
//!
//! When the deterministic parser doesn't understand a request, a text model
//! may turn it into the same typed commands. The model sees only the sentence
//! and today's date: no tasks, no ids, no database. It can only *propose*
//! commands from a restricted set ([`AiCommand`], which has no id fields).
//! They are then validated, resolved by name, and approved exactly like
//! parsed input. Any model failure falls back to the parser's own error.

use super::parser::{self, ParseError, MAX_STEPS};
use super::{Command, IntentProvider, JarvisError, TaskChange, MAX_INPUT};
use crate::tasks::Priority;
use crate::validate;
use serde::Deserialize;

/// A chat-completion backend (OpenRouter in the Worker; a fake in tests).
pub trait TextModel {
    /// One completion for `system` + `user`; `Err` describes the failure (logged).
    async fn complete(&self, system: &str, user: &str) -> Result<String, String>;
}

/// Whether one more AI call is allowed now (the daily cap).
pub trait AiBudget {
    async fn allow(&self) -> bool;
}

/// Only model ids that are free on OpenRouter (`…:free`) may be configured:
/// no paid model, no automatic router.
pub fn is_free_model(id: &str) -> bool {
    let id = id.trim();
    id.ends_with(":free")
        && id.len() <= 100
        && !id.starts_with("openrouter/")
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/-_.:".contains(c))
}

/// What the model may ask for. No ids: names are resolved by Directory.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AiCommand {
    CreateTask {
        title: String,
        #[serde(default)]
        due_date: Option<String>,
        #[serde(default)]
        priority: Option<Priority>,
        #[serde(default)]
        estimate_minutes: Option<u32>,
        #[serde(default)]
        project: Option<String>,
    },
    UpdateTask {
        task: String,
        #[serde(default)]
        due_date: Option<String>,
        #[serde(default)]
        clear_due_date: bool,
        #[serde(default)]
        priority: Option<Priority>,
        #[serde(default)]
        estimate_minutes: Option<u32>,
    },
    CompleteTask {
        task: String,
    },
    Priorities {
        #[serde(default)]
        date: Option<String>,
    },
    Suggest {
        minutes: u32,
    },
    ListTasks,
    Agenda {
        #[serde(default)]
        date: Option<String>,
    },
    Capture {
        text: String,
    },
    Find {
        query: String,
    },
}

pub const SYSTEM_PROMPT: &str = r#"You convert one spoken request for a personal task app into JSON commands.
Reply with ONLY a JSON array (no prose, no code fences). Each element is one of:
{"type":"create_task","title":"…","due_date":"YYYY-MM-DD"?,"priority":"high|medium|low"?,"estimate_minutes":N?,"project":"name"?}
{"type":"update_task","task":"words from the task's title","due_date":"YYYY-MM-DD"?,"clear_due_date":true?,"priority":"high|medium|low|none"?,"estimate_minutes":N?}
{"type":"complete_task","task":"words from the task's title"}
{"type":"priorities","date":"YYYY-MM-DD"?}   (what to do / focus on)
{"type":"suggest","minutes":N}                (what fits in N free minutes)
{"type":"list_tasks"}
{"type":"agenda","date":"YYYY-MM-DD"?}        (schedule / calendar for a day)
{"type":"capture","text":"…"}                 (anything else worth remembering)
{"type":"find","query":"…"}
Rules: at most 5 elements. Resolve relative dates against TODAY. Keep titles short and in the user's words.
If the request is not about tasks, planning, the agenda, or remembering something, reply [].
Never invent tasks the user did not mention."#;

/// The user message for one request.
pub fn user_prompt(input: &str, today: &str) -> String {
    let weekday = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ][validate::weekday(today) as usize];
    format!("TODAY: {today} ({weekday})\nREQUEST: {input}")
}

/// The JSON array in a model reply (tolerating code fences or stray prose).
fn json_part(reply: &str) -> Option<&str> {
    let start = reply.find('[')?;
    let end = reply.rfind(']')?;
    (end > start).then(|| &reply[start..=end])
}

fn date_or(today: &str, d: Option<String>) -> Result<String, JarvisError> {
    match d {
        None => Ok(today.to_owned()),
        Some(d) => validate::date(&d).map_err(|w| JarvisError::Invalid("date", w)),
    }
}

fn text(s: String, what: &'static str) -> Result<String, JarvisError> {
    let s = s.trim().to_owned();
    if s.is_empty() || s.chars().count() > MAX_INPUT {
        return Err(JarvisError::Invalid(what, "empty or too long"));
    }
    Ok(s)
}

/// Turn a model reply into commands, validating everything the model chose.
pub fn commands_from_reply(reply: &str, today: &str) -> Result<Vec<Command>, JarvisError> {
    let not_understood = || JarvisError::Invalid("ai", "reply was not valid commands");
    let raw: Vec<AiCommand> = serde_json::from_str(json_part(reply).ok_or_else(not_understood)?)
        .map_err(|_| not_understood())?;
    if raw.is_empty() {
        return Err(JarvisError::Invalid("ai", "no commands"));
    }
    if raw.len() > MAX_STEPS {
        return Err(JarvisError::Parse(ParseError::TooManySteps));
    }
    let minutes = |m: Option<u32>| match m {
        Some(m) if m == 0 || m > crate::tasks::MAX_ESTIMATE => Err(JarvisError::Invalid(
            "estimate_minutes",
            "must be 1 to 1440",
        )),
        m => Ok(m),
    };
    raw.into_iter()
        .map(|c| {
            Ok(match c {
                AiCommand::CreateTask {
                    title,
                    due_date,
                    priority,
                    estimate_minutes,
                    project,
                } => Command::CreateTask {
                    title: text(title, "title")?,
                    due_date: due_date.map(|d| date_or(today, Some(d))).transpose()?,
                    project: project.map(|p| text(p, "project")).transpose()?,
                    project_id: None,
                    priority,
                    estimate_minutes: minutes(estimate_minutes)?,
                },
                AiCommand::UpdateTask {
                    task,
                    due_date,
                    clear_due_date,
                    priority,
                    estimate_minutes,
                } => Command::UpdateTask {
                    query: text(task, "task")?,
                    change: TaskChange {
                        due_date: match (clear_due_date, due_date) {
                            (true, _) => Some(None),
                            (false, Some(d)) => Some(Some(date_or(today, Some(d))?)),
                            (false, None) => None,
                        },
                        priority,
                        estimate_minutes: minutes(estimate_minutes)?,
                    },
                    task_id: None,
                    title: None,
                },
                AiCommand::CompleteTask { task } => Command::CompleteTask {
                    query: text(task, "task")?,
                    task_id: None,
                    title: None,
                },
                AiCommand::Priorities { date } => Command::Priorities {
                    date: date_or(today, date)?,
                },
                AiCommand::Suggest { minutes } => {
                    if minutes == 0 || minutes > crate::planner::MAX_AVAILABLE {
                        return Err(JarvisError::Invalid("minutes", "must be 1 to 960"));
                    }
                    Command::Suggest {
                        date: today.to_owned(),
                        minutes,
                    }
                }
                AiCommand::ListTasks => Command::ListTasks,
                AiCommand::Agenda { date } => Command::Agenda {
                    date: date_or(today, date)?,
                },
                AiCommand::Capture { text: t } => Command::Capture {
                    text: text(t, "text")?,
                },
                AiCommand::Find { query } => Command::Find {
                    query: text(query, "query")?,
                },
            })
        })
        .collect()
}

/// The parser first; the model only for sentences the parser doesn't know.
pub struct ParserThenModel<'a, M, B> {
    pub model: Option<&'a M>,
    pub budget: &'a B,
}

impl<M: TextModel, B: AiBudget> IntentProvider for ParserThenModel<'_, M, B> {
    async fn interpret(&self, input: &str, today: &str) -> Result<Vec<Command>, JarvisError> {
        let parsed = parser::parse(input, today);
        let err = match parsed {
            Ok(commands) => return Ok(commands),
            // Only "didn't understand" goes to the model; a recognised command
            // with a bad date or missing text is the user's to fix.
            Err(e @ ParseError::Unknown(_)) => e,
            Err(e) => return Err(JarvisError::Parse(e)),
        };
        let Some(model) = self.model else {
            return Err(JarvisError::Parse(err));
        };
        if !self.budget.allow().await {
            return Err(JarvisError::Parse(err));
        }
        match model
            .complete(SYSTEM_PROMPT, &user_prompt(input, today))
            .await
        {
            Ok(reply) => commands_from_reply(&reply, today).map_err(|_| JarvisError::Parse(err)),
            Err(_) => Err(JarvisError::Parse(err)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::block_on;
    use std::cell::Cell;

    const TODAY: &str = "2026-10-21";

    struct Fake(Result<String, String>, Cell<u32>);
    impl TextModel for Fake {
        async fn complete(&self, system: &str, user: &str) -> Result<String, String> {
            assert!(system.contains("JSON"));
            assert!(user.starts_with("TODAY: 2026-10-21 (Wednesday)"));
            self.1.set(self.1.get() + 1);
            self.0.clone()
        }
    }
    struct Budget(bool);
    impl AiBudget for Budget {
        async fn allow(&self) -> bool {
            self.0
        }
    }

    fn run(model: Option<&Fake>, allow: bool, input: &str) -> Result<Vec<Command>, JarvisError> {
        let p = ParserThenModel {
            model,
            budget: &Budget(allow),
        };
        block_on(p.interpret(input, TODAY))
    }

    #[test]
    fn free_models_only() {
        assert!(is_free_model("meta-llama/llama-3.3-70b-instruct:free"));
        assert!(!is_free_model("meta-llama/llama-3.3-70b-instruct"));
        assert!(!is_free_model("openrouter/auto:free"));
        assert!(!is_free_model("x:free?y"));
        assert!(!is_free_model(""));
    }

    #[test]
    fn parser_first_model_only_for_unknown_sentences() {
        let model = Fake(Ok(r#"[{"type":"list_tasks"}]"#.into()), Cell::new(0));
        assert_eq!(
            run(Some(&model), true, "list tasks").unwrap(),
            vec![Command::ListTasks]
        );
        assert_eq!(model.1.get(), 0, "parser handled it");
        // A recognised command with a bad date is not sent to the model.
        assert!(matches!(
            run(Some(&model), true, "move taxes to someday"),
            Err(JarvisError::Parse(ParseError::BadDate(_)))
        ));
        assert_eq!(model.1.get(), 0);
        assert_eq!(
            run(Some(&model), true, "anything on my plate?").unwrap(),
            vec![Command::ListTasks]
        );
        assert_eq!(model.1.get(), 1);
        // No model, or over budget: the parser's error.
        assert!(matches!(
            run(None, true, "anything on my plate?"),
            Err(JarvisError::Parse(ParseError::Unknown(_)))
        ));
        assert!(matches!(
            run(Some(&model), false, "anything on my plate?"),
            Err(JarvisError::Parse(ParseError::Unknown(_)))
        ));
        assert_eq!(model.1.get(), 1);
        // Model failures fall back to the parser's error too.
        let down = Fake(Err("503".into()), Cell::new(0));
        assert!(matches!(
            run(Some(&down), true, "anything on my plate?"),
            Err(JarvisError::Parse(ParseError::Unknown(_)))
        ));
        let junk = Fake(Ok("Sure! Here you go.".into()), Cell::new(0));
        assert!(matches!(
            run(Some(&junk), true, "anything on my plate?"),
            Err(JarvisError::Parse(ParseError::Unknown(_)))
        ));
    }

    #[test]
    fn replies_are_validated_and_carry_no_ids() {
        let reply = "```json\n[{\"type\":\"create_task\",\"title\":\" Call mom \",\"due_date\":\"2026-10-23\",\"priority\":\"high\",\"task_id\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"project_id\":\"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"},{\"type\":\"update_task\",\"task\":\"taxes\",\"clear_due_date\":true},{\"type\":\"suggest\",\"minutes\":45}]\n```";
        let got = commands_from_reply(reply, TODAY).unwrap();
        assert_eq!(
            got[0],
            Command::CreateTask {
                title: "Call mom".into(),
                due_date: Some("2026-10-23".into()),
                project: None,
                project_id: None,
                priority: Some(Priority::High),
                estimate_minutes: None,
            }
        );
        assert!(
            matches!(&got[1], Command::UpdateTask { change, task_id: None, .. } if change.due_date == Some(None))
        );
        assert_eq!(
            got[2],
            Command::Suggest {
                date: TODAY.into(),
                minutes: 45
            }
        );
        let bad = |r: &str| commands_from_reply(r, TODAY).is_err();
        assert!(bad(
            r#"[{"type":"create_task","title":"x","due_date":"soon"}]"#
        ));
        assert!(bad(r#"[{"type":"suggest","minutes":0}]"#));
        assert!(bad(r#"[{"type":"delete_everything"}]"#));
        assert!(bad(r#"[{"type":"create_task","title":"   "}]"#));
        assert!(bad("[]"));
        assert!(bad(&format!(
            "[{}]",
            [r#"{"type":"list_tasks"}"#; 6].join(",")
        )));
    }
}
