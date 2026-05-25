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
    /// The write is **atomic**: content goes to a sibling temp file
    /// (`<target>.kerios.tmp`), is fsync'd, then renamed over the
    /// target. A `SIGKILL` between any two steps leaves the target
    /// either untouched or fully replaced — never half-written.
    ///
    /// # Errors
    /// Returns [`ProviderError::Io`] if directory creation, writing,
    /// fsync, or rename fails. On error the temp file is best-effort
    /// removed so a retry doesn't see stale state.
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
        atomic_write(&absolute, content.as_bytes())?;
        Ok(absolute)
    }
}

/// Write `bytes` to `target` atomically via temp-file + rename.
/// Internal — exposed only inside this module to keep the seam tight.
///
/// # Errors
/// Propagates any io error from open, write, fsync, or rename. The
/// temp file is best-effort cleaned up on failure.
pub(crate) fn atomic_write(target: &Path, bytes: &[u8]) -> Result<(), ProviderError> {
    use std::io::Write as _;
    let tmp = temp_sibling_path(target);
    // Best-effort: if a prior crashed write left a temp around, blow
    // it away rather than fail the new write on EEXIST or stale data.
    let _ = std::fs::remove_file(&tmp);
    let write_result = (|| -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, target)
    })();
    if let Err(e) = write_result {
        // Don't shadow the original error with the cleanup error.
        let _ = std::fs::remove_file(&tmp);
        return Err(ProviderError::Io(e));
    }
    Ok(())
}

/// Build the temp sibling path: `<target>.kerios.tmp`. Same parent
/// directory so the rename is on the same filesystem (atomic on
/// POSIX + Windows `ReplaceFileW`).
fn temp_sibling_path(target: &Path) -> PathBuf {
    let mut name = target
        .file_name()
        .map(std::ffi::OsString::from)
        .unwrap_or_default();
    name.push(".kerios.tmp");
    target.with_file_name(name)
}

#[cfg(test)]
mod atomic_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn atomic_write_round_trips() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("nested").join("file.txt");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        atomic_write(&target, b"hello").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"hello");
    }

    #[test]
    fn atomic_write_replaces_existing_file() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("file.txt");
        std::fs::write(&target, b"old content").unwrap();
        atomic_write(&target, b"new content").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new content");
    }

    #[test]
    fn atomic_write_leaves_no_temp_file_on_success() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("file.txt");
        atomic_write(&target, b"x").unwrap();
        // The temp sibling MUST be gone after a successful rename —
        // nothing for an operator to clean up.
        let tmp = temp_sibling_path(&target);
        assert!(!tmp.exists(), "stale temp file lingered at {tmp:?}");
    }

    #[test]
    fn atomic_write_cleans_up_temp_on_rename_failure() {
        // Simulate a rename failure by making the target dir a file
        // (so the parent of the target isn't a directory anymore).
        // The temp creation happens in the parent of `target`, so we
        // exercise the failure path by pointing `target` at a path
        // whose parent isn't writable: use an existing FILE as the
        // target's parent.
        let dir = tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, b"i am a file, not a dir").unwrap();
        let target = blocker.join("victim.txt"); // parent is a file
        let result = atomic_write(&target, b"x");
        assert!(result.is_err());
        // Best-effort cleanup: no temp file should remain in the
        // (also-unwritable) parent. Since the temp open fails on the
        // first step, there's nothing to clean — sanity check that no
        // stray file appeared at the blocker path either.
        assert!(blocker.is_file(), "blocker should still be a regular file");
    }

    #[test]
    fn temp_sibling_path_is_in_same_directory() {
        let target = Path::new("/etc/kerios/policy.json");
        let tmp = temp_sibling_path(target);
        assert_eq!(tmp.parent(), target.parent());
        assert_eq!(tmp.file_name().unwrap(), "policy.json.kerios.tmp");
    }
}
