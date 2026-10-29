//! Owner preferences that follow them across devices (e.g. the dashboard
//! layout). Stored as one JSON document per owner; validated here.

use crate::identity::Principal;
use crate::validate;
use serde::{Deserialize, Serialize};

/// Dashboard widgets the clients know how to render.
pub const WIDGETS: &[&str] = &["capture", "today", "habits", "notes", "status", "modules"];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preferences {
    /// `None` = the client's default layout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dashboard: Option<Dashboard>,
    /// `None` = the client's default working hours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planner: Option<Planner>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dashboard {
    /// Widget ids in display order; widgets not listed are hidden.
    pub widgets: Vec<String>,
}

/// Working hours the planner suggests slots within (D-047).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Planner {
    /// `HH:MM`
    pub day_start: String,
    /// `HH:MM`, after `day_start` (`24:00` allowed).
    pub day_end: String,
    /// Working weekdays, 0 = Monday … 6 = Sunday.
    pub work_days: Vec<u8>,
    /// Preferred session length, 15–480 minutes.
    pub session_minutes: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PreferencesPatch {
    /// `null` resets to the default layout.
    #[serde(default, deserialize_with = "validate::double_option")]
    pub dashboard: Option<Option<Dashboard>>,
    /// `null` resets to the default working hours.
    #[serde(default, deserialize_with = "validate::double_option")]
    pub planner: Option<Option<Planner>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreferencesError {
    Invalid(&'static str, &'static str),
    Backend(String),
}

pub trait PreferencesRepository {
    /// The stored JSON document, if any.
    async fn get(&self, owner_id: &str) -> Result<Option<String>, String>;
    async fn put(&self, owner_id: &str, data: &str, now: i64) -> Result<(), String>;
}

fn backend(e: String) -> PreferencesError {
    PreferencesError::Backend(e)
}

fn check_dashboard(d: Dashboard) -> Result<Dashboard, PreferencesError> {
    let mut seen: Vec<String> = Vec::new();
    for w in d.widgets {
        if !WIDGETS.contains(&w.as_str()) {
            return Err(PreferencesError::Invalid("dashboard", "unknown widget"));
        }
        if seen.contains(&w) {
            return Err(PreferencesError::Invalid(
                "dashboard",
                "widget listed twice",
            ));
        }
        seen.push(w);
    }
    Ok(Dashboard { widgets: seen })
}

fn check_planner(mut p: Planner) -> Result<Planner, PreferencesError> {
    let invalid = |why| PreferencesError::Invalid("planner", why);
    let start = validate::clock(&p.day_start, false).map_err(invalid)?;
    let end = validate::clock(&p.day_end, true).map_err(invalid)?;
    if end <= start {
        return Err(invalid("day_end must be after day_start"));
    }
    if p.work_days.iter().any(|d| *d > 6) {
        return Err(invalid("work_days are 0 (Monday) to 6"));
    }
    p.work_days.sort_unstable();
    p.work_days.dedup();
    if !(15..=480).contains(&p.session_minutes) {
        return Err(invalid("session_minutes must be 15 to 480"));
    }
    Ok(p)
}

/// Parse a stored document leniently, dropping widgets that no longer exist.
pub fn from_stored(data: &str) -> Preferences {
    let mut prefs: Preferences = serde_json::from_str(data).unwrap_or_default();
    if let Some(d) = &mut prefs.dashboard {
        d.widgets.retain(|w| WIDGETS.contains(&w.as_str()));
    }
    prefs.planner = prefs.planner.and_then(|p| check_planner(p).ok());
    prefs
}

pub struct PreferencesService<'a, R> {
    repo: &'a R,
    now: i64,
}

impl<'a, R: PreferencesRepository> PreferencesService<'a, R> {
    pub fn new(repo: &'a R, now: i64) -> Self {
        Self { repo, now }
    }

    pub async fn get(&self, p: &Principal) -> Result<Preferences, PreferencesError> {
        Ok(self
            .repo
            .get(&p.user_id)
            .await
            .map_err(backend)?
            .map(|d| from_stored(&d))
            .unwrap_or_default())
    }

    /// Apply `patch` (last write wins) and return the result.
    pub async fn update(
        &self,
        p: &Principal,
        patch: PreferencesPatch,
    ) -> Result<Preferences, PreferencesError> {
        let mut prefs = self.get(p).await?;
        if let Some(d) = patch.dashboard {
            prefs.dashboard = d.map(check_dashboard).transpose()?;
        }
        if let Some(p) = patch.planner {
            prefs.planner = p.map(check_planner).transpose()?;
        }
        let data = serde_json::to_string(&prefs).map_err(|e| backend(e.to_string()))?;
        self.repo
            .put(&p.user_id, &data, self.now)
            .await
            .map_err(backend)?;
        Ok(prefs)
    }
}

