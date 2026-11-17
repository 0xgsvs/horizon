//! Ely.by accounts (https://ely.by): the Yggdrasil-compatible authserver behind
//! `auth::Provider::ElyBy` (docs/elyby-account.md). A player signs in with the
//! Ely.by login and password once; the game keeps the tokens (`horizon-elyby.json`
//! in the player's folder, readable by its owner only), never the password, and
//! joins servers that accept Ely.by the way vanilla joins with a Microsoft
//! session: `session/join` with the access token and the server hash.
//!
//! The authserver client is `horizon_elyby`, shared with the launcher. This module keeps the
//! account in a file: the game binary has no keychain dependency, and an Ely.by token only
//! grants what the password would for server joins.
use crate::auth::{AuthSession, Provider, parse_uuid};
use std::{fmt, io::Write, path::Path, time::Duration};

pub use horizon_elyby::{Error, base_url, set_built_in};

/// The kept account, in the folder `horizon-player.txt` lives in.
pub const FILE: &str = "horizon-elyby.json";

/// A signed-in Ely.by account with the tokens that keep it signed in.
#[derive(Clone)]
pub struct Account {
    pub name: String,
    pub uuid: u128,
    access_token: String,
    client_token: String,
}
impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Account")
            .field("name", &self.name)
            .field("uuid", &horizon_protocol::crypto::uuid_string(self.uuid))
            .field("tokens", &"<hidden>")
            .finish()
    }
}

impl Account {
    /// The session the network code joins servers with.
    pub fn session(&self) -> AuthSession {
        let mut session = AuthSession::new(&self.name, self.uuid, &self.access_token);
        session.provider = Provider::ElyBy;
        session
    }

    fn from_grant(grant: horizon_elyby::Grant) -> Result<Self, Error> {
        Ok(Self {
            name: grant.name,
            uuid: parse_uuid(&grant.uuid)
                .ok_or_else(|| Error::Other("Ely.by sent an account without a valid id".into()))?,
            access_token: grant.access_token,
            client_token: grant.client_token,
        })
    }

    pub fn to_json(&self) -> String {
        serde_json::json!({
            "name": self.name,
            "uuid": format!("{:032x}", self.uuid),
            "access_token": self.access_token,
            "client_token": self.client_token,
        })
        .to_string()
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let v: serde_json::Value =
            serde_json::from_str(text).map_err(|_| "not valid JSON".to_owned())?;
        let field = |k: &str| {
            v.get(k)
                .and_then(|s| s.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("no {k}"))
        };
        let name = field("name")?;
        if name.chars().count() > 16 {
            return Err("name is too long".into());
        }
        Ok(Self {
            name: name.to_owned(),
            uuid: parse_uuid(field("uuid")?).ok_or("no valid uuid")?,
            access_token: field("access_token")?.to_owned(),
            client_token: field("client_token")?.to_owned(),
        })
    }

    /// `invalidate`: the token stops working. Best effort: the player is signed out here
    /// whatever the answer.
    pub fn sign_out(&self) {
        client(&base_url()).invalidate(&self.access_token, &self.client_token);
    }
}

fn client(base: &str) -> horizon_elyby::Client {
    horizon_elyby::Client::at(
        base,
        &format!("Horizon/{}", horizon_core::release::VERSION),
        Duration::from_secs(10),
    )
}

/// Sign in with the account's login (username or e-mail) and password. An
/// account with two-factor authentication answers `TwoFactorRequired` until
/// the code comes along.
pub fn authenticate(
    login: &str,
    password: &str,
    two_factor: Option<&str>,
) -> Result<Account, Error> {
    authenticate_at(&base_url(), login, password, two_factor)
}

fn authenticate_at(
    base: &str,
    login: &str,
    password: &str,
    two_factor: Option<&str>,
) -> Result<Account, Error> {
    client(base)
        .authenticate(login, password, two_factor)
        .and_then(Account::from_grant)
}

/// Make a kept account usable: its token checked, renewed when refused. The flag says the
/// token was renewed, so the caller knows to save it.
fn restore(account: &Account) -> Result<(Account, bool), Error> {
    restore_at(account, &base_url())
}

fn restore_at(account: &Account, base: &str) -> Result<(Account, bool), Error> {
    let client = client(base);
    if client.validate(&account.access_token)? {
        return Ok((account.clone(), false));
    }
    client
        .refresh(&account.access_token, &account.client_token)
        .and_then(Account::from_grant)
        .map(|fresh| (fresh, true))
}

/// The account kept in `dir`, if any (a damaged file counts as none).
pub fn load(dir: &Path) -> Option<Account> {
    let text = std::fs::read_to_string(dir.join(FILE)).ok()?;
    match Account::from_json(&text) {
        Ok(account) => Some(account),
        Err(why) => {
            log::warn!("Horizon: ignoring {FILE}: {why}");
            None
        }
    }
}

