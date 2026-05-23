use std::path::PathBuf;

use super::{Provider, ProviderEnv};

/// Adapter for [Cursor](https://cursor.sh), the VS Code fork.
///
/// Cursor reads its project-level rules from `.cursorrules` per-repo, but
/// also honors a global config under `~/.cursor/` for user-wide agent
/// definitions and snippets — which is what a fleet-wide config sync
/// targets. The editor itself's settings live under
/// `~/Library/Application Support/Cursor/` (macOS) or
/// `~/.config/Cursor/` (Linux), which we do NOT manage; the editor
/// owns those.
#[derive(Debug, Default)]
pub struct Cursor;

impl Cursor {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Provider for Cursor {
    fn name(&self) -> &'static str {
        "cursor"
    }

    fn detect(&self, env: &ProviderEnv) -> bool {
        env.home.join(".cursor").is_dir() || env.binary_in_path("cursor")
    }

    fn config_dir(&self, env: &ProviderEnv) -> PathBuf {
        env.home.join(".cursor")
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn env_with_home(home: &Path) -> ProviderEnv {
        ProviderEnv {
            home: home.to_path_buf(),
            path_dirs: vec![],
        }
    }

    #[test]
    fn name_is_cursor() {
        assert_eq!(Cursor::new().name(), "cursor");
    }

    #[test]
    fn detect_returns_true_when_dot_cursor_dir_exists() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(home.path().join(".cursor")).unwrap();

        assert!(Cursor::new().detect(&env_with_home(home.path())));
    }

    #[test]
    fn detect_returns_true_when_cursor_binary_in_path() {
        let home = tempfile::tempdir().unwrap();
        let bin_dir = tempfile::tempdir().unwrap();
        std::fs::write(bin_dir.path().join("cursor"), b"#!/bin/sh\n").unwrap();

        let env = ProviderEnv {
            home: home.path().to_path_buf(),
            path_dirs: vec![bin_dir.path().to_path_buf()],
        };

        assert!(Cursor::new().detect(&env));
    }

    #[test]
    fn detect_returns_false_when_nothing_installed() {
        let home = tempfile::tempdir().unwrap();
        assert!(!Cursor::new().detect(&env_with_home(home.path())));
    }

    #[test]
    fn write_config_persists_file_under_dot_cursor() {
        let home = tempfile::tempdir().unwrap();
        let env = env_with_home(home.path());

        let written = Cursor::new()
            .write_config(&env, Path::new("rules/general.md"), "be helpful\n")
            .unwrap();

        assert_eq!(written, home.path().join(".cursor/rules/general.md"));
        assert_eq!(std::fs::read_to_string(&written).unwrap(), "be helpful\n");
    }
}
