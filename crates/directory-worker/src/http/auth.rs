//! Authentication routes and the `authenticate` guard used by every private route.

use super::{error, error_with, json, no_content, read_json};
use crate::d1::{D1IdentityRepository, D1RateLimitStore};
use crate::{bindings, entropy::WebCryptoEntropy, try_or_respond};
use directory_core::identity::webauthn::RelyingParty;
use directory_core::identity::{
    bearer_token, AssertionInput, AuthMethod, IdentityConfig, IdentityError, IdentityService,
    IssuedSession, Principal, RegistrationInput,
};
use directory_core::rate_limit::{self, Decision, AUTH_POLICY};
use serde::{Deserialize, Serialize};
use worker::{Date, Env, Method, Request, Response, Result, RouteContext};

/// `__Host-` prefix: Secure, Path=/, no Domain, so it cannot be set by subdomains.
pub const SESSION_COOKIE: &str = "__Host-dir_session";

pub struct Identity {
    repo: D1IdentityRepository,
    entropy: WebCryptoEntropy,
    config: IdentityConfig,
    now: i64,
}

impl Identity {
    pub fn from_env(env: &Env) -> Result<Self> {
        let var = |name: &str| {
            env.var(name)
                .ok()
                .map(|v| v.to_string())
                .filter(|v| !v.trim().is_empty())
        };
        let origins = var(bindings::ORIGINS)
            .map(|v| {
                v.split(',')
                    .map(|o| o.trim().trim_end_matches('/').to_owned())
                    .filter(|o| !o.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            repo: D1IdentityRepository::new(env.d1(bindings::DB)?),
            entropy: WebCryptoEntropy,
            config: IdentityConfig {
                rp: RelyingParty {
                    id: var(bindings::RP_ID).unwrap_or_default(),
                    name: "Directory".into(),
                    origins,
                },
                setup_token: env
                    .secret(bindings::SETUP_TOKEN)
                    .ok()
                    .map(|s| s.to_string()),
            },
            now: Date::now().as_millis() as i64,
        })
    }

    pub fn service(&self) -> IdentityService<'_, D1IdentityRepository, WebCryptoEntropy> {
        IdentityService::new(&self.repo, &self.entropy, &self.config, self.now)
    }

    fn origin_allowed(&self, origin: Option<&str>) -> bool {
        origin.is_some_and(|o| self.config.rp.origins.iter().any(|a| a == o))
    }
}

