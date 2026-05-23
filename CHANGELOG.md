# Changelog

All notable changes to this project will be documented in this file. The
format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- **Sync lock** (`~/.kerios/sync.lock`) — `kerios daemon` and `kerios sync` now acquire an exclusive file lock for the duration of a cycle. Two `kerios` processes (cron racing the daemon, two daemons mistakenly running) no longer collide on the git cache; the second exits cleanly with a `RunResult::Skipped` rather than producing partial state. Implementation: `fd-lock` crate.
- Two new provider adapters: **GitHub Copilot** (`~/.config/github-copilot/`) and **Cursor** (`~/.cursor/`). Bundle prefixes `copilot/` and `cursor/` now route to their respective config dirs. Detection mirrors the existing Claude / Codex pattern (home dir or binary in PATH).
- `kerios-core::sync::sync_once` now takes a `&[&dyn Provider]` slice instead of two fixed provider arguments — adding a future provider (Cody, Continue, …) needs no signature change.
- `SyncReport.files_written_per_provider: BTreeMap<String, usize>` replaces the old per-provider counters. `kerios status` and `kerios sync` print the counts per provider; old state files load cleanly thanks to `#[serde(default)]`.
- Homebrew tap at [`artemislab/homebrew-tap`](https://github.com/artemislab/homebrew-tap) — `brew install artemislab/tap/kerios` installs the latest stable release on macOS (arm64 + x86_64) and Linux (arm64 + x86_64).
- `kerios enroll <bootstrap-url> --team <t> --user <u>` — fetches a partial config from a URL (http/https), merges identity from CLI flags, and writes a mode-0600 `~/.kerios/config.toml`. P1 of the bootstrap flow.
- `[auth].ssh_key_path` — the daemon now passes `GIT_SSH_COMMAND="ssh -i <path> -o IdentitiesOnly=yes -o StrictHostKeyChecking=accept-new"` to git when configured. Subprocess git inherits the identity for every fetch / pull.
- `[auth].secret_url` — at enroll time, the daemon fetches the SSH private key from a URL (`https://` direct, `gs://` via `gsutil cp`), writes it to `~/.kerios/secrets/ssh_key` mode 0600, and rewrites `ssh_key_path` to that local path. `secret_url` is transient and never round-trips through the saved config. P2 of the bootstrap flow.
- `SECURITY.md` — explicit threat model, what's protected at rest (file mode 0600 + OS disk encryption), what's intentionally NOT protected, and a roadmap for OS-keychain integration. P3 of the bootstrap flow chose docs over security-theater encryption (see the doc for rationale).
- `[auth].ssh_key_in_keychain` — reserved schema field for a future OS-keychain release. Currently a no-op; parsed but not enforced. Lets bootstrap.toml authors declare intent today without breaking the rollout when the implementation lands.
- Configurable drift policy via `[sync].drift_policy = "warn" | "enforce" | "preserve"`:
  - `warn` (default) — overwrite the hand-edited file and log a `warn`.
  - `enforce` — same as `warn` but logged at `error` so monitoring picks it up.
  - `preserve` — keep the local edit; the bundle version is NOT applied for that key.
- `SyncReport.preserved_keys` lists the keys kept under `preserve` policy.

### Changed
- `kerios sync` and `kerios daemon` now print / log a `preserved=N` counter in addition to `drifted=N`.

## [0.1.0] — 2026-05-21

First usable OSS release. A team can already point their developers at
this binary, drop a `~/.kerios/config.toml`, and have their
Claude Code / Codex configs sync from a git repo on every machine.

### Added

#### Daemon and orchestration
- `kerios daemon` — long-running tokio sync loop with SIGTERM / SIGINT
  graceful shutdown.
- `kerios sync` — one-shot sync, useful for cron, first-run, and
  troubleshooting.
- `kerios status` — reads `~/.kerios/state.toml` and prints the last
  sync time, source, result, file counts, and time-ago.
- `kerios validate <path>` — parses a config file and reports its
  source, sync mode, and interval.
- `kerios install` — prints the OS-native service unit (launchd plist
  on macOS, systemd user unit on Linux) with the running binary path
  substituted, ready to pipe into the right location.

#### Config sources
- `kerios-core::sources::ConfigSource` trait with `fetch`,
  `read_org_layer`, `read_team_layer`, `read_user_layer`.
- `kerios-core::sources::git::GitSource` — shells out to system `git`
  for clone / pull, walks `org/`, `teams/<name>/`, `users/<name>/`.
  Reading an absent layer returns an empty `ConfigLayer` (not an error).

#### Config layout
- `~/.kerios/config.toml` schema with `[source]` (currently only
  `type = "git"`), `[sync]` (`mode = "pull"`, `interval_secs`), and
  `[identity]` (`team`, `user`).
- Bundle paths are routed to providers by their leading segment:
  `claude/...` → `~/.claude/`, `codex/...` → `~/.codex/`. Unknown
  prefixes are skipped and reported in `SyncReport.unknown_prefix_keys`.

#### Merge
- `kerios-core::merge::merge(org, team, user)` — deterministic
  `BTreeMap`-based merge with `user > team > org` precedence and
  additive union. Covered by 3 `proptest` property tests.

#### Providers
- `kerios-core::providers::Provider` trait with `name`, `detect`,
  `config_dir`, default-impl `write_config` (atomic temp + rename,
  parent-dir creation).
- `Claude` adapter — detects `~/.claude/` or `claude` in PATH;
  config dir `~/.claude`.
- `Codex` adapter — detects `~/.codex/` or `codex` in PATH;
  config dir `~/.codex`.

#### State and drift
- `~/.kerios/state.toml` — `last_sync_at`, `last_source`, `last_report`,
  `last_error`, and `last_hashes` (SHA-256 hex per managed bundle key).
- `kerios sync` and `kerios daemon` persist state after every cycle.
- Drift detection — every cycle compares each managed file's current
  on-disk hash to the last-applied hash. Mismatch = drift; the current
  v0.1.0 policy is `warn` (log and overwrite); configurable policies
  arrive in a later release.

#### Packaging and operations
- `packaging/launchd/io.artemislab.kerios.plist` — user-level launchd
  template (KeepAlive, RUST_LOG=info, logs to `/tmp/kerios.{out,err}.log`).
- `packaging/systemd/kerios.service` — user-level systemd unit
  (`Type=simple`, `Restart=on-failure`, `WantedBy=default.target`).
- `docker/` — runnable docker compose demo (1 git server + 3 agents
  with different identities) that exercises identity-based layering,
  propagation under 5 s, and drift detection.

#### Quality
- Workspace lints: `unsafe_code = forbid`, `clippy::all` warn,
  `clippy::pedantic` warn (`module_name_repetitions` allowed).
- 45+ tests across the workspace: unit, property, real-bare-repo E2E
  for sources/sync, and real-binary subprocess E2E for the CLI
  surface.
- E2E "real user simulation" rule documented in
  `.claude/rules/testing.md` and enforced for the binary tests.
- CI on every PR: `cargo fmt --check`, `cargo clippy -D warnings`,
  `cargo test --all`.

[Unreleased]: https://github.com/artemislab/kerios/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/artemislab/kerios/releases/tag/v0.1.0
