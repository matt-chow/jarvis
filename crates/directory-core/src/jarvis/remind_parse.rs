//! Reminder and category phrases for the deterministic parser (D-049):
//! "remind me to … tomorrow at 8 pm", "remind me every Monday and Wednesday
//! to …", "snooze my assignment reminder for one hour", "mark my dentist
//! reminder as complete", "show me my overdue reminders", "add School and
//! Important to my assignment task". Dates and times are the owner's local
//! wall clock; the plan fills in the zone.

use super::parser::{parse_date, parse_minutes, parse_time, word_number, ParseError};
use super::remind::{CategoryTarget, ReminderChange, ReminderView};
use super::Command;
use crate::reminders::recurrence::{Freq, Rule};
use crate::tasks::Priority;
use crate::validate;

const WEEKDAYS: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

fn weekday_index(w: &str) -> Option<u32> {
    let w = w.trim_end_matches('s');
    WEEKDAYS
        .iter()
        .position(|d| *d == w || (w.len() >= 3 && d.starts_with(w) && w.len() <= d.len()))
        .map(|i| i as u32)
}

fn clean(w: &str) -> String {
    w.trim_matches(|c: char| ",.!?;:".contains(c))
        .to_lowercase()
}

/// When a reminder is for, read off the ends of its text.
#[derive(Debug, Default, PartialEq, Eq)]
struct When {
    date: Option<String>,
    time: Option<String>,
    after_minutes: Option<u32>,
    repeat: Option<Rule>,
}

fn rule(freq: Freq, interval: u32, weekdays: Vec<u32>) -> Rule {
    Rule {
        freq,
        interval,
        weekdays,
        until: None,
        anchor: String::new(),
    }
}

/// "every monday and wednesday", "every 2 weeks", "daily", "every weekday".
fn parse_repeat(words: &[String]) -> Option<Rule> {
    let one = |w: &str| match w {
        "daily" => Some(rule(Freq::Daily, 1, vec![])),
        "weekly" => Some(rule(Freq::Weekly, 1, vec![])),
        "monthly" => Some(rule(Freq::Monthly, 1, vec![])),
        "yearly" | "annually" => Some(rule(Freq::Yearly, 1, vec![])),
        _ => None,
    };
    match words {
        [w] => return one(w),
        [first, rest @ ..] if first == "every" || first == "each" => {
            let unit = |w: &str| match w.trim_end_matches('s') {
                "day" => Some(Freq::Daily),
                "week" => Some(Freq::Weekly),
                "month" => Some(Freq::Monthly),
                "year" => Some(Freq::Yearly),
                _ => None,
            };
            match rest {
                [w] if w == "weekday" || w == "weekdays" => {
                    return Some(rule(Freq::Weekly, 1, vec![0, 1, 2, 3, 4]))
                }
                [w] if unit(w).is_some() => return Some(rule(unit(w)?, 1, vec![])),
                [n, w] if unit(w).is_some() => {
                    let n = word_number(n).filter(|n| n.fract() == 0.0 && *n >= 1.0)?;
                    return Some(rule(unit(w)?, n as u32, vec![]));
                }
                _ => {}
            }
            let mut days = Vec::new();
            for w in rest {
                match w.as_str() {
                    "and" | "&" | "on" => {}
                    w => days.push(weekday_index(w)?),
                }
            }
            if days.is_empty() {
                return None;
            }
            days.sort_unstable();
            days.dedup();
            return Some(rule(Freq::Weekly, 1, days));
        }
        _ => {}
    }
    None
}

/// One date phrase: "today", "tomorrow", "friday", "on friday",
/// "next friday", "in three days", "in 2 weeks", "2026-11-20".
fn parse_day(words: &[String], today: &str) -> Option<String> {
    match words {
        [w] => parse_date(w, today).filter(|_| w != "next"),
        [on, w] if ["on", "next", "this", "for"].contains(&on.as_str()) => parse_date(w, today),
        [w, n, unit] if w == "in" => {
            let n = word_number(n).filter(|n| n.fract() == 0.0 && *n >= 1.0 && *n <= 400.0)?;
            let per = match unit.trim_end_matches('s') {
                "day" => 1,
                "week" => 7,
                _ => return None,
            };
            Some(validate::add_days(today, n as i64 * per))
        }
        [a, w] if a == "a" && w == "week" => Some(validate::add_days(today, 7)),
        _ => None,
    }
}

