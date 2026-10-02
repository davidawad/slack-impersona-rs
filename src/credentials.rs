//! Where a Slack session's token and cookie come from. A [`Credentials`]
//! pair always resolves together, from one source: environment variables
//! whose names the caller chooses ([`EnvNames`]), or a JSON file the
//! caller points at (the shape [`save`] writes).
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::io::Write as _;
use std::path::Path;

/// One Slack session's credential pair: an `xoxc-` token and the `d`
/// session cookie, as a signed-in browser tab holds them.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    pub token: String,
    /// Ready-to-send `Cookie` header value (e.g. `"d=xoxd-..."`).
    pub cookie: String,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials").finish_non_exhaustive()
    }
}

/// Names of the environment variables that hold the token and cookie. The
/// caller picks names meaningful to their own setup; this crate has no
/// built-in names.
#[derive(Clone, Debug)]
pub struct EnvNames {
    pub token: String,
    pub cookie: String,
}

impl EnvNames {
    pub fn new(token: impl Into<String>, cookie: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            cookie: cookie.into(),
        }
    }
}

/// The `Cookie` header: a raw `d` value gets `d=`; a full header passes through.
pub fn cookie_header(raw: &str) -> String {
    let raw = raw.trim();
    if raw.starts_with("d=") {
        raw.to_string()
    } else {
        format!("d={raw}")
    }
}

fn pair(token: Option<String>, cookie: Option<String>) -> Option<Credentials> {
    let token = token.filter(|t| t.starts_with("xox"))?;
    let cookie = cookie.filter(|c| !c.trim().is_empty())?;
    Some(Credentials {
        token,
        cookie: cookie_header(&cookie),
    })
}

/// Pure resolution over already-read inputs: the environment, then a
/// whole credentials-file pair (token and cookie are never mixed across
/// sources).
pub fn resolve(
    names: &EnvNames,
    env: &dyn Fn(&str) -> Option<String>,
    file: Option<&Credentials>,
) -> Option<Credentials> {
    pair(env(&names.token), env(&names.cookie)).or_else(|| file.cloned())
}

/// Reads a credentials file: a JSON object `{"token": "...", "cookie":
/// "..."}`, the shape [`save`] writes.
pub fn load_file(path: &Path) -> Result<Credentials> {
    let text = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

/// Live resolution: the real process environment, then `creds_file` if given.
pub fn load(names: &EnvNames, creds_file: Option<&Path>) -> Result<Credentials> {
    let file = creds_file.map(load_file).transpose()?;
    let env = |k: &str| {
        std::env::var(k)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    resolve(names, &env, file.as_ref()).ok_or_else(|| Error::MissingCredentials {
        token_var: names.token.clone(),
        cookie_var: names.cookie.clone(),
    })
}

/// Atomic, owner-only write: the pair is a whole session identity.
pub fn save(path: &Path, creds: &Credentials) -> Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).ok();
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(serde_json::to_string_pretty(creds)?.as_bytes())?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_header_wraps_a_raw_value() {
        assert_eq!(cookie_header("xoxd-1"), "d=xoxd-1");
        assert_eq!(cookie_header("d=xoxd-1; d-s=1"), "d=xoxd-1; d-s=1");
    }

    #[test]
    fn env_wins_over_file_and_pairs_never_mix() {
        let names = EnvNames::new("TOKEN", "COOKIE");
        let file = Credentials {
            token: "xoxc-file".into(),
            cookie: "d=xoxd-file".into(),
        };
        let none = |_: &str| None;
        assert_eq!(resolve(&names, &none, Some(&file)), Some(file.clone()));
        // Only the token set in env: not a full pair, so the file wins
        // rather than borrowing the file's cookie.
        let half = |k: &str| (k == "TOKEN").then(|| "xoxc-env".to_string());
        assert_eq!(resolve(&names, &half, Some(&file)), Some(file.clone()));
        let full = |k: &str| Some(if k == "TOKEN" { "xoxc-env" } else { "xoxd-env" }.to_string());
        let got = resolve(&names, &full, Some(&file)).unwrap();
        assert_eq!(got.token, "xoxc-env");
        assert_eq!(got.cookie, "d=xoxd-env");
        assert_eq!(resolve(&names, &none, None), None);
    }

    #[test]
    fn debug_never_shows_secrets() {
        let c = Credentials {
            token: "xoxc-secret".into(),
            cookie: "d=xoxd-secret".into(),
        };
        assert!(!format!("{c:?}").contains("secret"));
    }

    #[test]
    fn save_and_load_round_trip_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sub/creds.json");
        let c = Credentials {
            token: "xoxc-1".into(),
            cookie: "d=xoxd-1".into(),
        };
        save(&p, &c).unwrap();
        assert_eq!(load_file(&p).unwrap(), c);
        assert_eq!(
            std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
