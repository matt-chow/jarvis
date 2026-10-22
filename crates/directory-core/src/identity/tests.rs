use super::memory::{CountingEntropy, MemoryIdentity};
use super::webauthn::testing::{rp, SoftKey};
use super::*;
use crate::test_util::block_on;

const SETUP: &str = "setup-token-0123456789abcdef0123456789";
const T0: i64 = 1_800_000_000_000;

fn config() -> IdentityConfig {
    IdentityConfig {
        rp: rp(),
        setup_token: Some(SETUP.into()),
    }
}

fn registration_input(key: &SoftKey, opts: &CreationOptions) -> RegistrationInput {
    registration_for(key, &opts.challenge_id, &opts.public_key.challenge)
}

fn registration_for(key: &SoftKey, challenge_id: &str, challenge_b64: &str) -> RegistrationInput {
    let challenge = b64url_decode(challenge_b64).unwrap();
    let reg = key.register(&challenge, &rp());
    RegistrationInput {
        challenge_id: challenge_id.into(),
        credential_id: b64url_encode(&reg.credential_id),
        client_data_json: b64url_encode(&reg.client_data_json),
        authenticator_data: b64url_encode(&reg.authenticator_data),
        public_key: b64url_encode(&reg.public_key),
        algorithm: reg.algorithm,
        name: Some("Laptop".into()),
    }
}

fn assertion_input(
    key: &SoftKey,
    opts: &RequestOptions,
    user_handle: Option<&str>,
) -> AssertionInput {
    let challenge = b64url_decode(&opts.public_key.challenge).unwrap();
    let a = key.assert(&challenge, &rp());
    AssertionInput {
        challenge_id: opts.challenge_id.clone(),
        credential_id: b64url_encode(&a.credential_id),
        client_data_json: b64url_encode(&a.client_data_json),
        authenticator_data: b64url_encode(&a.authenticator_data),
        signature: b64url_encode(&a.signature),
        user_handle: user_handle.map(|h| b64url_encode(h.as_bytes())),
    }
}

struct Fixture {
    repo: MemoryIdentity,
    entropy: CountingEntropy,
    config: IdentityConfig,
}

impl Fixture {
    fn new() -> Self {
        Self {
            repo: MemoryIdentity::default(),
            entropy: CountingEntropy::default(),
            config: config(),
        }
    }
    fn at(&self, now: i64) -> IdentityService<'_, MemoryIdentity, CountingEntropy> {
        IdentityService::new(&self.repo, &self.entropy, &self.config, now)
    }
    fn setup_owner(&self, key: &SoftKey) -> IssuedSession {
        let svc = self.at(T0);
        let opts = block_on(svc.begin_setup(SETUP, Some("Matthew"))).unwrap();
        block_on(svc.finish_setup(&registration_input(key, &opts))).unwrap()
    }
}

#[test]
fn setup_then_login_then_logout() {
    let f = Fixture::new();
    let key = SoftKey::new(1);
    assert!(block_on(f.at(T0).setup_required()).unwrap());

    let issued = f.setup_owner(&key);
    assert_eq!(issued.user.display_name, "Matthew");
    assert!(!block_on(f.at(T0).setup_required()).unwrap());
    assert_eq!(f.repo.passkeys.borrow()[0].name, "Laptop");

    // Session token authenticates; only its hash is stored.
    let svc = f.at(T0 + 1000);
    let p = block_on(svc.authenticate_session(&issued.token))
        .unwrap()
        .unwrap();
    assert_eq!(p.user_id, issued.user.id);
    assert!(f
        .repo
        .sessions
        .borrow()
        .iter()
        .all(|s| s.token_hash != issued.token));

    let opts = block_on(svc.begin_login()).unwrap();
    assert!(opts.public_key.allow_credentials.is_empty());
    let login =
        block_on(svc.finish_login(&assertion_input(&key, &opts, Some(&issued.user.id)))).unwrap();
    assert_eq!(login.user.id, issued.user.id);
    assert_eq!(f.repo.passkeys.borrow()[0].last_used_at, Some(T0 + 1000));

    block_on(svc.logout(&login.token)).unwrap();
    assert_eq!(
        block_on(svc.authenticate_session(&login.token)).unwrap(),
        None
    );
}

