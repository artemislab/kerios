# Backlog: Kerios
**Version:** 1.0
**Phase:** model

## Round 1: Foundation (daemon skeleton)

### T-001: Cargo workspace + crate structure
- **Component:** all
- **Priority:** P0
- **Type:** setup
- **Description:** Create Cargo workspace with kerios-core, kerios-daemon, kerios-cli crates. Set up CI (cargo fmt, clippy, test).
- **Depends on:** none
- **Acceptance Criteria:**
  - `cargo build` succeeds for all crates
  - `cargo test` runs (even if no tests yet)
  - CI pipeline green on push
- **Files to create:** `Cargo.toml`, `kerios-core/`, `kerios-daemon/`, `kerios-cli/`, `.github/workflows/ci.yml`

### T-002: Daemon bootstrap — tokio runtime + config loading
- **Component:** kerios-daemon
- **Priority:** P0
- **Type:** feature
- **Description:** Daemon binary that reads `~/.kerios/config.toml` (or env vars), starts tokio runtime, runs a sync loop on interval.
- **Depends on:** T-001
- **Acceptance Criteria:**
  - Binary starts and logs "Kerios daemon started"
  - Reads config from `~/.kerios/config.toml` or `KERIOS_CONFIG_URL` env var
  - Runs a loop at configurable interval (default 60s)
  - Graceful shutdown on SIGTERM
- **Files to create:** `kerios-daemon/src/main.rs`, `kerios-core/src/config.rs`

### T-003: Provider adapters — detect + write configs
- **Component:** kerios-core
- **Priority:** P0
- **Type:** feature
- **Description:** Detect which AI providers are installed (Claude Code, Codex) and write config files to their expected paths.
- **Depends on:** T-002
- **Acceptance Criteria:**
  - Detects Claude Code (checks `~/.claude/` exists or `claude` in PATH)
  - Detects Codex (checks `~/.codex/` or `codex` in PATH)
  - Writes a skill file to `~/.claude/skills/test.md` and verifies it persists
  - Provider adapter trait: `detect()`, `write_config()`, `config_paths()`
- **Files to create:** `kerios-core/src/providers/mod.rs`, `kerios-core/src/providers/claude.rs`, `kerios-core/src/providers/codex.rs`

### T-004: Config merge engine — layered merge (org → team → user)
- **Component:** kerios-core
- **Priority:** P0
- **Type:** feature
- **Description:** Merge config layers with predictable precedence. User overrides team, team overrides org.
- **Depends on:** T-003
- **Acceptance Criteria:**
  - Merge function: `merge(org, team, user) → final_config`
  - Same slug at different layers: higher layer wins (user > team > org)
  - Different slugs: all included (additive merge)
  - Property test: merge is deterministic for same inputs
- **Files to create:** `kerios-core/src/merge.rs`

## Round 2: Config source + sync loop

### T-005: Git config source — clone/pull + read layers
- **Component:** kerios-core
- **Priority:** P0
- **Type:** feature
- **Description:** Pull configs from a git repo with directory structure: `org/`, `teams/<name>/`, `users/<name>/`.
- **Depends on:** T-004
- **Acceptance Criteria:**
  - Clones repo on first run, pulls on subsequent runs
  - Reads `org/` as org layer, `teams/<team>/` as team layer
  - Maps files to config types (`.md` → skill/rule/agent, `.toml`/`.json` → setting)
- **Files to create:** `kerios-core/src/sources/git.rs`

### T-006: HTTP config source — pull from brain API
- **Component:** kerios-core
- **Priority:** P1
- **Type:** feature
- **Description:** Pull configs from HTTP endpoint (brain API). Used when org manages configs via brain dashboard instead of git.
- **Depends on:** T-004
- **Acceptance Criteria:**
  - GET /api/v1/config with API key → returns config layers as JSON
  - Parses response into same layer structure as git source
  - Falls back to cached config if request fails
- **Files to create:** `kerios-core/src/sources/http.rs`

### T-007: Local cache + offline mode
- **Component:** kerios-daemon
- **Priority:** P1
- **Type:** feature
- **Description:** Cache last successful config in SQLite. Apply from cache if source unreachable.
- **Depends on:** T-005
- **Acceptance Criteria:**
  - After successful sync, config saved to `~/.kerios/cache.db`
  - If git/HTTP fails, daemon loads from cache and logs warning
  - Never deletes existing provider configs on sync failure
- **Files to create:** `kerios-daemon/src/cache.rs`

### T-008: Local API — /health + /status + /sync
- **Component:** kerios-daemon
- **Priority:** P1
- **Type:** feature
- **Description:** HTTP server on localhost:19100 for status checks and manual sync trigger.
- **Depends on:** T-002
- **Acceptance Criteria:**
  - GET /health → `{"status": "ok", "last_sync": "...", "version": "..."}`
  - GET /status → detailed: providers detected, configs applied, sync errors
  - POST /sync → triggers immediate sync, returns result
  - Bound to 127.0.0.1 only
- **Files to create:** `kerios-daemon/src/api.rs`

## Round 3: Packaging + deployment

### T-009: Install script + launchd/systemd integration
- **Component:** kerios-daemon
- **Priority:** P1
- **Type:** infra
- **Description:** install.sh that downloads the binary, creates config, installs as system service.
- **Depends on:** T-008
- **Acceptance Criteria:**
  - `curl -sSf https://kerios.dev/install.sh | sh -s -- --config-url <URL>` works on macOS + Linux
  - Creates launchd plist (macOS) or systemd unit (Linux)
  - Daemon starts automatically on boot
  - Uninstall script removes everything cleanly
- **Files to create:** `scripts/install.sh`, `scripts/uninstall.sh`, `dist/dev.kerios.daemon.plist`, `dist/kerios-daemon.service`

