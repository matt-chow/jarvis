//! Spoken answers for JARVIS on iPhone (Apple Shortcuts, DECISIONS D-042).
//!
//! The Shortcut dictates text, posts it, and speaks back `speech`. This turns
//! plans and errors into one short sentence, and reads a dictated yes/no.
//! It changes nothing about planning, approval, or execution.

use super::parser::ParseError;
use super::{JarvisError, Plan, PlanStatus};
use serde::Serialize;

/// The reply to one `ask` (see docs/JARVIS_SHORTCUTS.md for the contract).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Answer {
    pub speech: String,
    /// `executed`, `pending`, `failed`, `cancelled`, `expired`, `running`,
    /// `not_understood`, `unresolved`, `invalid`, or `not_found`.
    pub status: &'static str,
    pub needs_confirmation: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
}

fn answer(speech: impl Into<String>, status: &'static str) -> Answer {
    Answer {
        speech: speech.into(),
        status,
        needs_confirmation: false,
        plan_id: None,
    }
}

const YES: &[&str] = &[
    "yes",
    "yeah",
    "yep",
    "yup",
    "sure",
    "ok",
    "okay",
    "confirm",
    "confirmed",
    "do it",
    "go ahead",
    "please do",
    "approve",
    "approved",
    "correct",
    "affirmative",
    "yes please",
    "sounds good",
];

const NO: &[&str] = &[
    "no",
    "nope",
    "nah",
    "cancel",
    "stop",
    "don't",
    "do not",
    "never mind",
    "nevermind",
    "no thanks",
    "no thank you",
    "abort",
    "forget it",
];

/// A dictated confirmation: `Some(true)` yes, `Some(false)` no, `None` unclear.
/// Only a whole answer counts ("yes", "Yes, do it."), never a yes inside a sentence.
pub fn yes_or_no(text: &str) -> Option<bool> {
    let clean: String = text
        .chars()
        .map(|c| match c {
            '’' => '\'',
            c if c.is_alphanumeric() || c == '\'' || c == ' ' => c,
            _ => ' ',
        })
        .collect::<String>()
        .to_lowercase();
    let words: Vec<&str> = clean.split_whitespace().collect();
    let phrase = words.join(" ");
    let first_is = |list: &[&str]| {
        list.iter()
            .any(|w| phrase == *w || phrase.starts_with(&format!("{w} ")))
    };
    // A single-word answer anywhere in the reply ("yes no") makes it unclear.
    let has = |list: &[&str]| words.iter().any(|w| list.contains(w));
    let yes = first_is(YES) && !has(NO);
    let no = first_is(NO) && !has(YES);
    match (yes, no) {
        (true, false) if words.len() <= 4 => Some(true),
        (false, true) if words.len() <= 4 => Some(false),
        _ => None,
    }
}

