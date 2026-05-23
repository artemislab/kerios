use std::path::PathBuf;

use super::{Provider, ProviderEnv};

/// Adapter for GitHub Copilot (and the Copilot CLI).
///
/// Copilot has historically been a VS Code / `JetBrains` extension only,
/// but the standalone `gh copilot` CLI and the agent-mode "Copilot Workspace"
/// both read from `~/.config/github-copilot/`. That is the directory we
/// manage. Per-IDE settings (`settings.json` inside an editor's user dir)
/// are NOT touched — bundling editor-private state is the editor's job.
#[derive(Debug, Default)]
pub struct Copilot;

impl Copilot {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Provider for Copilot {
    fn name(&self) -> &'static str {
        "copilot"
    }

    fn detect(&self, env: &ProviderEnv) -> bool {
        env.home.join(".config/github-copilot").is_dir() || env.binary_in_path("copilot")
    }

    fn config_dir(&self, env: &ProviderEnv) -> PathBuf {
        env.home.join(".config/github-copilot")
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
    fn name_is_copilot() {
        assert_eq!(Copilot::new().name(), "copilot");
    }

    #[test]
    fn detect_returns_true_when_dot_config_github_copilot_exists() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".config/github-copilot")).unwrap();

        assert!(Copilot::new().detect(&env_with_home(home.path())));
    }

    #[test]
    fn detect_returns_true_when_copilot_binary_in_path() {
        let home = tempfile::tempdir().unwrap();
        let bin_dir = tempfile::tempdir().unwrap();
        std::fs::write(bin_dir.path().join("copilot"), b"#!/bin/sh\n").unwrap();

        let env = ProviderEnv {
            home: home.path().to_path_buf(),
            path_dirs: vec![bin_dir.path().to_path_buf()],
        };

        assert!(Copilot::new().detect(&env));
    }

    #[test]
    fn detect_returns_false_when_nothing_installed() {
        let home = tempfile::tempdir().unwrap();
        assert!(!Copilot::new().detect(&env_with_home(home.path())));
    }

    #[test]
    fn write_config_persists_file_under_dot_config_github_copilot() {
        let home = tempfile::tempdir().unwrap();
        let env = env_with_home(home.path());

        let written = Copilot::new()
            .write_config(&env, Path::new("hosts.json"), "{}\n")
            .unwrap();

        assert_eq!(
            written,
            home.path().join(".config/github-copilot/hosts.json")
        );
        assert_eq!(std::fs::read_to_string(&written).unwrap(), "{}\n");
    }
}
