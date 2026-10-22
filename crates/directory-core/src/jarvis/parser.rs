//! Deterministic text → command parser (the default `IntentProvider`).
//!
//! Grammar (case-insensitive keywords, content kept verbatim):
//!   add task <title> [due|by <date>]   · also: task, todo, remind me to
//!   complete <query>                   · also: done, finish, complete task
//!   note <text>                        · also: add note, new note
//!   journal <text>                     · also: log, diary
//!   capture <text>                     · also: inbox, remember
//!   list tasks                         · also: tasks, open tasks, what's open
//!   find <text>                        · also: search, search for, look up
//! Steps are separated by `;` or ` then `. Dates: today, tomorrow, a weekday
//! (next occurrence), or YYYY-MM-DD.

use super::Command;
use crate::validate;

pub const MAX_STEPS: usize = 5;

/// Examples shown when input isn't understood.
pub const EXAMPLES: &[&str] = &[
    "add task call the dentist due friday",
    "complete dentist",
    "note gate code is 4512",
    "journal long walk by the river",
    "capture look into solar panels",
    "list tasks",
    "find passport",
    "add task buy milk; note milk was out of stock",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    TooManySteps,
    /// Text of the step that wasn't understood.
    Unknown(String),
    /// Keyword recognized but required content missing, e.g. "add task".
    MissingText(&'static str),
    BadDate(String),
}

const WEEKDAYS: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

/// Resolve a date word relative to `today` (YYYY-MM-DD).
pub fn parse_date(word: &str, today: &str) -> Option<String> {
    let w = word.trim().trim_end_matches(['.', '!', ',']).to_lowercase();
    match w.as_str() {
        "today" => return Some(today.to_owned()),
        "tomorrow" => return Some(validate::add_days(today, 1)),
        _ => {}
    }
    if let Some(i) = WEEKDAYS.iter().position(|d| *d == w || d[..3] == w) {
        let now = validate::weekday(today) as i64;
        let ahead = (i as i64 - now).rem_euclid(7);
        return Some(validate::add_days(
            today,
            if ahead == 0 { 7 } else { ahead },
        ));
    }
    validate::date(&w).ok()
}

/// Split on `;` and on the word "then" between steps.
fn steps(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    for chunk in input.split(';') {
        let mut current: Vec<&str> = Vec::new();
        for word in chunk.split_whitespace() {
            if word.eq_ignore_ascii_case("then") && !current.is_empty() {
                out.push(current.join(" "));
                current.clear();
            } else {
                current.push(word);
            }
        }
        if !current.is_empty() {
            out.push(current.join(" "));
        }
    }
    out
}

/// If `text` starts with one of `prefixes` (case-insensitive, whole words),
/// return the remainder (trimmed).
fn strip<'a>(text: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    prefixes.iter().find_map(|p| {
        // Compare in place (ASCII case-folding) so slicing stays on char boundaries.
        if !text.is_char_boundary(p.len()) || !text[..p.len()].eq_ignore_ascii_case(p) {
            return None;
        }
        let rest = &text[p.len()..];
        (rest.is_empty() || rest.starts_with(' ')).then(|| rest.trim())
    })
}

fn require<'a>(rest: &'a str, what: &'static str) -> Result<&'a str, ParseError> {
    if rest.is_empty() {
        Err(ParseError::MissingText(what))
    } else {
        Ok(rest)
    }
}

/// Split a trailing "due <date>" / "by <date>" off a task title.
fn split_due(rest: &str, today: &str) -> Result<(String, Option<String>), ParseError> {
    let words: Vec<&str> = rest.split_whitespace().collect();
    if words.len() >= 3 {
        let (kw, date) = (
            words[words.len() - 2].to_lowercase(),
            words[words.len() - 1],
        );
        if kw == "due" || kw == "by" {
            return match parse_date(date, today) {
                Some(d) => Ok((words[..words.len() - 2].join(" "), Some(d))),
                None if kw == "due" => Err(ParseError::BadDate(date.to_owned())),
                // "by" is also ordinary English ("by the river"): keep it as title.
                None => Ok((rest.to_owned(), None)),
            };
        }
    }
    Ok((rest.to_owned(), None))
}

fn parse_step(step: &str, today: &str) -> Result<Command, ParseError> {
    let step = strip(step, &["please", "jarvis,", "jarvis"]).unwrap_or(step);
    let lower = step.to_lowercase();
    let lower = lower.trim_end_matches(['.', '?', '!']);
    if matches!(
        lower,
        "list tasks" | "show tasks" | "tasks" | "open tasks" | "what's open" | "what is open"
    ) {
        return Ok(Command::ListTasks);
    }
    if let Some(rest) = strip(
        step,
        &[
            "add task",
            "new task",
            "create task",
            "remind me to",
            "task",
            "todo",
        ],
    ) {
        let (title, due_date) = split_due(require(rest, "task title")?, today)?;
        return Ok(Command::CreateTask { title, due_date });
    }
    if let Some(rest) = strip(
        step,
        &["complete task", "mark done", "complete", "finish", "done"],
    ) {
        return Ok(Command::CompleteTask {
            query: require(rest, "which task")?.to_owned(),
            task_id: None,
            title: None,
        });
    }
    if let Some(rest) = strip(step, &["add note", "new note", "note"]) {
        return Ok(Command::CreateNote {
            body: require(rest, "note text")?.to_owned(),
        });
    }
    if let Some(rest) = strip(step, &["journal", "diary", "log"]) {
        return Ok(Command::AppendJournal {
            date: today.to_owned(),
            text: require(rest, "journal text")?.to_owned(),
        });
    }
    if let Some(rest) = strip(step, &["search for", "look up", "search", "find"]) {
        let query = require(rest, "what to find")?.trim_end_matches(['?', '.']);
        return Ok(Command::Find {
            query: query.to_owned(),
        });
    }
    if let Some(rest) = strip(step, &["capture", "inbox", "remember"]) {
        return Ok(Command::Capture {
            text: require(rest, "text to capture")?.to_owned(),
        });
    }
    Err(ParseError::Unknown(step.to_owned()))
}