pub fn identity_error(e: IdentityError) -> Result<Response> {
    match e {
        IdentityError::NotConfigured => error(503, "auth_not_configured"),
        IdentityError::SetupUnavailable => error(409, "setup_unavailable"),
        IdentityError::SetupTokenInvalid => error(401, "setup_token_invalid"),
        IdentityError::InvalidInput(field) => error_with(400, "invalid_input", field),
        IdentityError::ChallengeInvalid => error(400, "challenge_invalid"),
        IdentityError::Verification(detail) => {
            worker::console_warn!("webauthn verification failed: {detail:?}");
            error(401, "authentication_failed")
        }
        IdentityError::UnknownCredential => error(401, "authentication_failed"),
        IdentityError::LastPasskey => error(409, "last_passkey"),
        IdentityError::NotFound => error(404, "not_found"),
        IdentityError::Backend(detail) => {
            worker::console_error!("identity backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}

fn read_cookie(req: &Request, name: &str) -> Result<Option<String>> {
    let Some(header) = req.headers().get("Cookie")? else {
        return Ok(None);
    };
    Ok(header.split(';').find_map(|pair| {
        let (k, v) = pair.trim().split_once('=')?;
        (k == name && !v.is_empty()).then(|| v.to_owned())
    }))
}

fn session_cookie(token: &str, max_age_secs: i64) -> String {
    format!("{SESSION_COOKIE}={token}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={max_age_secs}")
}

fn with_session(mut resp: Response, issued: &IssuedSession, now: i64) -> Result<Response> {
    let max_age = ((issued.expires_at - now) / 1000).max(0);
    resp.headers_mut()
        .append("Set-Cookie", &session_cookie(&issued.token, max_age))?;
    Ok(resp)
}

fn is_safe_method(m: &Method) -> bool {
    matches!(m, Method::Get | Method::Head | Method::Options)
}

/// Resolve the caller. `Err(response)` is a ready 401/403/5xx.
///
/// Bearer device tokens take precedence over cookies. Cookie-authenticated
/// writes must carry an allowed `Origin` (CSRF defence on top of SameSite=Strict).
pub async fn authenticate(
    req: &Request,
    identity: &Identity,
) -> Result<std::result::Result<Principal, Response>> {
    let svc = identity.service();
    if let Some(token) = bearer_token(req.headers().get("Authorization")?.as_deref()) {
        return match svc.authenticate_api_token(token).await {
            Ok(Some(p)) => Ok(Ok(p)),
            Ok(None) => Ok(Err(unauthorized()?)),
            Err(e) => Ok(Err(identity_error(e)?)),
        };
    }
    let Some(token) = read_cookie(req, SESSION_COOKIE)? else {
        return Ok(Err(unauthorized()?));
    };
    if !is_safe_method(&req.method())
        && !identity.origin_allowed(req.headers().get("Origin")?.as_deref())
    {
        return Ok(Err(error(403, "origin_not_allowed")?));
    }
    match svc.authenticate_session(&token).await {
        Ok(Some(p)) => Ok(Ok(p)),
        Ok(None) => Ok(Err(unauthorized()?)),
        Err(e) => Ok(Err(identity_error(e)?)),
    }
}

/// `authenticate` with identity built from the environment: the guard for data routes.
pub async fn principal(
    req: &Request,
    env: &Env,
) -> Result<std::result::Result<Principal, Response>> {
    authenticate(req, &Identity::from_env(env)?).await
}

fn unauthorized() -> Result<Response> {
    let mut resp = error(401, "unauthorized")?;
    resp.headers_mut().set("WWW-Authenticate", "Bearer")?;
    Ok(resp)
}

/// Credential management needs an interactive session: a leaked device token
/// must not be able to mint more tokens or passkeys.
fn require_session(p: &Principal) -> Option<Result<Response>> {
    (p.method != AuthMethod::Session).then(|| error(403, "session_required"))
}

/// Per-client throttle for unauthenticated sign-in and setup endpoints.
/// Fails open on storage errors (logged): availability over strictness here,
/// since WebAuthn itself is not guessable.
async fn throttle(req: &Request, ctx: &RouteContext<()>) -> Result<Option<Response>> {
    let ip = req
        .headers()
        .get("CF-Connecting-IP")?
        .unwrap_or_else(|| "local".into());
    let store = D1RateLimitStore::new(ctx.env.d1(bindings::DB)?);
    let now = Date::now().as_millis() as i64;
    match rate_limit::check(&store, AUTH_POLICY, &format!("auth:{ip}"), now).await {
        Ok(Decision::Allowed) => Ok(None),
        Ok(Decision::Limited { retry_after_secs }) => {
            let mut resp = error(429, "rate_limited")?;
            resp.headers_mut()
                .set("Retry-After", &retry_after_secs.to_string())?;
            Ok(Some(resp))
        }
        Err(e) => {
            worker::console_error!("rate limit store error: {e}");
            Ok(None)
        }
    }
}

macro_rules! throttled {
    ($req:expr, $ctx:expr) => {
        if let Some(resp) = throttle(&$req, &$ctx).await? {
            return Ok(resp);
        }
    };
}

/// Shared guard for auth-only routes.
macro_rules! principal {
    ($req:expr, $identity:expr) => {
        try_or_respond!(authenticate(&$req, &$identity).await?)
    };
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionBody {
    authenticated: bool,
    setup_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<UserBody>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserBody {
    id: String,
    display_name: String,
}

pub async fn session(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let identity = Identity::from_env(&ctx.env)?;
    let svc = identity.service();
    let setup_required = match svc.setup_required().await {
        Ok(v) => v,
        Err(e) => return identity_error(e),
    };
    let user = match authenticate(&req, &identity).await? {
        Ok(p) => match svc.current_user(&p).await {
            Ok(u) => Some(UserBody {
                id: u.id,
                display_name: u.display_name,
            }),
            Err(e) => return identity_error(e),
        },
        Err(_) => None,
    };
    json(
        200,
        &SessionBody {
            authenticated: user.is_some(),
            setup_required,
            user,
        },
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetupRequest {
    setup_token: String,
    #[serde(default)]
    display_name: Option<String>,
}

pub async fn setup_options(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    throttled!(req, ctx);
    let body: SetupRequest = try_or_respond!(read_json(&mut req).await?);
    let identity = Identity::from_env(&ctx.env)?;
    match identity
        .service()
        .begin_setup(&body.setup_token, body.display_name.as_deref())
        .await
    {
        Ok(opts) => json(200, &opts),
        Err(e) => identity_error(e),
    }
}

pub async fn setup_verify(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    throttled!(req, ctx);
    let input: RegistrationInput = try_or_respond!(read_json(&mut req).await?);
    let identity = Identity::from_env(&ctx.env)?;
    match identity.service().finish_setup(&input).await {
        Ok(issued) => {
            let body = UserBody {
                id: issued.user.id.clone(),
                display_name: issued.user.display_name.clone(),
            };
            with_session(json(201, &body)?, &issued, identity.now)
        }
        Err(e) => identity_error(e),
    }
}

pub async fn login_options(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    throttled!(req, ctx);
    let identity = Identity::from_env(&ctx.env)?;
    match identity.service().begin_login().await {
        Ok(opts) => json(200, &opts),
        Err(e) => identity_error(e),
    }
}

pub async fn login_verify(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    throttled!(req, ctx);
    let input: AssertionInput = try_or_respond!(read_json(&mut req).await?);
    let identity = Identity::from_env(&ctx.env)?;
    match identity.service().finish_login(&input).await {
        Ok(issued) => {
            let body = UserBody {
                id: issued.user.id.clone(),
                display_name: issued.user.display_name.clone(),
            };
            with_session(json(200, &body)?, &issued, identity.now)
        }
        Err(e) => identity_error(e),
    }
}

pub async fn logout(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let identity = Identity::from_env(&ctx.env)?;
    if let Some(token) = read_cookie(&req, SESSION_COOKIE)? {
        if !identity.origin_allowed(req.headers().get("Origin")?.as_deref()) {
            return error(403, "origin_not_allowed");
        }
        if let Err(e) = identity.service().logout(&token).await {
            return identity_error(e);
        }
    }
    let mut resp = no_content()?;
    resp.headers_mut()
        .append("Set-Cookie", &session_cookie("", 0))?;
    Ok(resp)
}

pub async fn list_passkeys(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let identity = Identity::from_env(&ctx.env)?;
    let p = principal!(req, identity);
    match identity.service().list_passkeys(&p).await {
        Ok(list) => json(200, &serde_json::json!({ "passkeys": list })),
        Err(e) => identity_error(e),
    }
}

pub async fn add_passkey_options(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let identity = Identity::from_env(&ctx.env)?;
    let p = principal!(req, identity);
    if let Some(resp) = require_session(&p) {
        return resp;
    }
    match identity.service().begin_add_passkey(&p).await {
        Ok(opts) => json(200, &opts),
        Err(e) => identity_error(e),
    }
}

pub async fn add_passkey_verify(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let identity = Identity::from_env(&ctx.env)?;
    let p = principal!(req, identity);
    if let Some(resp) = require_session(&p) {
        return resp;
    }
    let input: RegistrationInput = try_or_respond!(read_json(&mut req).await?);
    match identity.service().finish_add_passkey(&p, &input).await {
        Ok(info) => json(201, &info),
        Err(e) => identity_error(e),
    }
}

pub async fn delete_passkey(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let identity = Identity::from_env(&ctx.env)?;
    let p = principal!(req, identity);
    if let Some(resp) = require_session(&p) {
        return resp;
    }
    let id = ctx.param("id").cloned().unwrap_or_default();
    match identity.service().delete_passkey(&p, &id).await {
        Ok(()) => no_content(),
        Err(e) => identity_error(e),
    }
}

pub async fn list_tokens(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let identity = Identity::from_env(&ctx.env)?;
    let p = principal!(req, identity);
    if let Some(resp) = require_session(&p) {
        return resp;
    }
    match identity.service().list_api_tokens(&p).await {
        Ok(list) => json(200, &serde_json::json!({ "tokens": list })),
        Err(e) => identity_error(e),
    }
}

#[derive(Deserialize)]
struct NewTokenRequest {
    #[serde(default)]
    name: Option<String>,
}

pub async fn create_token(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let identity = Identity::from_env(&ctx.env)?;
    let p = principal!(req, identity);
    if let Some(resp) = require_session(&p) {
        return resp;
    }
    let body: NewTokenRequest = try_or_respond!(read_json(&mut req).await?);
    match identity
        .service()
        .create_api_token(&p, body.name.as_deref())
        .await
    {
        Ok((token, secret)) => json(
            201,
            &serde_json::json!({ "token": token, "secret": secret }),
        ),
        Err(e) => identity_error(e),
    }
}

pub async fn delete_token(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let identity = Identity::from_env(&ctx.env)?;
    let p = principal!(req, identity);
    if let Some(resp) = require_session(&p) {
        return resp;
    }
    let id = ctx.param("id").cloned().unwrap_or_default();
    match identity.service().delete_api_token(&p, &id).await {
        Ok(()) => no_content(),
        Err(e) => identity_error(e),
    }
}
