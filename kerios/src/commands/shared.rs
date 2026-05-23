//! Helpers shared between the long-running `daemon` and one-shot `sync`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use fd_lock::RwLock;
use kerios_core::config::{Config, SourceConfig};
use kerios_core::providers::claude::Claude;
use kerios_core::providers::codex::Codex;
use kerios_core::providers::copilot::Copilot;
use kerios_core::providers::cursor::Cursor;
use kerios_core::providers::{Provider, ProviderEnv};
use kerios_core::sources::git::GitSource;
use kerios_core::state::{State, SyncReportSummary};
use kerios_core::sync::{sync_once, FileHashes, SyncReport};

/// Default location for the user-level config file.
#[must_use]
pub fn default_config_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".kerios").join("config.toml"))
}

/// Default location for the daemon state file.
#[must_use]
pub fn default_state_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".kerios").join("state.toml"))
}

/// Lockfile guarding the sync workflow against concurrent runs (cron +
/// daemon, two daemons, double `kerios sync` from a script). One per
/// install — `~/.kerios/sync.lock`.
#[must_use]
pub fn default_lock_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".kerios").join("sync.lock"))
}

/// Persist a sync outcome to `~/.kerios/state.toml`. Best-effort: a write
/// failure is logged but does not propagate, so a transient disk issue
/// does not crash the daemon.
pub fn persist_sync_outcome(source_label: &str, report: Option<&SyncReport>, error: Option<&str>) {
    let Some(path) = default_state_path() else {
        return;
    };
    let state = State {
        last_sync_at: Some(Utc::now()),
        last_source: Some(source_label.to_string()),
        last_report: report.map(SyncReportSummary::from),
        last_error: error.map(str::to_string),
        last_hashes: report.map(|r| r.new_hashes.clone()).unwrap_or_default(),
    };
    if let Err(e) = state.save(&path) {
        tracing::warn!(error = %e, "failed to persist state");
    }
}

/// Read the previous file hashes from `~/.kerios/state.toml`. Returns an
/// empty map on first run or if the state cannot be read.
#[must_use]
pub fn read_previous_hashes() -> FileHashes {
    let Some(path) = default_state_path() else {
        return FileHashes::default();
    };
    State::load(&path)
        .map(|s| s.last_hashes)
        .unwrap_or_default()
}

/// Snapshot of `$HOME` and `$PATH` for the provider adapters.
#[must_use]
pub fn build_provider_env() -> ProviderEnv {
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from);
    let path_dirs = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    ProviderEnv { home, path_dirs }
}

/// Short string describing the configured source for logs.
#[must_use]
pub fn source_label(source: Option<&SourceConfig>) -> String {
    match source {
        Some(SourceConfig::Git { repo_url, .. }) => format!("git({repo_url})"),
        None => "<none>".to_string(),
    }
}

/// Result of one sync attempt. The caller decides logging level.
pub enum RunResult {
    /// No `[source]` configured — the daemon idles, `kerios sync`
    /// errors out with a helpful message.
    NoSource,
    /// Another `kerios` process already holds `~/.kerios/sync.lock`.
    /// Safe to retry on the next tick; the in-flight process is
    /// presumably making progress.
    Skipped,
    Done(SyncReport),
}

/// Run one sync cycle against `cfg.source`. Holds an exclusive
/// `~/.kerios/sync.lock` for the duration; returns `RunResult::Skipped`
/// if another `kerios` is already syncing.
///
/// # Errors
/// Returns an error if the source fetch / read or a provider write fails,
/// or if the lockfile cannot be created (which would indicate a missing
/// `~/.kerios/` directory or unusual permissions).
pub fn run_sync_once(
    cfg: &Config,
    env: &ProviderEnv,
    providers: &[&dyn Provider],
    previous_hashes: &FileHashes,
) -> Result<RunResult> {
    let Some(SourceConfig::Git {
        repo_url,
        cache_dir,
    }) = &cfg.source
    else {
        return Ok(RunResult::NoSource);
    };

    let lock_path =
        default_lock_path().context("could not resolve $HOME for ~/.kerios/sync.lock")?;
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("opening {}", lock_path.display()))?;
    let mut lock = RwLock::new(file);

    let Ok(_guard) = lock.try_write() else {
        return Ok(RunResult::Skipped);
    };

    let source = GitSource::new(repo_url.clone(), cache_dir.clone()).with_auth(cfg.auth.clone());
    let report = sync_once(
        &source,
        &cfg.identity,
        env,
        providers,
        previous_hashes,
        cfg.sync.drift_policy,
    )?;
    Ok(RunResult::Done(report))
}

/// The full set of providers the OSS build ships. The order is the
/// detection priority, but in practice prefixes are disjoint so order
/// only matters when adding a new provider whose name collides.
#[must_use]
pub fn oss_providers() -> Providers {
    Providers {
        claude: Claude::new(),
        codex: Codex::new(),
        copilot: Copilot::new(),
        cursor: Cursor::new(),
    }
}

/// Owned providers + a borrowed slice. We hand the slice to `sync_once`;
/// the struct owns the underlying instances so they outlive the slice.
pub struct Providers {
    pub claude: Claude,
    pub codex: Codex,
    pub copilot: Copilot,
    pub cursor: Cursor,
}

impl Providers {
    /// Borrow as `&[&dyn Provider]` for the canonical `sync_once` API.
    #[must_use]
    pub fn as_slice(&self) -> [&dyn Provider; 4] {
        [&self.claude, &self.codex, &self.copilot, &self.cursor]
    }
}
