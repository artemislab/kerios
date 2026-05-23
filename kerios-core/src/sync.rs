//! End-to-end sync orchestration: pull the bundle, merge the layers,
//! dispatch each entry to the provider whose prefix it carries.

use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::config::{DriftPolicy, Identity};
use crate::merge::{merge, ConfigLayer};
use crate::providers::{Provider, ProviderEnv, ProviderError};
use crate::sources::ConfigSource;

/// Map from bundle key (e.g. `claude/agents/foo.md`) to the SHA-256 hex of
/// the content the daemon last wrote there. Persisted across sync ticks so
/// drift detection can compare current disk state to the last applied one.
pub type FileHashes = BTreeMap<String, String>;

// Bundle entries are routed to a provider by their leading path segment.
// The segment must match a configured provider's `Provider::name()` (e.g.
// `claude/`, `codex/`, `copilot/`, `cursor/`). Anything else is reported
// and skipped.

/// What `sync_once` actually did, for logging and tests.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Files written this cycle, keyed by provider name. Order is stable
    /// (via `BTreeMap`) so logs and tests can compare.
    pub files_written_per_provider: BTreeMap<String, usize>,
    /// Keys whose leading segment did not match any configured provider.
    pub unknown_prefix_keys: Vec<String>,
    /// Keys whose on-disk content was modified out-of-band between syncs
    /// AND the bundle version was applied anyway (policy = `warn` or
    /// `enforce`). The on-disk content now matches the bundle.
    pub drifted_keys: Vec<String>,
    /// Keys whose on-disk content was modified out-of-band AND kept as-is
    /// (policy = `preserve`). The bundle version was NOT applied for
    /// these keys, so the on-disk content still differs from upstream.
    pub preserved_keys: Vec<String>,
    /// SHA-256 hex of each key the daemon wrote (or preserved) in this
    /// cycle, keyed by the bundle key. Persist this to compare against
    /// on the next tick.
    pub new_hashes: FileHashes,
}

impl SyncReport {
    /// Sum across all providers — handy for "did anything happen" logs.
    #[must_use]
    pub fn total_files_written(&self) -> usize {
        self.files_written_per_provider.values().sum()
    }
}

#[must_use]
pub fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// One sync tick: fetch from `source`, read the three layers selected by
/// `identity`, merge them, then write each entry to the provider matching
/// its prefix (only if the provider is detected on the host).
///
/// `providers` is a slice of trait objects so callers can add new
/// providers without changing this signature.
///
/// # Errors
/// Returns a [`SyncError`] when the source fetch/read fails or when a
/// provider write fails. Unknown-prefix entries are not errors — they
/// show up in [`SyncReport::unknown_prefix_keys`].
pub fn sync_once<S: ConfigSource>(
    source: &S,
    identity: &Identity,
    env: &ProviderEnv,
    providers: &[&dyn Provider],
    previous_hashes: &FileHashes,
    drift_policy: DriftPolicy,
) -> Result<SyncReport, SyncError> {
    source.fetch().map_err(|e| SyncError::Source(Box::new(e)))?;

    let org = source
        .read_org_layer()
        .map_err(|e| SyncError::Source(Box::new(e)))?;
    let team = match &identity.team {
        Some(t) => source
            .read_team_layer(t)
            .map_err(|e| SyncError::Source(Box::new(e)))?,
        None => ConfigLayer::new(),
    };
    let user = match &identity.user {
        Some(u) => source
            .read_user_layer(u)
            .map_err(|e| SyncError::Source(Box::new(e)))?,
        None => ConfigLayer::new(),
    };

    let merged = merge(&org, &team, &user);

    // Detect each provider once before the loop — `detect` may stat the
    // filesystem, so cache the result.
    let detected: Vec<(&dyn Provider, bool, String)> = providers
        .iter()
        .map(|p| (*p, p.detect(env), format!("{}/", p.name())))
        .collect();

    let mut report = SyncReport::default();
    for (key, content) in &merged {
        let Some(route) = Route::for_key(key, &detected, env) else {
            report.unknown_prefix_keys.push(key.clone());
            continue;
        };

        // Drift check: hash the on-disk content (if any) and compare
        // against the hash we last wrote for this key.
        let drifted = previous_hashes.get(key).is_some_and(|prev| {
            std::fs::read_to_string(route.config_dir.join(route.relative_path))
                .is_ok_and(|disk| sha256_hex(&disk) != *prev)
        });

        // Preserve policy short-circuits: keep on-disk content, do NOT
        // apply the bundle. Still record the previous hash so the next
        // tick does not flag the same file again.
        if drifted && drift_policy == DriftPolicy::Preserve {
            report.preserved_keys.push(key.clone());
            if let Some(prev_hash) = previous_hashes.get(key) {
                report.new_hashes.insert(key.clone(), prev_hash.clone());
            }
            continue;
        }

        // Apply via the matching provider.
        route
            .provider
            .write_config(env, Path::new(route.relative_path), content)?;
        *report
            .files_written_per_provider
            .entry(route.provider.name().to_string())
            .or_insert(0) += 1;

        if drifted {
            report.drifted_keys.push(key.clone());
        }
        report.new_hashes.insert(key.clone(), sha256_hex(content));
    }
    Ok(report)
}