#[test]
fn setup_requires_valid_token_and_closes_after_owner_exists() {
    let f = Fixture::new();
    let svc = f.at(T0);
    assert_eq!(
        block_on(svc.begin_setup("wrong", None)).unwrap_err(),
        IdentityError::SetupTokenInvalid
    );

    f.setup_owner(&SoftKey::new(1));
    assert_eq!(
        block_on(svc.begin_setup(SETUP, None)).unwrap_err(),
        IdentityError::SetupUnavailable
    );
}

#[test]
fn setup_fails_closed_without_strong_secret_or_origins() {
    let mut f = Fixture::new();
    f.config.setup_token = Some("short".into());
    assert_eq!(
        block_on(f.at(T0).begin_setup("short", None)).unwrap_err(),
        IdentityError::NotConfigured
    );
    f.config.setup_token = Some(SETUP.into());
    f.config.rp.origins.clear();
    assert_eq!(
        block_on(f.at(T0).begin_login()).unwrap_err(),
        IdentityError::NotConfigured
    );
}

#[test]
fn concurrent_setups_only_one_owner_wins() {
    let f = Fixture::new();
    let svc = f.at(T0);
    let a = block_on(svc.begin_setup(SETUP, None)).unwrap();
    let b = block_on(svc.begin_setup(SETUP, None)).unwrap();
    block_on(svc.finish_setup(&registration_input(&SoftKey::new(1), &a))).unwrap();
    assert_eq!(
        block_on(svc.finish_setup(&registration_input(&SoftKey::new(2), &b))).unwrap_err(),
        IdentityError::SetupUnavailable
    );
    assert_eq!(f.repo.users.borrow().len(), 1);
}

#[test]
fn challenges_are_single_use_expiring_and_purpose_bound() {
    let f = Fixture::new();
    let key = SoftKey::new(1);
    f.setup_owner(&key);
    let svc = f.at(T0);

    let opts = block_on(svc.begin_login()).unwrap();
    let input = assertion_input(&key, &opts, None);
    block_on(svc.finish_login(&input)).unwrap();
    assert_eq!(
        block_on(svc.finish_login(&input)).unwrap_err(),
        IdentityError::ChallengeInvalid
    );

    let opts = block_on(svc.begin_login()).unwrap();
    let late = f.at(T0 + CHALLENGE_TTL_MS + 1);
    assert_eq!(
        block_on(late.finish_login(&assertion_input(&key, &opts, None))).unwrap_err(),
        IdentityError::ChallengeInvalid
    );

    // A login challenge cannot complete a registration.
    let opts = block_on(svc.begin_login()).unwrap();
    let p = Principal {
        user_id: f.repo.users.borrow()[0].id.clone(),
        method: AuthMethod::Session,
    };
    let input = registration_for(
        &SoftKey::new(9),
        &opts.challenge_id,
        &opts.public_key.challenge,
    );
    assert_eq!(
        block_on(svc.finish_add_passkey(&p, &input)).unwrap_err(),
        IdentityError::ChallengeInvalid
    );
}

#[test]
fn login_rejects_unknown_credential_and_mismatched_user_handle() {
    let f = Fixture::new();
    f.setup_owner(&SoftKey::new(1));
    let svc = f.at(T0);
    let opts = block_on(svc.begin_login()).unwrap();
    assert_eq!(
        block_on(svc.finish_login(&assertion_input(&SoftKey::new(2), &opts, None))).unwrap_err(),
        IdentityError::UnknownCredential
    );
    let opts = block_on(svc.begin_login()).unwrap();
    assert_eq!(
        block_on(svc.finish_login(&assertion_input(
            &SoftKey::new(1),
            &opts,
            Some("someone-else")
        )))
        .unwrap_err(),
        IdentityError::UnknownCredential
    );
}