/// One time phrase: "at 8 pm", "at 20:00", "8pm", "at noon", "tonight".
fn parse_clock(words: &[String]) -> Option<String> {
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["tonight"] => Some("20:00".into()),
        ["this", "evening"] => Some("18:00".into()),
        ["this", "morning"] | ["in", "the", "morning"] => Some("09:00".into()),
        ["in", "the", "afternoon"] | ["this", "afternoon"] => Some("14:00".into()),
        ["at", rest @ ..] if !rest.is_empty() && rest.len() <= 2 => parse_time(rest),
        w if w.len() <= 2 && w.first().is_some_and(|x| x.ends_with('m') || w.len() == 2) => {
            parse_time(w).filter(|_| {
                let j = w.join("");
                j.ends_with("am") || j.ends_with("pm")
            })
        }
        _ => None,
    }
}

/// "in 2 hours", "in 30 minutes" (relative to the plan's time).
fn parse_after(words: &[String]) -> Option<u32> {
    match words {
        [w, rest @ ..] if w == "in" && !rest.is_empty() => {
            let rest: Vec<&str> = rest.iter().map(String::as_str).collect();
            parse_minutes(&rest)
        }
        _ => None,
    }
}

/// Take date, time, and repeat phrases off the end of `words` (and off the
/// start, before a "to"), leaving the reminder's text.
fn split_when(words: &[&str], today: &str) -> (Vec<String>, When) {
    let mut w: Vec<String> = words.iter().map(|x| x.to_string()).collect();
    let lower = |v: &[String]| v.iter().map(|x| clean(x)).collect::<Vec<_>>();
    let mut when = When::default();
    // Trailing phrases, longest first.
    for _ in 0..4 {
        let l = lower(&w);
        let n = l.len();
        let mut took = false;
        for len in (1..=n.min(6)).rev() {
            let tail = &l[n - len..];
            if when.repeat.is_none() {
                if let Some(r) = parse_repeat(tail) {
                    when.repeat = Some(r);
                    w.truncate(n - len);
                    took = true;
                    break;
                }
            }
            if when.time.is_none() && when.after_minutes.is_none() {
                if let Some(t) = parse_clock(tail) {
                    when.time = Some(t);
                    w.truncate(n - len);
                    took = true;
                    break;
                }
                if let Some(m) = parse_after(tail) {
                    when.after_minutes = Some(m);
                    w.truncate(n - len);
                    took = true;
                    break;
                }
            }
            if when.date.is_none() && when.after_minutes.is_none() {
                if let Some(d) = parse_day(tail, today) {
                    when.date = Some(d);
                    w.truncate(n - len);
                    took = true;
                    break;
                }
            }
        }
        if !took || w.is_empty() {
            break;
        }
    }
    // "every monday and wednesday to review my notes": the schedule leads.
    let l = lower(&w);
    if let Some(cut) = l.iter().position(|x| x == "to").filter(|c| *c > 0) {
        let (head, rest) = split_when(&words[..cut], today);
        if head.is_empty() && cut + 1 < w.len() {
            let merged = When {
                date: when.date.or(rest.date),
                time: when.time.or(rest.time),
                after_minutes: when.after_minutes.or(rest.after_minutes),
                repeat: when.repeat.or(rest.repeat),
            };
            return (w[cut + 1..].to_vec(), merged);
        }
    }
    (w, when)
}

/// The first date on or after `from` that a weekly rule's weekdays allow.
fn first_on(from: &str, days: &[u32]) -> String {
    (0..7)
        .map(|i| validate::add_days(from, i))
        .find(|d| days.contains(&validate::weekday(d)))
        .unwrap_or_else(|| from.to_owned())
}

