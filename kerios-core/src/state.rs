//! On-disk daemon state: last sync time + outcome. Read by `kerios status`,
//! written after every sync by `kerios daemon` and `kerios sync`.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::sync::{FileHashes, SyncReport};

/// Persistent state at `~/.kerios/state.toml`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct State {
    /// When the last successful or failed sync attempt finished.
    #[serde(default)]
    pub last_sync_at: Option<DateTime<Utc>>,

    /// Source label as logged at the time of the last sync (e.g. `git(...)`).
    #[serde(default)]
    pub last_source: Option<String>,

    /// Counts from the last successful sync. `None` if the last attempt failed.
    #[serde(default)]
    pub last_report: Option<SyncReportSummary>,

    /// Error message from the last failed sync. `None` if the last attempt succeeded.
    #[serde(default)]
    pub last_error: Option<String>,

    /// SHA-256 hex of every file the daemon wrote in the last successful
    /// sync, keyed by the bundle key (e.g. `claude/agents/security.md`).
    /// Used to detect hand-edits between syncs.
    #[serde(default)]
    pub last_hashes: FileHashes,
}

/// Compact, serializable summary of [`SyncReport`]. Persisted in
/// `~/.kerios/state.toml` by the daemon after each cycle.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncReportSummary {
    /// Files written this cycle keyed by provider name. Scales with the
    /// number of providers the agent ships (claude, codex, copilot,
    /// cursor today; more later) without a schema change.
    #[serde(default)]
    pub files_written_per_provider: std::collections::BTreeMap<String, usize>,
    #[serde(default)]
    pub unknown_prefix_keys_count: usize,
    #[serde(default)]
    pub drifted_keys_count: usize,
    #[serde(default)]
    pub preserved_keys_count: usize,
}

impl From<&SyncReport> for SyncReportSummary {
    fn from(report: &SyncReport) -> Self {
        Self {
            files_written_per_provider: report.files_written_per_provider.clone(),
            unknown_prefix_keys_count: report.unknown_prefix_keys.len(),
            drifted_keys_count: report.drifted_keys.len(),
            preserved_keys_count: report.preserved_keys.len(),
        }
    }
}

impl State {
    /// Read state from `path`. Returns a default (empty) state if the file
    /// is missing — first-run is not an error.
    ///
    /// # Errors
    /// Returns [`StateError`] when the file exists but cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self, StateError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(path).map_err(StateError::Io)?;
        toml::from_str(&raw).map_err(StateError::Parse)
    }

    /// Write state to `path`. Creates the parent directory if needed.
    ///
    /// # Errors
    /// Returns [`StateError`] on I/O or serialization failure.
    pub fn save(&self, path: &Path) -> Result<(), StateError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(StateError::Io)?;
        }
        let raw = toml::to_string_pretty(self).map_err(StateError::Serialize)?;
        std::fs::write(path, raw).map_err(StateError::Io)?;
        Ok(())
    }
}

/// Errors produced while loading or saving [`State`].
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("io error: {0}")]
    Io(#[source] std::io::Error),
    #[error("parse error: {0}")]
    Parse(#[source] toml::de::Error),
    #[error("serialize error: {0}")]
    Serialize(#[source] toml::ser::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_empty() {
        let s = State::default();
        assert!(s.last_sync_at.is_none());
        assert!(s.last_source.is_none());
        assert!(s.last_report.is_none());
        assert!(s.last_error.is_none());
    }

    #[test]
    fn save_then_load_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.toml");

        let mut written_counts = std::collections::BTreeMap::new();
        written_counts.insert("claude".to_string(), 2);
        written_counts.insert("codex".to_string(), 1);

        let written = State {
            last_sync_at: Some("2026-05-21T10:30:00Z".parse().unwrap()),
            last_source: Some("git(file:///tmp/repo.git)".into()),
            last_report: Some(SyncReportSummary {
                files_written_per_provider: written_counts,
                unknown_prefix_keys_count: 0,
                drifted_keys_count: 0,
                preserved_keys_count: 0,
            }),
            last_error: None,
            last_hashes: FileHashes::default(),
        };
        written.save(&path).unwrap();

        let loaded = State::load(&path).unwrap();
        assert_eq!(loaded.last_source, written.last_source);
        let report = loaded.last_report.as_ref().unwrap();
        assert_eq!(
            report.files_written_per_provider.get("claude").copied(),
            Some(2)
        );
        assert_eq!(
            report.files_written_per_provider.get("codex").copied(),
            Some(1)
        );
        assert_eq!(loaded.last_sync_at, written.last_sync_at);
    }

    #[test]
    fn load_returns_default_when_file_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let s = State::load(&tmp.path().join("does-not-exist.toml")).unwrap();
        assert!(s.last_sync_at.is_none());
    }
}
