//! Universal search across the owner's tasks, notes, projects, and journal.
//! Substring matching (case-insensitive) is enough at personal scale; the
//! `SearchSource` boundary lets a full-text index replace it later.

use crate::identity::Principal;
use crate::links::EntityKind;
use serde::Serialize;

pub const MIN_QUERY: usize = 2;
pub const MAX_QUERY: usize = 100;
pub const MAX_HITS: usize = 50;
/// Rows fetched per kind before ranking.
pub const PER_KIND: u32 = 50;
const SNIPPET_RADIUS: usize = 60;

/// A candidate row from a source: `body` is the text a snippet is cut from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub kind: EntityKind,
    pub id: String,
    pub title: String,
    pub body: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchHit {
    pub kind: EntityKind,
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub title_match: bool,
    pub updated_at: i64,
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
        kind: EntityKind,
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
    let pattern = like_pattern(q);
    let q_chars: Vec<char> = q.chars().collect();
    let mut hits = Vec::new();
    for kind in [
        EntityKind::Task,
        EntityKind::Note,
        EntityKind::Project,
        EntityKind::Journal,
    ] {
        let rows = source
            .candidates(&p.user_id, kind, &pattern, PER_KIND)
            .await
            .map_err(SearchError::Backend)?;
        for c in rows {
            let title_chars: Vec<char> = c.title.chars().collect();
            let title_match = find_ci(&title_chars, &q_chars).is_some();
            hits.push(SearchHit {
                snippet: snippet(&c.body, q),
                kind: c.kind,
                id: c.id,
                title: c.title,
                title_match,
                updated_at: c.updated_at,
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
            kind: EntityKind,
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

    fn c(kind: EntityKind, id: &str, title: &str, body: &str, at: i64) -> Candidate {
        Candidate {
            kind,
            id: id.into(),
            title: title.into(),
            body: body.into(),
            updated_at: at,
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
            c(EntityKind::Note, "n1", "Groceries", "buy dentist floss", 9),
            c(EntityKind::Task, "t1", "Call the dentist", "", 1),
            c(
                EntityKind::Journal,
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