### T-010: CLI — kerios status, kerios sync, kerios validate
- **Component:** kerios-cli
- **Priority:** P1
- **Type:** feature
- **Description:** Admin CLI for daemon interaction and config validation.
- **Depends on:** T-008
- **Acceptance Criteria:**
  - `kerios status` → calls daemon /status, prints human-readable output
  - `kerios sync` → calls daemon /sync, prints result
  - `kerios validate <path>` → validates a config repo structure without running the daemon
- **Files to create:** `kerios-cli/src/main.rs`

## Round 4: Brain server (foundation for paid tier)

### T-011: Brain server bootstrap — axum + PostgreSQL
- **Component:** kerios-brain
- **Priority:** P1
- **Type:** setup
- **Description:** Brain server binary with database migrations, org creation, API key management.
- **Depends on:** T-001
- **Acceptance Criteria:**
  - Binary starts, runs migrations, serves /health
  - POST /api/v1/orgs creates an org with API key
  - API key auth middleware validates Bearer token
- **Files to create:** `kerios-brain/`, `kerios-brain/src/main.rs`, `kerios-brain/migrations/`

### T-012: Config API — serve configs to daemons
- **Component:** kerios-brain
- **Priority:** P1
- **Type:** feature
- **Description:** API that serves config layers to daemons. Brain stores configs that admins manage.
- **Depends on:** T-011, T-006
- **Acceptance Criteria:**
  - GET /api/v1/config returns merged layers for the requesting machine (identified by API key + team + user headers)
  - CRUD endpoints for config layers (admin API)
  - Config versioning: daemon sends its last version, brain returns only if newer
- **Files to modify:** `kerios-brain/src/routes/`

### T-013: Heartbeat + fleet status
- **Component:** kerios-brain
- **Priority:** P1
- **Type:** feature
- **Description:** Daemons report heartbeat. Brain tracks fleet health.
- **Depends on:** T-011
- **Acceptance Criteria:**
  - POST /api/v1/heartbeat registers/updates machine record
  - GET /api/v1/dashboard/machines returns fleet status with last_heartbeat, version, sync status
  - Machines with no heartbeat for 10min → stale. 1 hour → offline.
- **Files to create:** `kerios-brain/src/routes/heartbeat.rs`, `kerios-brain/src/routes/dashboard.rs`

## Round 5: Policy engine (paid tier foundation)

### T-014: Policy engine — pattern matching + actions
- **Component:** kerios-core
- **Priority:** P2
- **Type:** feature
- **Description:** Engine that evaluates file paths and content against policy rules. Returns block/warn/redact/allow.
- **Depends on:** T-004
- **Acceptance Criteria:**
  - Path glob matching (`*.env`, `/mnt/hr/*`)
  - Regex content scanning (API key patterns, PII patterns)
  - Entropy detection for high-entropy strings (potential secrets)
  - Priority ordering: first match wins
  - Action: block, warn, redact, allow
- **Files to create:** `kerios-core/src/policy/mod.rs`, `kerios-core/src/policy/patterns.rs`, `kerios-core/src/policy/scanner.rs`

### T-015: Filesystem watcher — detect AI process file access
- **Component:** kerios-daemon
- **Priority:** P2
- **Type:** feature
- **Description:** Watch filesystem events from AI processes. Block access to forbidden paths.
- **Depends on:** T-014
- **Acceptance Criteria:**
  - Detects when claude/codex processes open files
  - Evaluates opened path against policy rules
  - Block = deny access (platform-specific: macOS sandbox, Linux fanotify)
  - Warn = allow but log event
- **Files to create:** `kerios-daemon/src/watcher.rs`

### T-016: Metrics collection + push to brain
- **Component:** kerios-daemon
- **Priority:** P2
- **Type:** feature
- **Description:** Collect anonymized metric events, buffer in SQLite, batch push to brain.
- **Depends on:** T-014, T-011
- **Acceptance Criteria:**
  - Each policy evaluation creates a MetricEvent (anonymized)
  - Events buffered in SQLite (max 10K, FIFO)
  - Batch push to brain every 5 minutes (configurable)
  - Anonymization: path → pattern, no file content
- **Files to create:** `kerios-daemon/src/metrics.rs`

### T-017: Metrics ingestion + dashboard API
- **Component:** kerios-brain
- **Priority:** P2
- **Type:** feature
- **Description:** Brain receives metrics, stores them, serves dashboard endpoints.
- **Depends on:** T-013, T-016
- **Acceptance Criteria:**
  - POST /api/v1/metrics accepts batch of events
  - Dashboard endpoints: overview, events, teams, trends
  - Data retention: configurable per org (default 90 days)
- **Files to modify:** `kerios-brain/src/routes/`

## Round 6: TLS Proxy (paid tier advanced)

### T-018: TLS proxy — intercept AI API traffic
- **Component:** kerios-proxy
- **Priority:** P2
- **Type:** feature
- **Description:** Local TLS-intercepting proxy for AI API domains. Inspects payloads before forwarding.
- **Depends on:** T-014
- **Acceptance Criteria:**
  - Proxy listens on configurable port
  - Intercepts only AI API domains (allowlist)
  - Decrypts TLS using installed CA cert
  - Evaluates payload against policy rules
  - Block/warn/redact before forwarding
- **Files to create:** `kerios-proxy/src/`, `kerios-proxy/src/proxy.rs`, `kerios-proxy/src/tls.rs`

## Summary

| Priority | Count | Theme |
|----------|-------|-------|
| P0 | 4 | Foundation — daemon skeleton, providers, merge engine |
| P1 | 9 | Config sync, packaging, brain server, deployment |
| P2 | 5 | Policy engine, metrics, proxy (paid features) |
| **Total** | **18** | |
