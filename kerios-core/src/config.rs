use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::auth::AuthConfig;

/// Daemon configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// Where config bundles come from. `None` means no source is configured —
    /// the daemon logs and idles instead of crashing, so an unconfigured host
    /// still starts cleanly.
    #[serde(default)]
    pub source: Option<SourceConfig>,

    /// How the daemon syncs (mode, interval).
    #[serde(default)]
    pub sync: SyncConfig,

    /// Which team / user this machine belongs to. Drives which `teams/<name>`
    /// and `users/<name>` layers the daemon reads from the bundle. Either or
    /// both can be unset.
    #[serde(default)]
    pub identity: Identity,

    /// Auth credentials for the source (currently only git SSH key path).
    /// Absent = use whatever ambient git auth the user has set up.
    #[serde(default)]
    pub auth: Option<AuthConfig>,
}

/// Per-machine identity: team and user this machine speaks for.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Identity {
    #[serde(default)]
    pub team: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
}

/// Where the daemon pulls its config bundles from.
///
/// The TOML representation uses an internal `type` tag:
/// ```toml
/// [source]
/// type = "git"
/// repo_url = "git@github.com:acme/kerios-config.git"
/// cache_dir = "/home/alice/.kerios/cache"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SourceConfig {
    Git {
        repo_url: String,
        cache_dir: PathBuf,
    },
}

/// How the daemon polls for updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    #[serde(default)]
    pub mode: SyncMode,
    #[serde(default = "default_interval_secs")]
    pub interval_secs: u64,
    /// What to do when a managed file was hand-edited between syncs.
    #[serde(default)]
    pub drift_policy: DriftPolicy,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            mode: SyncMode::default(),
            interval_secs: default_interval_secs(),
            drift_policy: DriftPolicy::default(),
        }
    }
}

/// Reaction to a hand-edit on a managed file between two syncs.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DriftPolicy {
    /// Overwrite the local edit with the bundle version and log a `warn`.
    /// Drift counts as a soft event that operators can dashboard.
    #[default]
    Warn,
    /// Same as `warn` but logged at `error` so monitoring picks it up.
    Enforce,
    /// Keep the local edit. The bundle version is NOT applied for this key.
    /// Use when you want a "local is sacred" workflow with explicit alerts.
    Preserve,
}

/// How the daemon picks up new bundles. The current build supports
/// `pull` only — the daemon polls on `interval_secs`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SyncMode {
    #[default]
    Pull,
}

const fn default_interval_secs() -> u64 {
    60
}

impl Config {
    /// Read a TOML config file from the given path.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed as TOML.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let raw = std::fs::read_to_string(path).map_err(ConfigError::Io)?;
        toml::from_str(&raw).map_err(ConfigError::Parse)
    }

    /// Load the daemon configuration.
    ///
    /// Returns defaults (no source, `pull` mode, 60 s interval) when `path` is
    /// `None` or does not exist.
    ///
    /// # Errors
    /// Returns an error if the file is present but cannot be read or parsed.
    pub fn load(path: Option<&Path>) -> Result<Self, ConfigError> {
        match path {
            Some(p) if p.exists() => Self::from_file(p),
            _ => Ok(Self::default()),
        }
    }
}

/// Errors produced while loading a [`Config`].
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not read config file: {0}")]
    Io(#[source] std::io::Error),
    #[error("could not parse config file: {0}")]
    Parse(#[source] toml::de::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_no_source_pull_mode_and_60s_interval() {
        let cfg = Config::default();
        assert!(cfg.source.is_none());
        assert_eq!(cfg.sync.mode, SyncMode::Pull);
        assert_eq!(cfg.sync.interval_secs, 60);
    }

    #[test]
    fn default_identity_is_empty() {
        let cfg = Config::default();
        assert!(cfg.identity.team.is_none());
        assert!(cfg.identity.user.is_none());
    }

    #[test]
    fn parses_identity_block() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
                [identity]
                team = "backend"
                user = "alice"
            "#,
        )
        .unwrap();

        let cfg = Config::from_file(&path).unwrap();

        assert_eq!(cfg.identity.team.as_deref(), Some("backend"));
        assert_eq!(cfg.identity.user.as_deref(), Some("alice"));
    }

    #[test]
    fn load_with_no_path_returns_defaults() {
        let cfg = Config::load(None).unwrap();
        assert!(cfg.source.is_none());
        assert_eq!(cfg.sync.interval_secs, 60);
    }

    #[test]
    fn parses_git_source_with_explicit_sync_block() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
                [source]
                type = "git"
                repo_url = "git@github.com:acme/kerios-config.git"
                cache_dir = "/var/lib/kerios/cache"

                [sync]
                mode = "pull"
                interval_secs = 30
            "#,
        )
        .unwrap();

        let cfg = Config::from_file(&path).unwrap();

        let SourceConfig::Git {
            repo_url,
            cache_dir,
        } = cfg.source.expect("source should be set");
        assert_eq!(repo_url, "git@github.com:acme/kerios-config.git");
        assert_eq!(cache_dir, PathBuf::from("/var/lib/kerios/cache"));
        assert_eq!(cfg.sync.mode, SyncMode::Pull);
        assert_eq!(cfg.sync.interval_secs, 30);
    }

    #[test]
    fn default_drift_policy_is_warn() {
        let cfg = SyncConfig::default();
        assert_eq!(cfg.drift_policy, DriftPolicy::Warn);
    }

    #[test]
    fn parses_explicit_drift_policy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
                [source]
                type = "git"
                repo_url = "x"
                cache_dir = "y"

                [sync]
                drift_policy = "preserve"
            "#,
        )
        .unwrap();
        let cfg = Config::from_file(&path).unwrap();
        assert_eq!(cfg.sync.drift_policy, DriftPolicy::Preserve);
    }

    #[test]
    fn sync_block_is_optional_and_falls_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
                [source]
                type = "git"
                repo_url = "git@example.com:cfg.git"
                cache_dir = "/tmp/cache"
            "#,
        )
        .unwrap();

        let cfg = Config::from_file(&path).unwrap();

        assert!(cfg.source.is_some());
        assert_eq!(cfg.sync.mode, SyncMode::Pull);
        assert_eq!(cfg.sync.interval_secs, 60);
    }

    #[test]
    fn unknown_source_type_is_a_parse_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
                [source]
                type = "imaginary"
                some_field = "value"
            "#,
        )
        .unwrap();

        let result = Config::from_file(&path);

        assert!(
            matches!(result, Err(ConfigError::Parse(_))),
            "config parser should reject unknown source types, got: {result:?}"
        );
    }
}
