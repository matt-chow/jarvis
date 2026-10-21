//! Bearer-token authorization used to protect private endpoints.
//!
//! Sprint 00 uses a single shared secret. The decision logic lives here so it
//! can be swapped for real identity (e.g. Cloudflare Access JWTs) later
//! without touching handlers.

/// Shortest secret accepted; anything weaker is treated as "not configured".
pub const MIN_TOKEN_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthDecision {
    Allowed,
    /// No credentials supplied.
    Missing,
    /// Credentials supplied but wrong.
    Invalid,
    /// Server has no usable secret; private endpoints fail closed.
    NotConfigured,
}

/// Decide whether an `Authorization` header grants access.
pub fn authorize_bearer(authorization: Option<&str>, configured: Option<&str>) -> AuthDecision {
    let expected = match configured.map(str::trim) {
        Some(t) if t.len() >= MIN_TOKEN_LEN => t,
        _ => return AuthDecision::NotConfigured,
    };
    let Some(header) = authorization else {
        return AuthDecision::Missing;
    };
    let Some(supplied) = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
    else {
        return AuthDecision::Invalid;
    };
    if constant_time_eq(supplied.trim().as_bytes(), expected.as_bytes()) {
        AuthDecision::Allowed
    } else {
        AuthDecision::Invalid
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn allows_matching_token() {
        let header = format!("Bearer {TOKEN}");
        assert_eq!(
            authorize_bearer(Some(&header), Some(TOKEN)),
            AuthDecision::Allowed
        );
    }

    #[test]
    fn rejects_wrong_or_malformed_token() {
        let wrong = format!("Bearer {}", TOKEN.replace('0', "x"));
        assert_eq!(
            authorize_bearer(Some(&wrong), Some(TOKEN)),
            AuthDecision::Invalid
        );
        assert_eq!(
            authorize_bearer(Some(TOKEN), Some(TOKEN)),
            AuthDecision::Invalid
        );
        assert_eq!(
            authorize_bearer(Some("Bearer short"), Some(TOKEN)),
            AuthDecision::Invalid
        );
    }

    #[test]
    fn missing_header() {
        assert_eq!(authorize_bearer(None, Some(TOKEN)), AuthDecision::Missing);
    }

    #[test]
    fn fails_closed_without_strong_secret() {
        let header = format!("Bearer {TOKEN}");
        assert_eq!(
            authorize_bearer(Some(&header), None),
            AuthDecision::NotConfigured
        );
        assert_eq!(
            authorize_bearer(Some("Bearer abc"), Some("abc")),
            AuthDecision::NotConfigured
        );
        assert_eq!(
            authorize_bearer(Some("Bearer "), Some("")),
            AuthDecision::NotConfigured
        );
    }
}
