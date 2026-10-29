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
//!   what should I do [today|tomorrow]  · also: priorities, what are my priorities
//!   I have <duration>                  · also: what can I do in, suggest work for
//!     (either can add "for|in|on project <name>" to stay within one project)
//!   move|reschedule <task> to <date>   · also: set due date of, clear due date of
//!   set priority of <task> to <p>      · also: make <task> high priority, prioritize
//!   estimate <task> at <duration>
//! New tasks also take "high priority" and "takes 20 minutes" clauses.
//! Steps are separated by `;` or ` then `. Dates: today, tomorrow, a weekday
//! (next occurrence), or YYYY-MM-DD.

use super::{Command, LinkTarget, ProjectScope, TaskChange};
use crate::links::EntityKind;
use crate::tasks::Priority;
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
    "what should I do today",
    "I have 30 minutes",
    "move call the dentist to monday",
    "set priority of call the dentist to high",
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

fn word_number(w: &str) -> Option<f64> {
    if let Ok(n) = w.parse::<f64>() {
        return (n.is_finite() && n > 0.0).then_some(n);
    }
    Some(match w {
        "a" | "an" | "one" => 1.0,
        "two" => 2.0,
        "three" => 3.0,
        "four" => 4.0,
        "five" => 5.0,
        "six" => 6.0,
        "seven" => 7.0,
        "eight" => 8.0,
        "nine" => 9.0,
        "ten" => 10.0,
        "fifteen" => 15.0,
        "twenty" => 20.0,
        "thirty" => 30.0,
        "forty" => 40.0,
        "forty-five" => 45.0,
        "fifty" => 50.0,
        "sixty" => 60.0,
        "ninety" => 90.0,
        _ => return None,
    })
}

