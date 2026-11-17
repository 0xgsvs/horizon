//! Ely.by's Yggdrasil-compatible authserver, for the launcher's account chooser: the profile it
//! keeps of a sign-in, and the tokens behind it. The client itself is `horizon-elyby`, shared
//! with the game and reached through the platform (`horizon_platform::elyby`); this module maps
//! its answer onto what the launcher stores. Passwords are never written anywhere.
use horizon_platform::elyby::{Client, Grant};
use std::time::Duration;

pub use horizon_platform::elyby::Error;

/// The authserver: `HORIZON_ELYBY_URL` (tests and mocks) else Horizon's own address, which the
/// shared client (`horizon-elyby`) cannot hold itself (it is engine code).
pub fn base_url() -> String {
    horizon_platform::elyby::set_built_in(horizon_addresses::ELYBY_AUTH);
    horizon_platform::elyby::base_url()
}

/// What the launcher keeps of an Ely.by account, never the password.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    /// The profile UUID, lowercase and without dashes.
    pub uuid: String,
    pub name: String,
}

/// `invalidate`: signing out of the launcher stops the token working too. Best effort: the
/// account leaves this computer whatever the authserver answers.
pub fn invalidate(secrets: &Secrets) {
    client(&base_url()).invalidate(&secrets.access_token, &secrets.client_token);
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
    client(base)
        .authenticate(login, password, two_factor)
        .map(parts)
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
    let client = client(base);
    match client.validate(&secrets.access_token) {
        Ok(true) => Ok((profile.clone(), secrets.clone())),
        Ok(false) => client
            .refresh(&secrets.access_token, &secrets.client_token)
            .map(parts),
        Err(Error::Unavailable) => Ok((profile.clone(), secrets.clone())),
        Err(e) => Err(e),
    }
}

fn client(base: &str) -> Client {
    Client::at(
        base,
        &format!("horizon-launcher/{}", crate::backend::VERSION),
        Duration::from_secs(20),
    )
}

fn parts(grant: Grant) -> (Profile, Secrets) {
    (
        Profile {
            uuid: grant.uuid,
            name: grant.name,
        },
        Secrets {
            access_token: grant.access_token,
            client_token: grant.client_token,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "069a79f444e94726a5befca90e38aaf5";
    const GRANT: &str = r#"{"accessToken":"acc","clientToken":"cli","selectedProfile":{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}}"#;

    #[test]
    fn authenticate_sends_the_code_and_reads_the_grant() {
        let base = horizon_elyby::mock::serve(1, |method, path, _, body| {
            assert_eq!((method, path), ("POST", "/auth/authenticate"));
            assert!(body.contains(r#""password":"pw:123456""#), "{body}");
            (200, GRANT.into())
        });
        let (profile, secrets) =
            authenticate_at(&base, "me@example.com", "pw", Some("123456")).unwrap();
        assert_eq!(
            (profile.uuid.as_str(), profile.name.as_str()),
            (UUID, "Notch")
        );
        assert_eq!(secrets.access_token, "acc");
    }

    #[test]
    fn a_refused_token_is_refreshed_and_an_unreachable_one_is_kept() {
        let base = horizon_elyby::mock::serve(2, |method, path, _, _| match (method, path) {
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
        let offline = "http://127.0.0.1:9";
        let (_, kept) = restore_at(offline, &profile, &old).unwrap();
        assert_eq!(kept.access_token, "old");
    }
}
