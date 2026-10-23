//! Deterministic text → command parser (the default `IntentProvider`).
//!
//! Grammar (case-insensitive keywords, content kept verbatim):
//!   add task <title> [in|to|for project <name>] [due|by <date>]
//!                                      · also: task, todo, remind me to
//!   new project <name>                 · also: add project, create project
//!   list projects                      · also: projects, show projects
//!   agenda [<date>]                    · also: what's on [<date>], schedule
//!   check in <habit> [today|yesterday|YYYY-MM-DD]  · also: check off
//!   complete <query>                   · also: done, finish, complete task
//!   note <text>                        · also: add note, new note
//!   journal <text>                     · also: log, diary
//!   capture <text>                     · also: inbox, remember
//!   list tasks                         · also: tasks, open tasks, what's open
//!   find <text>                        · also: search, search for, look up
//!   link <kind> <name> to|with <kind> <name>   · kinds: task, note, project, journal (a date)
//! Steps are separated by `;` or ` then `. Dates: today, tomorrow, a weekday
//! (next occurrence), or YYYY-MM-DD.

use super::{Command, LinkTarget};
use crate::links::EntityKind;
use crate::validate;

pub const MAX_STEPS: usize = 5;

/// Examples shown when input isn't understood.
pub const EXAMPLES: &[&str] = &[
    "add task call the dentist due friday",
    "new project garden; add task plant tomatoes in project garden",
    "list projects",
    "what's on tomorrow",
    "check in stretch",
    "link note trip ideas to task book flights",
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

/// Split "… in project <name>" (or `to`/`for`) off a task. A due date may sit
/// before the clause or at the very end, after the project name.
fn split_project(rest: &str) -> (String, Option<String>) {
    let words: Vec<&str> = rest.split_whitespace().collect();
    let at = (1..words.len().saturating_sub(2)).rev().find(|&i| {
        ["in", "to", "for"]
            .iter()
            .any(|k| words[i].eq_ignore_ascii_case(k))
            && words[i + 1].eq_ignore_ascii_case("project")
    });
    match at {
        Some(i) => (words[..i].join(" "), Some(words[i + 2..].join(" "))),
        None => (rest.to_owned(), None),
    }
}

fn link_kind(word: &str) -> Option<EntityKind> {
    Some(match word.to_ascii_lowercase().as_str() {
        "task" => EntityKind::Task,
        "note" => EntityKind::Note,
        "project" => EntityKind::Project,
        "journal" | "day" => EntityKind::Journal,
        _ => return None,
    })
}

/// `<kind> <name> to|with <kind> <name>`; a journal "name" is a date.
fn parse_link(rest: &str, today: &str) -> Result<Command, ParseError> {
    let words: Vec<&str> = rest.split_whitespace().collect();
    let usage =
        ParseError::MissingText("two items, like: link note trip ideas to task book flights");
    let a_kind = words
        .first()
        .and_then(|w| link_kind(w))
        .ok_or(usage.clone())?;
    let split = (2..words.len().saturating_sub(1)).find(|&i| {
        ["to", "with"]
            .iter()
            .any(|k| words[i].eq_ignore_ascii_case(k))
            && link_kind(words[i + 1]).is_some()
    });
    let Some(i) = split.filter(|&i| i + 2 < words.len()) else {
        return Err(usage);
    };
    let side = |kind: EntityKind, name: String| -> Result<LinkTarget, ParseError> {
        if kind != EntityKind::Journal {
            return Ok(LinkTarget::new(kind, name));
        }
        let date = if name.eq_ignore_ascii_case("yesterday") {
            Some(validate::add_days(today, -1))
        } else {
            parse_date(&name, today)
        };
        date.map(|d| LinkTarget::new(kind, d))
            .ok_or(ParseError::BadDate(name))
    };
    Ok(Command::LinkItems {
        a: side(a_kind, words[1..i].join(" "))?,
        b: side(
            link_kind(words[i + 1]).unwrap_or(EntityKind::Note),
            words[i + 2..].join(" "),
        )?,
    })
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
    if matches!(
        lower,
        "list projects" | "show projects" | "projects" | "active projects"
    ) {
        return Ok(Command::ListProjects);
    }
    if let Some(rest) = strip(
        step,
        &[
            "what's on",
            "what is on",
            "agenda for",
            "agenda",
            "schedule for",
            "schedule",
        ],
    ) {
        let rest = rest.trim_end_matches(['?', '.', '!']).trim();
        let rest = strip(rest, &["for"]).unwrap_or(rest);
        let date = if rest.is_empty() {
            today.to_owned()
        } else if rest.eq_ignore_ascii_case("yesterday") {
            validate::add_days(today, -1)
        } else {
            parse_date(rest, today).ok_or_else(|| ParseError::BadDate(rest.to_owned()))?
        };
        return Ok(Command::Agenda { date });
    }
    if let Some(rest) = strip(step, &["link"]) {
        return parse_link(require(rest, "what to link")?, today);
    }
    if let Some(rest) = strip(step, &["check in", "check off", "checkin"]) {
        let rest = require(rest, "which habit")?;
        // An optional trailing day: today, yesterday, or YYYY-MM-DD (not weekdays,
        // which would mean the *next* one).
        let (name, date) = match rest.rsplit_once(' ') {
            Some((name, last)) if last.eq_ignore_ascii_case("today") => (name, today.to_owned()),
            Some((name, last)) if last.eq_ignore_ascii_case("yesterday") => {
                (name, validate::add_days(today, -1))
            }
            Some((name, last)) if validate::date(last).is_ok() => (name, last.to_owned()),
            _ => (rest, today.to_owned()),
        };
        return Ok(Command::CheckHabit {
            query: name.trim().to_owned(),
            date,
            habit_id: None,
            name: None,
        });
    }
    if let Some(rest) = strip(
        step,
        &[
            "new project",
            "add project",
            "create project",
            "start project",
        ],
    ) {
        return Ok(Command::CreateProject {
            name: require(rest, "project name")?.to_owned(),
        });
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
        let (rest, project) = split_project(require(rest, "task title")?);
        let (title, project, due_date) = match project {
            // "… in project garden due friday": the date trails the project name.
            Some(name) => match split_due(&name, today)? {
                (name, Some(due)) => (rest, Some(name), Some(due)),
                (name, None) => {
                    let (title, due) = split_due(&rest, today)?;
                    (title, Some(name), due)
                }
            },
            None => {
                let (title, due) = split_due(&rest, today)?;
                (title, None, due)
            }
        };
        return Ok(Command::CreateTask {
            title,
            due_date,
            project,
            project_id: None,
        });
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
                due_date: None,
                project: None,
                project_id: None
            }
        );
        assert_eq!(
            one("Please add task Call the dentist due Friday"),
            Command::CreateTask {
                title: "Call the dentist".into(),
                due_date: Some("2026-10-23".into()),
                project: None,
                project_id: None
            }
        );
        assert_eq!(
            one("todo renew passport by 2026-12-01"),
            Command::CreateTask {
                title: "renew passport".into(),
                due_date: Some("2026-12-01".into()),
                project: None,
                project_id: None
            }
        );
        assert_eq!(
            one("remind me to water plants due tomorrow"),
            Command::CreateTask {
                title: "water plants".into(),
                due_date: Some("2026-10-22".into()),
                project: None,
                project_id: None
            }
        );
        // "by" followed by a non-date is just part of the title.
        assert_eq!(
            one("task walk by the river"),
            Command::CreateTask {
                title: "walk by the river".into(),
                due_date: None,
                project: None,
                project_id: None
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
                due_date: None,
                project: None,
                project_id: None
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

    #[test]
    fn project_commands_and_task_project_clauses() {
        let one = |s: &str| parse(s, TODAY).unwrap().remove(0);
        assert_eq!(one("list projects"), Command::ListProjects);
        assert_eq!(
            one("new project Home renovation"),
            Command::CreateProject {
                name: "Home renovation".into()
            }
        );
        let task = |title: &str, project: Option<&str>, due: Option<&str>| Command::CreateTask {
            title: title.into(),
            due_date: due.map(Into::into),
            project: project.map(Into::into),
            project_id: None,
        };
        assert_eq!(
            one("add task plant tomatoes in project garden due friday"),
            task("plant tomatoes", Some("garden"), Some("2026-10-23"))
        );
        assert_eq!(
            one("todo buy paint due tomorrow for project Home renovation"),
            task("buy paint", Some("Home renovation"), Some("2026-10-22")),
            "the date may also come before the clause"
        );
        assert_eq!(
            one("add task review project plan"),
            task("review project plan", None, None),
            "`project` only starts a clause after in/to/for"
        );
        assert_eq!(
            one("add task in project"),
            task("in project", None, None),
            "a clause needs a title before it and a name after it"
        );
        assert_eq!(
            one("add task move boxes to project Garage"),
            task("move boxes", Some("Garage"), None)
        );
        assert_eq!(
            parse("new project", TODAY),
            Err(ParseError::MissingText("project name"))
        );
    }

    #[test]
    fn agenda_and_habit_check_ins() {
        let one = |s: &str| parse(s, TODAY).unwrap().remove(0);
        let agenda = |d: &str| Command::Agenda { date: d.into() };
        assert_eq!(one("agenda"), agenda(TODAY));
        assert_eq!(one("What's on tomorrow?"), agenda("2026-10-22"));
        assert_eq!(one("schedule for friday"), agenda("2026-10-23"));
        assert_eq!(one("what's on yesterday"), agenda("2026-10-20"));
        assert_eq!(
            parse("agenda someday", TODAY),
            Err(ParseError::BadDate("someday".into()))
        );

        let check = |q: &str, d: &str| Command::CheckHabit {
            query: q.into(),
            date: d.into(),
            habit_id: None,
            name: None,
        };
        assert_eq!(one("check in stretch"), check("stretch", TODAY));
        assert_eq!(
            one("check in morning walk yesterday"),
            check("morning walk", "2026-10-20")
        );
        assert_eq!(
            one("check off read 2026-10-01"),
            check("read", "2026-10-01")
        );
        assert_eq!(
            one("check in friday prayers"),
            check("friday prayers", TODAY)
        );
        assert_eq!(
            parse("check in", TODAY),
            Err(ParseError::MissingText("which habit"))
        );
    }
}

#[cfg(test)]
mod link_tests {
    use super::*;
    const TODAY: &str = "2026-10-21";

    #[test]
    fn link_commands_parse_both_sides() {
        let one = |s: &str| parse(s, TODAY).unwrap().remove(0);
        assert_eq!(
            one("link note trip ideas to task book flights"),
            Command::LinkItems {
                a: LinkTarget::new(EntityKind::Note, "trip ideas"),
                b: LinkTarget::new(EntityKind::Task, "book flights"),
            }
        );
        // "to" inside a name is fine when it isn't followed by a kind.
        assert_eq!(
            one("link task talk to Sam with day yesterday"),
            Command::LinkItems {
                a: LinkTarget::new(EntityKind::Task, "talk to Sam"),
                b: LinkTarget::new(EntityKind::Journal, "2026-10-20"),
            }
        );
        assert!(matches!(
            parse("link journal someday to note x", TODAY),
            Err(ParseError::BadDate(_))
        ));
        for bad in [
            "link",
            "link trip ideas to flights",
            "link note x to task",
            "link note to task y",
        ] {
            assert!(
                matches!(parse(bad, TODAY), Err(ParseError::MissingText(_))),
                "{bad}"
            );
        }
    }
}
