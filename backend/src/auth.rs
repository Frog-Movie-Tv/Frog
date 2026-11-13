use argon2::password_hash::rand_core::OsRng as ArgonRng;
use argon2::password_hash::{PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Argon2, PasswordHash};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::cookie::{Cookie, SameSite};
use chrono::{Duration, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::sync::LazyLock;
use unicode_segmentation::UnicodeSegmentation;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const COOKIE_NAME: &str = "frog_session";

/// What a new name may be, in characters. Accounts from before the maximum
/// keep their names: this is checked when a name is chosen, never at sign-in.
pub const USERNAME_MIN: usize = 2;
pub const USERNAME_MAX: usize = 20;
pub const PASSWORD_MIN: usize = 8;

/// Past this, sign-in does not look a name up. Far beyond `USERNAME_MAX`, since
/// names older than it have no limit of their own, and short enough that
/// nothing a caller sends is held on to.
const SIGN_IN_NAME_MAX: usize = 256;

/// What an account is allowed to do. Ordered, so a check reads
/// `role >= Role::Admin` rather than listing every role that qualifies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Admin,
    Owner,
}

impl Role {
    pub fn from_db(raw: &str) -> Self {
        match raw {
            "owner" => Role::Owner,
            "admin" => Role::Admin,
            _ => Role::User,
        }
    }

    pub fn as_db(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Admin => "admin",
            Role::User => "user",
        }
    }
}

/// Columns a request needs about whoever is making it.
type SessionRow = (i64, String, String, i64);

pub fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut ArgonRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Other(anyhow::anyhow!("hashing failed: {e}")))
}

pub fn verify_password(password: &str, stored: &str) -> bool {
    match PasswordHash::new(stored) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// Something to check a password against when a name has no account, made
/// with the same settings as every real hash. Its answer is never read.
static DECOY_HASH: LazyLock<Option<String>> = LazyLock::new(|| hash_password("decoy").ok());

/// Make the decoy now, at start-up, so the first unknown name to sign in does
/// not pay for making it and stand out.
pub fn prepare_decoy() {
    LazyLock::force(&DECOY_HASH);
}

/// The work of checking a password, for a name with no account behind it.
pub fn spend_password_work(password: &str) {
    if let Some(hash) = DECOY_HASH.as_deref() {
        verify_password(password, hash);
    }
}

/// Characters as a person counts them: a flag or a family emoji is one, though
/// each is several code points.
fn characters(text: &str) -> usize {
    text.graphemes(true).count()
}

/// A name a new account or a rename may take. Surrounding spaces do not count;
/// `create_account` drops them.
pub fn check_username(name: &str) -> AppResult<()> {
    let name = name.trim();
    let length = characters(name);
    if length < USERNAME_MIN {
        return Err(AppError::BadRequest(format!(
            "username needs at least {USERNAME_MIN} characters"
        )));
    }
    // Twenty characters can hide any number of combining marks, and a name sign-in
    // would refuse must never be taken.
    if length > USERNAME_MAX || sign_in_name_too_long(name) {
        return Err(AppError::BadRequest(format!(
            "username can be at most {USERNAME_MAX} characters"
        )));
    }
    Ok(())
}

pub fn password_long_enough(password: &str) -> bool {
    characters(password) >= PASSWORD_MIN
}

/// True for a name too long to be anyone's. It stops at the limit, so a
/// megabyte of name is not walked to the end.
pub fn sign_in_name_too_long(name: &str) -> bool {
    name.trim().chars().nth(SIGN_IN_NAME_MAX).is_some()
}

/// A single-use code for a link. Random enough that guessing is not a plan.
pub fn new_code() -> String {
    let mut raw = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut raw);
    hex::encode(raw)
}

/// What a link's code is stored as, so the database never holds a working one.
pub fn hash_code(code: &str) -> String {
    hex::encode(Sha256::digest(code.trim().as_bytes()))
}

