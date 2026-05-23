use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(
    name = "kerios",
    version,
    about = "Kerios — agent governance for AI coding assistants"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the long-running sync daemon.
    Daemon,
    /// Show daemon health and last sync.
    Status,
    /// Force a sync now, bypassing the interval.
    Sync,
    /// Validate a config file before applying.
    Validate {
        /// Path to the config file to validate.
        path: std::path::PathBuf,
    },
    /// Print the OS-native service definition for `kerios daemon`.
    Install,
    /// Enroll this machine by fetching a bootstrap config from a URL.
    Enroll {
        /// URL of the `bootstrap.toml` to fetch (http:// or https://).
        url: String,
        /// Team this machine speaks for (writes into `[identity].team`).
        #[arg(long)]
        team: Option<String>,
        /// User this machine is for (writes into `[identity].user`).
        #[arg(long)]
        user: Option<String>,
        /// Overwrite `~/.kerios/config.toml` if it already exists.
        #[arg(long)]
        force: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Daemon => commands::daemon::run(),
        Command::Status => commands::status::run(),
        Command::Sync => commands::sync::run(),
        Command::Validate { path } => commands::validate::run(&path),
        Command::Install => commands::install::run(),
        Command::Enroll {
            url,
            team,
            user,
            force,
        } => commands::enroll::run(&url, team, user, force),
    }
}
