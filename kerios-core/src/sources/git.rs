use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use crate::auth::{AuthConfig, GitHubAppConfig};
use crate::github_app::{
    fetch_installation_token, fetch_installation_token_against, https_url_with_token, now_unix,
    sign_jwt, InstallationToken,
};
use crate::merge::ConfigLayer;
use crate::sources::ConfigSource;

/// Refresh the installation token when fewer than this many seconds
/// remain before expiry. GitHub mints tokens valid for ~1 hour.
const GITHUB_APP_TOKEN_REFRESH_SLACK_SECS: i64 = 300;

/// Set by tests to point GitHub App auth at a local mock server instead
/// of `api.github.com`. Untested in production.
const GITHUB_API_BASE_ENV: &str = "KERIOS_GITHUB_API_BASE";

/// A git-backed config source. Clones a remote repo into `cache_dir` on
/// first use, pulls on subsequent calls.
///
/// `token_cache` survives across clones of this struct because it is an
/// `Arc<Mutex<_>>`. A short-lived (~1h) GitHub App installation token is
/// stored there once minted, so back-to-back ticks reuse it instead of
/// burning rate-limit budget.
#[derive(Debug, Clone)]
pub struct GitSource {
    repo_url: String,
    cache_dir: PathBuf,
    auth: Option<AuthConfig>,
    token_cache: Arc<Mutex<Option<InstallationToken>>>,
}

impl GitSource {
    #[must_use]
    pub fn new(repo_url: String, cache_dir: PathBuf) -> Self {
        Self {
            repo_url,
            cache_dir,
            auth: None,
            token_cache: Arc::new(Mutex::new(None)),
        }
    }

    /// Attach an auth configuration. Honored at fetch time:
    /// - `auth.ssh_key_path` → exported as `GIT_SSH_COMMAND`.
    /// - `auth.github_app`   → mint an installation token, rewrite the
    ///   HTTPS remote URL with it.
    #[must_use]
    pub fn with_auth(mut self, auth: Option<AuthConfig>) -> Self {
        self.auth = auth;
        self
    }

    /// Build the `GIT_SSH_COMMAND` value if an SSH key is configured.
    /// Public for testing — the daemon code path uses [`Self::fetch`].
    #[must_use]
    pub fn git_ssh_command(&self) -> Option<String> {
        let key = self.auth.as_ref()?.ssh_key_path.as_ref()?;
        // `IdentitiesOnly=yes` — do not fall through to the user's
        // `~/.ssh/id_*` if this key fails. `StrictHostKeyChecking=accept-new`
        // — auto-accept first-seen hosts but refuse changed fingerprints.
        Some(format!(
            "ssh -i {} -o IdentitiesOnly=yes -o StrictHostKeyChecking=accept-new",
            key.display()
        ))
    }

    /// Clone the upstream repo into `cache_dir` on first call; pull on later
    /// calls when `cache_dir/.git` already exists.
    ///
    /// # Errors
    /// Returns [`GitError::Io`] if spawning git fails, [`GitError::Subprocess`]
    /// if git exits non-zero, or [`GitError::Auth`] if a configured GitHub
    /// App fails to mint a token.
    pub fn fetch(&self) -> Result<(), GitError> {
        let url = self.resolve_url()?;
        if self.cache_dir.join(".git").is_dir() {
            self.pull(&url)
        } else {
            self.clone(&url)
        }
    }

    fn clone(&self, url: &str) -> Result<(), GitError> {
        let parent = self
            .cache_dir
            .parent()
            .ok_or_else(|| GitError::InvalidCacheDir(self.cache_dir.clone()))?;
        std::fs::create_dir_all(parent).map_err(GitError::Io)?;

        run_git(
            "git clone",
            &[
                "clone".into(),
                "-q".into(),
                url.into(),
                self.cache_dir.as_os_str().into(),
            ],
            None,
            self.git_ssh_command().as_deref(),
        )
    }