/// Whether a name belongs to another account, regardless of case, so two people
/// cannot pick names that only a reader can tell apart. Compared here rather
/// than in SQL, whose NOCASE only knows ASCII. `except` lets an account change
/// the case of its own name.
pub async fn username_taken(pool: &SqlitePool, name: &str, except: Option<i64>) -> AppResult<bool> {
    let wanted = name.trim().to_lowercase();
    let accounts: Vec<(i64, String)> = sqlx::query_as("SELECT id, username FROM app_user")
        .fetch_all(pool)
        .await?;
    Ok(accounts
        .iter()
        .any(|(id, username)| Some(*id) != except && username.to_lowercase() == wanted))
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Issue a session and return the raw token. Only its SHA-256 is persisted, so
/// a leaked database file does not hand over live sessions.
pub async fn create_session(
    pool: &SqlitePool,
    user_id: i64,
    days: i64,
    user_agent: Option<&str>,
) -> AppResult<String> {
    let mut raw = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut raw);
    let token = hex::encode(raw);
    let now = Utc::now();
    let expires = now + Duration::days(days.max(1));

    sqlx::query(
        "INSERT INTO session (token_hash, user_id, created_at, expires_at, user_agent)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(hash_token(&token))
    .bind(user_id)
    .bind(now.to_rfc3339())
    .bind(expires.to_rfc3339())
    .bind(user_agent)
    .execute(pool)
    .await?;

    // Every session starts with a sign-in, and this outlives the session.
    sqlx::query("UPDATE app_user SET last_sign_in_at = ? WHERE id = ?")
        .bind(now.to_rfc3339())
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(token)
}

/// Sign out every device for one account, optionally sparing the session making
/// the request so "sign out everywhere" does not sign you out of here too.
pub async fn destroy_sessions_for(
    pool: &SqlitePool,
    user_id: i64,
    except: Option<&str>,
) -> AppResult<u64> {
    let res = match except {
        Some(token) => {
            sqlx::query("DELETE FROM session WHERE user_id = ? AND token_hash <> ?")
                .bind(user_id)
                .bind(hash_token(token))
                .execute(pool)
                .await?
        }
        None => {
            sqlx::query("DELETE FROM session WHERE user_id = ?")
                .bind(user_id)
                .execute(pool)
                .await?
        }
    };
    Ok(res.rows_affected())
}

/// Hash of a raw token, so a caller can mark which listed session is this one
/// without the token ever being compared in the clear elsewhere.
pub fn token_fingerprint(token: &str) -> String {
    hash_token(token)
}

pub async fn destroy_session(pool: &SqlitePool, token: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM session WHERE token_hash = ?")
        .bind(hash_token(token))
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn purge_expired_sessions(pool: &SqlitePool) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM session WHERE expires_at < ?")
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

/// Resolve a raw session token to the account it belongs to. Returns None for
/// an unknown or expired token, and for a disabled account — disabling has to
/// take effect on the next request, not at the next sign-in.
pub async fn session_user(pool: &SqlitePool, token: &str) -> Option<Identity> {
    let row: Option<SessionRow> = sqlx::query_as(
        "SELECT u.id, u.username, u.role, u.disabled
           FROM session s
           JOIN app_user u ON u.id = s.user_id
          WHERE s.token_hash = ? AND s.expires_at > ?",
    )
    .bind(hash_token(token))
    .bind(Utc::now().to_rfc3339())
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    match row {
        Some((_, _, _, disabled)) if disabled != 0 => None,
        Some((id, username, role, _)) => Some(Identity {
            user_id: id,
            username,
            role: Role::from_db(&role),
        }),
        None => None,
    }
}

/// `days` of `None` makes a session cookie: it dies with the browser, which
/// is what "do not remember this device" has to mean to be worth offering.
pub fn session_cookie(token: String, secure: bool, days: Option<i64>) -> Cookie<'static> {
    let mut cookie = Cookie::new(COOKIE_NAME, token);
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_secure(secure);
    cookie.set_path("/");
    if let Some(days) = days {
        cookie.set_max_age(::time::Duration::days(days.max(1)));
    }
    cookie
}

pub fn clear_cookie(secure: bool) -> Cookie<'static> {
    let mut cookie = Cookie::new(COOKIE_NAME, "");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_secure(secure);
    cookie.set_path("/");
    cookie.set_max_age(::time::Duration::seconds(0));
    cookie
}

pub fn token_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|raw| {
            Cookie::split_parse(raw.to_string())
                .filter_map(|c| c.ok())
                .find(|c| c.name() == COOKIE_NAME)
                .map(|c| c.value().to_string())
        })
}

