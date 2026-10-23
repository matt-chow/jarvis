//! Universal search across the owner's tasks, notes, projects, journal,
//! calendar events, and habits.
//! Substring matching (case-insensitive) is enough at personal scale; the
//! `SearchSource` boundary lets a full-text index replace it later.

use crate::identity::Principal;
use crate::links::EntityKind;
use serde::Serialize;

/// What a hit is: the linkable kinds plus calendar events and habits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchKind {
    Task,
    Note,
    Project,
    Journal,
    Event,
    Habit,
}

impl SearchKind {
    pub const ALL: [SearchKind; 6] = [
        Self::Task,
        Self::Note,
        Self::Project,
        Self::Journal,
        Self::Event,
        Self::Habit,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Note => "note",
            Self::Project => "project",
            Self::Journal => "journal",
            Self::Event => "event",
            Self::Habit => "habit",
        }
    }

    /// The link kind, for kinds that can be linked.
    pub fn entity(self) -> Option<EntityKind> {
        match self {
            Self::Task => Some(EntityKind::Task),
            Self::Note => Some(EntityKind::Note),
            Self::Project => Some(EntityKind::Project),
            Self::Journal => Some(EntityKind::Journal),
            Self::Event | Self::Habit => None,
        }
    }
}

pub const MIN_QUERY: usize = 2;
pub const MAX_QUERY: usize = 100;
pub const MAX_HITS: usize = 50;
/// Rows fetched per kind before ranking.
pub const PER_KIND: u32 = 50;
const SNIPPET_RADIUS: usize = 60;
/// Recent rows per kind matched here when a non-ASCII query has no ASCII
/// part to narrow the SQL by (D-038). Bounded for the Worker CPU budget.
pub const SCAN_PER_KIND: u32 = 100;

/// The LIKE pattern and row limit for a query. SQLite's LIKE folds case for
/// ASCII only, so a query with other letters is narrowed by its longest
/// ASCII run (or not at all) and the rows are matched here instead.
fn plan(q: &str) -> (String, u32) {
    if q.is_ascii() {
        return (like_pattern(q), PER_KIND);
    }
    let run = q
        .split(|c: char| !c.is_ascii())
        .max_by_key(|s| s.trim().len())
        .unwrap_or("");
    if run.trim().len() >= MIN_QUERY {
        (like_pattern(run), SCAN_PER_KIND)
    } else {
        ("%".into(), SCAN_PER_KIND)
    }
}