fn create(rest: &str, today: &str) -> Result<Command, ParseError> {
    let words: Vec<&str> = rest.split_whitespace().collect();
    let (title, when) = split_when(&words, today);
    let mut title: Vec<String> = title;
    while title
        .first()
        .is_some_and(|w| ["to", "about", "for", "that", "i", "should"].contains(&clean(w).as_str()))
    {
        title.remove(0);
    }
    let title = title.join(" ");
    let title = title.trim_end_matches(['.', '!', ',']).trim().to_owned();
    if title.is_empty() {
        return Err(ParseError::MissingText("what to be reminded of"));
    }
    let low = title.to_lowercase();
    let project = (low.contains("this project") || low.contains("the project"))
        .then(|| "this project".to_owned());
    let task = (low.contains("this task")).then(|| "this task".to_owned());
    let date = match (&when.date, &when.repeat) {
        (Some(d), _) => Some(d.clone()),
        (None, Some(r)) if !r.weekdays.is_empty() => Some(first_on(today, &r.weekdays)),
        (None, Some(_)) => Some(today.to_owned()),
        (None, None) => None,
    };
    Ok(Command::CreateReminder {
        id: None,
        title,
        due_date: date,
        due_time: when.time,
        after_minutes: when.after_minutes,
        repeat: when.repeat,
        task,
        task_id: None,
        project,
        project_id: None,
        priority: None,
        timezone: None,
        with_task: false,
    })
}

/// The reminder a phrase names: "my dentist reminder" → "dentist".
fn reminder_query(words: &[String]) -> String {
    words
        .iter()
        .filter(|w| {
            !["my", "the", "reminder", "reminders", "a", "that", "this"]
                .contains(&clean(w).as_str())
        })
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
}

fn has_word(words: &[String], w: &str) -> bool {
    words.iter().any(|x| clean(x) == w)
}

/// "School and Important", "School, Important", "the School category".
fn category_names(words: &[String]) -> Vec<String> {
    let joined = words.join(" ");
    let mut out = Vec::new();
    for part in joined.split([',', '&']) {
        for name in part.split(" and ") {
            let name: Vec<&str> = name
                .split_whitespace()
                .filter(|w| {
                    ![
                        "the",
                        "category",
                        "categories",
                        "tag",
                        "tags",
                        "label",
                        "labels",
                    ]
                    .contains(&clean(w).as_str())
                })
                .collect();
            let name = name.join(" ");
            let name = name.trim_matches(|c: char| "\"'“”".contains(c)).trim();
            if !name.is_empty() && !out.iter().any(|x: &String| x.eq_ignore_ascii_case(name)) {
                out.push(name.to_owned());
            }
        }
    }
    out
}

fn priority(w: &str) -> Option<Priority> {
    match w {
        "high" | "urgent" | "important" => Some(Priority::High),
        "medium" | "normal" => Some(Priority::Medium),
        "low" => Some(Priority::Low),
        "no" | "none" => Some(Priority::None),
        _ => None,
    }
}

/// Category assignment: "add School and Important to my assignment task",
/// "tag my essay task with School", "remove School from my essay task".
fn parse_categories(words: &[String], l: &[String]) -> Option<Command> {
    let noun = |w: &str| match w {
        "task" => Some(CategoryTarget::Task),
        "reminder" => Some(CategoryTarget::Reminder),
        _ => None,
    };
    let verb = l.first()?.as_str();
    if ["tag", "label", "categorize", "categorise"].contains(&verb) {
        // "tag my essay task with School": the item precedes "with".
        let with = l.iter().position(|w| w == "with" || w == "as")?;
        let target = noun(l.get(with.checked_sub(1)?)?)?;
        let q = reminder_query(&words[1..with - 1]);
        return finish(target, category_names(&words[with + 1..]), vec![], q);
    }
    let n = l.len();
    let target = noun(l.last()?)?;
    let item = |from: usize| reminder_query(&words[from.min(n - 1)..n - 1]);
    let (add, remove, query) = match verb {
        "add" | "put" | "assign" | "apply" => {
            let to = l.iter().position(|w| w == "to" || w == "on")?;
            (category_names(&words[1..to]), vec![], item(to + 1))
        }
        "remove" | "take" | "untag" => {
            let from = l.iter().position(|w| w == "from" || w == "off")?;
            let start = if l.get(1).is_some_and(|w| w == "off") {
                2
            } else {
                1
            };
            (vec![], category_names(&words[start..from]), item(from + 1))
        }
        _ => return None,
    };
    finish(target, add, remove, query)
}

