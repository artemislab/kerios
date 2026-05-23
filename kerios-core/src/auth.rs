//! Auth configuration for config sources.
//!
//! P1 ships `ssh_key_path` only — a path to an SSH private key already on
//! disk (devops bootstrap puts it there). P2 will add `secret_url` to fetch
//! the key from a blob store at enroll time. P3 will move the on-disk
//! storage behind the OS keychain. P4 will add a GitHub App flow.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Optional `[auth]` block in `~/.kerios/config.toml` (and in
/// bootstrap.toml served to `kerios enroll`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Path to an SSH private key file (e.g. provisioned by a devops
    /// machine-bootstrap script at `/etc/kerios/deploy_key`). The daemon
    /// passes `GIT_SSH_COMMAND='ssh -i <path>'` to git so it uses this
    /// identity for every fetch / pull.
    ///
    /// The file must exist and be readable by the user running the daemon
    /// (typically mode `0600`). The daemon does not chmod it for you.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<PathBuf>,

    /// URL of a remote blob containing the SSH private key. Fetched **once
    /// at enroll time** by `kerios enroll`; the bytes are written to
    /// `~/.kerios/secrets/ssh_key` (mode 0600) and `ssh_key_path` is
    /// rewritten to point there in the saved config. This field is
    /// transient: it never appears in `~/.kerios/config.toml`.
    ///
    /// Supported schemes:
    /// - `https://` — fetched directly with ureq + rustls.
    /// - `gs://` — shells out to `gsutil cp <url> -` (ambient gcloud
    ///   auth: ADC, service account, etc.).
    #[serde(default, skip_serializing)]
    pub secret_url: Option<String>,

    /// Reserved for a future release: when set, the daemon reads the
    /// SSH key from the OS-native secret store (macOS Keychain Services,
    /// Linux Secret Service, Windows Credential Manager) under the given
    /// account name. The value of `ssh_key_path` is then ignored.
    ///
    /// **Not implemented yet.** See `SECURITY.md` → "Roadmap" for the
    /// rationale. Parsing the field today is a no-op; a future PR will
    /// wire it through `GitSource`. The reservation exists so that
    /// today's bootstrap.toml can already declare intent without breaking
    /// when the implementation lands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_in_keychain: Option<String>,

    /// Authenticate to git using a GitHub App installation. Mutually
    /// exclusive with `ssh_key_path` / `secret_url` at runtime — when
    /// both are configured the GitHub App takes precedence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_app: Option<GitHubAppConfig>,
}

/// `[auth.github_app]` — credentials for the GitHub App installation
/// that will mint short-lived (1 h) installation access tokens for
/// every git fetch. Beats deploy keys for fleet ops: one app rotates
/// keys for N machines, no per-machine key provisioning.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitHubAppConfig {
    /// Numeric App ID from `https://github.com/settings/apps/<name>`.
    pub app_id: String,
    /// Installation ID — get it from
    /// `https://github.com/organizations/<org>/settings/installations`
    /// or via the App's installations API.
    pub installation_id: String,

    /// Path to the App's RSA private key (PEM, `BEGIN RSA PRIVATE KEY` or
    /// `BEGIN PRIVATE KEY`). Materialized from `private_key_secret_url`
    /// at enroll time when configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key_path: Option<std::path::PathBuf>,

    /// Transient: URL pointing at the PEM. Fetched once at `kerios enroll`,
    /// written to `~/.kerios/secrets/github-app.pem` mode 0600, and
    /// dropped from the saved config. Supported schemes match
    /// `[auth].secret_url`: `https://`, `gs://`.
    #[serde(default, skip_serializing)]
    pub private_key_secret_url: Option<String>,
}

impl AuthConfig {
    /// Returns true if at least one auth mechanism is configured.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.ssh_key_path.is_none()
            && self.secret_url.is_none()
            && self.ssh_key_in_keychain.is_none()
            && self.github_app.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_auth_is_empty() {
        let a = AuthConfig::default();
        assert!(a.is_empty());
        assert!(a.ssh_key_path.is_none());
    }

    #[test]
    fn parses_ssh_key_path() {
        let raw = r#"ssh_key_path = "/etc/kerios/deploy_key""#;
        let a: AuthConfig = toml::from_str(raw).unwrap();
        assert_eq!(
            a.ssh_key_path.as_deref(),
            Some(std::path::Path::new("/etc/kerios/deploy_key"))
        );
        assert!(!a.is_empty());
    }

    #[test]
    fn parses_github_app_block() {
        let raw = r#"
            [github_app]
            app_id = "123456"
            installation_id = "78901234"
            private_key_path = "/etc/kerios/github-app.pem"
        "#;
        let a: AuthConfig = toml::from_str(raw).unwrap();
        let ga = a.github_app.as_ref().expect("github_app should parse");
        assert_eq!(ga.app_id, "123456");
        assert_eq!(ga.installation_id, "78901234");
        assert_eq!(
            ga.private_key_path.as_deref(),
            Some(std::path::Path::new("/etc/kerios/github-app.pem"))
        );
        assert!(!a.is_empty());
    }

    #[test]
    fn github_app_private_key_secret_url_is_transient() {
        let raw = r#"
            [github_app]
            app_id = "1"
            installation_id = "2"
            private_key_secret_url = "gs://acme/github-app.pem"
        "#;
        let a: AuthConfig = toml::from_str(raw).unwrap();
        let ga = a.github_app.as_ref().unwrap();
        assert_eq!(
            ga.private_key_secret_url.as_deref(),
            Some("gs://acme/github-app.pem")
        );

        // Must not round-trip.
        let serialized = toml::to_string(&a).unwrap();
        assert!(
            !serialized.contains("private_key_secret_url"),
            "secret URL must not survive serialize, got: {serialized}"
        );
    }

    #[test]
    fn parses_secret_url_and_skips_it_on_serialize() {
        let raw = r#"secret_url = "gs://acme-secrets/deploy-key""#;
        let a: AuthConfig = toml::from_str(raw).unwrap();
        assert_eq!(
            a.secret_url.as_deref(),
            Some("gs://acme-secrets/deploy-key")
        );
        assert!(!a.is_empty());

        // secret_url is transient and must NOT round-trip through serialize.
        let serialized = toml::to_string(&a).unwrap();
        assert!(
            !serialized.contains("secret_url"),
            "secret_url must be skipped on serialize, got: {serialized}"
        );
    }
}