/// A candidate row from a source: `body` is the text a snippet is cut from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub kind: SearchKind,
    pub id: String,
    pub title: String,
    pub body: String,
    pub updated_at: i64,
    /// Events: their (first) start date, so clients can open that day.
    pub date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchHit {
    pub kind: SearchKind,
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub title_match: bool,
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchError {
    Invalid(&'static str),
    Backend(String),
}

pub trait SearchSource {
    /// Rows of `kind` owned by `owner_id` whose title or body matches the SQL
    /// LIKE `pattern` (already escaped with `\`).
    async fn candidates(
        &self,
        owner_id: &str,
        kind: SearchKind,
        pattern: &str,
        limit: u32,
    ) -> Result<Vec<Candidate>, String>;
}

/// `%query%` with LIKE wildcards escaped (use `ESCAPE '\'`).
pub fn like_pattern(query: &str) -> String {
    let mut out = String::with_capacity(query.len() + 2);
    out.push('%');
    for c in query.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// Case-insensitive char index of `needle` in `hay`.
fn find_ci(hay: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    let lower = |c: &char| c.to_lowercase().next().unwrap_or(*c);
    (0..=hay.len() - needle.len()).find(|&i| {
        hay[i..i + needle.len()]
            .iter()
            .map(lower)
            .eq(needle.iter().map(lower))
    })
}

/// Text around the first match, on one line, with ellipses where cut.
pub fn snippet(text: &str, query: &str) -> String {
    let flat: Vec<char> = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .collect();
    let q: Vec<char> = query.chars().collect();
    let (start, end) = match find_ci(&flat, &q) {
        Some(i) => (
            i.saturating_sub(SNIPPET_RADIUS),
            (i + q.len() + SNIPPET_RADIUS).min(flat.len()),
        ),
        None => (0, flat.len().min(SNIPPET_RADIUS * 2)),
    };
    let mut s: String = flat[start..end].iter().collect();
    if start > 0 {
        s.insert(0, '…');
    }
    if end < flat.len() {
        s.push('…');
    }
    s
}

pub async fn search<S: SearchSource>(
    source: &S,
    p: &Principal,
    query: &str,
) -> Result<Vec<SearchHit>, SearchError> {
    let q = query.trim();
    let len = q.chars().count();
    if len < MIN_QUERY {
        return Err(SearchError::Invalid("query too short"));
    }
    if len > MAX_QUERY {
        return Err(SearchError::Invalid("query too long"));
    }
    if q.chars().any(char::is_control) {
        return Err(SearchError::Invalid("invalid characters"));
    }
    let (pattern, limit) = plan(q);
    let q_chars: Vec<char> = q.chars().collect();
    let mut hits = Vec::new();
    for kind in SearchKind::ALL {
        let rows = source
            .candidates(&p.user_id, kind, &pattern, limit)
            .await
            .map_err(SearchError::Backend)?;
        let mut kept = 0;
        for c in rows {
            let title_chars: Vec<char> = c.title.chars().collect();
            let title_match = find_ci(&title_chars, &q_chars).is_some();
            // Unicode-aware match on the same fields as the SQL (journal: body only).
            let title_counts = title_match && kind != SearchKind::Journal;
            if !title_counts && find_ci(&c.body.chars().collect::<Vec<_>>(), &q_chars).is_none() {
                continue;
            }
            kept += 1;
            if kept > PER_KIND {
                break;
            }
            hits.push(SearchHit {
                snippet: snippet(&c.body, q),
                kind: c.kind,
                id: c.id,
                title: c.title,
                title_match,
                updated_at: c.updated_at,
                date: c.date,
            });
        }
    }
    hits.sort_by(|a, b| {
        b.title_match
            .cmp(&a.title_match)
            .then(b.updated_at.cmp(&a.updated_at))
    });
    hits.truncate(MAX_HITS);
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::AuthMethod;
    use crate::test_util::block_on;

    struct Fake(Vec<Candidate>);

    impl SearchSource for Fake {
        async fn candidates(
            &self,
            _: &str,
            kind: SearchKind,
            pattern: &str,
            _: u32,
        ) -> Result<Vec<Candidate>, String> {
            let needle = pattern.trim_matches('%').replace('\\', "").to_lowercase();
            Ok(self
                .0
                .iter()
                .filter(|c| {
                    c.kind == kind
                        && (c.title.to_lowercase().contains(&needle)
                            || c.body.to_lowercase().contains(&needle))
                })
                .cloned()
                .collect())
        }
    }

    fn c(kind: SearchKind, id: &str, title: &str, body: &str, at: i64) -> Candidate {
        Candidate {
            kind,
            id: id.into(),
            title: title.into(),
            body: body.into(),
            updated_at: at,
            date: None,
        }
    }

    fn me() -> Principal {
        Principal {
            user_id: "u1".into(),
            method: AuthMethod::Session,
        }
    }

    #[test]
    fn ranks_title_matches_first_then_recent() {
        let src = Fake(vec![
            c(SearchKind::Note, "n1", "Groceries", "buy dentist floss", 9),
            c(SearchKind::Task, "t1", "Call the dentist", "", 1),
            c(
                SearchKind::Journal,
                "2026-10-21",
                "2026-10-21",
                "Went to the dentist today",
                5,
            ),
        ]);
        let hits = block_on(search(&src, &me(), "Dentist")).unwrap();
        let order: Vec<_> = hits.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(order, ["t1", "n1", "2026-10-21"]);
        assert!(hits[0].title_match && !hits[1].title_match);
        assert_eq!(hits[1].snippet, "buy dentist floss");
    }

    #[test]
    fn events_and_habits_are_searched_and_events_carry_their_date() {
        let mut event = c(SearchKind::Event, "e1", "Dentist appointment", "Main St", 3);
        event.date = Some("2026-10-23".into());
        let src = Fake(vec![
            event,
            c(SearchKind::Habit, "h1", "Floss after dentist", "", 2),
        ]);
        let hits = block_on(search(&src, &me(), "dentist")).unwrap();
        assert_eq!(
            hits.iter().map(|h| h.kind.as_str()).collect::<Vec<_>>(),
            ["event", "habit"]
        );
        assert_eq!(hits[0].date.as_deref(), Some("2026-10-23"));
        assert_eq!(SearchKind::Event.entity(), None);
        assert_eq!(SearchKind::Note.entity(), Some(EntityKind::Note));
    }

    #[test]
    fn non_ascii_letters_match_regardless_of_case() {
        let src = Fake(vec![
            c(SearchKind::Note, "n1", "Über alles", "", 2),
            c(SearchKind::Task, "t1", "Café opening", "", 3),
            c(SearchKind::Project, "p1", "Ελληνικά", "μάθημα", 1),
            c(SearchKind::Task, "t2", "Unrelated", "", 4),
        ]);
        // "über" vs "Über": SQLite LIKE alone misses this.
        assert_eq!(block_on(search(&src, &me(), "über")).unwrap()[0].id, "n1");
        assert_eq!(block_on(search(&src, &me(), "CAFÉ")).unwrap()[0].id, "t1");
        let greek = block_on(search(&src, &me(), "ΜΆΘΗΜΑ")).unwrap();
        assert_eq!(
            greek.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
            ["p1"]
        );
        assert_eq!(plan("über"), (like_pattern("ber"), SCAN_PER_KIND));
        assert_eq!(plan("日本"), ("%".to_string(), SCAN_PER_KIND));
        assert_eq!(plan("plain"), (like_pattern("plain"), PER_KIND));
    }

    #[test]
    fn journal_days_match_on_their_body_only() {
        let src = Fake(vec![c(
            SearchKind::Journal,
            "2026-10-21",
            "2026-10-21",
            "quiet",
            1,
        )]);
        assert!(block_on(search(&src, &me(), "2026-10")).unwrap().is_empty());
    }

    #[test]
    fn validation() {
        let src = Fake(vec![]);
        assert_eq!(
            block_on(search(&src, &me(), " a ")).unwrap_err(),
            SearchError::Invalid("query too short")
        );
        assert!(block_on(search(&src, &me(), &"x".repeat(101))).is_err());
        assert!(block_on(search(&src, &me(), "a\u{7}b")).is_err());
    }

    #[test]
    fn like_escaping_and_snippets() {
        assert_eq!(like_pattern("50%_off\\"), r"%50\%\_off\\%");
        let long = format!("{} needle {}", "a ".repeat(100), "b ".repeat(100));
        let s = snippet(&long, "NEEDLE");
        assert!(s.starts_with('…') && s.ends_with('…') && s.contains("needle"));
        assert!(s.chars().count() <= 2 * SNIPPET_RADIUS + 8);
        assert_eq!(
            snippet("Ünïcödé text here", "TEXT"),
            "Ünïcödé text here",
            "unicode-safe"
        );
        assert_eq!(snippet("line one\n\nline two", "two"), "line one line two");
    }
}
