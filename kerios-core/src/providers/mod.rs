//! Provider adapters for AI coding assistants.

use std::path::{Path, PathBuf};

pub mod claude;
pub mod codex;
pub mod copilot;
pub mod cursor;

/// Ambient environment passed to providers so detection and config writing
/// can be tested without touching the real `$HOME` or `$PATH`.
#[derive(Debug, Clone)]
pub struct ProviderEnv {
    /// User home directory (e.g. `/Users/alice`).
    pub home: PathBuf,
    /// Directories from `$PATH`, in order.
    pub path_dirs: Vec<PathBuf>,
}

impl ProviderEnv {
    /// Returns `true` if any `path_dirs` entry contains a file named `bin`.
    #[must_use]
    pub fn binary_in_path(&self, bin: &str) -> bool {
        self.path_dirs.iter().any(|dir| dir.join(bin).is_file())
    }
}

/// Errors produced by a provider adapter.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("failed to write provider config: {0}")]
    Io(#[source] std::io::Error),
}

/// Adapter contract for an AI coding assistant CLI (Claude Code, Codex, ...).
pub trait Provider {
    /// Short, stable identifier (e.g. `"claude"`).
    fn name(&self) -> &'static str;

    /// Returns `true` if this provider is installed on the host described by `env`.
    fn detect(&self, env: &ProviderEnv) -> bool;

    /// Root directory where this provider expects its config files
    /// (e.g. `~/.claude/`).
    fn config_dir(&self, env: &ProviderEnv) -> PathBuf;

    /// Write `content` to `relative` inside [`Self::config_dir`]. Parent
    /// directories are created as needed. Returns the absolute path written.
    ///
    /// # Errors
    /// Returns [`ProviderError::Io`] if directory creation or writing fails.
    fn write_config(
        &self,
        env: &ProviderEnv,
        relative: &Path,
        content: &str,
    ) -> Result<PathBuf, ProviderError> {
        let absolute = self.config_dir(env).join(relative);
        if let Some(parent) = absolute.parent() {
            std::fs::create_dir_all(parent).map_err(ProviderError::Io)?;
        }
        std::fs::write(&absolute, content).map_err(ProviderError::Io)?;
        Ok(absolute)
    }
}
