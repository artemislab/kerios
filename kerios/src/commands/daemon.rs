use std::time::Duration;

use anyhow::Result;
use kerios_core::config::{Config, DriftPolicy};
use tokio::signal::unix::{signal, SignalKind};
use tokio::time::interval;
use tracing::{error, info, warn};

use super::shared::{
    build_provider_env, default_config_path, oss_providers, persist_sync_outcome,
    read_previous_hashes, run_sync_once, source_label, RunResult,
};

/// Entry point for `kerios daemon`.
pub fn run() -> Result<()> {
    // current_thread is enough: the daemon owns at most one in-flight sync
    // at a time, and the heavy lifting (subprocess `git`) is delegated to
    // the `spawn_blocking` thread pool.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(run_async())
}

async fn run_async() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config_path = default_config_path();
    let cfg = Config::load(config_path.as_deref())?;

    info!(
        interval_secs = cfg.sync.interval_secs,
        source = %source_label(cfg.source.as_ref()),
        "Kerios daemon started"
    );

    sync_loop(cfg).await
}

async fn sync_loop(cfg: Config) -> Result<()> {
    let mut ticker = interval(Duration::from_secs(cfg.sync.interval_secs));
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    let env = build_provider_env();
    let label = source_label(cfg.source.as_ref());

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                // The sync work is blocking (subprocess `git`, filesystem IO).
                // Push it onto the blocking thread pool so signal handling
                // and the ticker stay responsive on the runtime thread.
                let cfg_for_task = cfg.clone();
                let env_for_task = env.clone();
                let outcome = tokio::task::spawn_blocking(move || {
                    let providers = oss_providers();
                    let previous = read_previous_hashes();
                    run_sync_once(&cfg_for_task, &env_for_task, &providers.as_slice(), &previous)
                })
                .await
                .map_err(anyhow::Error::from)
                .and_then(|inner| inner);
                handle_sync_outcome(&cfg, &label, outcome);
            }
            _ = sigterm.recv() => {
                warn!("SIGTERM received — shutting down");
                return Ok(());
            }
            _ = sigint.recv() => {
                warn!("SIGINT received — shutting down");
                return Ok(());
            }
        }
    }
}

fn handle_sync_outcome(cfg: &Config, label: &str, outcome: Result<RunResult>) {
    match outcome {
        Ok(RunResult::Done(report)) => {
            if !report.drifted_keys.is_empty() {
                let first = report.drifted_keys.first().map(String::as_str);
                if cfg.sync.drift_policy == DriftPolicy::Enforce {
                    error!(
                        drifted = report.drifted_keys.len(),
                        first, "drift detected — overwriting (enforce policy)"
                    );
                } else {
                    warn!(
                        drifted = report.drifted_keys.len(),
                        first, "drift detected — overwriting hand-edited files"
                    );
                }
            }
            if !report.preserved_keys.is_empty() {
                warn!(
                    preserved = report.preserved_keys.len(),
                    first = report.preserved_keys.first().map(String::as_str),
                    "drift detected — local kept (preserve policy)"
                );
            }
            let written = report
                .files_written_per_provider
                .iter()
                .map(|(name, count)| format!("{name}={count}"))
                .collect::<Vec<_>>()
                .join(",");
            info!(
                written = %written,
                total = report.total_files_written(),
                unknown_prefixes = report.unknown_prefix_keys.len(),
                drifted = report.drifted_keys.len(),
                preserved = report.preserved_keys.len(),
                "sync tick"
            );
            persist_sync_outcome(label, Some(&report), None);
        }
        Ok(RunResult::NoSource) => {
            info!("sync tick (no source configured — idle)");
            persist_sync_outcome(label, None, None);
        }
        Ok(RunResult::Skipped) => {
            // Another `kerios` is already syncing — most commonly a
            // `kerios sync` from cron racing the daemon. Logging at
            // info level avoids alert spam.
            info!("sync tick skipped (another kerios holds sync.lock)");
        }
        Err(e) => {
            error!(error = %e, "sync tick failed");
            persist_sync_outcome(label, None, Some(&e.to_string()));
        }
    }
}