/// "30 minutes", "1 hour 30 min", "1.5 hours", "an hour and a half",
/// "half an hour", "45m". The whole of `words` must be a duration.
pub fn parse_minutes(words: &[&str]) -> Option<u32> {
    let words: Vec<String> = words
        .iter()
        .map(|w| w.trim_matches(|c: char| ",.!?".contains(c)).to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    let joined = words.join(" ");
    let special = match joined.as_str() {
        "half an hour" | "a half hour" | "half hour" => Some(30),
        "an hour and a half" | "one and a half hours" | "hour and a half" => Some(90),
        "quarter of an hour" | "a quarter hour" => Some(15),
        _ => None,
    };
    if special.is_some() {
        return special;
    }
    let mut total = 0.0;
    let mut i = 0;
    let mut any = false;
    while i < words.len() {
        let w = words[i].as_str();
        if w == "and" && any {
            i += 1;
            continue;
        }
        // "45m", "2h", "30min"
        let split = w.find(|c: char| !c.is_ascii_digit() && c != '.');
        let (num, unit, step) = match split {
            Some(at) if at > 0 => (word_number(&w[..at])?, &w[at..], 1),
            _ => (word_number(w)?, words.get(i + 1).map(String::as_str)?, 2),
        };
        let per = match unit {
            "m" | "min" | "mins" | "minute" | "minutes" => 1.0,
            "h" | "hr" | "hrs" | "hour" | "hours" => 60.0,
            _ => return None,
        };
        total += num * per;
        any = true;
        i += step;
    }
    let total = total.round();
    (any && (1.0..=24.0 * 60.0).contains(&total)).then_some(total as u32)
}

/// The longest leading run of `words` that is a duration, and the words after it.
fn leading_minutes<'a>(words: &'a [&'a str]) -> Option<(u32, &'a [&'a str])> {
    (1..=words.len().min(6))
        .rev()
        .find_map(|j| parse_minutes(&words[..j]).map(|m| (m, &words[j..])))
}

/// Planning requests, optionally limited to one project: "what should I do
/// for project garden", "I have 30 minutes in project work".
fn parse_planning(step: &str, lower: &str, today: &str) -> Option<Command> {
    let words: Vec<&str> = step.split_whitespace().collect();
    let at = (1..words.len().saturating_sub(2)).find(|&i| {
        ["for", "in", "on"]
            .iter()
            .any(|k| words[i].eq_ignore_ascii_case(k))
            && words[i + 1].eq_ignore_ascii_case("project")
    });
    let Some(i) = at else {
        return parse_planning_plain(step, lower, today);
    };
    let mut name = words[i + 2..].to_vec();
    // "… for project garden tomorrow": the day belongs to the request.
    let mut day = "";
    if name.len() > 1 {
        match name[name.len() - 1]
            .trim_end_matches(['.', '?', '!'])
            .to_lowercase()
            .as_str()
        {
            "today" => day = " today",
            "tomorrow" => day = " tomorrow",
            _ => {}
        }
        if !day.is_empty() {
            name.pop();
        }
    }
    let name = name.join(" ");
    let name = name.trim_end_matches(['.', '?', '!', ',']).trim();
    if name.is_empty() {
        return None;
    }
    let rest = format!("{}{day}", words[..i].join(" "));
    let rest_lower = rest.to_lowercase();
    let scope = Some(ProjectScope::new(name));
    match parse_planning_plain(&rest, rest_lower.trim_end_matches(['.', '?', '!']), today)? {
        Command::Priorities { date, .. } => Some(Command::Priorities {
            date,
            project: scope,
        }),
        Command::Suggest { date, minutes, .. } => Some(Command::Suggest {
            date,
            minutes,
            project: scope,
        }),
        _ => None,
    }
}

/// "what are my priorities [today|tomorrow]", "what should I do", and
/// "I have 30 minutes", "what can I do in an hour", "suggest work for 45 min".
fn parse_planning_plain(step: &str, lower: &str, today: &str) -> Option<Command> {
    let budget = strip(
        step,
        &[
            "what should i do in",
            "what should i do with",
            "what should i do for",
            "what should i work on for",
            "what should i work on in",
            "what can i do in",
            "what can i do with",
            "what can i get done in",
            "suggest work for",
            "suggest tasks for",
            "suggest something for",
            "plan my next",
            "plan the next",
            "i have got",
            "i've got",
            "i have",
            "i got",
            "i'm free for",
            "i am free for",
        ],
    );
    if let Some(rest) = budget {
        let words: Vec<&str> = rest.split_whitespace().collect();
        if let Some((minutes, _)) = leading_minutes(&words) {
            return Some(Command::Suggest {
                date: today.to_owned(),
                minutes,
                project: None,
            });
        }
    }
    let (base, date) = [
        (" for tomorrow", 1),
        (" tomorrow", 1),
        (" for today", 0),
        (" today", 0),
    ]
    .iter()
    .find_map(|(suffix, days)| lower.strip_suffix(suffix).map(|b| (b, *days)))
    .unwrap_or((lower, 0));
    let base = base
        .trim_start_matches("today's ")
        .trim_start_matches("todays ");
    matches!(
        base,
        "priorities"
            | "my priorities"
            | "top priorities"
            | "my top priorities"
            | "what are my priorities"
            | "what are my top priorities"
            | "what are the priorities"
            | "what should i do"
            | "what should i work on"
            | "what should i focus on"
            | "what's next"
            | "what is next"
            | "what's important"
            | "plan my day"
    )
    .then(|| Command::Priorities {
        date: validate::add_days(today, date),
        project: None,
    })
}

/// The Sunday ending `today`'s week (today, on a Sunday).
fn end_of_week(today: &str) -> String {
    validate::add_days(today, 6 - validate::weekday(today) as i64)
}

/// "9pm", "9 pm", "9:30pm", "21:00", "noon" → "HH:MM". The whole of `words`
/// must be the time.
pub fn parse_time(words: &[&str]) -> Option<String> {
    let joined = words
        .iter()
        .map(|w| w.trim_matches(|c: char| ",.!?".contains(c)).to_lowercase())
        .collect::<Vec<_>>()
        .join("");
    let (body, add) = match joined.as_str() {
        "noon" | "midday" => return Some("12:00".into()),
        "midnight" => return Some("24:00".into()),
        j => match (j.strip_suffix("pm"), j.strip_suffix("am")) {
            (Some(b), _) => (b, Some(12)),
            (_, Some(b)) => (b, Some(0)),
            _ => (j, None),
        },
    };
    let (h, m) = match body.split_once(':') {
        Some((h, m)) if m.len() == 2 => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        Some(_) => return None,
        // A bare number is only a time with am/pm.
        None if add.is_some() => (body.parse::<u32>().ok()?, 0),
        None => return None,
    };
    let h = match add {
        Some(_) if !(1..=12).contains(&h) => return None,
        Some(12) if h == 12 => 12,
        Some(0) if h == 12 => 0,
        Some(a) => h + a,
        None => h,
    };
    (h < 24 && m < 60).then(|| format!("{h:02}:{m:02}"))
}

/// A trailing "by friday" / "before friday" / "this week" on `rest`: the text
/// before it and the last day meant ("before" excludes the day itself).
fn split_deadline(rest: &str, today: &str) -> Result<(String, Option<String>), ParseError> {
    let words: Vec<&str> = rest.split_whitespace().collect();
    let clean = |w: &str| w.trim_end_matches(['.', '?', '!', ',']).to_lowercase();
    let n = words.len();
    if n >= 2 && clean(words[n - 1]) == "week" && clean(words[n - 2]) == "this" {
        let cut = if n >= 3 && ["by", "for", "before"].contains(&clean(words[n - 3]).as_str()) {
            n - 3
        } else {
            n - 2
        };
        return Ok((words[..cut].join(" "), Some(end_of_week(today))));
    }
    if n >= 2 {
        let key = clean(words[n - 2]);
        if ["by", "before", "until"].contains(&key.as_str()) {
            let day = clean(words[n - 1]);
            let date = parse_date(&day, today).ok_or(ParseError::BadDate(day))?;
            let date = if key == "before" {
                validate::add_days(&date, -1)
            } else {
                date
            };
            return Ok((words[..n - 2].join(" "), Some(date)));
        }
    }
    Ok((rest.to_owned(), None))
}

/// The day and hours of "my afternoon", "tomorrow morning", "friday".
fn day_part(words: &[String], today: &str) -> Option<(String, Option<String>, Option<String>)> {
    let mut date = today.to_owned();
    let mut range: (Option<&str>, Option<&str>) = (None, None);
    for w in words {
        match w.as_str() {
            "my" | "the" | "day" | "schedule" | "plan" | "plans" | "rest" | "of" => {}
            "morning" => range = (None, Some("12:00")),
            "afternoon" => range = (Some("12:00"), Some("17:00")),
            "evening" | "tonight" => range = (Some("17:00"), None),
            w => date = parse_date(w, today)?,
        }
    }
    Some((date, range.0.map(Into::into), range.1.map(Into::into)))
}

/// Scheduling, review, and undo requests (DECISIONS D-048).
fn parse_scheduling(step: &str, lower: &str, today: &str) -> Result<Option<Command>, ParseError> {
    let words: Vec<String> = lower
        .split_whitespace()
        .map(|w| w.trim_end_matches(',').to_owned())
        .collect();
    let first = words.first().map(String::as_str).unwrap_or_default();

    if matches!(first, "undo" | "revert") || lower.starts_with("take that back") {
        return Ok(Some(Command::Undo { plan_id: None }));
    }

    if lower.contains("unscheduled")
        || lower.contains("still need time")
        || lower.contains("still needs time")
    {
        let rest = lower
            .rsplit_once("unscheduled")
            .or_else(|| lower.rsplit_once("time"))
            .map(|(_, r)| r.trim())
            .unwrap_or_default();
        let to = match rest {
            "" | "this week" | "for this week" => end_of_week(today),
            "today" => today.to_owned(),
            "tomorrow" => validate::add_days(today, 1),
            r => match split_deadline(&format!("x {r}"), today)? {
                (_, Some(d)) => d,
                _ => return Err(ParseError::BadDate(r.to_owned())),
            },
        };
        return Ok(Some(Command::Unscheduled {
            from: today.to_owned(),
            to,
        }));
    }

    if let Some(rest) = strip(
        lower,
        &[
            "when am i free",
            "when'm i free",
            "what free time do i have",
            "how much free time do i have",
            "find free time",
            "show free time",
            "free time",
        ],
    ) {
        let rest = rest.trim_end_matches(['?', '.', '!']).trim();
        let rest = strip(rest, &["on", "for"]).unwrap_or(rest);
        let (from, to) = match rest {
            "" | "today" => (today.to_owned(), today.to_owned()),
            "this week" => (today.to_owned(), end_of_week(today)),
            "next week" => {
                let start = validate::add_days(&end_of_week(today), 1);
                (start.clone(), validate::add_days(&start, 6))
            }
            r => {
                let d = parse_date(r, today).ok_or_else(|| ParseError::BadDate(r.to_owned()))?;
                (d.clone(), d)
            }
        };
        return Ok(Some(Command::FreeTime {
            from,
            to,
            now: None,
        }));
    }

    if let Some(rest) = strip(
        lower,
        &[
            "what did i forget to finish",
            "what did i forget to do",
            "what did i forget",
            "what did i miss",
            "what did i not finish",
            "what didn't i finish",
            "what didn't i do",
            "what's left over from",
            "what is left over from",
        ],
    ) {
        let rest = rest.trim_end_matches(['?', '.', '!']).trim();
        let date = match rest {
            "" | "yesterday" => validate::add_days(today, -1),
            "today" => today.to_owned(),
            r => parse_date(r, today)
                .filter(|d| d.as_str() <= today)
                .ok_or_else(|| ParseError::BadDate(r.to_owned()))?,
        };
        return Ok(Some(Command::Missed { date }));
    }

    if matches!(
        first,
        "rearrange" | "reorganize" | "reorganise" | "reshuffle" | "replan" | "re-plan"
    ) {
        // "… without moving my classes": fixed items never move anyway.
        let head = lower
            .split(" without ")
            .next()
            .unwrap_or(lower)
            .split(" but ")
            .next()
            .unwrap_or(lower);
        let head: Vec<String> = head.split_whitespace().skip(1).map(str::to_owned).collect();
        if let Some((date, start, end)) = day_part(&head, today) {
            return Ok(Some(Command::Rearrange {
                date,
                start,
                end,
                keep: vec![],
            }));
        }
    }

    // "from now on, no sessions after 9pm" / "never schedule anything after
    // 21:00": standing preferences only when said to be standing.
    if lower.starts_with("from now on") || lower.starts_with("always") || lower.starts_with("never")
    {
        let w: Vec<&str> = lower.split_whitespace().collect();
        let mut prefs = crate::preferences::JarvisPrefs::default();
        if let Some(i) = w.iter().position(|x| *x == "after") {
            let t = (1..=2)
                .rev()
                .find_map(|j| w.get(i + 1..i + 1 + j).and_then(parse_time));
            prefs.avoid_after = t;
        }
        if let Some(i) = w.iter().position(|x| x.starts_with("break")) {
            if let Some((m, _)) = w.get(i.saturating_sub(3)..i).and_then(|s| {
                (0..s.len()).find_map(|k| leading_minutes(&s[k..]).filter(|(_, r)| r.is_empty()))
            }) {
                prefs.break_minutes = m;
            }
        }
        if prefs.avoid_after.is_some() || prefs.break_minutes > 0 {
            return Ok(Some(Command::Remember { prefs }));
        }
    }

    // "find 4 hours for the essay by friday", "schedule time for it",
    // "make time for my assignments before friday".
    let lead = strip(
        step,
        &[
            "find",
            "schedule",
            "block",
            "book",
            "make",
            "set aside",
            "plan",
            "reserve",
            "block out",
            "carve out",
        ],
    );
    if let Some(rest) = lead {
        let rest_words: Vec<&str> = rest.split_whitespace().collect();
        let (minutes, after) = match leading_minutes(&rest_words) {
            Some((m, after)) => (Some(m), after.to_vec()),
            None => (None, rest_words.clone()),
        };
        let mut after: &[&str] = &after;
        for filler in ["enough", "some", "more", "time", "of", "focus", "study"] {
            if after
                .first()
                .is_some_and(|w| w.eq_ignore_ascii_case(filler))
            {
                after = &after[1..];
            }
        }
        let for_word = after.first().is_some_and(|w| w.eq_ignore_ascii_case("for"));
        if for_word && (minutes.is_some() || rest_words.len() > after.len()) {
            let (query, by) = split_deadline(&after[1..].join(" "), today)?;
            let query = query.trim_end_matches(['.', '?', '!']).trim();
            let q = query.to_lowercase();
            let q = q.strip_prefix("my ").unwrap_or(&q);
            let q = q.strip_prefix("unfinished ").unwrap_or(q);
            let q = q.strip_prefix("open ").unwrap_or(q);
            let everything = matches!(
                q,
                "them"
                    | "all"
                    | "everything"
                    | "tasks"
                    | "assignments"
                    | "homework"
                    | "work"
                    | "deadlines"
                    | "everything due"
                    | "all of them"
            );
            if everything && minutes.is_none() {
                // "Before Friday" here means work due by Friday.
                let by = match split_deadline(&after[1..].join(" "), today)? {
                    (_, Some(d)) if lower.contains(" before ") => validate::add_days(&d, 1),
                    (_, Some(d)) => d,
                    _ => end_of_week(today),
                };
                return Ok(Some(Command::ScheduleDue { by, keep: vec![] }));
            }
            let query = match q {
                "it" | "that" | "this" | "this task" | "that task" => String::new(),
                _ => query.to_owned(),
            };
            return Ok(Some(Command::ScheduleTask {
                query,
                task_id: None,
                minutes,
                from: None,
                by,
                session_minutes: None,
                keep: vec![],
            }));
        }
    }
    // "find my unfinished assignments and schedule time for them before friday"
    if let Some((_, tail)) = lower.split_once(" and ") {
        if strip(tail, &["schedule", "find", "make", "block", "plan"]).is_some() {
            if let Some(c @ Command::ScheduleDue { .. }) = parse_scheduling(tail, tail, today)? {
                return Ok(Some(c));
            }
        }
    }
    Ok(None)
}

fn priority_word(w: &str) -> Option<Priority> {
    match w
        .trim_matches(|c: char| ",.!?".contains(c))
        .to_lowercase()
        .as_str()
    {
        "high" | "urgent" | "top" => Some(Priority::High),
        "medium" | "normal" => Some(Priority::Medium),
        "low" => Some(Priority::Low),
        "no" | "none" => Some(Priority::None),
        _ => None,
    }
}

/// Words that may follow a priority or estimate clause inside a new task.
const CLAUSE_END: [&str; 7] = ["due", "by", "in", "to", "for", "takes", "taking"];

fn clause_ends(words: &[&str], at: usize) -> bool {
    at == words.len() || CLAUSE_END.iter().any(|k| words[at].eq_ignore_ascii_case(k))
}

/// Take "high priority" / "priority high" and "takes 20 minutes" out of a new
/// task's text, when they end it or come before another clause (due, in project).
fn split_extras(rest: &str) -> (String, Option<Priority>, Option<u32>) {
    let mut words: Vec<&str> = rest.split_whitespace().collect();
    let mut priority = None;
    let mut estimate = None;
    let mut i = 1;
    while i < words.len() {
        let w = words[i];
        let next = words.get(i + 1).copied().unwrap_or("");
        let is_priority = |s: &str| {
            s.trim_end_matches([',', '.'])
                .eq_ignore_ascii_case("priority")
        };
        if priority.is_none() && is_priority(next) && clause_ends(&words, i + 2) {
            if let Some(p) = priority_word(w) {
                priority = Some(p);
                words.drain(i..i + 2);
                continue;
            }
        }
        if priority.is_none() && is_priority(w) && clause_ends(&words, i + 2) {
            if let Some(p) = priority_word(next) {
                priority = Some(p);
                words.drain(i..i + 2);
                continue;
            }
        }
        if estimate.is_none()
            && (w.eq_ignore_ascii_case("takes") || w.eq_ignore_ascii_case("taking"))
        {
            let found = (i + 2..=words.len().min(i + 7))
                .rev()
                .find(|&j| clause_ends(&words, j) && parse_minutes(&words[i + 1..j]).is_some());
            if let Some(j) = found {
                estimate = parse_minutes(&words[i + 1..j]);
                words.drain(i..j);
                continue;
            }
        }
        i += 1;
    }
    (words.join(" "), priority, estimate)
}

/// Changes to an existing task: deadline, priority, estimate.
fn parse_update(step: &str, today: &str) -> Result<Option<Command>, ParseError> {
    let update = |query: &str, change: TaskChange| {
        let query = query
            .trim()
            .trim_matches(|c: char| ",.!?\"“”".contains(c))
            .trim();
        if query.is_empty() {
            Err(ParseError::MissingText("which task"))
        } else {
            Ok(Some(Command::UpdateTask {
                query: query.to_owned(),
                change,
                task_id: None,
                title: None,
            }))
        }
    };
    let date_of = |w: &str| parse_date(w, today).ok_or_else(|| ParseError::BadDate(w.to_owned()));
    // "<verb> <task> to <rest>": the last " to " splits task from value.
    let split_to = |rest: &str| -> Option<(String, String)> {
        let words: Vec<&str> = rest.split_whitespace().collect();
        let at = (1..words.len().saturating_sub(1)).rev().find(|&i| {
            ["to", "for", "at"]
                .iter()
                .any(|k| words[i].eq_ignore_ascii_case(k))
        })?;
        Some((words[..at].join(" "), words[at + 1..].join(" ")))
    };
    if let Some(rest) = strip(
        step,
        &[
            "set the priority of",
            "set priority of",
            "set priority for",
            "change the priority of",
            "change priority of",
        ],
    ) {
        let usage = ParseError::MissingText("a priority, like: set priority of taxes to high");
        let (task, value) = split_to(rest).ok_or(usage.clone())?;
        let priority = priority_word(&value).ok_or(usage)?;
        return update(
            &task,
            TaskChange {
                priority: Some(priority),
                ..Default::default()
            },
        );
    }
    if let Some(rest) = strip(step, &["make", "mark"]) {
        let words: Vec<&str> = rest.split_whitespace().collect();
        let n = words.len();
        if n >= 3
            && words[n - 1]
                .trim_end_matches(['.', '!'])
                .eq_ignore_ascii_case("priority")
        {
            if let Some(p) = priority_word(words[n - 2]) {
                let end = if n >= 4 && words[n - 3].eq_ignore_ascii_case("as") {
                    n - 3
                } else {
                    n - 2
                };
                return update(
                    &words[..end].join(" "),
                    TaskChange {
                        priority: Some(p),
                        ..Default::default()
                    },
                );
            }
        }
        return Ok(None);
    }
    if let Some(rest) = strip(step, &["prioritize", "prioritise"]) {
        return update(
            rest,
            TaskChange {
                priority: Some(Priority::High),
                ..Default::default()
            },
        );
    }
    if let Some(rest) = strip(step, &["deprioritize", "deprioritise"]) {
        return update(
            rest,
            TaskChange {
                priority: Some(Priority::Low),
                ..Default::default()
            },
        );
    }
    if let Some(rest) = strip(
        step,
        &[
            "clear the due date of",
            "clear due date of",
            "clear the due date for",
            "clear due date for",
            "remove the due date from",
            "remove due date from",
        ],
    ) {
        return update(
            rest,
            TaskChange {
                due_date: Some(None),
                ..Default::default()
            },
        );
    }
    if let Some(rest) = strip(
        step,
        &[
            "set the due date of",
            "set due date of",
            "set due date for",
            "change the due date of",
            "change due date of",
            "reschedule",
            "move",
            "push",
        ],
    ) {
        let usage = ParseError::MissingText("a date, like: move taxes to friday");
        let (task, value) = split_to(rest).ok_or(usage)?;
        return update(
            &task,
            TaskChange {
                due_date: Some(Some(date_of(&value)?)),
                ..Default::default()
            },
        );
    }
    if let Some(rest) = strip(
        step,
        &[
            "set the estimate of",
            "set estimate of",
            "set estimate for",
            "estimate",
        ],
    ) {
        let usage = ParseError::MissingText("a length, like: estimate taxes at 45 minutes");
        let (task, value) = split_to(rest).ok_or(usage.clone())?;
        let words: Vec<&str> = value.split_whitespace().collect();
        let minutes = parse_minutes(&words).ok_or(usage)?;
        return update(
            &task,
            TaskChange {
                estimate_minutes: Some(minutes),
                ..Default::default()
            },
        );
    }
    Ok(None)
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
    if let Some(c) = parse_planning(step, lower, today) {
        return Ok(c);
    }
    if let Some(c) = parse_scheduling(step, lower, today)? {
        return Ok(c);
    }
    if let Some(c) = parse_update(step, today)? {
        return Ok(c);
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
        let (rest, priority, estimate_minutes) = split_extras(require(rest, "task title")?);
        let (rest, project) = split_project(&rest);
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
        // Dictation ends sentences with a period; it isn't part of the title.
        let title = title.trim_end_matches('.').trim().to_owned();
        if title.is_empty() {
            return Err(ParseError::MissingText("task title"));
        }
        return Ok(Command::CreateTask {
            id: None,
            title,
            due_date,
            project,
            project_id: None,
            priority,
            estimate_minutes,
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
                id: None,
                title: "Call the dentist".into(),
                due_date: None,
                project: None,
                project_id: None,
                priority: None,
                estimate_minutes: None
            }
        );
        assert_eq!(
            one("Please add task Call the dentist due Friday"),
            Command::CreateTask {
                id: None,
                title: "Call the dentist".into(),
                due_date: Some("2026-10-23".into()),
                project: None,
                project_id: None,
                priority: None,
                estimate_minutes: None
            }
        );
        assert_eq!(
            one("todo renew passport by 2026-12-01"),
            Command::CreateTask {
                id: None,
                title: "renew passport".into(),
                due_date: Some("2026-12-01".into()),
                project: None,
                project_id: None,
                priority: None,
                estimate_minutes: None
            }
        );
        assert_eq!(
            one("remind me to water plants due tomorrow"),
            Command::CreateTask {
                id: None,
                title: "water plants".into(),
                due_date: Some("2026-10-22".into()),
                project: None,
                project_id: None,
                priority: None,
                estimate_minutes: None
            }
        );
        // "by" followed by a non-date is just part of the title.
        assert_eq!(
            one("task walk by the river"),
            Command::CreateTask {
                id: None,
                title: "walk by the river".into(),
                due_date: None,
                project: None,
                project_id: None,
                priority: None,
                estimate_minutes: None
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
                id: None,
                title: "日本語".into(),
                due_date: None,
                project: None,
                project_id: None,
                priority: None,
                estimate_minutes: None
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
            id: None,
            title: title.into(),
            due_date: due.map(Into::into),
            project: project.map(Into::into),
            project_id: None,
            priority: None,
            estimate_minutes: None,
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

    #[test]
    fn durations() {
        let m = |s: &str| parse_minutes(&s.split_whitespace().collect::<Vec<_>>());
        assert_eq!(m("30 minutes"), Some(30));
        assert_eq!(m("1 hour 30 min"), Some(90));
        assert_eq!(m("1 hour and 15 minutes."), Some(75));
        assert_eq!(m("1.5 hours"), Some(90));
        assert_eq!(m("an hour"), Some(60));
        assert_eq!(m("half an hour"), Some(30));
        assert_eq!(m("an hour and a half"), Some(90));
        assert_eq!(m("45m"), Some(45));
        assert_eq!(m("2h"), Some(120));
        assert_eq!(m("twenty minutes"), Some(20));
        assert_eq!(m("minutes"), None);
        assert_eq!(m("30 apples"), None);
        assert_eq!(m("30"), None);
        assert_eq!(m("25 hours"), None);
    }

    #[test]
    fn planning_and_update_phrases() {
        let one = |s: &str| parse(s, TODAY).unwrap().remove(0);
        let suggest = |minutes| Command::Suggest {
            date: TODAY.into(),
            minutes,
            project: None,
        };
        assert_eq!(one("I have 30 minutes"), suggest(30));
        assert_eq!(
            one("Jarvis, I've got an hour. What should I do?"),
            suggest(60)
        );
        assert_eq!(one("what can I do in 45 min?"), suggest(45));
        assert_eq!(one("suggest work for 2 hours"), suggest(120));
        let top = |date: &str| Command::Priorities {
            date: date.into(),
            project: None,
        };
        assert_eq!(one("What should I do today?"), top(TODAY));
        assert_eq!(one("what are my priorities"), top(TODAY));
        assert_eq!(one("priorities for tomorrow"), top("2026-10-22"));
        assert_eq!(one("Today's priorities."), top(TODAY));
        let scoped = |s: &str| match one(s) {
            Command::Priorities { date, project } => (date, project.map(|p| p.query)),
            Command::Suggest { date, project, .. } => (date, project.map(|p| p.query)),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            scoped("what should I do for project Home renovation tomorrow?"),
            ("2026-10-22".into(), Some("Home renovation".into()))
        );
        assert_eq!(
            scoped("I have 30 minutes in project work."),
            (TODAY.into(), Some("work".into()))
        );
        // A project clause on other commands is left to them.
        assert!(matches!(
            one("add task paint for project shed"),
            Command::CreateTask { .. }
        ));

        let up = |q: &str, change: TaskChange| Command::UpdateTask {
            query: q.into(),
            change,
            task_id: None,
            title: None,
        };
        let pri = |p| TaskChange {
            priority: Some(p),
            ..Default::default()
        };
        let due = |d: Option<&str>| TaskChange {
            due_date: Some(d.map(Into::into)),
            ..Default::default()
        };
        assert_eq!(
            one("set priority of taxes to high"),
            up("taxes", pri(Priority::High))
        );
        assert_eq!(
            one("make walk to the park low priority"),
            up("walk to the park", pri(Priority::Low))
        );
        assert_eq!(
            one("mark taxes as urgent priority"),
            up("taxes", pri(Priority::High))
        );
        assert_eq!(one("prioritize taxes"), up("taxes", pri(Priority::High)));
        assert_eq!(
            one("move talk to Sam to friday"),
            up("talk to Sam", due(Some("2026-10-23")))
        );
        assert_eq!(
            one("reschedule taxes for tomorrow."),
            up("taxes", due(Some("2026-10-22")))
        );
        assert_eq!(one("clear the due date of taxes"), up("taxes", due(None)));
        assert_eq!(
            one("estimate taxes at 1 hour"),
            up(
                "taxes",
                TaskChange {
                    estimate_minutes: Some(60),
                    ..Default::default()
                }
            )
        );
        assert!(matches!(
            parse("move taxes to someday", TODAY),
            Err(ParseError::BadDate(_))
        ));
        assert!(matches!(
            parse("set priority of taxes to soon", TODAY),
            Err(ParseError::MissingText(_))
        ));
        // "mark done" still completes a task.
        assert!(matches!(
            one("mark done taxes"),
            Command::CompleteTask { .. }
        ));

        // Priority and estimate clauses on a new task; elsewhere they stay in the title.
        match one("Add task file taxes high priority due Friday.") {
            Command::CreateTask {
                id: None,
                title,
                priority,
                due_date,
                ..
            } => assert_eq!(
                (title.as_str(), priority, due_date.as_deref()),
                ("file taxes", Some(Priority::High), Some("2026-10-23"))
            ),
            other => panic!("{other:?}"),
        }
        match one("add task review high priority bugs takes 45 minutes in project work") {
            Command::CreateTask {
                id: None,
                title,
                priority,
                estimate_minutes,
                project,
                ..
            } => assert_eq!(
                (
                    title.as_str(),
                    priority,
                    estimate_minutes,
                    project.as_deref()
                ),
                ("review high priority bugs", None, Some(45), Some("work"))
            ),
            other => panic!("{other:?}"),
        }
    }

    fn one(input: &str) -> Command {
        let mut v = parse(input, TODAY).unwrap();
        assert_eq!(v.len(), 1, "{input}");
        v.remove(0)
    }

    #[test]
    fn clock_times() {
        let t = |s: &str| parse_time(&s.split_whitespace().collect::<Vec<_>>());
        assert_eq!(t("9pm").as_deref(), Some("21:00"));
        assert_eq!(t("9 pm").as_deref(), Some("21:00"));
        assert_eq!(t("9:30am").as_deref(), Some("09:30"));
        assert_eq!(t("12am").as_deref(), Some("00:00"));
        assert_eq!(t("12pm").as_deref(), Some("12:00"));
        assert_eq!(t("21:15").as_deref(), Some("21:15"));
        assert_eq!(t("noon").as_deref(), Some("12:00"));
        assert_eq!(t("9"), None, "a bare number isn't a time");
        assert_eq!(t("13pm"), None);
        assert_eq!(t("25:00"), None);
    }

    #[test]
    fn scheduling_requests() {
        // Conversational phrasing isn't guessed at: it goes to the model.
        assert!(matches!(
            parse(
                "I've got an assignment due Friday. Find four hours for it.",
                TODAY
            ),
            Err(ParseError::Unknown(_))
        ));
        assert_eq!(
            one("find four hours for the essay by friday"),
            Command::ScheduleTask {
                query: "the essay".into(),
                task_id: None,
                minutes: Some(240),
                from: None,
                by: Some("2026-10-23".into()),
                session_minutes: None,
                keep: vec![],
            }
        );
        match one("schedule time for it before friday") {
            Command::ScheduleTask {
                query, minutes, by, ..
            } => assert_eq!(
                (query.as_str(), minutes, by.as_deref()),
                ("", None, Some("2026-10-22"))
            ),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            one("Find my unfinished assignments and schedule enough time for them before Friday."),
            Command::ScheduleDue {
                by: "2026-10-23".into(),
                keep: vec![]
            }
        );
        assert_eq!(
            one("make time for my assignments this week"),
            Command::ScheduleDue {
                by: "2026-10-25".into(),
                keep: vec![]
            }
        );
        // Unchanged meanings.
        assert!(matches!(one("find dentist"), Command::Find { .. }));
        assert!(matches!(
            one("schedule for friday"),
            Command::Agenda { date } if date == "2026-10-23"
        ));
    }

    #[test]
    fn review_rearrange_undo_and_standing_preferences() {
        assert_eq!(
            one("What's still unscheduled this week?"),
            Command::Unscheduled {
                from: TODAY.into(),
                to: "2026-10-25".into()
            }
        );
        assert_eq!(
            one("when am i free tomorrow"),
            Command::FreeTime {
                from: "2026-10-22".into(),
                to: "2026-10-22".into(),
                now: None
            }
        );
        assert_eq!(
            one("What did I forget to finish yesterday?"),
            Command::Missed {
                date: "2026-10-20".into()
            }
        );
        assert_eq!(
            one("Reorganize my afternoon without moving my classes."),
            Command::Rearrange {
                date: TODAY.into(),
                start: Some("12:00".into()),
                end: Some("17:00".into()),
                keep: vec![]
            }
        );
        assert_eq!(
            one("rearrange tomorrow morning"),
            Command::Rearrange {
                date: "2026-10-22".into(),
                start: None,
                end: Some("12:00".into()),
                keep: vec![]
            }
        );
        for undo in [
            "Undo the last change.",
            "undo that",
            "Undo the changes you just made",
        ] {
            assert_eq!(one(undo), Command::Undo { plan_id: None }, "{undo}");
        }
        match one("From now on, no study sessions after 9pm and 10 minute breaks") {
            Command::Remember { prefs } => {
                assert_eq!(prefs.avoid_after.as_deref(), Some("21:00"));
                assert_eq!(prefs.break_minutes, 10);
            }
            other => panic!("{other:?}"),
        }
        // A one-off isn't promoted to a standing preference.
        assert!(!matches!(
            parse("no sessions after 9pm", TODAY),
            Ok(ref v) if matches!(v[0], Command::Remember { .. })
        ));
    }
}
