use anyhow::{bail, Context, Result};
use kerios_core::config::Config;

use super::shared::{
    build_provider_env, default_config_path, oss_providers, persist_sync_outcome,
    read_previous_hashes, run_sync_once, source_label, RunResult,
};

/// Entry point for `kerios sync`.
///
/// One-shot: loads the config, runs the same `sync_once` the daemon uses,
/// prints a one-line summary, and exits. Useful for cron, for first-run
/// before the daemon is started, and for troubleshooting.
pub fn run() -> Result<()> {
    let path = default_config_path();
    let cfg = Config::load(path.as_deref()).context("loading ~/.kerios/config.toml")?;

    let env = build_provider_env();
    let providers = oss_providers();

    let label = source_label(cfg.source.as_ref());
    let previous = read_previous_hashes();
    match run_sync_once(&cfg, &env, &providers.as_slice(), &previous) {
        Ok(RunResult::Done(report)) => {
            let per_provider: String = report
                .files_written_per_provider
                .iter()
                .map(|(name, count)| format!("{name}={count}"))
                .collect::<Vec<_>>()
                .join(" ");
            println!(
                "ok — source={label} written=[{per_provider}] unknown_prefixes={} drifted={} preserved={}",
                report.unknown_prefix_keys.len(),
                report.drifted_keys.len(),
                report.preserved_keys.len(),
            );
            if !report.unknown_prefix_keys.is_empty() {
                eprintln!("warning: skipped keys with unknown provider prefix:");
                for k in &report.unknown_prefix_keys {
                    eprintln!("  - {k}");
                }
            }
            if !report.drifted_keys.is_empty() {
                eprintln!("warning: drift detected — hand-edited files overwritten:");
                for k in &report.drifted_keys {
                    eprintln!("  - {k}");
                }
            }
            if !report.preserved_keys.is_empty() {
                eprintln!("warning: drift detected — local kept (preserve policy):");
                for k in &report.preserved_keys {
                    eprintln!("  - {k}");
                }
            }
            persist_sync_outcome(&label, Some(&report), None);
            Ok(())
        }
        Ok(RunResult::NoSource) => {
            bail!("no source configured in ~/.kerios/config.toml — add a [source] block")
        }
        Ok(RunResult::Skipped) => {
            eprintln!(
                "another `kerios` process is already syncing (~/.kerios/sync.lock held) — exiting"
            );
            Ok(())
        }
        Err(e) => {
            persist_sync_outcome(&label, None, Some(&e.to_string()));
            Err(e)
        }
    }
}
