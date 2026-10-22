//! System metadata (schema version) and the database health check.

use crate::health::Check;

/// Schema version the code expects. Bump alongside each migration that
/// updates `system_meta.schema_version`.
pub const EXPECTED_SCHEMA_VERSION: u32 = 11;

pub trait SystemRepository {
    /// Current schema version, or `None` if migrations have not run.
    async fn schema_version(&self) -> Result<Option<u32>, String>;
}

pub async fn database_check<R: SystemRepository>(repo: &R) -> Check {
    match repo.schema_version().await {
        Ok(Some(v)) if v == EXPECTED_SCHEMA_VERSION => {
            Check::ok("database", format!("schema v{v}"))
        }
        Ok(Some(v)) => Check::degraded(
            "database",
            format!("schema v{v}, expected v{EXPECTED_SCHEMA_VERSION}"),
        ),
        Ok(None) => Check::degraded("database", "migrations not applied"),
        // Do not leak backend error details through a public endpoint.
        Err(_) => Check::degraded("database", "unreachable"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::Status;
    use crate::test_util::block_on;

    struct Fixed(Result<Option<u32>, String>);

    impl SystemRepository for Fixed {
        async fn schema_version(&self) -> Result<Option<u32>, String> {
            self.0.clone()
        }
    }

    #[test]
    fn reports_expected_version_as_ok() {
        let c = block_on(database_check(&Fixed(Ok(Some(EXPECTED_SCHEMA_VERSION)))));
        assert_eq!(c.status, Status::Ok);
    }

    #[test]
    fn reports_mismatch_missing_and_errors_as_degraded() {
        for repo in [
            Fixed(Ok(Some(99))),
            Fixed(Ok(None)),
            Fixed(Err("boom".into())),
        ] {
            let c = block_on(database_check(&repo));
            assert_eq!(c.status, Status::Degraded);
            assert!(!c.detail.unwrap().contains("boom"));
        }
    }
}