/// What to say about a plan: a preview to confirm, or the outcome.
pub fn from_plan(plan: &Plan) -> Answer {
    let results = || {
        plan.results
            .iter()
            .map(|r| r.message.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    };
    match plan.status {
        PlanStatus::Pending => {
            let steps: Vec<&str> = plan.actions.iter().map(|a| a.summary.as_str()).collect();
            let speech = match steps.as_slice() {
                [one] => format!("{one}. Say yes to confirm."),
                many => format!(
                    "{} steps: {}. Say yes to confirm.",
                    many.len(),
                    many.join("; ")
                ),
            };
            Answer {
                speech,
                status: "pending",
                needs_confirmation: true,
                plan_id: Some(plan.id.clone()),
            }
        }
        PlanStatus::Executed => answer(results(), "executed"),
        PlanStatus::Failed => answer(results(), "failed"),
        PlanStatus::Cancelled => answer("Okay, cancelled.", "cancelled"),
        PlanStatus::Expired => answer(
            "That request expired, so nothing changed. Please ask again.",
            "expired",
        ),
        PlanStatus::Running => answer("Still working on that.", "running"),
    }
}

/// What to say when a request can't run. `None` for backend failures, which
/// the caller reports as a server error.
pub fn from_error(e: &JarvisError) -> Option<Answer> {
    Some(match e {
        JarvisError::Parse(p) => answer(
            match p {
                ParseError::Empty => "I didn't catch anything. Please try again.".to_owned(),
                ParseError::TooManySteps => {
                    "That's too many steps at once. Try up to five.".to_owned()
                }
                ParseError::Unknown(step) => {
                    format!("Sorry, I don't know how to “{step}” yet. Try: what should I do today.")
                }
                ParseError::MissingText(what) => format!("I need {what}."),
                ParseError::BadDate(d) => format!("I don't understand the date “{d}”."),
            },
            "not_understood",
        ),
        JarvisError::Invalid(field, why) => answer(format!("Invalid {field}: {why}."), "invalid"),
        JarvisError::Unresolved(message) => answer(message.clone(), "unresolved"),
        JarvisError::NotFound => answer(
            "I couldn't find that request. Please ask again.",
            "not_found",
        ),
        JarvisError::NotPending(status) => match status {
            PlanStatus::Executed | PlanStatus::Failed | PlanStatus::Running => {
                answer("That was already done.", "executed")
            }
            PlanStatus::Cancelled => answer("That was already cancelled.", "cancelled"),
            PlanStatus::Expired | PlanStatus::Pending => answer(
                "That request expired, so nothing changed. Please ask again.",
                "expired",
            ),
        },
        JarvisError::Backend(_) => return None,
    })
}

/// The reply when a confirmation answer wasn't clearly yes or no.
pub fn ask_again(plan_id: &str) -> Answer {
    Answer {
        speech: "Please say yes or no.".into(),
        status: "pending",
        needs_confirmation: true,
        plan_id: Some(plan_id.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{ActionResult, Command, PlannedAction, Risk};
    use super::*;

    fn plan(status: PlanStatus, actions: &[&str], results: &[&str]) -> Plan {
        Plan {
            id: "p".repeat(32),
            owner_id: "u".into(),
            input: "x".into(),
            actions: actions
                .iter()
                .map(|s| PlannedAction {
                    command: Command::ListTasks,
                    summary: (*s).into(),
                    risk: Risk::Write,
                })
                .collect(),
            status,
            requires_approval: true,
            created_at: 0,
            expires_at: 1,
            results: results
                .iter()
                .map(|m| ActionResult {
                    ok: true,
                    message: (*m).into(),
                    entity: None,
                    changed: false,
                })
                .collect(),
        }
    }

    #[test]
    fn dictated_answers() {
        for y in [
            "Yes",
            "yes.",
            "Yeah, do it",
            "OK",
            "go ahead",
            "Yes please!",
            "Sure.",
        ] {
            assert_eq!(yes_or_no(y), Some(true), "{y}");
        }
        for n in ["No", "no.", "Cancel", "Never mind", "Don’t", "nope, stop"] {
            assert_eq!(yes_or_no(n), Some(false), "{n}");
        }
        for unclear in [
            "",
            "maybe",
            "yes no",
            "okay so add another task about the garden",
            "I don't know",
        ] {
            assert_eq!(yes_or_no(unclear), None, "{unclear}");
        }
    }

    #[test]
    fn plans_become_one_spoken_line() {
        let one = from_plan(&plan(PlanStatus::Pending, &["Create task “Buy milk”"], &[]));
        assert_eq!(one.speech, "Create task “Buy milk”. Say yes to confirm.");
        assert!(one.needs_confirmation && one.plan_id.is_some());
        let two = from_plan(&plan(PlanStatus::Pending, &["A", "B"], &[]));
        assert_eq!(two.speech, "2 steps: A; B. Say yes to confirm.");
        let done = from_plan(&plan(
            PlanStatus::Executed,
            &["A"],
            &["Created task “A”.", "Done."],
        ));
        assert_eq!(
            (done.speech.as_str(), done.status),
            ("Created task “A”. Done.", "executed")
        );
        assert!(!done.needs_confirmation && done.plan_id.is_none());
        assert_eq!(
            from_plan(&plan(PlanStatus::Cancelled, &[], &[])).status,
            "cancelled"
        );
    }

    #[test]
    fn errors_are_spoken_and_repeats_are_safe() {
        let e = from_error(&JarvisError::Parse(ParseError::Unknown("dance".into()))).unwrap();
        assert_eq!(e.status, "not_understood");
        assert!(e.speech.contains("“dance”"));
        let again = from_error(&JarvisError::NotPending(PlanStatus::Executed)).unwrap();
        assert_eq!(again.speech, "That was already done.");
        assert!(from_error(&JarvisError::Backend("x".into())).is_none());
        assert_eq!(ask_again("p").speech, "Please say yes or no.");
    }
}
