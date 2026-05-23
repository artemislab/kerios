use std::path::PathBuf;

use super::{Provider, ProviderEnv};

/// Adapter for the Claude Code CLI.
#[derive(Debug, Default)]
pub struct Claude;

impl Claude {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Provider for Claude {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn detect(&self, env: &ProviderEnv) -> bool {
        env.home.join(".claude").is_dir() || env.binary_in_path("claude")
    }

    fn config_dir(&self, env: &ProviderEnv) -> PathBuf {
        env.home.join(".claude")
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
    fn name_is_claude() {
        assert_eq!(Claude::new().name(), "claude");
    }

    #[test]
    fn detect_returns_true_when_dot_claude_dir_exists() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(home.path().join(".claude")).unwrap();

        assert!(Claude::new().detect(&env_with_home(home.path())));
    }

    #[test]
    fn detect_returns_true_when_claude_binary_in_path() {
        let home = tempfile::tempdir().unwrap();
        let bin_dir = tempfile::tempdir().unwrap();
        std::fs::write(bin_dir.path().join("claude"), b"#!/bin/sh\n").unwrap();

        let env = ProviderEnv {
            home: home.path().to_path_buf(),
            path_dirs: vec![bin_dir.path().to_path_buf()],
        };

        assert!(Claude::new().detect(&env));
    }

    #[test]
    fn detect_returns_false_when_nothing_installed() {
        let home = tempfile::tempdir().unwrap();

        assert!(!Claude::new().detect(&env_with_home(home.path())));
    }

    #[test]
    fn write_config_persists_file_under_dot_claude() {
        let home = tempfile::tempdir().unwrap();
        let env = env_with_home(home.path());

        let written = Claude::new()
            .write_config(&env, Path::new("skills/test.md"), "hello")
            .unwrap();

        assert_eq!(written, home.path().join(".claude/skills/test.md"));
        assert_eq!(std::fs::read_to_string(&written).unwrap(), "hello");
    }
}
