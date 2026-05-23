//! Bootstrap: fetch and parse a `bootstrap.toml` from a URL.
//!
//! A devops machine-bootstrap script (or a user running `kerios enroll`)
//! points the agent at a URL that serves a partial `Config`. The agent
//! merges identity (team / user) on top from CLI flags and writes the
//! final `~/.kerios/config.toml`.
//!
//! P1 only fetches HTTPS (and HTTP for tests against localhost). P2 will
//! add `gs://`. The URL scheme is dispatched by [`fetch_bootstrap`].

use std::path::Path;
use std::time::Duration;

use crate::auth::AuthConfig;
use crate::config::{Identity, SourceConfig, SyncConfig};
use serde::{Deserialize, Serialize};

/// The subset of a `Config` a bootstrap can carry. `identity` is provided
/// at enroll time, not in the bootstrap, because it is per-machine.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Bootstrap {
    pub source: Option<SourceConfig>,
    #[serde(default)]
    pub sync: SyncConfig,
    #[serde(default)]
    pub auth: Option<AuthConfig>,
}

/// Fetch a bootstrap from `url`. P1 supports `https://` (and `http://`
/// for localhost in tests). Times out at 10 seconds.
///
/// # Errors
/// Returns [`BootstrapError`] for network, parse, or unsupported-scheme issues.
pub fn fetch_bootstrap(url: &str) -> Result<Bootstrap, BootstrapError> {
    if !is_supported_scheme(url) {
        return Err(BootstrapError::UnsupportedScheme(url.to_string()));
    }
    let body = ureq::get(url)
        .timeout(Duration::from_secs(10))
        .call()
        .map_err(|e| BootstrapError::Http(e.to_string()))?
        .into_string()
        .map_err(|e| BootstrapError::Http(e.to_string()))?;
    parse_bootstrap(&body)
}

/// Parse a bootstrap TOML string without fetching.
///
/// # Errors
/// Returns [`BootstrapError::Parse`] if the body is not a valid TOML
/// matching the [`Bootstrap`] schema.
pub fn parse_bootstrap(toml_str: &str) -> Result<Bootstrap, BootstrapError> {
    toml::from_str(toml_str).map_err(BootstrapError::Parse)
}

/// Validate that any local-path references in `bootstrap` actually exist
/// on disk. Right now this checks `auth.ssh_key_path`.
///
/// # Errors
/// Returns [`BootstrapError::MissingFile`] for each unreachable file.
pub fn validate_local_paths(bootstrap: &Bootstrap) -> Result<(), BootstrapError> {
    if let Some(auth) = &bootstrap.auth {
        if let Some(p) = &auth.ssh_key_path {
            if !Path::new(p).is_file() {
                return Err(BootstrapError::MissingFile(p.display().to_string()));
            }
        }
    }
    Ok(())
}

fn is_supported_scheme(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// Compose a [`crate::config::Config`] from a [`Bootstrap`] and an
/// [`Identity`]. Returned config is ready to write to `~/.kerios/config.toml`.
#[must_use]
pub fn compose_config(bootstrap: Bootstrap, identity: Identity) -> crate::config::Config {
    crate::config::Config {
        source: bootstrap.source,
        sync: bootstrap.sync,
        identity,
        auth: bootstrap.auth,
    }
}

/// Errors produced while fetching or parsing a bootstrap.
#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error("unsupported URL scheme: {0} (P1 supports http:// and https:// only)")]
    UnsupportedScheme(String),
    #[error("http error: {0}")]
    Http(String),
    #[error("parse error: {0}")]
    Parse(#[source] toml::de::Error),
    #[error("referenced file does not exist: {0}")]
    MissingFile(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SyncMode;

    #[test]
    fn parses_minimal_bootstrap() {
        let raw = r#"
            [source]
            type = "git"
            repo_url = "git@github.com:acme/cfg.git"
            cache_dir = "/var/cache/kerios"
        "#;
        let b = parse_bootstrap(raw).unwrap();
        assert!(b.source.is_some());
        assert!(b.auth.is_none());
        assert_eq!(b.sync.mode, SyncMode::Pull);
    }

    #[test]
    fn parses_bootstrap_with_auth_and_sync() {
        let raw = r#"
            [source]
            type = "git"
            repo_url = "git@github.com:acme/cfg.git"
            cache_dir = "/var/cache/kerios"

            [sync]
            interval_secs = 30
            drift_policy = "enforce"

            [auth]
            ssh_key_path = "/etc/kerios/deploy_key"
        "#;
        let b = parse_bootstrap(raw).unwrap();
        assert_eq!(b.sync.interval_secs, 30);
        assert!(b.auth.is_some());
        assert_eq!(
            b.auth.as_ref().unwrap().ssh_key_path.as_deref(),
            Some(std::path::Path::new("/etc/kerios/deploy_key"))
        );
    }

    #[test]
    fn rejects_unsupported_scheme() {
        let result = fetch_bootstrap("gs://acme/bootstrap.toml");
        assert!(matches!(result, Err(BootstrapError::UnsupportedScheme(_))));
    }

    #[test]
    fn validates_missing_ssh_key_path() {
        let b = Bootstrap {
            auth: Some(AuthConfig {
                ssh_key_path: Some("/nope/this/does/not/exist".into()),
                secret_url: None,
                ssh_key_in_keychain: None,
                github_app: None,
            }),
            ..Default::default()
        };
        let result = validate_local_paths(&b);
        assert!(matches!(result, Err(BootstrapError::MissingFile(_))));
    }

    #[test]
    fn compose_config_attaches_identity() {
        let b = parse_bootstrap(
            r#"
            [source]
            type = "git"
            repo_url = "git@github.com:acme/cfg.git"
            cache_dir = "/c"
        "#,
        )
        .unwrap();
        let cfg = compose_config(
            b,
            Identity {
                team: Some("backend".into()),
                user: Some("alice".into()),
            },
        );
        assert_eq!(cfg.identity.team.as_deref(), Some("backend"));
        assert_eq!(cfg.identity.user.as_deref(), Some("alice"));
    }
}