    fn pull(&self, url: &str) -> Result<(), GitError> {
        // Rewriting the remote URL each pull lets the installation token
        // rotate without invalidating the clone. Cheap; only touches
        // `.git/config`.
        if self.uses_github_app() {
            run_git(
                "git remote set-url",
                &[
                    "remote".into(),
                    "set-url".into(),
                    "origin".into(),
                    url.into(),
                ],
                Some(&self.cache_dir),
                None,
            )?;
        }
        run_git(
            "git pull",
            &["pull".into(), "-q".into()],
            Some(&self.cache_dir),
            self.git_ssh_command().as_deref(),
        )
    }

    fn uses_github_app(&self) -> bool {
        self.auth
            .as_ref()
            .and_then(|a| a.github_app.as_ref())
            .is_some()
    }

    /// Returns the URL to hand to `git`, rewritten with a GitHub App
    /// installation token when configured.
    fn resolve_url(&self) -> Result<String, GitError> {
        let Some(gh_app) = self.auth.as_ref().and_then(|a| a.github_app.as_ref()) else {
            return Ok(self.repo_url.clone());
        };
        let token = self.ensure_github_token(gh_app)?;
        Ok(https_url_with_token(&self.repo_url, &token))
    }

    /// Return a fresh installation token, minting one when the cache is
    /// empty or near expiry. Holds the mutex only for the duration of
    /// the cache read/write; the network call happens outside the lock.
    fn ensure_github_token(&self, gh_app: &GitHubAppConfig) -> Result<String, GitError> {
        let now = now_unix();
        // Fast path: cached, not near expiry.
        if let Some(token) = self.token_cache.lock().expect("token cache mutex").as_ref() {
            if !token.is_near_expiry(now, GITHUB_APP_TOKEN_REFRESH_SLACK_SECS) {
                return Ok(token.token.clone());
            }
        }

        let pem_path = gh_app.private_key_path.as_ref().ok_or_else(|| {
            GitError::Auth(
                "[auth.github_app] missing private_key_path — \
                 run `kerios enroll` to materialize private_key_secret_url"
                    .into(),
            )
        })?;
        let pem = std::fs::read(pem_path).map_err(GitError::Io)?;
        let jwt = sign_jwt(&gh_app.app_id, &pem, now).map_err(|e| GitError::Auth(e.to_string()))?;
        let token = match std::env::var(GITHUB_API_BASE_ENV) {
            Ok(base) => fetch_installation_token_against(&base, &jwt, &gh_app.installation_id),
            Err(_) => fetch_installation_token(&jwt, &gh_app.installation_id),
        }
        .map_err(|e| GitError::Auth(e.to_string()))?;

        let token_value = token.token.clone();
        *self.token_cache.lock().expect("token cache mutex") = Some(token);
        Ok(token_value)
    }

    /// Read every file under `cache_dir/org/` into a [`ConfigLayer`] keyed
    /// by path relative to `org/`. Returns an empty layer if `org/` is absent.
    ///
    /// # Errors
    /// Returns [`GitError::Io`] if a file under `org/` cannot be read.
    pub fn read_org_layer(&self) -> Result<ConfigLayer, GitError> {
        read_layer(&self.cache_dir.join("org"))
    }

    /// Read every file under `cache_dir/teams/<name>/` into a [`ConfigLayer`]
    /// keyed by path relative to that team directory. Returns an empty layer
    /// if the team directory does not exist.
    ///
    /// # Errors
    /// Returns [`GitError::Io`] if a file under the team directory cannot be read.
    pub fn read_team_layer(&self, team: &str) -> Result<ConfigLayer, GitError> {
        read_layer(&self.cache_dir.join("teams").join(team))
    }