#[test]
fn passkey_management_keeps_at_least_one() {
    let f = Fixture::new();
    let issued = f.setup_owner(&SoftKey::new(1));
    let svc = f.at(T0);
    let p = block_on(svc.authenticate_session(&issued.token))
        .unwrap()
        .unwrap();

    let opts = block_on(svc.begin_add_passkey(&p)).unwrap();
    assert_eq!(opts.public_key.exclude_credentials.len(), 1);
    let added =
        block_on(svc.finish_add_passkey(&p, &registration_input(&SoftKey::new(2), &opts))).unwrap();
    assert_eq!(block_on(svc.list_passkeys(&p)).unwrap().len(), 2);

    block_on(svc.delete_passkey(&p, &added.id)).unwrap();
    let remaining = block_on(svc.list_passkeys(&p)).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(
        block_on(svc.delete_passkey(&p, &remaining[0].id)).unwrap_err(),
        IdentityError::LastPasskey
    );
    assert_eq!(
        block_on(svc.delete_passkey(&p, "nope")).unwrap_err(),
        IdentityError::NotFound
    );
}

#[test]
fn sessions_slide_then_expire() {
    let f = Fixture::new();
    let issued = f.setup_owner(&SoftKey::new(1));
    assert_eq!(issued.expires_at, T0 + SESSION_TTL_MS);

    // Refreshed after a day of activity.
    let later = T0 + 2 * DAY_MS;
    block_on(f.at(later).authenticate_session(&issued.token))
        .unwrap()
        .unwrap();
    assert_eq!(
        f.repo.sessions.borrow()[0].expires_at,
        later + SESSION_TTL_MS
    );

    // Never beyond the absolute maximum age.
    let near_max = T0 + SESSION_MAX_AGE_MS - DAY_MS;
    for t in (later..near_max).step_by((20 * DAY_MS) as usize) {
        block_on(f.at(t).authenticate_session(&issued.token))
            .unwrap()
            .unwrap();
    }
    block_on(f.at(near_max).authenticate_session(&issued.token))
        .unwrap()
        .unwrap();
    assert_eq!(
        f.repo.sessions.borrow()[0].expires_at,
        T0 + SESSION_MAX_AGE_MS
    );
    assert_eq!(
        block_on(
            f.at(T0 + SESSION_MAX_AGE_MS)
                .authenticate_session(&issued.token)
        )
        .unwrap(),
        None
    );
    assert!(
        f.repo.sessions.borrow().is_empty(),
        "expired session is deleted"
    );
}

#[test]
fn api_tokens_authenticate_and_revoke() {
    let f = Fixture::new();
    let issued = f.setup_owner(&SoftKey::new(1));
    let svc = f.at(T0);
    let p = block_on(svc.authenticate_session(&issued.token))
        .unwrap()
        .unwrap();

    let (token, secret) = block_on(svc.create_api_token(&p, Some("iPhone"))).unwrap();
    assert!(secret.starts_with(API_TOKEN_PREFIX));
    assert_ne!(token.token_hash, secret);
    let via_token = block_on(svc.authenticate_api_token(&secret))
        .unwrap()
        .unwrap();
    assert_eq!(via_token.method, AuthMethod::ApiToken);
    assert_eq!(
        block_on(svc.authenticate_api_token("dir_wrong")).unwrap(),
        None
    );
    assert_eq!(
        block_on(svc.authenticate_api_token(&issued.token)).unwrap(),
        None,
        "session tokens are not API tokens"
    );

    block_on(svc.delete_api_token(&p, &token.id)).unwrap();
    assert_eq!(block_on(svc.authenticate_api_token(&secret)).unwrap(), None);
    assert_eq!(
        block_on(svc.delete_api_token(&p, &token.id)).unwrap_err(),
        IdentityError::NotFound
    );
}

#[test]
fn names_are_validated() {
    assert_eq!(clean_name(None, "Passkey").unwrap(), "Passkey");
    assert_eq!(clean_name(Some("  Phone "), "x").unwrap(), "Phone");
    assert!(clean_name(Some(&"x".repeat(65)), "x").is_err());
    assert!(clean_name(Some("bad\nname"), "x").is_err());
}

#[test]
fn bearer_parsing() {
    assert_eq!(bearer_token(Some("Bearer abc")), Some("abc"));
    assert_eq!(bearer_token(Some("Basic abc")), None);
    assert_eq!(bearer_token(Some("Bearer ")), None);
    assert_eq!(bearer_token(None), None);
}
