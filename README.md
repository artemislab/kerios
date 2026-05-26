# Kerios

Every developer on your team has slightly different AI assistant configs — and nobody knows whose is authoritative. Kerios fixes that.

Kerios is a small Rust daemon that keeps Claude Code, Codex, Copilot, and Cursor configs in sync across your entire team from a single git repo — org-wide defaults, team overrides, and per-user exceptions, merged automatically on each machine.

- One git repo holds the org-wide, per-team, and per-user config layers.
- A small daemon on each developer machine pulls + merges them, then writes the result to `~/.claude/`, `~/.codex/`, etc.
- Hand-edits are detected and reconciled according to a configurable drift policy (`warn` / `enforce` / `preserve`).
- Zero telemetry, zero call-home. The only outbound network traffic is the git pull.

For diagrams and the longer "what / why", read [`docs/architecture.md`](docs/architecture.md). To see the daemon in action against a tiny git server + three agents on your laptop in two minutes, see [`docker/`](docker/).

---

## Why this exists

The teams we talked to had the same failure modes. You'll recognize them.

- **New hire onboarding**: someone joins, copies a colleague's `~/.claude/` over Slack, and that snapshot is what they use forever — stale the moment it's shared.
- **Policy drift**: security posts an updated `CLAUDE.md` with a new tool restriction. Three weeks later, half the team is still running the old one because there's no push mechanism.
- **Behavioral inconsistency**: two engineers on the same feature have subtly different agent instructions. The AI behaves differently for each. Nobody notices until a review.
- **Off-boarding gap**: someone leaves. Their local config — the prompt tuning they spent months on — leaves with them. There's no central record.
- **No audit trail**: you can't answer "what config was Alice running when that incident happened?"

If any of these have happened on your team, Kerios is for you.

---

## Why not Puppet / Chef / Ansible?

Those tools exist and are good. They're also wrong for this problem.

| | Kerios | Puppet / Chef / Ansible |
|---|---|---|
| **Scope** | Purpose-built for AI assistant config files (`~/.claude/`, `~/.codex/`, etc.) with provider detection and layer merging | General-purpose; no concept of AI config structure or user-level home directory management |
| **Daemon weight** | Single static binary, ~5 MB, user-level service, no agent certificates, no server required (OSS) | Requires an agent (Puppet), controller node (Ansible), or Chef server; operational overhead before day one |
| **Install ceremony** | `brew install kerios && kerios enroll <url>` — two commands for a new machine | Bootstrap scripts, package repos, server enrollment, certificate authority setup |
| **Secrets model** | Deploy key fetched once at enroll time, stored at `~/.kerios/` (0600). Never in the config repo. | Depends on tool: Hiera, Vault integration, or custom; significant setup to avoid secrets in the manifest repo |

Kerios does one thing. If you already run Puppet for the rest of your fleet, Kerios sits alongside it for the AI layer.

---

## Need centralized control, audit logs, and SOC 2 evidence?

**kerios-enterprise** adds a brain server with mTLS, an admin web UI, per-user sync history, a tamper-evident audit log, and one-command evidence packs for compliance audits (CC6.1 / CC6.6 / CC6.7 / CC7.1 / CC7.2). Closed-source paid tier — contact **enterprise@artemislab.io**.

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

### Windows

Kerios doesn't ship a native Windows binary today. Use [WSL2](https://learn.microsoft.com/windows/wsl/install) — install Ubuntu (`wsl --install -d Ubuntu`), then run any of the install paths above inside WSL. The daemon writes to your WSL home; if you also want it touching `C:\Users\<you>\.claude\` on the Windows side, drop a Windows-side symlink into the WSL filesystem. Native Windows (`cargo` target + MSI + Windows service) is on the v0.4 roadmap — open an issue if you need it sooner.

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
