//! Ely.by's Yggdrasil-compatible authserver (https://ely.by), the client the launcher and the
//! game share: a sign-in with a login and password (and a two-factor code when the account asks
//! for one), then the `validate`, `refresh` and `invalidate` calls that keep the token alive
//! ([docs/elyby-account.md](../../docs/elyby-account.md)). Passwords are never kept; each caller
//! stores what [`Grant`] carries its own way (the launcher's keychain, the game's private file).
//!
//! The address comes from the product: `HORIZON_ELYBY_URL` when set (tests and local mocks),
//! else the address the executable passed in ([`set_built_in`]) from the Horizon layer's list
//! (`horizon-addresses`, docs/layers.md#built-in-addresses). This crate is engine code, so it
//! holds no outside address of its own.

use std::{fmt, sync::OnceLock, time::Duration};

static BUILT_IN: OnceLock<&'static str> = OnceLock::new();

/// The product's authserver address, for the engine code already running (the game's launch
/// options, its sign-in provider): the Horizon layer calls this once at startup.
pub fn set_built_in(base: &'static str) {
    let _ = BUILT_IN.set(base);
}

/// The authserver: `HORIZON_ELYBY_URL` overrides it, else the address from [`set_built_in`].
/// Empty when the product set neither, and then no call can reach it.
pub fn base_url() -> String {
    normalize_base(std::env::var("HORIZON_ELYBY_URL").ok().as_deref())
        .or_else(|| BUILT_IN.get().copied().map(str::to_owned))
        .unwrap_or_default()
}

pub fn normalize_base(text: Option<&str>) -> Option<String> {
    text.map(|t| t.trim().trim_end_matches('/'))
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
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

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TwoFactorRequired => {
                f.write_str("This Ely.by account uses two-factor authentication")
            }
            Self::InvalidCredentials => f.write_str("Wrong Ely.by login or password"),
            Self::SessionExpired => f.write_str("The Ely.by session expired: sign in again"),
            Self::Unavailable => f.write_str("Ely.by is not reachable right now"),
            Self::Other(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for Error {}

/// What `authenticate` and `refresh` answer: the profile and the tokens that keep it signed in.
#[derive(Clone, PartialEq, Eq)]
pub struct Grant {
    pub name: String,
    /// The profile UUID, lowercase and without dashes.
    pub uuid: String,
    pub access_token: String,
    pub client_token: String,
}

impl fmt::Debug for Grant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Grant")
            .field("name", &self.name)
            .field("uuid", &self.uuid)
            .field("tokens", &"<hidden>")
            .finish()
    }
}

/// The authserver at one address, with one time limit and one user agent.
#[derive(Clone, Debug)]
pub struct Client {
    base: String,
    agent: ureq::Agent,
}

impl Client {
    /// Against [`base_url`].
    pub fn new(user_agent: &str, timeout: Duration) -> Self {
        Self::at(base_url(), user_agent, timeout)
    }

    pub fn at(base: impl Into<String>, user_agent: &str, timeout: Duration) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(timeout)
            .user_agent(user_agent)
            .build();
        let base = normalize_base(Some(base.into().as_str())).unwrap_or_default();
        Self { base, agent }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// Sign in with the account's login (username or e-mail) and password. An account with
    /// two-factor authentication answers `TwoFactorRequired` until the code comes along.
    pub fn authenticate(
        &self,
        login: &str,
        password: &str,
        two_factor: Option<&str>,
    ) -> Result<Grant, Error> {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|e| Error::Other(e.to_string()))?;
        let client_token = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let body = self.post(
            "/auth/authenticate",
            serde_json::json!({
                "username": login.trim(),
                "password": login_password(password, two_factor),
                "clientToken": client_token,
                "requestUser": false,
            }),
        )?;
        grant(&body)
    }

    /// Ok(true) the token works, Ok(false) it was refused, Err(Unavailable) no answer (so
    /// nothing is known).
    pub fn validate(&self, access_token: &str) -> Result<bool, Error> {
        match self
            .agent
            .post(&format!("{}/auth/validate", self.base))
            .set("Content-Type", "application/json; charset=utf-8")
            .send_string(&serde_json::json!({ "accessToken": access_token }).to_string())
        {
            Ok(_) => Ok(true),
            Err(ureq::Error::Status(status, _)) if status < 500 => Ok(false),
            Err(_) => Err(Error::Unavailable),
        }
    }

    /// A new access token for the same client token. A token the authserver no longer accepts is
    /// `SessionExpired`: the player signs in again.
    pub fn refresh(&self, access_token: &str, client_token: &str) -> Result<Grant, Error> {
        let body = self
            .post(
                "/auth/refresh",
                serde_json::json!({
                    "accessToken": access_token,
                    "clientToken": client_token,
                    "requestUser": false,
                }),
            )
            .map_err(|e| match e {
                Error::InvalidCredentials | Error::TwoFactorRequired => Error::SessionExpired,
                other => other,
            })?;
        grant(&body)
    }

    /// `invalidate`: the token stops working. Best effort: the player is signed out whatever the
    /// answer.
    pub fn invalidate(&self, access_token: &str, client_token: &str) {
        let _ = self.post(
            "/auth/invalidate",
            serde_json::json!({
                "accessToken": access_token,
                "clientToken": client_token,
            }),
        );
    }

    fn post(&self, path: &str, body: serde_json::Value) -> Result<String, Error> {
        match self
            .agent
            .post(&format!("{}{path}", self.base))
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
}