#[cfg(test)]
pub(crate) mod testing {
    pub use super::tests::Memory as MemoryPreferences;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::AuthMethod;
    use crate::test_util::block_on;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    pub struct Memory(pub RefCell<HashMap<String, String>>);

    impl PreferencesRepository for Memory {
        async fn get(&self, owner_id: &str) -> Result<Option<String>, String> {
            Ok(self.0.borrow().get(owner_id).cloned())
        }
        async fn put(&self, owner_id: &str, data: &str, _now: i64) -> Result<(), String> {
            self.0.borrow_mut().insert(owner_id.into(), data.into());
            Ok(())
        }
    }

    fn me() -> Principal {
        Principal {
            user_id: "u1".into(),
            method: AuthMethod::Session,
        }
    }

    fn patch(json: &str) -> PreferencesPatch {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn dashboard_layout_is_validated_stored_and_reset() {
        let repo = Memory::default();
        let svc = PreferencesService::new(&repo, 1);
        assert_eq!(block_on(svc.get(&me())).unwrap(), Preferences::default());

        let saved = block_on(svc.update(
            &me(),
            patch(r#"{"dashboard":{"widgets":["today","capture"]}}"#),
        ))
        .unwrap();
        assert_eq!(saved.dashboard.unwrap().widgets, ["today", "capture"]);
        assert_eq!(
            block_on(svc.get(&me())).unwrap().dashboard.unwrap().widgets,
            ["today", "capture"]
        );

        for (json, why) in [
            (r#"{"dashboard":{"widgets":["weather"]}}"#, "unknown widget"),
            (
                r#"{"dashboard":{"widgets":["today","today"]}}"#,
                "widget listed twice",
            ),
        ] {
            assert_eq!(
                block_on(svc.update(&me(), patch(json))).unwrap_err(),
                PreferencesError::Invalid("dashboard", why)
            );
        }

        // An omitted field is unchanged; null resets to the default.
        block_on(svc.update(&me(), patch("{}"))).unwrap();
        assert!(block_on(svc.get(&me())).unwrap().dashboard.is_some());
        block_on(svc.update(&me(), patch(r#"{"dashboard":null}"#))).unwrap();
        assert_eq!(block_on(svc.get(&me())).unwrap().dashboard, None);
    }

    #[test]
    fn working_hours_are_validated_and_independent_of_the_dashboard() {
        let repo = Memory::default();
        let svc = PreferencesService::new(&repo, 1);
        block_on(svc.update(&me(), patch(r#"{"dashboard":{"widgets":["today"]}}"#))).unwrap();
        let saved = block_on(svc.update(
            &me(),
            patch(
                r#"{"planner":{"day_start":"08:30","day_end":"18:00","work_days":[4,0,0,2],"session_minutes":50}}"#,
            ),
        ))
        .unwrap();
        let planner = saved.planner.unwrap();
        assert_eq!(planner.work_days, [0, 2, 4]);
        assert!(saved.dashboard.is_some(), "other fields are kept");
        for (json, why) in [
            (
                r#"{"planner":{"day_start":"18:00","day_end":"08:00","work_days":[],"session_minutes":60}}"#,
                "day_end must be after day_start",
            ),
            (
                r#"{"planner":{"day_start":"08:00","day_end":"18:00","work_days":[7],"session_minutes":60}}"#,
                "work_days are 0 (Monday) to 6",
            ),
            (
                r#"{"planner":{"day_start":"08:00","day_end":"18:00","work_days":[],"session_minutes":5}}"#,
                "session_minutes must be 15 to 480",
            ),
        ] {
            assert_eq!(
                block_on(svc.update(&me(), patch(json))).unwrap_err(),
                PreferencesError::Invalid("planner", why)
            );
        }
        block_on(svc.update(&me(), patch(r#"{"planner":null}"#))).unwrap();
        assert_eq!(block_on(svc.get(&me())).unwrap().planner, None);
    }

    #[test]
    fn stored_documents_are_read_leniently() {
        let repo = Memory::default();
        repo.0.borrow_mut().insert(
            "u1".into(),
            r#"{"dashboard":{"widgets":["retired","notes"]},"future":1}"#.into(),
        );
        let svc = PreferencesService::new(&repo, 1);
        assert_eq!(
            block_on(svc.get(&me())).unwrap().dashboard.unwrap().widgets,
            ["notes"]
        );
        repo.0.borrow_mut().insert("u1".into(), "not json".into());
        assert_eq!(block_on(svc.get(&me())).unwrap(), Preferences::default());
    }
}