/// Parse user text into one or more commands. `today` is the user's local date.
pub fn parse(input: &str, today: &str) -> Result<Vec<Command>, ParseError> {
    let parts = steps(input);
    if parts.is_empty() {
        return Err(ParseError::Empty);
    }
    if parts.len() > MAX_STEPS {
        return Err(ParseError::TooManySteps);
    }
    parts.iter().map(|s| parse_step(s, today)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODAY: &str = "2026-10-21"; // a Wednesday

    fn one(input: &str) -> Command {
        let mut v = parse(input, TODAY).unwrap();
        assert_eq!(v.len(), 1, "{input}");
        v.remove(0)
    }

    #[test]
    fn tasks_with_due_dates() {
        assert_eq!(
            one("add task Call the dentist"),
            Command::CreateTask {
                title: "Call the dentist".into(),
                due_date: None
            }
        );
        assert_eq!(
            one("Please add task Call the dentist due Friday"),
            Command::CreateTask {
                title: "Call the dentist".into(),
                due_date: Some("2026-10-23".into())
            }
        );
        assert_eq!(
            one("todo renew passport by 2026-12-01"),
            Command::CreateTask {
                title: "renew passport".into(),
                due_date: Some("2026-12-01".into())
            }
        );
        assert_eq!(
            one("remind me to water plants due tomorrow"),
            Command::CreateTask {
                title: "water plants".into(),
                due_date: Some("2026-10-22".into())
            }
        );
        // "by" followed by a non-date is just part of the title.
        assert_eq!(
            one("task walk by the river"),
            Command::CreateTask {
                title: "walk by the river".into(),
                due_date: None
            }
        );
        assert_eq!(
            parse("add task x due someday", TODAY),
            Err(ParseError::BadDate("someday".into()))
        );
        assert_eq!(
            parse("add task", TODAY),
            Err(ParseError::MissingText("task title"))
        );
    }

    #[test]
    fn other_commands_keep_text_verbatim() {
        assert_eq!(
            one("note Gate code is 4512"),
            Command::CreateNote {
                body: "Gate code is 4512".into()
            }
        );
        assert_eq!(
            one("journal Long walk."),
            Command::AppendJournal {
                date: TODAY.into(),
                text: "Long walk.".into()
            }
        );
        assert_eq!(
            one("capture Look into solar"),
            Command::Capture {
                text: "Look into solar".into()
            }
        );
        assert_eq!(
            one("done dentist"),
            Command::CompleteTask {
                query: "dentist".into(),
                task_id: None,
                title: None
            }
        );
        assert_eq!(one("What's open?"), Command::ListTasks);
        assert_eq!(
            one("search for passport?"),
            Command::Find {
                query: "passport".into()
            }
        );
        // Keywords must be whole words.
        assert_eq!(
            parse("notebook shopping", TODAY),
            Err(ParseError::Unknown("notebook shopping".into()))
        );
    }

    #[test]
    fn multi_step() {
        let v = parse(
            "add task buy milk; note milk was out then journal went shopping",
            TODAY,
        )
        .unwrap();
        assert_eq!(v.len(), 3);
        assert!(matches!(v[2], Command::AppendJournal { .. }));
        assert_eq!(parse("  ;  ", TODAY), Err(ParseError::Empty));
        assert_eq!(parse("a;b;c;d;e;f", TODAY), Err(ParseError::TooManySteps));
    }

    #[test]
    fn non_ascii_input_never_panics() {
        for s in [
            "\u{212A}",
            "\u{212A}ask x",
            "ñote",
            "日本語",
            "task 日本語のタスク",
            "é",
        ] {
            let _ = parse(s, TODAY);
        }
        assert_eq!(
            one("task 日本語"),
            Command::CreateTask {
                title: "日本語".into(),
                due_date: None
            }
        );
    }

    #[test]
    fn weekdays_are_next_occurrence() {
        assert_eq!(
            parse_date("wednesday", TODAY).as_deref(),
            Some("2026-10-28"),
            "same weekday → next week"
        );
        assert_eq!(parse_date("thu", TODAY).as_deref(), Some("2026-10-22"));
        assert_eq!(parse_date("Sunday.", TODAY).as_deref(), Some("2026-10-25"));
        assert_eq!(parse_date("2026-02-30", TODAY), None);
    }
}