fn finish(
    target: CategoryTarget,
    add: Vec<String>,
    remove: Vec<String>,
    query: String,
) -> Option<Command> {
    if (add.is_empty() && remove.is_empty()) || query.is_empty() {
        return None;
    }
    Some(Command::SetCategories {
        target,
        query,
        add,
        remove,
        id: None,
        title: None,
    })
}

/// Reminder and category requests, or `None` to try the other parsers.
pub fn parse(step: &str, today: &str) -> Result<Option<Command>, ParseError> {
    let words: Vec<String> = step.split_whitespace().map(str::to_owned).collect();
    let l: Vec<String> = words.iter().map(|w| clean(w)).collect();
    let s = l.join(" ");
    let starts = |p: &str| s == p || s.starts_with(&format!("{p} "));
    let rest_after = |n: usize| words[n..].join(" ");

    // Categories first: "add School to my essay task" isn't a new task.
    if let Some(c) = parse_categories(&words, &l) {
        return Ok(Some(c));
    }
    for p in [
        "create category",
        "new category",
        "add category",
        "make a category",
    ] {
        if starts(p) {
            let name = category_names(&words[p.split(' ').count()..]);
            return match name.as_slice() {
                [n] => Ok(Some(Command::CreateCategory {
                    name: n.clone(),
                    id: None,
                })),
                [] => Err(ParseError::MissingText("category name")),
                _ => Err(ParseError::MissingText("one category name at a time")),
            };
        }
    }

    // Reminders from task deadlines (needs approval like any change).
    if (starts("remind me about") || starts("remind me of") || starts("create reminders for"))
        && (s.contains("deadline") || s.contains("tasks due") || s.contains("due tasks"))
    {
        let by = if s.contains("this week") {
            let wd = validate::weekday(today) as i64;
            validate::add_days(today, 6 - wd)
        } else if s.contains("today") {
            today.to_owned()
        } else if s.contains("tomorrow") {
            validate::add_days(today, 1)
        } else {
            validate::add_days(today, 13)
        };
        return Ok(Some(Command::RemindDeadlines { by }));
    }

    for p in [
        "remind me to",
        "remind me about",
        "remind me",
        "set a reminder to",
        "set a reminder for",
        "set a reminder",
        "set reminder",
        "add a reminder to",
        "add a reminder",
        "add reminder",
        "new reminder",
        "create a reminder to",
        "create a reminder",
        "create reminder",
    ] {
        if starts(p) {
            let n = p.split(' ').count();
            // "remind me to …" keeps "to"/"about" for create() to drop, so
            // "remind me every monday to …" still sees its schedule.
            let n = if p.ends_with(" to") || p.ends_with(" about") || p.ends_with(" for") {
                n - 1
            } else {
                n
            };
            return create(&rest_after(n), today).map(Some);
        }
    }

    let mentions = has_word(&l, "reminder") || has_word(&l, "reminders");

    // Lists and search.
    if mentions {
        let read = [
            "show", "list", "what", "what's", "which", "any", "my", "see", "get", "do",
        ];
        let first = l.first().map(String::as_str).unwrap_or("");
        if read.contains(&first)
            || matches!(
                s.as_str(),
                "reminders" | "overdue reminders" | "upcoming reminders"
            )
            || s.starts_with("overdue reminders")
            || s.starts_with("upcoming reminders")
            || s.starts_with("today's reminders")
        {
            let view = if s.contains("overdue") || s.contains("missed") || s.contains("late") {
                ReminderView::Overdue
            } else if s.contains("today") {
                ReminderView::Today
            } else if s.contains("upcoming")
                || s.contains("coming up")
                || s.contains("next")
                || s.contains("this week")
            {
                ReminderView::Upcoming
            } else {
                ReminderView::Open
            };
            return Ok(Some(Command::ListReminders {
                view,
                query: None,
                today: today.to_owned(),
            }));
        }
        for p in [
            "search reminders for",
            "search my reminders for",
            "find reminders about",
            "find reminder",
            "search reminders",
            "find my reminder",
            "find the reminder",
            "find reminders",
        ] {
            if starts(p) {
                let q = reminder_query(&words[p.split(' ').count()..]);
                let q = q
                    .trim_start_matches("about ")
                    .trim_start_matches("for ")
                    .trim()
                    .to_owned();
                if q.is_empty() {
                    return Err(ParseError::MissingText("what to find"));
                }
                return Ok(Some(Command::ListReminders {
                    view: ReminderView::Search,
                    query: Some(q),
                    today: today.to_owned(),
                }));
            }
        }
    }

    // Snooze: "snooze my assignment reminder for one hour", "snooze dentist until tomorrow".
    if starts("snooze") {
        let body: Vec<String> = words[1..].to_vec();
        let lb: Vec<String> = l[1..].to_vec();
        let cut = lb
            .iter()
            .position(|w| ["for", "until", "till", "til", "to"].contains(&w.as_str()));
        let (target, how) = match cut {
            Some(c) => (&body[..c], &lb[c..]),
            None => (&body[..], &lb[lb.len()..]),
        };
        // "snooze it 10 minutes": a trailing duration with no "for".
        let (target, how): (Vec<String>, Vec<String>) = if cut.is_none() {
            let lt: Vec<&str> = lb.iter().map(String::as_str).collect();
            match (1..lt.len()).find(|i| parse_minutes(&lt[*i..]).is_some()) {
                Some(i) => (body[..i].to_vec(), {
                    let mut v = vec!["for".to_owned()];
                    v.extend(lb[i..].iter().cloned());
                    v
                }),
                None => (target.to_vec(), how.to_vec()),
            }
        } else {
            (target.to_vec(), how.to_vec())
        };
        let query = reminder_query(&target);
        if query.is_empty() && !has_word(&l, "it") {
            return Err(ParseError::MissingText("which reminder"));
        }
        let (minutes, tomorrow) = match how.as_slice() {
            [] => (Some(10), false),
            [k, w]
                if (k == "until" || k == "till" || k == "til" || k == "to") && w == "tomorrow" =>
            {
                (None, true)
            }
            [k, rest @ ..] if k == "for" => {
                let rest: Vec<&str> = rest.iter().map(String::as_str).collect();
                match parse_minutes(&rest) {
                    Some(m) => (Some(m), false),
                    None => match rest.as_slice() {
                        ["a", "day"] | ["one", "day"] | ["1", "day"] | ["24", "hours"] => {
                            (Some(24 * 60), false)
                        }
                        _ => return Err(ParseError::MissingText("how long to snooze")),
                    },
                }
            }
            _ => return Err(ParseError::MissingText("how long to snooze")),
        };
        return Ok(Some(Command::SnoozeReminder {
            query: if query.is_empty() { "it".into() } else { query },
            minutes,
            tomorrow,
            reminder_id: None,
            title: None,
        }));
    }

    if !mentions {
        return Ok(None);
    }

    // Complete: "mark my dentist reminder as complete", "complete the dentist reminder".
    let done_words = ["complete", "completed", "done", "finished"];
    let lead = ["mark", "complete", "finish", "check", "tick", "close"];
    if l.first().is_some_and(|w| lead.contains(&w.as_str()))
        && (l.first().is_some_and(|w| w == "complete" || w == "finish")
            || l.last()
                .is_some_and(|w| done_words.contains(&w.as_str()) || w == "off"))
    {
        let mut body: Vec<String> = words[1..].to_vec();
        while body.last().is_some_and(|w| {
            ["as", "complete", "completed", "done", "finished", "off"].contains(&clean(w).as_str())
        }) {
            body.pop();
        }
        if body.first().is_some_and(|w| clean(w) == "off") {
            body.remove(0);
        }
        let query = reminder_query(&body);
        if query.is_empty() {
            return Err(ParseError::MissingText("which reminder"));
        }
        return Ok(Some(Command::CompleteReminder {
            query,
            reminder_id: None,
            title: None,
        }));
    }

    // Flag: "flag my dentist reminder", "unflag …".
    if l.first().is_some_and(|w| w == "flag" || w == "unflag") {
        let query = reminder_query(&words[1..]);
        if query.is_empty() {
            return Err(ParseError::MissingText("which reminder"));
        }
        return Ok(Some(Command::UpdateReminder {
            query,
            change: ReminderChange {
                flagged: Some(l[0] == "flag"),
                ..Default::default()
            },
            reminder_id: None,
            title: None,
        }));
    }

    // Priority: "set the priority of my dentist reminder to high",
    // "make my dentist reminder high priority".
    if has_word(&l, "priority") {
        let Some(r) = l.iter().position(|w| w == "reminder") else {
            return Ok(None);
        };
        let Some(p) = l[r + 1..].iter().chain(&l[..r]).find_map(|w| priority(w)) else {
            return Err(ParseError::MissingText("which priority"));
        };
        let start = l[..r]
            .iter()
            .rposition(|w| ["of", "make", "set", "for", "on"].contains(&w.as_str()))
            .map(|i| i + 1)
            .unwrap_or(1);
        let query = reminder_query(&words[start..r]);
        if query.is_empty() {
            return Err(ParseError::MissingText("which reminder"));
        }
        return Ok(Some(Command::UpdateReminder {
            query,
            change: ReminderChange {
                priority: Some(p),
                ..Default::default()
            },
            reminder_id: None,
            title: None,
        }));
    }

    // Reschedule: "move my dentist reminder to friday at 3pm".
    let moves = [
        "move",
        "reschedule",
        "push",
        "change",
        "shift",
        "postpone",
        "delay",
    ];
    if l.first().is_some_and(|w| moves.contains(&w.as_str())) {
        let Some(r) = l.iter().position(|w| w == "reminder") else {
            return Ok(None);
        };
        let query = reminder_query(&words[1..r]);
        let tail: Vec<String> = l[r + 1..]
            .iter()
            .skip_while(|w| ["to", "for", "until", "till"].contains(&w.as_str()))
            .cloned()
            .collect();
        let tail_ref: Vec<&str> = tail.iter().map(String::as_str).collect();
        let (left, when) = split_when(&tail_ref, today);
        if query.is_empty() {
            return Err(ParseError::MissingText("which reminder"));
        }
        if !left.is_empty()
            || (when.date.is_none() && when.time.is_none() && when.after_minutes.is_none())
        {
            return Err(ParseError::MissingText("when to move it to"));
        }
        return Ok(Some(Command::UpdateReminder {
            query,
            change: ReminderChange {
                due_date: when.date,
                due_time: when.time.map(Some),
                after_minutes: when.after_minutes,
                ..Default::default()
            },
            reminder_id: None,
            title: None,
        }));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODAY: &str = "2026-11-02"; // a Monday

    fn one(input: &str) -> Command {
        parse(input, TODAY)
            .unwrap()
            .unwrap_or_else(|| panic!("{input}"))
    }

    fn created(input: &str) -> (String, Option<String>, Option<String>, Option<Rule>) {
        match one(input) {
            Command::CreateReminder {
                title,
                due_date,
                due_time,
                repeat,
                ..
            } => (title, due_date, due_time, repeat),
            c => panic!("{input}: {c:?}"),
        }
    }

    #[test]
    fn creates_reminders_with_dates_times_and_repeats() {
        assert_eq!(
            created("Remind me to submit my assignment tomorrow at 8 PM"),
            (
                "submit my assignment".into(),
                Some("2026-11-03".into()),
                Some("20:00".into()),
                None
            )
        );
        let (title, date, time, repeat) =
            created("Remind me every Monday and Wednesday to review my notes");
        assert_eq!(title, "review my notes");
        assert_eq!(date.as_deref(), Some("2026-11-02"), "today is a Monday");
        assert_eq!(time, None);
        assert_eq!(repeat.unwrap().weekdays, vec![0, 2]);
        let (title, date, _, repeat) = created("remind me to water the plants every 3 days");
        assert_eq!(title, "water the plants");
        assert_eq!(date.as_deref(), Some(TODAY));
        let r = repeat.unwrap();
        assert_eq!((r.freq, r.interval), (Freq::Daily, 3));
        assert_eq!(
            created("remind me to call mom on friday at 17:30"),
            (
                "call mom".into(),
                Some("2026-11-06".into()),
                Some("17:30".into()),
                None
            )
        );
        assert_eq!(
            created("remind me to stretch"),
            ("stretch".into(), None, None, None),
            "no date: an unscheduled reminder"
        );
        assert_eq!(
            created("remind me to check the oven at noon").2.as_deref(),
            Some("12:00")
        );
    }

    #[test]
    fn this_project_and_relative_times() {
        match one("Remind me to follow up on this project in three days") {
            Command::CreateReminder {
                title,
                due_date,
                project,
                ..
            } => {
                assert_eq!(title, "follow up on this project");
                assert_eq!(due_date.as_deref(), Some("2026-11-05"));
                assert_eq!(project.as_deref(), Some("this project"));
            }
            c => panic!("{c:?}"),
        }
        match one("remind me in 2 hours to take the laundry out") {
            Command::CreateReminder {
                title,
                after_minutes,
                due_date,
                ..
            } => {
                assert_eq!(title, "take the laundry out");
                assert_eq!(after_minutes, Some(120));
                assert_eq!(due_date, None, "the plan fills in the date");
            }
            c => panic!("{c:?}"),
        }
    }

    #[test]
    fn lists_snoozes_and_completes() {
        assert_eq!(
            one("Show me my overdue reminders"),
            Command::ListReminders {
                view: ReminderView::Overdue,
                query: None,
                today: TODAY.into()
            }
        );
        assert!(matches!(
            one("what reminders do I have today"),
            Command::ListReminders {
                view: ReminderView::Today,
                ..
            }
        ));
        assert!(matches!(
            one("search reminders for passport"),
            Command::ListReminders { view: ReminderView::Search, query: Some(q), .. } if q == "passport"
        ));
        assert_eq!(
            one("Snooze my assignment reminder for one hour"),
            Command::SnoozeReminder {
                query: "assignment".into(),
                minutes: Some(60),
                tomorrow: false,
                reminder_id: None,
                title: None
            }
        );
        assert!(matches!(
            one("snooze dentist until tomorrow"),
            Command::SnoozeReminder {
                tomorrow: true,
                minutes: None,
                ..
            }
        ));
        assert!(matches!(
            one("snooze the dentist reminder 10 minutes"),
            Command::SnoozeReminder { minutes: Some(10), ref query, .. } if query == "dentist"
        ));
        assert_eq!(
            one("Mark my dentist reminder as complete"),
            Command::CompleteReminder {
                query: "dentist".into(),
                reminder_id: None,
                title: None
            }
        );
        assert!(matches!(
            one("complete the dentist reminder"),
            Command::CompleteReminder { ref query, .. } if query == "dentist"
        ));
    }

    #[test]
    fn edits_and_categories() {
        match one("move my dentist reminder to friday at 3pm") {
            Command::UpdateReminder { query, change, .. } => {
                assert_eq!(query, "dentist");
                assert_eq!(change.due_date.as_deref(), Some("2026-11-06"));
                assert_eq!(change.due_time, Some(Some("15:00".into())));
            }
            c => panic!("{c:?}"),
        }
        assert!(matches!(
            one("flag my dentist reminder"),
            Command::UpdateReminder {
                change: ReminderChange {
                    flagged: Some(true),
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            one("set the priority of my dentist reminder to high"),
            Command::UpdateReminder { change: ReminderChange { priority: Some(Priority::High), .. }, ref query, .. } if query == "dentist"
        ));
        assert_eq!(
            one("Add School and Important to my assignment task"),
            Command::SetCategories {
                target: CategoryTarget::Task,
                query: "assignment".into(),
                add: vec!["School".into(), "Important".into()],
                remove: vec![],
                id: None,
                title: None
            }
        );
        assert!(matches!(
            one("remove School from my essay task"),
            Command::SetCategories { ref remove, .. } if remove == &["School".to_owned()]
        ));
        assert!(matches!(
            one("tag my dentist reminder with Health"),
            Command::SetCategories { target: CategoryTarget::Reminder, ref add, ref query, .. }
                if add == &["Health".to_owned()] && query == "dentist"
        ));
        assert_eq!(
            one("create category School"),
            Command::CreateCategory {
                name: "School".into(),
                id: None
            }
        );
        assert!(matches!(
            one("remind me about my task deadlines this week"),
            Command::RemindDeadlines { ref by } if by == "2026-11-08"
        ));
    }

    #[test]
    fn leaves_other_requests_alone() {
        for input in [
            "add task buy milk",
            "complete dentist",
            "move call the dentist to monday",
            "what's on tomorrow",
            "add milk to the shopping list",
        ] {
            assert_eq!(parse(input, TODAY).unwrap(), None, "{input}");
        }
    }
}