    /// Read every file under `cache_dir/users/<name>/` into a [`ConfigLayer`]
    /// keyed by path relative to that user directory. Returns an empty layer
    /// if the user directory does not exist.
    ///
    /// # Errors
    /// Returns [`GitError::Io`] if a file under the user directory cannot be read.
    pub fn read_user_layer(&self, user: &str) -> Result<ConfigLayer, GitError> {
        read_layer(&self.cache_dir.join("users").join(user))
    }
}

fn read_layer(root: &Path) -> Result<ConfigLayer, GitError> {
    let mut layer = ConfigLayer::new();
    if !root.is_dir() {
        return Ok(layer);
    }
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry.map_err(|e| GitError::Io(e.into()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        // walkdir guarantees this in practice; we propagate as Io rather
        // than panic so a daemon never crashes on a hypothetical edge case
        // (symlink resolution race, path canonicalization quirk).
        let rel = entry
            .path()
            .strip_prefix(root)
            .map_err(|e| GitError::Io(std::io::Error::other(e.to_string())))?;
        let content = std::fs::read_to_string(entry.path()).map_err(GitError::Io)?;
        layer.insert(rel.to_string_lossy().into_owned(), content);
    }
    Ok(layer)
}

fn run_git(
    label: &'static str,
    args: &[std::ffi::OsString],
    cwd: Option<&std::path::Path>,
    git_ssh_command: Option<&str>,
) -> Result<(), GitError> {
    let mut cmd = Command::new("git");
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    if let Some(ssh) = git_ssh_command {
        cmd.env("GIT_SSH_COMMAND", ssh);
    }
    let status = cmd.args(args).status().map_err(GitError::Io)?;
    if !status.success() {
        return Err(GitError::Subprocess {
            command: label,
            code: status.code(),
        });
    }
    Ok(())
}

impl ConfigSource for GitSource {
    type Error = GitError;

    fn fetch(&self) -> Result<(), Self::Error> {
        GitSource::fetch(self)
    }

    fn read_org_layer(&self) -> Result<ConfigLayer, Self::Error> {
        GitSource::read_org_layer(self)
    }

    fn read_team_layer(&self, team: &str) -> Result<ConfigLayer, Self::Error> {
        GitSource::read_team_layer(self, team)
    }

    fn read_user_layer(&self, user: &str) -> Result<ConfigLayer, Self::Error> {
        GitSource::read_user_layer(self, user)
    }
}

/// Errors produced by [`GitSource`].
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("io error: {0}")]
    Io(#[source] std::io::Error),
    #[error("`{command}` exited with status {code:?}")]
    Subprocess {
        command: &'static str,
        code: Option<i32>,
    },
    #[error("invalid cache dir: {0:?}")]
    InvalidCacheDir(PathBuf),
    #[error("auth: {0}")]
    Auth(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// Create an ephemeral bare upstream repo with one initial commit so
    /// the `GitSource` under test has something real to clone.
    fn make_upstream(dir: &std::path::Path) {
        let work = dir.join("work");
        std::fs::create_dir(&work).unwrap();
        run(&work, &["init", "-q", "-b", "main"]);
        std::fs::write(work.join("README.md"), "hello").unwrap();
        run(&work, &["add", "README.md"]);
        run(
            &work,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "init",
            ],
        );

        let bare = dir.join("upstream.git");
        run(
            dir,
            &[
                "clone",
                "--bare",
                "-q",
                work.to_str().unwrap(),
                bare.to_str().unwrap(),
            ],
        );
    }

    fn run(cwd: &std::path::Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    #[test]
    fn read_team_layer_returns_files_under_teams_name_dir() {
        let tmp = tempfile::tempdir().unwrap();
        make_upstream_with_layout(
            tmp.path(),
            &[
                ("org/agents/security.md", "org-sec"),
                ("teams/backend/agents/api.md", "team-api"),
                ("teams/frontend/agents/ui.md", "other-team"),
            ],
        );
        let cache = tmp.path().join("cache");
        let source = GitSource::new(
            format!("file://{}", tmp.path().join("upstream.git").display()),
            cache,
        );
        source.fetch().unwrap();

        let team = source.read_team_layer("backend").unwrap();

        assert_eq!(
            team.len(),
            1,
            "only files under teams/backend/ should appear"
        );
        assert_eq!(team["agents/api.md"], "team-api");
    }

    #[test]
    fn read_user_layer_returns_files_under_users_name_dir() {
        let tmp = tempfile::tempdir().unwrap();
        make_upstream_with_layout(
            tmp.path(),
            &[
                ("org/agents/security.md", "org-sec"),
                ("users/alice/agents/me.md", "alice-me"),
                ("users/bob/agents/me.md", "bob-me"),
            ],
        );
        let cache = tmp.path().join("cache");
        let source = GitSource::new(
            format!("file://{}", tmp.path().join("upstream.git").display()),
            cache,
        );
        source.fetch().unwrap();

        let alice = source.read_user_layer("alice").unwrap();

        assert_eq!(
            alice.len(),
            1,
            "only files under users/alice/ should appear"
        );
        assert_eq!(alice["agents/me.md"], "alice-me");
    }

    #[test]
    fn read_team_layer_returns_empty_when_team_dir_missing() {
        let tmp = tempfile::tempdir().unwrap();
        make_upstream_with_layout(tmp.path(), &[("org/agents/security.md", "org-sec")]);
        let cache = tmp.path().join("cache");
        let source = GitSource::new(
            format!("file://{}", tmp.path().join("upstream.git").display()),
            cache,
        );
        source.fetch().unwrap();

        let team = source.read_team_layer("missing-team").unwrap();

        assert!(team.is_empty());
    }

    #[test]
    fn read_org_layer_returns_files_under_org_dir() {
        let tmp = tempfile::tempdir().unwrap();
        make_upstream_with_layout(
            tmp.path(),
            &[
                ("org/agents/security.md", "org-sec"),
                ("org/commands/review.md", "org-review"),
                ("teams/backend/agents/api.md", "team-api"),
                ("README.md", "noise"),
            ],
        );
        let cache = tmp.path().join("cache");
        let source = GitSource::new(
            format!("file://{}", tmp.path().join("upstream.git").display()),
            cache,
        );
        source.fetch().unwrap();

        let org = source.read_org_layer().unwrap();

        assert_eq!(org.len(), 2, "only org/* files should appear");
        assert_eq!(org["agents/security.md"], "org-sec");
        assert_eq!(org["commands/review.md"], "org-review");
    }

    /// Same as `make_upstream` but seeds the work tree with a structured
    /// directory layout before the initial commit.
    fn make_upstream_with_layout(dir: &std::path::Path, files: &[(&str, &str)]) {
        let work = dir.join("work");
        std::fs::create_dir(&work).unwrap();
        run(&work, &["init", "-q", "-b", "main"]);
        for (rel, content) in files {
            let path = work.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
        run(&work, &["add", "."]);
        run(
            &work,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "init",
            ],
        );

        let bare = dir.join("upstream.git");
        run(
            dir,
            &[
                "clone",
                "--bare",
                "-q",
                work.to_str().unwrap(),
                bare.to_str().unwrap(),
            ],
        );
    }

    #[test]
    fn git_ssh_command_includes_key_path_when_auth_configured() {
        let src = GitSource::new("file:///dev/null".into(), "/tmp/c".into()).with_auth(Some(
            AuthConfig {
                ssh_key_path: Some(PathBuf::from("/etc/kerios/deploy_key")),
                secret_url: None,
                ssh_key_in_keychain: None,
                github_app: None,
            },
        ));
        let cmd = src.git_ssh_command().expect("expected GIT_SSH_COMMAND");
        assert!(cmd.starts_with("ssh -i /etc/kerios/deploy_key"));
        assert!(cmd.contains("IdentitiesOnly=yes"));
    }

    #[test]
    fn resolve_url_passes_through_without_github_app() {
        let src = GitSource::new("https://github.com/acme/repo.git".into(), "/tmp/c".into());
        assert_eq!(
            src.resolve_url().unwrap(),
            "https://github.com/acme/repo.git"
        );
    }

    #[test]
    fn resolve_url_mints_token_and_caches_it() {
        // Point the GitHub API at a local mock that returns a canned token.
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        let calls = Arc::new(Mutex::new(0_u32));
        let calls_t = calls.clone();
        thread::spawn(move || {
            ready_tx.send(()).unwrap();
            // Accept two requests; cache must skip the second.
            for _ in 0..2 {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                        break;
                    }
                }
                *calls_t.lock().unwrap() += 1;
                let body = r#"{"token":"ghs_T0K3N","expires_at":"2999-01-01T00:00:00Z"}"#;
                let response = format!(
                    "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        ready_rx.recv().unwrap();

        let pem = include_bytes!("../../tests/fixtures/test_rsa_2048.pem");
        let tmp = tempfile::tempdir().unwrap();
        let pem_path = tmp.path().join("app.pem");
        std::fs::write(&pem_path, pem).unwrap();

        let src = GitSource::new(
            "https://github.com/acme/repo.git".into(),
            tmp.path().join("cache"),
        )
        .with_auth(Some(AuthConfig {
            ssh_key_path: None,
            secret_url: None,
            ssh_key_in_keychain: None,
            github_app: Some(GitHubAppConfig {
                app_id: "123".into(),
                installation_id: "456".into(),
                private_key_path: Some(pem_path),
                private_key_secret_url: None,
            }),
        }));

        std::env::set_var(GITHUB_API_BASE_ENV, format!("http://127.0.0.1:{port}"));
        let url1 = src.resolve_url().unwrap();
        let url2 = src.resolve_url().unwrap();
        std::env::remove_var(GITHUB_API_BASE_ENV);

        assert_eq!(
            url1,
            "https://x-access-token:ghs_T0K3N@github.com/acme/repo.git"
        );
        assert_eq!(url1, url2, "second call must reuse the cached token");
        assert_eq!(
            *calls.lock().unwrap(),
            1,
            "cached token should not trigger a second HTTP call"
        );
    }

    #[test]
    fn git_ssh_command_is_none_without_auth() {
        let src = GitSource::new("file:///dev/null".into(), "/tmp/c".into());
        assert!(src.git_ssh_command().is_none());
    }

    #[test]
    fn fetch_pulls_new_commits_when_cache_exists() {
        let tmp = tempfile::tempdir().unwrap();
        make_upstream(tmp.path());
        let cache = tmp.path().join("cache");
        let upstream = tmp.path().join("upstream.git");

        let source = GitSource::new(format!("file://{}", upstream.display()), cache.clone());
        source.fetch().unwrap(); // first call: clone

        // Add a new commit upstream by pushing from the original work dir.
        let work = tmp.path().join("work");
        std::fs::write(work.join("UPDATE.md"), "v2").unwrap();
        run(&work, &["add", "UPDATE.md"]);
        run(
            &work,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "v2",
            ],
        );
        run(&work, &["push", "-q", upstream.to_str().unwrap(), "main"]);

        source.fetch().unwrap(); // second call: pull

        assert_eq!(
            std::fs::read_to_string(cache.join("UPDATE.md")).unwrap(),
            "v2",
            "second fetch should pull new commits into the existing clone"
        );
    }

    #[test]
    fn fetch_clones_repo_when_cache_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        make_upstream(tmp.path());
        let cache = tmp.path().join("cache");

        let source = GitSource::new(
            format!("file://{}", tmp.path().join("upstream.git").display()),
            cache.clone(),
        );

        source.fetch().unwrap();

        assert!(
            cache.join(".git").is_dir(),
            "expected .git directory after clone"
        );
        assert_eq!(
            std::fs::read_to_string(cache.join("README.md")).unwrap(),
            "hello"
        );
    }
}