/// The two-factor code goes after the password, `password:code`.
fn login_password(password: &str, two_factor: Option<&str>) -> String {
    match two_factor.map(str::trim).filter(|c| !c.is_empty()) {
        Some(code) => format!("{password}:{code}"),
        None => password.to_owned(),
    }
}

/// `authenticate` and `refresh` answer `accessToken`, `clientToken` and
/// `selectedProfile {id, name}`.
fn grant(body: &str) -> Result<Grant, Error> {
    let bad = || Error::Other("Ely.by sent an answer Horizon can't read".into());
    let v: serde_json::Value = serde_json::from_str(body).map_err(|_| bad())?;
    let profile = v.get("selectedProfile").ok_or_else(|| {
        Error::Other("This Ely.by account has no profile (choose a username on ely.by)".into())
    })?;
    let text =
        |v: &serde_json::Value, key: &str| v.get(key).and_then(|s| s.as_str()).map(str::to_owned);
    let name = text(profile, "name").ok_or_else(bad)?;
    if name.is_empty() || name.chars().count() > 16 {
        return Err(Error::Other(format!(
            "The Ely.by username {name:?} is not a valid player name"
        )));
    }
    Ok(Grant {
        name,
        uuid: text(profile, "id")
            .as_deref()
            .and_then(uuid_hex)
            .ok_or_else(bad)?,
        access_token: text(&v, "accessToken").ok_or_else(bad)?,
        client_token: text(&v, "clientToken").ok_or_else(bad)?,
    })
}

/// A profile id as 32 lowercase hex digits, dashes dropped.
fn uuid_hex(text: &str) -> Option<String> {
    let hex: String = text.chars().filter(|c| *c != '-').collect();
    (hex.len() == 32 && hex.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| hex.to_ascii_lowercase())
}

/// The authserver's `{"error", "errorMessage"}` bodies.
fn map_error(status: u16, body: &str) -> Error {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    let message = v.get("errorMessage").and_then(|m| m.as_str()).unwrap_or("");
    if status >= 500 {
        return Error::Unavailable;
    }
    if message.to_ascii_lowercase().contains("two factor") {
        return Error::TwoFactorRequired;
    }
    if status == 401
        || v.get("error").and_then(|e| e.as_str()) == Some("ForbiddenOperationException")
    {
        return Error::InvalidCredentials;
    }
    if message.is_empty() {
        Error::Other(format!("Ely.by answered status {status}"))
    } else {
        Error::Other(message.to_owned())
    }
}

