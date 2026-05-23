# Architecture: Kerios
**Version:** 1.0
**Phase:** model

## System Overview

```
┌─────────────────────────────────────────────────────┐
│                 Developer Machine                    │
│                                                     │
│  ┌───────────┐     ┌──────────────────────────┐    │
│  │ Claude    │     │     kerios-daemon         │    │
│  │ Code      │◄────│                          │    │
│  │ ~/.claude/│     │  ┌─────────┐ ┌────────┐  │    │
│  └───────────┘     │  │ Config  │ │ Policy │  │    │
│  ┌───────────┐     │  │ Sync    │ │ Engine │  │    │
│  │ Codex     │◄────│  └────┬────┘ └───┬────┘  │    │
│  │ ~/.codex/ │     │       │          │        │    │
│  └───────────┘     │  ┌────▼──────────▼────┐   │    │
│                    │  │   Local SQLite     │   │    │
│  ┌───────────┐     │  │   (cache+metrics)  │   │    │
│  │ Browser   │     │  └────────────────────┘   │    │
│  │ ChatGPT   │     │       │                   │    │
│  │ Claude.ai │◄─┐  │  ┌────▼────┐             │    │
│  └───────────┘  │  │  │ TLS     │ [PAID]      │    │
│                 └──│──│ Proxy   │             │    │
│                    │  └─────────┘             │    │
│                    └──────────┬───────────────┘    │
│                               │                    │
└───────────────────────────────┼────────────────────┘
                                │ HTTPS (metrics + config pull)
                                ▼
┌───────────────────────────────────────────────────┐
│              kerios-brain (Server)                  │
│                                                    │
│  ┌──────────┐  ┌───────────┐  ┌──────────────┐   │
│  │ Config   │  │ Metrics   │  │ Dashboard    │   │
│  │ API      │  │ Ingestion │  │ (Web UI)     │   │
│  └────┬─────┘  └─────┬─────┘  └──────┬───────┘   │
│       │              │               │            │
│       └──────────────┼───────────────┘            │
│                      ▼                            │
│              ┌──────────────┐                     │
│              │  PostgreSQL  │                     │
│              └──────────────┘                     │
└───────────────────────────────────────────────────┘
```

## Components

### kerios-daemon (Rust binary)
- **Type:** service (local daemon)
- **Responsibility:** runs on each dev machine — syncs configs, enforces policies, collects metrics
- **Tech:** Rust, tokio, axum (local API), SQLite (cache), notify (filesystem watcher)
- **Depends on:** kerios-brain (optional — works offline)
- **Crate structure:**
  - `kerios-core` — config merge, policy engine, provider adapters (shared between OSS and paid)
  - `kerios-daemon` — the binary, tokio runtime, sync loop, local API
  - `kerios-proxy` — TLS proxy, content inspection (paid feature, separate crate behind feature flag)

### kerios-brain (Rust binary)
- **Type:** service (server)
- **Responsibility:** central config API, metrics ingestion, dashboard
- **Tech:** Rust, axum, PostgreSQL, optional: embedded web UI (or separate SPA)
- **Depends on:** PostgreSQL

### kerios-cli
- **Type:** CLI tool
- **Responsibility:** admin commands — validate configs, check daemon status, register machine
- **Tech:** Rust, clap
- **Depends on:** kerios-core (library)

## Data Model

### Organization
| Field | Type | Constraints |
|-------|------|------------|
| id | UUID | PK |
| name | VARCHAR(255) | NOT NULL |
| slug | VARCHAR(100) | UNIQUE, NOT NULL |
| api_key_hash | VARCHAR(255) | NOT NULL (daemon auth) |
| license_tier | ENUM | 'free', 'paid', 'enterprise' |
| settings | JSONB | retention, features, etc. |
| created_at | TIMESTAMPTZ | NOT NULL |

### Machine (daemon registration)
| Field | Type | Constraints |
|-------|------|------------|
| id | UUID | PK |
| org_id | UUID | FK → Organization |
| hostname | VARCHAR(255) | NOT NULL |
| os | VARCHAR(50) | macOS, linux, windows |
| arch | VARCHAR(20) | x86_64, aarch64 |
| team | VARCHAR(100) | nullable — team assignment |
| user | VARCHAR(100) | nullable — user identity |
| daemon_version | VARCHAR(20) | NOT NULL |
| last_sync_at | TIMESTAMPTZ | nullable |
| last_heartbeat_at | TIMESTAMPTZ | nullable |
| status | ENUM | 'active', 'stale', 'offline' |

**Relationships:**
- belongs_to → Organization

