use anyhow::{Context, Result};
use chrono::Utc;
use kerios_core::config::Config;
use kerios_core::state::State;

use super::shared::{default_config_path, default_state_path, source_label};

/// Entry point for `kerios status`.
pub fn run() -> Result<()> {
    let cfg_path = default_config_path();
    let cfg = Config::load(cfg_path.as_deref()).context("loading ~/.kerios/config.toml")?;

    let state_path =
        default_state_path().context("could not resolve $HOME for ~/.kerios/state.toml")?;
    let state = State::load(&state_path).context("loading ~/.kerios/state.toml")?;

    println!("source (configured): {}", source_label(cfg.source.as_ref()));
    println!("sync mode:           {:?}", cfg.sync.mode);
    println!("interval:            {} s", cfg.sync.interval_secs);

    match state.last_sync_at {
        None => println!("last sync:           (never)"),
        Some(t) => {
            let ago = Utc::now().signed_duration_since(t);
            println!(
                "last sync:           {} ({} ago)",
                t.to_rfc3339(),
                humanize_duration(ago)
            );
            if let Some(src) = &state.last_source {
                println!("last source:         {src}");
            }
            match (&state.last_report, &state.last_error) {
                (Some(r), _) => {
                    println!("last result:         ok");
                    if r.files_written_per_provider.is_empty() {
                        println!("  files written:     (none)");
                    } else {
                        println!("  files written:");
                        for (name, count) in &r.files_written_per_provider {
                            println!("    {name:<10} {count}");
                        }
                    }
                    println!("  unknown prefixes:  {}", r.unknown_prefix_keys_count);
                    if r.drifted_keys_count > 0 {
                        println!("  drifted:           {}", r.drifted_keys_count);
                    }
                    if r.preserved_keys_count > 0 {
                        println!("  preserved:         {}", r.preserved_keys_count);
                    }
                }
                (None, Some(err)) => {
                    println!("last result:         error");
                    println!("  message:           {err}");
                }
                (None, None) => println!("last result:         (no report — idle)"),
            }
        }
    }
    Ok(())
}

fn humanize_duration(d: chrono::Duration) -> String {
    let secs = d.num_seconds();
    if secs < 60 {
        format!("{secs} s")
    } else if secs < 3600 {
        format!("{} min", secs / 60)
    } else if secs < 86_400 {
        format!("{} h", secs / 3600)
    } else {
        format!("{} d", secs / 86_400)
    }
}
