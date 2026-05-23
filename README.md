# Kerios

**Agent governance for AI coding assistants across your team.**

Kerios is a lightweight Rust daemon that keeps Claude Code and Codex configs in sync across an engineering team — like Puppet, but for AI tools.

- One git repo holds the org-wide, per-team, and per-user config layers.
- A small daemon on each developer machine pulls + merges them, then writes the result to `~/.claude/`, `~/.codex/`, etc.
- Hand-edits are detected and reconciled according to a configurable drift policy (`warn` / `enforce` / `preserve`).
- Zero telemetry, zero call-home. The only outbound network traffic is the git pull.

For diagrams and the longer "what / why", read [`docs/architecture.md`](docs/architecture.md). To see the daemon in action against a tiny git server + three agents on your laptop in two minutes, see [`docker/`](docker/).

## Status

**v0.1.0** — first usable release. APIs and the bundle layout may still shift before 1.0. See [CHANGELOG](CHANGELOG.md).

---

## Install

### Homebrew (macOS, Linux)

```sh
brew install artemislab/tap/kerios
```

Upgrades: `brew upgrade kerios`. The tap lives at [`artemislab/homebrew-tap`](https://github.com/artemislab/homebrew-tap).

### One-line installer (any Unix)

```sh
curl -fsSL https://raw.githubusercontent.com/artemislab/kerios/main/scripts/install.sh | sh
```

Drops the binary in `~/.local/bin/kerios` (or `/usr/local/bin/kerios` as root). Override with `KERIOS_BIN_DIR` and pin with `KERIOS_VERSION=v0.1.0`.

### From source

```sh
cargo install --git https://github.com/artemislab/kerios kerios   # Rust 1.85+
```

---

## Quick start

Devops (or you, the first time) publishes a `bootstrap.toml` somewhere over HTTPS:

```toml
# served at https://kerios-bootstrap.acme.com/bootstrap.toml
[source]
type = "git"
repo_url  = "git@github.com:acme/kerios-config.git"
cache_dir = "/var/lib/kerios/cache"

[sync]
interval_secs = 60
drift_policy  = "warn"

[auth]
secret_url = "gs://acme-kerios-secrets/deploy-key"   # or https:// or ssh_key_path
```

Each user runs two commands:

```sh
# 1. enroll: fetches the bootstrap, materializes the SSH key, writes ~/.kerios/config.toml (0600)
kerios enroll https://kerios-bootstrap.acme.com/bootstrap.toml --team backend --user alice

# 2. install + start the background service (see "Run as a service" for Linux)
kerios install > ~/Library/LaunchAgents/io.artemislab.kerios.plist
launchctl load ~/Library/LaunchAgents/io.artemislab.kerios.plist
```

That's it. The daemon now pulls from the configured git repo every `interval_secs` seconds.

---

## Configure (manual alternative)

If you skip `enroll`, drop the same file at `~/.kerios/config.toml` by hand:

```toml
[source]
type = "git"
repo_url  = "git@github.com:your-org/kerios-config.git"
cache_dir = "/home/alice/.kerios/cache"

[sync]
mode = "pull"                # "pull" is the only OSS mode
interval_secs = 60
drift_policy = "warn"        # "warn" | "enforce" | "preserve"

[identity]
team = "backend"             # optional, picks up teams/backend/
user = "alice"               # optional, picks up users/alice/

[auth]
ssh_key_path = "/etc/kerios/deploy_key"   # optional, used as GIT_SSH_COMMAND
```

### Expected git repo layout

```
org/<provider>/<files...>            # applies to everyone
teams/<name>/<provider>/<files...>   # applies to that team
users/<name>/<provider>/<files...>   # applies to that user
```

`<provider>` is one of `claude`, `codex`, `copilot`, `cursor`. Mapping:

| Prefix in bundle | Detection | Destination on disk |
|------------------|-----------|---------------------|
| `claude/...`     | `~/.claude/` exists OR `claude` in PATH       | `~/.claude/`                  |
| `codex/...`      | `~/.codex/` exists OR `codex` in PATH         | `~/.codex/`                   |
| `copilot/...`    | `~/.config/github-copilot/` OR `copilot` PATH | `~/.config/github-copilot/`   |
| `cursor/...`     | `~/.cursor/` exists OR `cursor` in PATH       | `~/.cursor/`                  |

Other prefixes are skipped and reported in `kerios status`.

### Drift policy

When a managed file on disk no longer matches what the daemon last wrote (someone hand-edited it):

| Value | Behavior |
|-------|----------|
| `warn` (default) | Overwrite with the bundle version, log a `warn`. |
| `enforce` | Same as `warn` but logged at `error` so monitoring escalates. |
| `preserve` | Keep the local edit, do NOT overwrite. The bundle version is **not** applied for that key. |

---

## Run as a service

`kerios install` prints the OS-native service definition with the running binary path substituted. Pipe it to the right location:

**macOS (user-level launchd):**
```sh
mkdir -p ~/Library/LaunchAgents
kerios install > ~/Library/LaunchAgents/io.artemislab.kerios.plist
launchctl load ~/Library/LaunchAgents/io.artemislab.kerios.plist
```

**Linux (user-level systemd):**
```sh
mkdir -p ~/.config/systemd/user
kerios install > ~/.config/systemd/user/kerios.service
systemctl --user daemon-reload
systemctl --user enable --now kerios
```

Logs:
- macOS: `/tmp/kerios.out.log`, `/tmp/kerios.err.log`
- Linux: `journalctl --user -u kerios -f`

Uninstall: `launchctl unload …` then `rm` the file (macOS), or `systemctl --user disable --now kerios` then `rm` the unit (Linux).

---

## Subcommands

| Command | What it does |
|---------|--------------|
| `kerios daemon`             | Long-running sync loop. Started by the service unit. |
| `kerios sync`               | One-shot: pull, merge, write, exit. Cron-friendly. |
| `kerios status`             | Last sync time, source, result, file counts. Reads `~/.kerios/state.toml`. |
| `kerios validate <file>`    | Parse a config file and print what it would mean. |
| `kerios enroll <url>`       | Fetch a bootstrap from a URL, materialize secrets, write `~/.kerios/config.toml`. |
| `kerios install`            | Print the OS-native service unit. |

---

## Try it locally — `docker/`

The `docker/` directory has a runnable compose stack (1 git server + 3 agents with different identities) and a `demo.sh` that exercises propagation and drift detection end-to-end. See [`docker/README.md`](docker/README.md).

---

## Privacy and security

Kerios never calls home. The only outbound traffic is the git pull against the source you configure (and, at enroll time, the bootstrap URL + optional `secret_url`). No telemetry, no analytics.

Threat model, what is protected at rest, and what is intentionally NOT protected: [SECURITY.md](SECURITY.md). Vulnerability reports: **security@artemislab.io**.

---

## Build from source

```sh
cargo build --release       # Rust 1.85+
cargo test --all
```

Workspace layout:

```
kerios-core/   shared library: sources, merge, providers, sync, state, auth, bootstrap
kerios/        the single dev-machine binary
```

CI runs `cargo fmt --check`, `cargo clippy -D warnings`, and the full test suite on every PR. See `.github/workflows/ci.yml`.

---

## Architecture and docs

- [`docs/architecture.md`](docs/architecture.md) — visual contract: system map, sync-tick sequence diagram, trust boundaries.
- [`.claude/output/architecture.md`](.claude/output/architecture.md) — detailed ADRs, data model, full API surface.
- [`.claude/output/backlog.md`](.claude/output/backlog.md) — implementation tasks.
- [`SECURITY.md`](SECURITY.md) — threat model and roadmap for security-relevant items.

---

## License

GPL-3.0-or-later. See [LICENSE](LICENSE). Contributions accepted under the same license.
