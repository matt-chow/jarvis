//! VAPID (RFC 8292): the server's ES256 identity for Web Push, and the
//! push-service endpoint allowlist.
//!
//! Pushes carry no payload (D-049): the service worker fetches what to show
//! from `/notifications/claim` with the device's own session, so reminder
//! text never passes through a push service and no message encryption is
//! needed.

use crate::encoding::{b64url_decode, b64url_encode};
use p256::ecdsa::{signature::Signer, Signature, SigningKey};

/// Longest accepted endpoint URL.
pub const MAX_ENDPOINT: usize = 1_024;
/// VAPID tokens may live up to 24 h; use 12 h so clock skew never voids one.
pub const TOKEN_TTL_S: i64 = 12 * 3_600;

/// Push services Directory will send to. Anything else is refused at
/// subscribe time and again at send time, so a stored URL can't turn the
/// Worker into a request forwarder.
const HOSTS: &[&str] = &[
    "fcm.googleapis.com",
    "android.googleapis.com",
    "updates.push.services.mozilla.com",
    "push.services.mozilla.com",
    "web.push.apple.com",
];
const HOST_SUFFIXES: &[&str] = &[".push.apple.com", ".notify.windows.com"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VapidError {
    /// The private key isn't a base64url P-256 scalar.
    Key,
    /// The subject must be a `mailto:` or `https://` URL.
    Subject,
}

pub struct Vapid {
    key: SigningKey,
    public: String,
    subject: String,
}

impl Vapid {
    /// `private` is the 32-byte scalar, base64url (`scripts/vapid-keys.mjs`).
    pub fn new(private: &str, subject: &str) -> Result<Self, VapidError> {
        let raw = b64url_decode(private.trim()).map_err(|_| VapidError::Key)?;
        if raw.len() != 32 {
            return Err(VapidError::Key);
        }
        let key = SigningKey::from_slice(&raw).map_err(|_| VapidError::Key)?;
        let subject = subject.trim();
        let ok = subject.len() <= 200
            && !subject.chars().any(|c| c.is_whitespace() || c == '"')
            && (subject.starts_with("mailto:") && subject.len() > 8
                || subject.starts_with("https://") && subject.len() > 8);
        if !ok {
            return Err(VapidError::Subject);
        }
        let point = key.verifying_key().to_encoded_point(false);
        Ok(Self {
            public: b64url_encode(point.as_bytes()),
            key,
            subject: subject.to_owned(),
        })
    }

    /// The uncompressed public key, base64url: the browser's
    /// `applicationServerKey`.
    pub fn public_key(&self) -> &str {
        &self.public
    }

    /// A signed JWT for `audience` (the endpoint's origin), expiring at `exp`
    /// (Unix seconds).
    pub fn token(&self, audience: &str, exp: i64) -> String {
        let header = b64url_encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = serde_json::json!({ "aud": audience, "exp": exp, "sub": self.subject });
        let claims = b64url_encode(claims.to_string().as_bytes());
        let input = format!("{header}.{claims}");
        let sig: Signature = self.key.sign(input.as_bytes());
        format!("{input}.{}", b64url_encode(&sig.to_bytes()))
    }

    /// The `Authorization` header value for a push to `audience`.
    pub fn authorization(&self, audience: &str, now_ms: i64) -> String {
        let exp = now_ms / 1_000 + TOKEN_TTL_S;
        format!("vapid t={}, k={}", self.token(audience, exp), self.public)
    }
}

/// `scheme://host[:port]` of an endpoint, or `None` if it isn't a plain
/// absolute URL.
pub fn audience(endpoint: &str) -> Option<String> {
    let (scheme, rest) = endpoint.split_once("://")?;
    if scheme != "https" && scheme != "http" {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    let bad = |c: char| c.is_whitespace() || c.is_control() || "@\\\"<>{}|^`".contains(c);
    if authority.is_empty() || endpoint.chars().any(bad) {
        return None;
    }
    Some(format!("{scheme}://{}", authority.to_ascii_lowercase()))
}

/// Whether Directory may send to `endpoint`: HTTPS on the default port to a
/// known push service, or (local tests only) under `test_origin`.
pub fn endpoint_allowed(endpoint: &str, test_origin: Option<&str>) -> bool {
    if endpoint.len() > MAX_ENDPOINT {
        return false;
    }
    let Some(aud) = audience(endpoint) else {
        return false;
    };
    if let Some(origin) = test_origin.map(str::trim).filter(|o| !o.is_empty()) {
        if aud == origin.trim_end_matches('/') && endpoint.len() > aud.len() + 1 {
            return true;
        }
    }
    let Some(host) = aud.strip_prefix("https://") else {
        return false;
    };
    if host.contains(':') {
        return false;
    }
    HOSTS.contains(&host)
        || HOST_SUFFIXES
            .iter()
            .any(|s| host.len() > s.len() && host.ends_with(s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::{signature::Verifier, VerifyingKey};

    const KEY: &str = "AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA";

    #[test]
    fn signs_a_verifiable_es256_token() {
        let v = Vapid::new(KEY, "mailto:owner@example.com").unwrap();
        let public = b64url_decode(v.public_key()).unwrap();
        assert_eq!((public.len(), public[0]), (65, 4));
        let jwt = v.token("https://fcm.googleapis.com", 1_800_000_000);
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let claims: serde_json::Value =
            serde_json::from_slice(&b64url_decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://fcm.googleapis.com");
        assert_eq!(claims["exp"], 1_800_000_000);
        assert_eq!(claims["sub"], "mailto:owner@example.com");
        let sig = Signature::from_slice(&b64url_decode(parts[2]).unwrap()).unwrap();
        let key = VerifyingKey::from_sec1_bytes(&public).unwrap();
        let input = format!("{}.{}", parts[0], parts[1]);
        assert!(key.verify(input.as_bytes(), &sig).is_ok());
        let auth = v.authorization("https://fcm.googleapis.com", 1_000_000);
        assert!(auth.starts_with("vapid t=") && auth.ends_with(&format!(", k={}", v.public_key())));
    }

    #[test]
    fn rejects_bad_keys_and_subjects() {
        assert_eq!(
            Vapid::new("short", "mailto:a@b.c").err(),
            Some(VapidError::Key)
        );
        assert_eq!(
            Vapid::new(&b64url_encode(&[0; 32]), "mailto:a@b.c").err(),
            Some(VapidError::Key)
        );
        for s in [
            "",
            "owner@example.com",
            "mailto:",
            "https://",
            "mailto:a b@c.d",
            "http://x.y",
        ] {
            assert_eq!(Vapid::new(KEY, s).err(), Some(VapidError::Subject), "{s}");
        }
        assert!(Vapid::new(KEY, "https://directory.example.workers.dev").is_ok());
    }

    #[test]
    fn allows_only_known_push_services() {
        for ok in [
            "https://fcm.googleapis.com/fcm/send/abc",
            "https://updates.push.services.mozilla.com/wpush/v2/x",
            "https://web.push.apple.com/QK7f",
            "https://wns2-par02p.notify.windows.com/w/?token=x",
        ] {
            assert!(endpoint_allowed(ok, None), "{ok}");
        }
        for bad in [
            "http://fcm.googleapis.com/fcm/send/abc",
            "https://fcm.googleapis.com:8443/x",
            "https://evil.example/fcm.googleapis.com",
            "https://fcm.googleapis.com.evil.example/x",
            "https://user@fcm.googleapis.com/x",
            "https://notify.windows.com/x",
            "https://169.254.169.254/latest",
            "https://fcm.googleapis.com/a b",
            "fcm.googleapis.com/x",
            "",
        ] {
            assert!(!endpoint_allowed(bad, None), "{bad}");
        }
        assert!(!endpoint_allowed(
            &format!("https://fcm.googleapis.com/{}", "a".repeat(1_100)),
            None
        ));
        // The local test origin, only when configured, and only beneath it.
        let test = Some("http://127.0.0.1:8799");
        assert!(endpoint_allowed("http://127.0.0.1:8799/push/1", test));
        assert!(!endpoint_allowed("http://127.0.0.1:8799/", test));
        assert!(!endpoint_allowed("http://127.0.0.1:8798/push/1", test));
        assert!(!endpoint_allowed("http://127.0.0.1:8799/push/1", None));
        assert_eq!(
            audience("https://Web.Push.Apple.com/x?y"),
            Some("https://web.push.apple.com".into())
        );
    }
}
