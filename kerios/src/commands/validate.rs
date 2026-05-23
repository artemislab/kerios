use std::path::Path;

use anyhow::{Context, Result};
use kerios_core::config::{Config, SourceConfig};

/// Entry point for `kerios validate <path>`.
pub fn run(path: &Path) -> Result<()> {
    let cfg = Config::from_file(path)
        .with_context(|| format!("could not load config from {}", path.display()))?;
    let source_label = match &cfg.source {
        Some(SourceConfig::Git { repo_url, .. }) => format!("git({repo_url})"),
        None => "<none>".to_string(),
    };
    println!(
        "ok — source={source_label} sync.mode={:?} sync.interval_secs={}",
        cfg.sync.mode, cfg.sync.interval_secs
    );
    Ok(())
}
