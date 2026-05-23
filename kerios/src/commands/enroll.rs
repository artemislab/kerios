use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use kerios_core::bootstrap::{compose_config, fetch_bootstrap, validate_local_paths, Bootstrap};
use kerios_core::config::Identity;
use kerios_core::secrets::fetch_secret;

use super::shared::default_config_path;

/// Entry point for `kerios enroll <url> --team <t> --user <u> [--force]`.
///
/// Fetches a `bootstrap.toml` from `url`, merges identity from CLI flags,
/// writes `~/.kerios/config.toml`. Refuses to overwrite an existing config
/// unless `--force` is set.
pub fn run(
    bootstrap_url: &str,
    team: Option<String>,
    user: Option<String>,
    force: bool,
) -> Result<()> {
    let path =
        default_config_path().context("could not resolve $HOME for ~/.kerios/config.toml")?;
    if path.exists() && !force {
        return Err(anyhow!(
            "{} already exists — pass --force to overwrite",
            path.display()
        ));
    }

    println!("fetching {bootstrap_url} ...");
    let mut bootstrap = fetch_bootstrap(bootstrap_url)
        .with_context(|| format!("fetching bootstrap from {bootstrap_url}"))?;

    // Resolve any [auth].secret_url and [auth.github_app].private_key_secret_url
    // before validating local paths — the freshly-written files become the
    // canonical `ssh_key_path` and `private_key_path`.
    let secrets_dir = default_secrets_dir()?;
    materialize_secret_url(&mut bootstrap, &secrets_dir)?;
    materialize_github_app_key(&mut bootstrap, &secrets_dir)?;

    validate_local_paths(&bootstrap).context("validating bootstrap references")?;

    let cfg = compose_config(bootstrap, Identity { team, user });

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let rendered = toml::to_string_pretty(&cfg).context("serializing config")?;
    write_secure(&path, &rendered).with_context(|| format!("writing {}", path.display()))?;

    println!("wrote {}", path.display());
    println!();
    println!("next steps:");
    println!("  kerios sync                                # validate end-to-end");
    println!("  kerios install > ~/Library/LaunchAgents/io.artemislab.kerios.plist   # macOS");
    println!("  kerios install > ~/.config/systemd/user/kerios.service                # Linux");
    Ok(())
}

/// Write `data` to `path` with mode 0600 on Unix; falls back to a plain
/// write on non-Unix targets (Windows handles permissions differently).
fn write_secure(path: &Path, data: impl AsRef<[u8]>) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(data.as_ref())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, data.as_ref())
    }
}

fn default_secrets_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| anyhow!("$HOME is not set; cannot place ~/.kerios/secrets/"))?;
    Ok(PathBuf::from(home).join(".kerios").join("secrets"))
}

/// If `bootstrap.auth.secret_url` is set, download it once, write it to
/// `<secrets_dir>/ssh_key` mode 0600, and rewrite `ssh_key_path` to point
/// there. After this call the runtime config carries only an on-disk
/// path; `secret_url` is consumed and dropped at serialize time.
fn materialize_secret_url(bootstrap: &mut Bootstrap, secrets_dir: &Path) -> Result<()> {
    let Some(auth) = bootstrap.auth.as_mut() else {
        return Ok(());
    };
    let Some(url) = auth.secret_url.take() else {
        return Ok(());
    };

    println!("fetching secret from {url} ...");
    let bytes = fetch_secret(&url).with_context(|| format!("fetching secret from {url}"))?;

    let dest = secrets_dir.join("ssh_key");
    persist_secret(secrets_dir, &dest, &bytes)?;
    auth.ssh_key_path = Some(dest);
    Ok(())
}

/// Same shape as `materialize_secret_url`, but for the GitHub App's
/// RSA private key referenced by `auth.github_app.private_key_secret_url`.
/// Drops `private_key_secret_url` and points `private_key_path` at the
/// materialized file.
fn materialize_github_app_key(bootstrap: &mut Bootstrap, secrets_dir: &Path) -> Result<()> {
    let Some(auth) = bootstrap.auth.as_mut() else {
        return Ok(());
    };
    let Some(gh) = auth.github_app.as_mut() else {
        return Ok(());
    };
    let Some(url) = gh.private_key_secret_url.take() else {
        return Ok(());
    };

    println!("fetching GitHub App private key from {url} ...");
    let bytes =
        fetch_secret(&url).with_context(|| format!("fetching GitHub App key from {url}"))?;

    let dest = secrets_dir.join("github-app.pem");
    persist_secret(secrets_dir, &dest, &bytes)?;
    gh.private_key_path = Some(dest);
    Ok(())
}

fn persist_secret(dir: &Path, dest: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    write_secure(dest, bytes).with_context(|| format!("writing secret to {}", dest.display()))?;
    println!(
        "wrote {} ({} bytes, mode 0600)",
        dest.display(),
        bytes.len()
    );
    Ok(())
}
