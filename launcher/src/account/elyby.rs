//! Ely.by's Yggdrasil-compatible authserver, for the launcher's account chooser.
//!
//! The game itself has the same client in `horizon-client-render`; this one is separate because
//! the launcher must not link the renderer. It authenticates a login and password (and a
//! two-factor code when the account asks for one), validates and renews the kept token, and
//! returns only a profile and the tokens the game's session file needs. Passwords are never
//! written anywhere.
use std::time::Duration;

pub const DEFAULT_BASE: &str = "https://authserver.ely.by";

/// The authserver: `HORIZON_ELYBY_URL` overrides it (tests and local mocks).
pub fn base_url() -> String {
    normalize_base(std::env::var("HORIZON_ELYBY_URL").ok().as_deref())
}

fn normalize_base(text: Option<&str>) -> String {
    text.map(|t| t.trim().trim_end_matches('/'))
        .filter(|t| !t.is_empty())
        .unwrap_or(DEFAULT_BASE)
        .to_owned()
}

/// What the launcher keeps of an Ely.by account, never the password.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    /// The profile UUID, lowercase and without dashes.
    pub uuid: String,
    pub name: String,
}

/// The tokens that keep an Ely.by account signed in.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Secrets {
    pub access_token: String,
    pub client_token: String,
}

impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secrets(<hidden>)")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The account has two-factor authentication: ask for the code and call again with it.
    TwoFactorRequired,
    /// Wrong login, password or code.
    InvalidCredentials,
    /// The kept token was refused and can't be renewed: sign in again.
    SessionExpired,
    /// No answer, or a 5xx.
    Unavailable,
    /// Anything else, with the authserver's message.
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TwoFactorRequired => {
                f.write_str("This Ely.by account uses two-factor authentication.")
            }
            Self::InvalidCredentials => f.write_str("Wrong Ely.by login or password."),
            Self::SessionExpired => f.write_str("The Ely.by session expired. Sign in again."),
            Self::Unavailable => f.write_str("Ely.by is not reachable right now."),
            Self::Other(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {}

/// Sign in with an Ely.by login (username or e-mail) and password. `two_factor` is the code when
/// the account asks for one.
pub fn authenticate(
    login: &str,
    password: &str,
    two_factor: Option<&str>,
) -> Result<(Profile, Secrets), Error> {
    authenticate_at(&base_url(), login, password, two_factor)
}

fn authenticate_at(
    base: &str,
    login: &str,
    password: &str,
    two_factor: Option<&str>,
) -> Result<(Profile, Secrets), Error> {
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|e| Error::Other(e.to_string()))?;
    let body = post_at(
        base,
        "/auth/authenticate",
        serde_json::json!({
            "username": login.trim(),
            "password": login_password(password, two_factor),
            "clientToken": random.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "requestUser": false,
        }),
    )?;
    from_grant(&body)
}

/// Check the kept token and renew it when the authserver refuses it. No answer keeps the token:
/// joining then says the authentication servers are unreachable, instead of signing the player out.
pub fn restore(profile: &Profile, secrets: &Secrets) -> Result<(Profile, Secrets), Error> {
    restore_at(&base_url(), profile, secrets)
}

fn restore_at(
    base: &str,
    profile: &Profile,
    secrets: &Secrets,
) -> Result<(Profile, Secrets), Error> {
    match validate_at(base, secrets) {
        Ok(true) => Ok((profile.clone(), secrets.clone())),
        Ok(false) => refresh_at(base, secrets).map_err(|e| match e {
            Error::InvalidCredentials | Error::TwoFactorRequired => Error::SessionExpired,
            other => other,
        }),
        Err(Error::Unavailable) => Ok((profile.clone(), secrets.clone())),
        Err(e) => Err(e),
    }
}

fn validate_at(base: &str, secrets: &Secrets) -> Result<bool, Error> {
    let body = serde_json::json!({ "accessToken": secrets.access_token });
    match agent()
        .post(&format!("{base}/auth/validate"))
        .set("Content-Type", "application/json; charset=utf-8")
        .send_string(&body.to_string())
    {
        Ok(_) => Ok(true),
        Err(ureq::Error::Status(status, _)) if status < 500 => Ok(false),
        Err(_) => Err(Error::Unavailable),
    }
}

fn refresh_at(base: &str, secrets: &Secrets) -> Result<(Profile, Secrets), Error> {
    let body = post_at(
        base,
        "/auth/refresh",
        serde_json::json!({
            "accessToken": secrets.access_token,
            "clientToken": secrets.client_token,
            "requestUser": false,
        }),
    )?;
    from_grant(&body)
}

fn login_password(password: &str, two_factor: Option<&str>) -> String {
    match two_factor.map(str::trim).filter(|c| !c.is_empty()) {
        Some(code) => format!("{password}:{code}"),
        None => password.to_owned(),
    }
}