/// One bundle entry's destination, decided once before any work.
struct Route<'a> {
    provider: &'a dyn Provider,
    relative_path: &'a str,
    config_dir: std::path::PathBuf,
}

impl<'a> Route<'a> {
    fn for_key(
        key: &'a str,
        detected: &'a [(&'a dyn Provider, bool, String)],
        env: &ProviderEnv,
    ) -> Option<Self> {
        for (provider, is_detected, prefix) in detected {
            if !is_detected {
                continue;
            }
            if let Some(rel) = key.strip_prefix(prefix.as_str()) {
                return Some(Self {
                    provider: *provider,
                    relative_path: rel,
                    config_dir: provider.config_dir(env),
                });
            }
        }
        None
    }
}

/// Errors produced by [`sync_once`].
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// The underlying [`ConfigSource`] failed (network, parse, missing
    /// auth). The original error chain is preserved via `Box<dyn Error>`
    /// so `anyhow`-formatted output in the daemon shows the full cause.
    #[error("config source")]
    Source(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("provider")]
    Provider(#[from] ProviderError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Identity;
    use crate::providers::claude::Claude;
    use crate::providers::codex::Codex;
    use crate::providers::copilot::Copilot;
    use crate::providers::cursor::Cursor;
    use crate::providers::ProviderEnv;
    use crate::sources::git::GitSource;
    use std::process::Command;

    fn make_upstream(dir: &std::path::Path, files: &[(&str, &str)]) -> String {
        let work = dir.join("work");
        std::fs::create_dir(&work).unwrap();
        run(&work, &["init", "-q", "-b", "main"]);
        for (rel, content) in files {
            let p = work.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, content).unwrap();
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
                "seed",
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
        format!("file://{}", bare.display())
    }

    fn run(cwd: &std::path::Path, args: &[&str]) {
        let st = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .status()
            .unwrap();
        assert!(st.success(), "git {args:?} failed");
    }

    /// Stand up: a real git source, a tempdir HOME with .claude/ pre-existing
    /// so the Claude provider's `detect()` returns true, and an empty Codex.
    fn setup(
        bundle_files: &[(&str, &str)],
    ) -> (tempfile::TempDir, GitSource, ProviderEnv, Identity) {
        let tmp = tempfile::tempdir().unwrap();
        let url = make_upstream(tmp.path(), bundle_files);
        let cache = tmp.path().join("cache");

        let home = tmp.path().join("home");
        std::fs::create_dir(&home).unwrap();
        std::fs::create_dir(home.join(".claude")).unwrap(); // makes Claude::detect true

        let source = GitSource::new(url, cache);
        let env = ProviderEnv {
            home: home.clone(),
            path_dirs: vec![],
        };

        (tmp, source, env, Identity::default())
    }

    /// All four OSS providers, ready for `sync_once`. Tests opt into
    /// detection by creating the matching `~/.foo/` directories.
    fn all_providers() -> (Claude, Codex, Copilot, Cursor) {
        (Claude::new(), Codex::new(), Copilot::new(), Cursor::new())
    }

    fn provider_slice<'a>(
        claude: &'a Claude,
        codex: &'a Codex,
        copilot: &'a Copilot,
        cursor: &'a Cursor,
    ) -> [&'a dyn Provider; 4] {
        [claude, codex, copilot, cursor]
    }

    #[test]
    fn preserve_policy_keeps_local_edits_and_does_not_overwrite() {
        let (tmp, source, env, identity) =
            setup(&[("org/claude/agents/sec.md", "v1 — from upstream")]);
        let (c, x, p, u) = all_providers();
        let providers = provider_slice(&c, &x, &p, &u);

        let first = sync_once(
            &source,
            &identity,
            &env,
            &providers,
            &FileHashes::default(),
            DriftPolicy::Preserve,
        )
        .unwrap();

        // Hand-edit between syncs
        let path = tmp.path().join("home/.claude/agents/sec.md");
        std::fs::write(&path, "tampered locally").unwrap();

        let second = sync_once(
            &source,
            &identity,
            &env,
            &providers,
            &first.new_hashes,
            DriftPolicy::Preserve,
        )
        .unwrap();

        assert_eq!(
            second.preserved_keys,
            vec!["claude/agents/sec.md".to_string()]
        );
        assert!(second.drifted_keys.is_empty());
        assert_eq!(second.total_files_written(), 0);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "tampered locally");
    }

    #[test]
    fn enforce_policy_overwrites_just_like_warn() {
        let (tmp, source, env, identity) =
            setup(&[("org/claude/agents/sec.md", "upstream-version")]);
        let (c, x, p, u) = all_providers();
        let providers = provider_slice(&c, &x, &p, &u);

        let first = sync_once(
            &source,
            &identity,
            &env,
            &providers,
            &FileHashes::default(),
            DriftPolicy::Enforce,
        )
        .unwrap();
        std::fs::write(tmp.path().join("home/.claude/agents/sec.md"), "local-hack").unwrap();

        let second = sync_once(
            &source,
            &identity,
            &env,
            &providers,
            &first.new_hashes,
            DriftPolicy::Enforce,
        )
        .unwrap();

        assert_eq!(
            second.drifted_keys,
            vec!["claude/agents/sec.md".to_string()]
        );
        assert!(second.preserved_keys.is_empty());
        assert_eq!(
            second.files_written_per_provider.get("claude").copied(),
            Some(1)
        );
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("home/.claude/agents/sec.md")).unwrap(),
            "upstream-version"
        );
    }

    #[test]
    fn drift_is_detected_when_disk_content_changed_since_last_sync() {
        let (tmp, source, env, identity) =
            setup(&[("org/claude/agents/sec.md", "v1 — from upstream")]);
        let (c, x, p, u) = all_providers();
        let providers = provider_slice(&c, &x, &p, &u);

        let first = sync_once(
            &source,
            &identity,
            &env,
            &providers,
            &FileHashes::default(),
            DriftPolicy::Warn,
        )
        .unwrap();
        assert!(first.drifted_keys.is_empty());
        assert_eq!(first.new_hashes.len(), 1);

        let path = tmp.path().join("home/.claude/agents/sec.md");
        std::fs::write(&path, "tampered locally").unwrap();

        let second = sync_once(
            &source,
            &identity,
            &env,
            &providers,
            &first.new_hashes,
            DriftPolicy::Warn,
        )
        .unwrap();
        assert_eq!(
            second.drifted_keys,
            vec!["claude/agents/sec.md".to_string()]
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "v1 — from upstream"
        );
    }

    #[test]
    fn user_layer_overrides_team_overrides_org_when_identity_is_set() {
        let (tmp, source, env, _) = setup(&[
            ("org/claude/agents/sec.md", "org-version"),
            ("teams/backend/claude/agents/sec.md", "team-version"),
            ("users/alice/claude/agents/sec.md", "alice-version"),
        ]);
        let identity = Identity {
            team: Some("backend".to_string()),
            user: Some("alice".to_string()),
        };
        let (c, x, p, u) = all_providers();
        let providers = provider_slice(&c, &x, &p, &u);

        sync_once(
            &source,
            &identity,
            &env,
            &providers,
            &FileHashes::default(),
            DriftPolicy::Warn,
        )
        .unwrap();

        let written = tmp.path().join("home/.claude/agents/sec.md");
        assert_eq!(std::fs::read_to_string(&written).unwrap(), "alice-version");
    }

    #[test]
    fn sync_once_writes_codex_prefixed_keys_under_dot_codex() {
        let (tmp, source, mut env, identity) =
            setup(&[("org/codex/config.toml", "model = \"o1\"\n")]);
        std::fs::create_dir(tmp.path().join("home/.codex")).unwrap();
        env.home = tmp.path().join("home");
        let (c, x, p, u) = all_providers();
        let providers = provider_slice(&c, &x, &p, &u);

        let report = sync_once(
            &source,
            &identity,
            &env,
            &providers,
            &FileHashes::default(),
            DriftPolicy::Warn,
        )
        .unwrap();

        let written = tmp.path().join("home/.codex/config.toml");
        assert!(written.is_file(), "expected {written:?} to exist");
        assert_eq!(
            std::fs::read_to_string(&written).unwrap(),
            "model = \"o1\"\n"
        );
        assert_eq!(
            report.files_written_per_provider.get("codex").copied(),
            Some(1)
        );
    }

    #[test]
    fn sync_once_skips_unknown_prefix_and_reports_it() {
        let (_tmp, source, env, identity) = setup(&[("org/notaprovider/settings.json", "{}")]);
        let (c, x, p, u) = all_providers();
        let providers = provider_slice(&c, &x, &p, &u);

        let report = sync_once(
            &source,
            &identity,
            &env,
            &providers,
            &FileHashes::default(),
            DriftPolicy::Warn,
        )
        .unwrap();

        assert!(report.files_written_per_provider.is_empty());
        assert_eq!(
            report.unknown_prefix_keys,
            vec!["notaprovider/settings.json"]
        );
    }

    #[test]
    fn sync_once_writes_claude_prefixed_keys_under_dot_claude() {
        let (tmp, source, env, identity) = setup(&[("org/claude/agents/security.md", "the rule")]);
        let (c, x, p, u) = all_providers();
        let providers = provider_slice(&c, &x, &p, &u);

        let report = sync_once(
            &source,
            &identity,
            &env,
            &providers,
            &FileHashes::default(),
            DriftPolicy::Warn,
        )
        .unwrap();

        let written = tmp.path().join("home/.claude/agents/security.md");
        assert!(written.is_file());
        assert_eq!(std::fs::read_to_string(&written).unwrap(), "the rule");
        assert_eq!(
            report.files_written_per_provider.get("claude").copied(),
            Some(1)
        );
        assert!(report.unknown_prefix_keys.is_empty());
    }

    #[test]
    fn sync_once_writes_copilot_and_cursor_prefixed_keys() {
        let (tmp, source, env, identity) = setup(&[
            ("org/copilot/hosts.json", "{}\n"),
            ("org/cursor/rules/general.md", "be helpful\n"),
        ]);
        // Pre-create the per-provider home dirs so detect() returns true.
        std::fs::create_dir_all(tmp.path().join("home/.config/github-copilot")).unwrap();
        std::fs::create_dir(tmp.path().join("home/.cursor")).unwrap();
        let (c, x, p, u) = all_providers();
        let providers = provider_slice(&c, &x, &p, &u);

        let report = sync_once(
            &source,
            &identity,
            &env,
            &providers,
            &FileHashes::default(),
            DriftPolicy::Warn,
        )
        .unwrap();

        assert!(tmp
            .path()
            .join("home/.config/github-copilot/hosts.json")
            .is_file());
        assert!(tmp.path().join("home/.cursor/rules/general.md").is_file());
        assert_eq!(
            report.files_written_per_provider.get("copilot").copied(),
            Some(1)
        );
        assert_eq!(
            report.files_written_per_provider.get("cursor").copied(),
            Some(1)
        );
    }
}
