use std::path::PathBuf;

use super::{Provider, ProviderEnv};

/// Adapter for the Codex CLI.
#[derive(Debug, Default)]
pub struct Codex;

impl Codex {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Provider for Codex {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn detect(&self, env: &ProviderEnv) -> bool {
        env.home.join(".codex").is_dir() || env.binary_in_path("codex")
    }

    fn config_dir(&self, env: &ProviderEnv) -> PathBuf {
        env.home.join(".codex")
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
    fn name_is_codex() {
        assert_eq!(Codex::new().name(), "codex");
    }

    #[test]
    fn detect_returns_true_when_dot_codex_dir_exists() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(home.path().join(".codex")).unwrap();

        assert!(Codex::new().detect(&env_with_home(home.path())));
    }

    #[test]
    fn detect_returns_true_when_codex_binary_in_path() {
        let home = tempfile::tempdir().unwrap();
        let bin_dir = tempfile::tempdir().unwrap();
        std::fs::write(bin_dir.path().join("codex"), b"#!/bin/sh\n").unwrap();

        let env = ProviderEnv {
            home: home.path().to_path_buf(),
            path_dirs: vec![bin_dir.path().to_path_buf()],
        };

        assert!(Codex::new().detect(&env));
    }

    #[test]
    fn detect_returns_false_when_nothing_installed() {
        let home = tempfile::tempdir().unwrap();

        assert!(!Codex::new().detect(&env_with_home(home.path())));
    }

    #[test]
    fn write_config_persists_file_under_dot_codex() {
        let home = tempfile::tempdir().unwrap();
        let env = env_with_home(home.path());

        let written = Codex::new()
            .write_config(&env, Path::new("config.toml"), "model = \"o1\"\n")
            .unwrap();

        assert_eq!(written, home.path().join(".codex/config.toml"));
        assert_eq!(
            std::fs::read_to_string(&written).unwrap(),
            "model = \"o1\"\n"
        );
    }
}