fn from_grant(body: &str) -> Result<(Profile, Secrets), Error> {
    let bad = || Error::Other("Ely.by sent an answer the launcher can't read.".into());
    let v: serde_json::Value = serde_json::from_str(body).map_err(|_| bad())?;
    let profile = v.get("selectedProfile").ok_or_else(|| {
        Error::Other("This Ely.by account has no profile (choose a username on ely.by).".into())
    })?;
    let text = |v: &serde_json::Value, key: &str| {
        v.get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let name = text(profile, "name").ok_or_else(bad)?;
    if name.is_empty() || name.chars().count() > 16 {
        return Err(Error::Other(format!(
            "The Ely.by username {name:?} is not a valid player name."
        )));
    }
    let uuid = text(profile, "id")
        .and_then(|id| crate::heads::parse_uuid(&id))
        .map(|uuid| format!("{uuid:032x}"))
        .ok_or_else(bad)?;
    Ok((
        Profile { uuid, name },
        Secrets {
            access_token: text(&v, "accessToken").ok_or_else(bad)?,
            client_token: text(&v, "clientToken").ok_or_else(bad)?,
        },
    ))
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(20))
        .user_agent(&format!("horizon-launcher/{}", crate::backend::VERSION))
        .build()
}

fn post_at(base: &str, path: &str, body: serde_json::Value) -> Result<String, Error> {
    match agent()
        .post(&format!("{base}{path}"))
        .set("Content-Type", "application/json; charset=utf-8")
        .send_string(&body.to_string())
    {
        Ok(response) => response.into_string().map_err(|_| Error::Unavailable),
        Err(ureq::Error::Status(status, response)) => Err(map_error(
            status,
            &response.into_string().unwrap_or_default(),
        )),
        Err(ureq::Error::Transport(_)) => Err(Error::Unavailable),
    }
}

fn map_error(status: u16, body: &str) -> Error {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    let message = v
        .get("errorMessage")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if status >= 500 {
        return Error::Unavailable;
    }
    if message.to_ascii_lowercase().contains("two factor") {
        return Error::TwoFactorRequired;
    }
    if status == 401
        || v.get("error").and_then(serde_json::Value::as_str) == Some("ForbiddenOperationException")
    {
        return Error::InvalidCredentials;
    }
    if message.is_empty() {
        Error::Other(format!("Ely.by sign-in failed (status {status})."))
    } else {
        Error::Other(message.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::super::http::serve;
    use super::*;

    const UUID: &str = "069a79f444e94726a5befca90e38aaf5";
    const GRANT: &str = r#"{"accessToken":"acc","clientToken":"cli","selectedProfile":{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}}"#;

    #[test]
    fn a_grant_becomes_a_profile_and_secrets() {
        let (profile, secrets) = from_grant(GRANT).unwrap();
        assert_eq!(profile.uuid, UUID);
        assert_eq!(profile.name, "Notch");
        assert_eq!(secrets.access_token, "acc");
        assert!(from_grant(r#"{"accessToken":"a","clientToken":"c"}"#).is_err());
        assert!(from_grant("").is_err());
    }

    #[test]
    fn two_factor_and_bad_passwords_are_told_apart() {
        assert_eq!(
            map_error(
                401,
                r#"{"error":"ForbiddenOperationException","errorMessage":"Account protected with two factor auth."}"#
            ),
            Error::TwoFactorRequired
        );
        assert_eq!(
            map_error(
                401,
                r#"{"error":"ForbiddenOperationException","errorMessage":"Invalid credentials."}"#
            ),
            Error::InvalidCredentials
        );
        assert_eq!(map_error(503, ""), Error::Unavailable);
        assert_eq!(
            map_error(400, r#"{"errorMessage":"Bad"}"#),
            Error::Other("Bad".into())
        );
    }

    #[test]
    fn authenticate_sends_the_code_and_reads_the_grant() {
        let base = serve(1, |method, path, _, body| {
            assert_eq!((method, path), ("POST", "/auth/authenticate"));
            assert!(body.contains(r#""password":"pw:123456""#), "{body}");
            assert!(body.contains(r#""clientToken":""#), "{body}");
            (200, GRANT.into())
        });
        let (profile, _) = authenticate_at(&base, "me@example.com", "pw", Some("123456")).unwrap();
        assert_eq!(profile.name, "Notch");
    }

    #[test]
    fn a_refused_token_is_refreshed_and_an_unreachable_one_is_kept() {
        let base = serve(2, |method, path, _, _| match (method, path) {
            ("POST", "/auth/validate") => (401, "{}".into()),
            ("POST", "/auth/refresh") => (200, GRANT.into()),
            other => panic!("unexpected {other:?}"),
        });
        let old = Secrets {
            access_token: "old".into(),
            client_token: "cli".into(),
        };
        let profile = Profile {
            uuid: UUID.into(),
            name: "Notch".into(),
        };
        let (_, fresh) = restore_at(&base, &profile, &old).unwrap();
        assert_eq!(fresh.access_token, "acc");
        let offline = normalize_base(Some("http://127.0.0.1:9"));
        let (_, kept) = restore_at(&offline, &profile, &old).unwrap();
        assert_eq!(kept.access_token, "old");
    }
}