/// Keep the account in `dir`: written to a new file only its owner can read,
/// then moved over the old one.
pub fn save(dir: &Path, account: &Account) -> std::io::Result<()> {
    let tmp = dir.join(format!("{FILE}.tmp"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(account.to_json().as_bytes())?;
    file.sync_all()?;
    drop(file);
    let dest = dir.join(FILE);
    #[cfg(windows)]
    let _ = std::fs::remove_file(&dest);
    std::fs::rename(&tmp, dest)
}

/// Forget the kept account (the token is also invalidated by `sign_out`).
pub fn forget(dir: &Path) {
    let _ = std::fs::remove_file(dir.join(FILE));
}

/// The kept account, ready for this run. No file: `Ok(None)`. No network
/// keeps the token as it is (joining then says the servers are unreachable);
/// a token that was refused and can't be renewed is an error: sign in again.
pub fn stored_for_start(dir: &Path) -> Result<Option<Account>, Error> {
    let Some(kept) = load(dir) else {
        return Ok(None);
    };
    match restore(&kept) {
        Ok((fresh, renewed)) => {
            if renewed && let Err(e) = save(dir, &fresh) {
                log::warn!("Horizon: could not keep the renewed Ely.by session: {e}");
            }
            Ok(Some(fresh))
        }
        Err(Error::Unavailable) => Ok(Some(kept)),
        Err(e) => Err(e),
    }
}

/// `--elyby-login`: sign in from the terminal and keep the account. The
/// password comes from `HORIZON_ELYBY_PASSWORD` or a prompt (shown as typed:
/// pipe it in if that matters), the two-factor code from `HORIZON_ELYBY_CODE`
/// or a prompt when the account asks for one.
pub fn cli_sign_in(login: &str, dir: &Path) -> Result<Account, String> {
    use std::io::BufRead;
    let ask = |prompt: &str| -> Result<String, String> {
        eprint!("{prompt}");
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        Ok(line.trim_end_matches(['\r', '\n']).to_owned())
    };
    let env = |key: &str| std::env::var(key).ok().filter(|v| !v.is_empty());
    let password = match env("HORIZON_ELYBY_PASSWORD") {
        Some(p) => p,
        None => ask(&format!("Ely.by password for {login}: "))?,
    };
    let mut code = env("HORIZON_ELYBY_CODE");
    let account = loop {
        match authenticate(login, &password, code.as_deref()) {
            Ok(account) => break account,
            Err(Error::TwoFactorRequired) if code.is_none() => {
                code = Some(ask("Two-factor code: ")?);
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    save(dir, &account).map_err(|e| {
        format!(
            "could not keep the Ely.by session in {}: {e}",
            dir.display()
        )
    })?;
    Ok(account)
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: u128 = 0x069a79f444e94726a5befca90e38aaf5;

    fn grant() -> horizon_elyby::Grant {
        horizon_elyby::Grant {
            name: "Notch".into(),
            uuid: format!("{UUID:032x}"),
            access_token: "acc".into(),
            client_token: "cli".into(),
        }
    }

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("horizon-elyby-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn grants_become_accounts() {
        let a = Account::from_grant(grant()).unwrap();
        assert_eq!((a.name.as_str(), a.uuid), ("Notch", UUID));
        let s = a.session();
        assert_eq!(s.provider, Provider::ElyBy);
        assert_eq!(s.uuid, UUID);
        // An id the launcher's client would not send still has to parse.
        let mut bad = grant();
        bad.uuid = "nope".into();
        assert!(Account::from_grant(bad).is_err());
    }

    #[test]
    fn accounts_round_trip_and_hide_their_tokens() {
        let a = Account::from_grant(grant()).unwrap();
        let back = Account::from_json(&a.to_json()).unwrap();
        assert_eq!((back.name.as_str(), back.uuid), ("Notch", UUID));
        assert_eq!(back.access_token, "acc");
        assert_eq!(back.client_token, "cli");
        let shown = format!("{a:?} {:?}", a.session());
        assert!(
            !shown.contains("\"acc\"") && !shown.contains("\"cli\""),
            "{shown}"
        );
        assert!(Account::from_json("{}").is_err());
        assert!(Account::from_json("nope").is_err());
    }

    #[test]
    fn the_authserver_flow_works_against_a_mock() {
        let refreshed = r#"{"accessToken":"new","clientToken":"cli","selectedProfile":{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}}"#;
        let base = horizon_elyby::mock::serve(3, move |method, path, _, body| {
            assert_eq!(method, "POST");
            match path {
                "/auth/authenticate" => {
                    assert!(body.contains("\"username\":\"Notch\""), "{body}");
                    assert!(body.contains("\"password\":\"pw\""), "{body}");
                    (
                        200,
                        r#"{"accessToken":"acc","clientToken":"cli","selectedProfile":{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}}"#
                            .to_owned(),
                    )
                }
                "/auth/validate" => (401, "{}".into()),
                "/auth/refresh" => (200, refreshed.to_owned()),
                other => panic!("unexpected path {other}"),
            }
        });
        let account = authenticate_at(&base, "Notch", "pw", None).unwrap();
        assert_eq!(account.name, "Notch");
        let (fresh, renewed) = restore_at(&account, &base).unwrap();
        assert!(renewed);
        assert_eq!(fresh.access_token, "new");
    }

    #[test]
    fn the_kept_account_is_private_and_replaceable() {
        let d = dir("kept");
        assert!(load(&d).is_none());
        let a = Account::from_grant(grant()).unwrap();
        save(&d, &a).unwrap();
        assert_eq!(load(&d).unwrap().uuid, UUID);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(d.join(FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0, "{mode:o}");
        }
        let mut b = a.clone();
        b.access_token = "newer".into();
        save(&d, &b).unwrap();
        assert_eq!(load(&d).unwrap().access_token, "newer");
        assert!(!d.join(format!("{FILE}.tmp")).exists());
        std::fs::write(d.join(FILE), "garbage").unwrap();
        assert!(load(&d).is_none());
        forget(&d);
        assert!(!d.join(FILE).exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