/// A test HTTP server: answers `count` requests through `handler` (method, path, authorization
/// header, body) and returns its address.
#[cfg(any(test, feature = "mock"))]
pub mod mock {
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
    };

    pub fn serve<F>(count: usize, mut handler: F) -> String
    where
        F: FnMut(&str, &str, &str, &str) -> (u16, String) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(count) {
                let Ok(mut stream) = stream else { continue };
                let Ok(read) = stream.try_clone() else {
                    continue;
                };
                let mut reader = BufReader::new(read);
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or("").to_owned();
                let path = parts.next().unwrap_or("").to_owned();
                let (mut length, mut auth) = (0, String::new());
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).is_err() {
                        break;
                    }
                    let header = header.trim_end();
                    if header.is_empty() {
                        break;
                    }
                    let Some((key, value)) = header.split_once(':') else {
                        continue;
                    };
                    match key.to_ascii_lowercase().as_str() {
                        "content-length" => length = value.trim().parse().unwrap_or(0),
                        "authorization" => auth = value.trim().to_owned(),
                        _ => {}
                    }
                }
                let mut body = vec![0; length];
                if reader.read_exact(&mut body).is_err() {
                    continue;
                }
                let body = String::from_utf8_lossy(&body).into_owned();
                let (status, text) = handler(&method, &path, &auth, &body);
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                    text.len()
                );
            }
        });
        format!("http://{addr}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "069a79f444e94726a5befca90e38aaf5";
    const GRANT: &str = r#"{"accessToken":"acc","clientToken":"cli","selectedProfile":{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}}"#;

    fn client(base: &str) -> Client {
        Client::at(base, "horizon-test", Duration::from_secs(5))
    }

    #[test]
    fn a_grant_carries_the_profile_and_its_tokens() {
        let read = grant(GRANT).unwrap();
        assert_eq!(read.name, "Notch");
        assert_eq!(read.uuid, UUID);
        assert_eq!(
            (read.access_token.as_str(), read.client_token.as_str()),
            ("acc", "cli")
        );
        // Dashed ids read the same.
        let dashed = GRANT.replace(UUID, "069a79f4-44e9-4726-a5be-fca90e38aaf5");
        assert_eq!(grant(&dashed).unwrap().uuid, UUID);
        // No profile, a bad id, a name Minecraft can't carry, a broken body.
        assert!(grant(r#"{"accessToken":"a","clientToken":"c"}"#).is_err());
        assert!(grant(
            r#"{"accessToken":"a","clientToken":"c","selectedProfile":{"id":"nope","name":"N"}}"#
        )
        .is_err());
        let long = GRANT.replace("\"name\":\"Notch\"", "\"name\":\"ThisNameIsTooLong17\"");
        assert!(grant(&long).is_err());
        assert!(grant("").is_err());
        assert!(!format!("{:?}", grant(GRANT).unwrap()).contains("\"acc\""));
    }

    #[test]
    fn errors_map_like_the_authserver_talks() {
        let two = r#"{"error":"ForbiddenOperationException","errorMessage":"Account protected with two factor auth."}"#;
        assert_eq!(map_error(401, two), Error::TwoFactorRequired);
        let bad = r#"{"error":"ForbiddenOperationException","errorMessage":"Invalid credentials. Invalid email or password."}"#;
        assert_eq!(map_error(401, bad), Error::InvalidCredentials);
        assert_eq!(map_error(401, ""), Error::InvalidCredentials);
        assert_eq!(map_error(503, "down"), Error::Unavailable);
        assert_eq!(
            map_error(
                400,
                r#"{"error":"IllegalArgumentException","errorMessage":"Bad"}"#
            ),
            Error::Other("Bad".into())
        );
        assert_eq!(
            map_error(400, ""),
            Error::Other("Ely.by answered status 400".into())
        );
    }

    #[test]
    fn two_factor_code_follows_the_password() {
        assert_eq!(login_password("pw", None), "pw");
        assert_eq!(login_password("pw", Some("")), "pw");
        assert_eq!(login_password("pw", Some(" 123456 ")), "pw:123456");
    }

    #[test]
    fn base_addresses_normalize_and_the_environment_wins() {
        assert_eq!(normalize_base(None), None);
        assert_eq!(normalize_base(Some("  ")), None);
        assert_eq!(
            normalize_base(Some("http://127.0.0.1:9/")),
            Some("http://127.0.0.1:9".to_owned())
        );
    }

    #[test]
    fn the_authserver_flow_works_against_a_mock() {
        let refreshed = GRANT.replace("\"acc\"", "\"new\"");
        let base = mock::serve(4, move |method, path, _, body| {
            assert_eq!(method, "POST");
            match path {
                "/auth/authenticate" => {
                    assert!(body.contains("\"username\":\"Notch\""), "{body}");
                    assert!(body.contains("\"password\":\"pw\""), "{body}");
                    (200, GRANT.to_owned())
                }
                "/auth/validate" => (200, String::new()),
                "/auth/refresh" => (200, refreshed.clone()),
                "/auth/invalidate" => (200, String::new()),
                other => panic!("unexpected path {other}"),
            }
        });
        let client = client(&base);
        let grant = client.authenticate("Notch", "pw", None).unwrap();
        assert_eq!(grant.name, "Notch");
        assert!(client.validate(&grant.access_token).unwrap());
        let fresh = client
            .refresh(&grant.access_token, &grant.client_token)
            .unwrap();
        assert_eq!(fresh.access_token, "new");
        client.invalidate(&grant.access_token, &grant.client_token);
    }

    #[test]
    fn a_refused_token_is_a_session_that_expired() {
        let base = mock::serve(2, |method, path, _, _| match (method, path) {
            ("POST", "/auth/validate") => (401, "{}".into()),
            ("POST", "/auth/refresh") => (
                401,
                r#"{"error":"ForbiddenOperationException","errorMessage":"Invalid token."}"#.into(),
            ),
            other => panic!("unexpected {other:?}"),
        });
        let client = client(&base);
        assert!(!client.validate("old").unwrap());
        assert_eq!(client.refresh("old", "cli"), Err(Error::SessionExpired));
    }
}