### ConfigLayer
| Field | Type | Constraints |
|-------|------|------------|
| id | UUID | PK |
| org_id | UUID | FK → Organization |
| scope | ENUM | 'org', 'team', 'role', 'user' |
| scope_value | VARCHAR(100) | e.g., "frontend", "adrien" |
| provider | ENUM | 'claude', 'codex', 'copilot', 'all' |
| config_type | ENUM | 'skill', 'rule', 'agent', 'setting' |
| slug | VARCHAR(100) | NOT NULL |
| content | TEXT | file content (markdown, TOML, JSON) |
| version | INT | increments on update |
| updated_at | TIMESTAMPTZ | NOT NULL |

**Relationships:**
- belongs_to → Organization

### PolicyRule (paid)
| Field | Type | Constraints |
|-------|------|------------|
| id | UUID | PK |
| org_id | UUID | FK → Organization |
| name | VARCHAR(255) | NOT NULL |
| rule_type | ENUM | 'path_block', 'pattern_block', 'content_scan', 'pii_detect' |
| pattern | VARCHAR(500) | glob, regex, or path |
| action | ENUM | 'block', 'warn', 'redact' |
| priority | INT | higher = checked first |
| enabled | BOOLEAN | default true |

**Relationships:**
- belongs_to → Organization