/// The caller's address. `X-Forwarded-For` is trusted because the only
/// supported deployment puts Caddy directly in front of Frog; without a proxy
/// the socket address is used. A spoofed header can only dodge the per-address
/// bucket, never the per-account one.
pub fn client_key(headers: &axum::http::HeaderMap, peer: Option<std::net::SocketAddr>) -> String {
    for name in ["x-forwarded-for", "x-real-ip"] {
        if let Some(raw) = headers.get(name).and_then(|v| v.to_str().ok()) {
            if let Some(first) = raw.split(',').next() {
                let trimmed = first.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
    }
    peer.map(|p| p.ip().to_string())
        .unwrap_or_else(|| "unknown".into())
}

/// The wait a throttle asked for, as a person reads it.
pub fn too_many_tries(seconds: u64) -> AppError {
    let minutes = seconds.div_ceil(60);
    AppError::TooManyRequests(format!(
        "Too many tries. Try again in {minutes} {}.",
        if minutes == 1 { "minute" } else { "minutes" }
    ))
}

/// The token in `Authorization: Bearer …`, if the request carries one.
pub fn bearer_token(headers: &axum::http::HeaderMap) -> Option<String> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let (scheme, token) = raw.split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then(|| token.to_string())
}

/// The caller's socket address, when the server was started with connect info
/// and something is actually listening on a socket. It is None under the test
/// harness, so this extractor never fails a request.
pub struct ClientAddr(pub Option<std::net::SocketAddr>);

impl<S: Send + Sync> FromRequestParts<S> for ClientAddr {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(ClientAddr(
            parts
                .extensions
                .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                .map(|c| c.0),
        ))
    }
}

/// Who is making this request.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Identity {
    pub user_id: i64,
    pub username: String,
    pub role: Role,
}

/// Extractor that gates every private route. Rejecting with 401 rather than a
/// redirect keeps the API honest; the SvelteKit shell turns that into a trip to
/// the login screen.
///
/// It carries the account, so a handler cannot read or write anyone's data
/// without first saying whose it is: `auth.user_id` is the only way in.
pub struct Auth {
    pub identity: Identity,
    /// The raw token of this request, for "sign out everywhere but here".
    pub token: String,
}

impl Auth {
    pub fn user_id(&self) -> i64 {
        self.identity.user_id
    }

    pub fn role(&self) -> Role {
        self.identity.role
    }

    pub fn is_admin(&self) -> bool {
        self.identity.role >= Role::Admin
    }

    /// Guard for anything that reaches another account's data or settings.
    pub fn require_admin(&self) -> AppResult<()> {
        if self.is_admin() {
            Ok(())
        } else {
            Err(AppError::Forbidden(
                "This needs an administrator account.".into(),
            ))
        }
    }
}

impl FromRequestParts<AppState> for Auth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Some(token) = token_from_headers(&parts.headers) else {
            // A browser extension's token is good for /api/ext only. Saying so,
            // rather than "sign in", is what tells its owner why it stopped.
            if bearer_token(&parts.headers).is_some() {
                return Err(AppError::Forbidden(
                    "This token only reaches the browser extension's endpoints.".into(),
                ));
            }
            return Err(AppError::Unauthorised);
        };
        let identity = session_user(&state.pool, &token)
            .await
            .ok_or(AppError::Unauthorised)?;
        Ok(Auth { identity, token })
    }
}

/// Same as `Auth`, but refuses anyone below administrator. Using it in the
/// route table means an admin-only endpoint cannot forget the check.
pub struct AdminAuth(pub Auth);

impl FromRequestParts<AppState> for AdminAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = Auth::from_request_parts(parts, state).await?;
        auth.require_admin()?;
        Ok(AdminAuth(auth))
    }
}

/// How many accounts there are: none means the instance still needs setting up.
pub async fn account_count(pool: &SqlitePool) -> AppResult<i64> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM app_user")
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// When the account's profile picture last changed, if it has one.
pub async fn avatar_changed_at(pool: &SqlitePool, user_id: i64) -> AppResult<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT updated_at FROM user_image WHERE user_id = ? AND kind = 'avatar'")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|r| r.0))
}

/// (id, username, password hash, totp secret, disabled)
pub type LoginRow = (i64, String, String, Option<String>, i64);

/// The account a name signs in to. NOCASE so signing in is not a spelling
/// test, and one row per username.
pub async fn sign_in_row(pool: &SqlitePool, username: &str) -> AppResult<Option<LoginRow>> {
    Ok(sqlx::query_as(
        "SELECT id, username, password_hash, totp_secret, disabled
           FROM app_user WHERE username = ? COLLATE NOCASE",
    )
    .bind(username.trim())
    .fetch_optional(pool)
    .await?)
}

