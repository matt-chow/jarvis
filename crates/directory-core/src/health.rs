//! Health reporting shared by every runtime.

use serde::Serialize;

pub const SERVICE_NAME: &str = "directory";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    pub name: String,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Check {
    pub fn ok(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: Status::Ok,
            detail: Some(detail.into()),
        }
    }

    pub fn degraded(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: Status::Degraded,
            detail: Some(detail.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HealthReport {
    pub status: Status,
    pub service: &'static str,
    pub version: String,
    pub checks: Vec<Check>,
}

/// Aggregate individual checks; any degraded check degrades the report.
pub fn report(version: &str, checks: Vec<Check>) -> HealthReport {
    let status = if checks.iter().all(|c| c.status == Status::Ok) {
        Status::Ok
    } else {
        Status::Degraded
    };
    HealthReport {
        status,
        service: SERVICE_NAME,
        version: version.into(),
        checks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_when_all_checks_pass() {
        let r = report("1.0.0", vec![Check::ok("database", "v1")]);
        assert_eq!(r.status, Status::Ok);
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["service"], "directory");
        assert_eq!(json["checks"][0]["name"], "database");
    }

    #[test]
    fn degraded_when_any_check_fails() {
        let r = report(
            "1.0.0",
            vec![Check::ok("a", "fine"), Check::degraded("b", "down")],
        );
        assert_eq!(r.status, Status::Degraded);
    }

    #[test]
    fn ok_with_no_checks() {
        assert_eq!(report("1.0.0", vec![]).status, Status::Ok);
    }
}