### MetricEvent (paid)
| Field | Type | Constraints |
|-------|------|------------|
| id | UUID | PK |
| org_id | UUID | FK → Organization |
| machine_id | UUID | FK → Machine |
| event_type | ENUM | 'file_access', 'request', 'block', 'warn' |
| provider | VARCHAR(50) | claude, codex, copilot, chatgpt |
| file_type | VARCHAR(20) | rs, py, env, pem, etc. |
| path_pattern | VARCHAR(255) | anonymized path (src/auth/*) |
| contains_sensitive | BOOLEAN | secret/PII detected |
| policy_matched | VARCHAR(100) | which rule triggered |
| action_taken | ENUM | 'allowed', 'blocked', 'warned', 'redacted' |
| metadata | JSONB | extra context (no file content) |
| recorded_at | TIMESTAMPTZ | NOT NULL |

**Relationships:**
- belongs_to → Organization, Machine

## API Surface

### Daemon → Brain (config pull + metrics push)

| Method | Path | Description | Auth |
|--------|------|-------------|------|
| GET | /api/v1/config | Pull config layers for this machine (org+team+user merged) | org API key |
| GET | /api/v1/policies | Pull active policies for this machine | org API key |
| POST | /api/v1/heartbeat | Report daemon status + version | org API key |
| POST | /api/v1/metrics | Push batch of metric events | org API key |

### Brain Dashboard API

| Method | Path | Description | Auth |
|--------|------|-------------|------|
| GET | /api/v1/dashboard/overview | Org-wide metrics summary | JWT |
| GET | /api/v1/dashboard/machines | Fleet status (active, stale, offline) | JWT |
| GET | /api/v1/dashboard/events | Recent security events (blocks, warns) | JWT |
| GET | /api/v1/dashboard/teams/:team | Per-team usage metrics | JWT |
| GET | /api/v1/dashboard/trends | Usage over time (daily/weekly) | JWT |

### Brain Admin API

| Method | Path | Description | Auth |
|--------|------|-------------|------|
| POST | /api/v1/orgs | Create org | admin |
| GET | /api/v1/orgs/:id/configs | List config layers | JWT |
| PUT | /api/v1/orgs/:id/configs/:id | Update config layer | JWT |
| GET | /api/v1/orgs/:id/policies | List policies | JWT |
| PUT | /api/v1/orgs/:id/policies/:id | Update policy | JWT |

### Daemon Local API (localhost only)

| Method | Path | Description | Auth |
|--------|------|-------------|------|
| GET | /health | Daemon health + last sync time | none (localhost) |
| GET | /status | Detailed status: providers detected, configs applied, policies active | none |
| POST | /sync | Force immediate sync | none |

## Infrastructure

### Daemon (on dev machines)
- **Compute:** single Rust binary, runs as launchd (macOS) / systemd (Linux) / service (Windows)
- **Storage:** ~/.kerios/cache.db (SQLite), ~/.kerios/config.toml
- **Networking:** outbound HTTPS to brain endpoint only. Local listener on configurable port (default 19100).
- **Memory:** <10MB RSS target. No GC pauses (Rust).

### Brain (server)
- **Compute:** single Rust binary, or Docker container. Stateless except DB.
- **Database:** PostgreSQL 15+ (per-org row isolation, not RLS — simpler)
- **Deployment:** single VM or container for MVP. Scales horizontally behind a load balancer for enterprise.
- **Networking:** HTTPS (TLS terminated at LB or by axum with rustls)
- **CI/CD:** GitHub Actions → build + test → release binaries (daemon) + Docker image (brain)

### Distribution
- **macOS:** .pkg installer (MDM compatible) + Homebrew tap
- **Linux:** .deb + .rpm + static binary + install.sh
- **Docker:** brain image on GHCR
- **Updates:** daemon checks for updates on heartbeat response (brain tells it the latest version)

## Security

### Authentication
- **Daemon → Brain:** org-scoped API key (hashed in DB, passed as Bearer token)
- **Dashboard users:** email/password → JWT (or SSO for enterprise tier)
- **Daemon local API:** no auth (localhost only, bound to 127.0.0.1)

### Authorization
- **Org isolation:** all queries scoped by org_id. No cross-org access.
- **Dashboard roles:** admin (full access), viewer (read-only dashboard), manager (team-scoped view)

### Encryption
- **In transit:** TLS everywhere (daemon→brain, dashboard)
- **At rest:** PostgreSQL disk encryption. SQLite local cache is NOT encrypted (machine-local, same trust as ~/.claude/)
- **Secrets:** API keys hashed with argon2. No plaintext storage.

### TLS Proxy (paid)
- **CA certificate:** generated per-org by the brain, distributed via daemon config. Installed in system keychain by the daemon installer.
- **Scope:** only intercepts traffic to known AI API domains (allowlist). Does NOT proxy general web traffic.
- **Privacy:** payload inspection happens locally on the dev machine. Only anonymized metadata sent to brain.

## ADRs

### ADR-001: Rust for both daemon and brain
- **Decision:** use Rust for the daemon AND the brain server
- **Rationale:** single language = shared code (kerios-core crate for config merge, policy engine). Rust daemon gives security credibility with CISOs. Brain doesn't need to be Rust, but code sharing justifies it.
- **Alternatives:** brain in TypeScript/Go (faster dashboard dev) — rejected because config merge logic would be duplicated
- **Trade-offs:** slower dashboard UI development in Rust. Mitigate with embedded SPA or htmx.

### ADR-002: SQLite for local cache, PostgreSQL for brain
- **Decision:** SQLite on daemon, PostgreSQL on brain
- **Rationale:** daemon needs zero-config local storage for metrics buffer + config cache. PostgreSQL gives the brain proper indexing, JSON support, and scales to 1000+ orgs.
- **Alternatives:** daemon with no local DB (pure filesystem cache) — rejected because metrics buffering needs atomic writes and queuing
- **Trade-offs:** SQLite adds ~1MB to daemon binary. Acceptable.

### ADR-003: Feature-flag paid features at compile time
- **Decision:** paid features (proxy, policy engine, metrics) are behind Cargo feature flags. OSS repo has the feature disabled. Paid repo enables it.
- **Rationale:** single codebase, two builds. No runtime license checking for the OSS build (keeps it truly open). Paid build adds the features at compile time.
- **Alternatives:** runtime license check (single binary, feature unlocked by key) — considered for later. Compile-time is simpler for MVP and prevents reverse-engineering.
- **Trade-offs:** two CI pipelines (OSS + paid). Two binary artifacts. Acceptable complexity.

### ADR-004: Config layers merged locally on daemon
- **Decision:** the brain sends raw layers (org, team, user) and the daemon merges them locally
- **Rationale:** merge logic needs to be deterministic and testable. Local merge means the brain is a simple config store, not a merge engine. Also enables offline mode (daemon re-merges from cache).
- **Alternatives:** brain sends pre-merged config per machine — rejected because it couples brain to merge logic and breaks offline mode
- **Trade-offs:** merge logic must be identical across daemon versions. Versioned merge protocol mitigates this.

### ADR-005: Proxy intercepts only AI API domains (allowlist)
- **Decision:** the TLS proxy only intercepts traffic to a maintained allowlist of AI API domains (api.anthropic.com, api.openai.com, etc.)
- **Rationale:** intercepting all HTTPS traffic is a corporate proxy (Zscaler territory). Kerios is NOT a general proxy — it's AI-specific. Allowlist reduces risk, complexity, and privacy concerns.
- **Alternatives:** intercept all traffic, filter by domain — rejected because it's invasive and the CA cert scope would alarm developers
- **Trade-offs:** new AI providers require an allowlist update. Daemon auto-updates handle this.

### ADR-006: Anonymization happens on the daemon, not the brain
- **Decision:** metric events are anonymized (path → pattern, no content) on the developer's machine before being sent to the brain
- **Rationale:** privacy by design. File contents NEVER leave the machine. The brain only sees metadata. This is a key trust differentiator vs surveillance tools.
- **Alternatives:** send raw data, anonymize server-side — rejected because it violates the privacy promise and creates a liability
- **Trade-offs:** less flexibility for server-side analysis. But anonymized metadata (file type, path pattern, sensitive flag, action taken) is sufficient for dashboards.