/// A new account with a hashed password. Returns its id.
pub async fn insert_user(pool: &SqlitePool, name: &str, hash: &str, role: Role) -> AppResult<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO app_user (username, password_hash, role, created_at)
         VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(name)
    .bind(hash)
    .bind(role.as_db())
    .bind(Utc::now().to_rfc3339())
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// The account's own password, checked before anything that changes how it
/// signs in or what it holds.
pub async fn check_password(pool: &SqlitePool, user_id: i64, password: &str) -> AppResult<()> {
    let (hash,): (String,) = sqlx::query_as("SELECT password_hash FROM app_user WHERE id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    if verify_password(password, &hash) {
        Ok(())
    } else {
        Err(AppError::BadRequest("That password is not right.".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(verify_password("correct horse battery staple", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn a_name_is_measured_in_characters() {
        assert!(check_username("ab").is_ok());
        assert!(check_username(&"x".repeat(USERNAME_MAX)).is_ok());
        assert!(check_username(&"é".repeat(USERNAME_MAX)).is_ok());
        assert!(check_username(&"😀".repeat(USERNAME_MAX)).is_ok());
        assert!(check_username("😀").is_err(), "one emoji is one character");
        assert!(check_username("é").is_err());
        assert!(check_username(&"x".repeat(USERNAME_MAX + 1)).is_err());
        assert!(check_username(&"😀".repeat(USERNAME_MAX + 1)).is_err());
        assert!(check_username(&format!("  {}  ", "x".repeat(USERNAME_MAX))).is_ok());
        assert!(check_username("   ").is_err());
    }

    const FLAG: &str = "\u{1F1E7}\u{1F1EA}";
    const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";

    #[test]
    fn a_flag_and_a_family_are_one_character_each() {
        assert!(check_username(FLAG).is_err(), "one is too few");
        assert!(check_username(FAMILY).is_err());
        assert!(
            check_username("e\u{301}").is_err(),
            "a letter and its accent"
        );
        assert!(check_username(&FAMILY.repeat(2)).is_ok());
        assert!(check_username(&FAMILY.repeat(USERNAME_MAX)).is_ok());
        assert!(check_username(&FLAG.repeat(USERNAME_MAX)).is_ok());
        assert!(check_username(&FAMILY.repeat(USERNAME_MAX + 1)).is_err());
        assert!(check_username(&FLAG.repeat(USERNAME_MAX + 1)).is_err());
    }

    #[test]
    fn a_name_sign_in_would_refuse_cannot_be_taken() {
        let piled_up = format!("a{}", "\u{301}".repeat(SIGN_IN_NAME_MAX));
        assert_eq!(characters(&piled_up), 1);
        assert!(check_username(&format!("{piled_up}{piled_up}")).is_err());
    }

    #[test]
    fn a_password_is_measured_in_characters() {
        assert!(password_long_enough("12345678"));
        assert!(!password_long_enough("1234567"));
        assert!(!password_long_enough("😀😀😀"));
        assert!(
            !password_long_enough(FAMILY),
            "seven code points, one character"
        );
    }

    #[test]
    fn sign_in_only_refuses_names_past_any_account() {
        assert!(!sign_in_name_too_long(&"x".repeat(SIGN_IN_NAME_MAX)));
        assert!(!sign_in_name_too_long(&format!(
            "  {}  ",
            "x".repeat(SIGN_IN_NAME_MAX)
        )));
        assert!(sign_in_name_too_long(&"x".repeat(SIGN_IN_NAME_MAX + 1)));
        assert!(sign_in_name_too_long(&"x".repeat(1024 * 1024)));
    }

    #[test]
    fn a_code_is_hashed_and_whitespace_does_not_change_it() {
        let h = hash_code("abc123");
        assert_eq!(h, hash_code("  abc123  "));
        assert_ne!(h, "abc123");
        assert_eq!(h.len(), 64);
    }

    #[test]
    fn codes_do_not_repeat() {
        assert_ne!(new_code(), new_code());
        assert_eq!(new_code().len(), 32);
    }

    #[test]
    fn the_decoy_is_a_real_hash() {
        assert!(DECOY_HASH
            .as_deref()
            .is_some_and(|h| h.starts_with("$argon2")));
    }

    #[test]
    fn token_hash_is_stable_and_not_the_token() {
        let h = hash_token("abc");
        assert_eq!(h, hash_token("abc"));
        assert_ne!(h, "abc");
        assert_eq!(h.len(), 64);
    }
}
