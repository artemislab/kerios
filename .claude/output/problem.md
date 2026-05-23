# Problem Definition: Kerios
**Version:** 1.1
**Phase:** break

## WHY — Business Case

### The Problem in One Sentence
Every enterprise deploying AI coding tools (Claude Code, Codex, Copilot) has zero visibility into what their developers send to LLMs and zero control over how these tools are configured — Kerios fixes both.

### Why Now
- **AI coding tools are exploding:** Anthropic, OpenAI, GitHub are all pushing enterprise adoption aggressively. 70%+ of Fortune 500 have at least one AI coding tool deployed (2025-2026).
- **CISOs are panicking:** Existing DLP tools (Netskope, Zscaler) don't cover AI coding assistants — these tools run locally, not through the corporate proxy. It's a blind spot.
- **No one owns this space:** Lakera/Prompt Security focus on LLM API security (prompt injection, output filtering). Nobody does agent governance + file-level DLP for coding assistants. The gap is wide open.
- **Provider lock-in is growing:** Companies use Claude AND Codex AND Copilot. Each has its own config format. No one standardizes across providers. This pain gets worse every quarter.

### WHY — The Three Pillars

**1. Sécurité — Empêcher les fuites avant qu'elles arrivent**
Les devs envoient du code propriétaire, des clés API, des .env, des secrets à l'IA tous les jours. Sans le savoir, sans le vouloir. Un seul .env leaké dans un prompt = credentials en clair chez un provider tiers. Les DLP traditionnels (Netskope, Zscaler) ne voient pas ce trafic — les agents IA tournent en local, hors du réseau. Kerios est le seul outil qui intercepte ça au niveau machine, avant que le fichier quitte le poste.

**2. Contrôle — Standardiser et gouverner l'usage IA**
200 devs, 3 providers différents, zéro cohérence. Chacun configure ses propres rules, ses propres skills, ses propres prompts. Un junior envoie du code sans review, un senior a des guardrails custom que personne d'autre n'a. Kerios impose une politique org-wide : mêmes rules de sécurité, mêmes skills validés, mêmes agents approuvés — poussés automatiquement sur chaque poste. Config-as-Code, comme Terraform pour l'infra.

**3. Visibilité — Comprendre comment vos équipes utilisent l'IA**
Aujourd'hui le CTO ne peut pas répondre à : "Est-ce que notre investissement IA rend nos devs plus productifs ?" Kerios analyse les conversations IA de manière anonymisée pour répondre aux vraies questions :
- **Rendement** : qui utilise l'IA activement vs qui l'ignore ? Quelles équipes en tirent le plus de valeur ?
- **Bien-être** : un dev qui envoie des messages frustrants ou qui n'utilise plus l'IA du tout — signal d'alerte pour le manager
- **Patterns** : quels types de tâches sont délégués à l'IA ? Debug ? Architecture ? Tests ? Ça révèle les gaps de compétences
- **Abus** : usage personnel excessif vs travail réel ? Envoi d'infos personnelles (CV, problèmes perso) vs code ?

Ce n'est pas de la surveillance — c'est de l'analytics agrégé. Le manager voit des tendances par équipe, pas les conversations individuelles. Mais le CISO peut drill-down sur les alertes sécurité.

### Revenue Model
- **Free (OSS):** Config sync daemon. Unlimited machines. No dashboard, no blocking.
- **Paid (per-seat):** $8-15/dev/month. File blocking, policy engine, anonymized metrics to central brain, CISO dashboard, compliance reports.
- **Enterprise:** Custom pricing. Self-hosted brain, SSO, audit logs, SLA, dedicated support.

### Go-to-Market
- **Distribution:** sell through IT resellers/VARs targeting medium enterprises (100-300 devs). Resellers already have relationships with IT directors.
- **Land:** free OSS adoption (dev installs it for better configs) → IT notices → CISO asks "what is this?" → upgrade conversation.
- **Expand:** per-seat pricing scales with company. Start with one team, expand to org.

### 12-Month Competitive Window
Anthropic and GitHub WILL add basic admin controls to their enterprise tiers. Kerios's moat is:
1. **Provider-agnostic** (they'll only manage their own tool)
2. **OSS community** (switching cost once adopted)
3. **DLP depth** (file-level blocking, not just admin toggles)

The race: get to 500+ companies on free tier and 50+ paying customers before the big players ship native admin features.

### Competitive Landscape

**Direct competitors (AI security for dev tools):**
- **Prompt Security** — closest competitor. Has "AI Code Assistants" product + MCP Gateway. BUT focused on IDE (Copilot in VS Code), not CLI agents (Claude Code, Codex). API proxy model, not local daemon.
- **Nightfall AI** — strong DLP for AI. Blocks files/secrets sent to AI apps. BUT covers web apps (Claude.ai, Copilot web), not CLI/terminal tools. Endpoint agents exist but for browser/SaaS, not coding CLIs.
- **Lakera** — runtime guardrails + workforce DLP. Mentions "IDEs" vaguely but no dedicated coding assistant product. API-first, latency <50ms.

**Adjacent (not direct competitors):**
- **Protect AI** — ML model supply chain security (scan PyTorch/ONNX models for backdoors). Zero overlap with agent governance.
- **Arthur AI** — AI observability + evals. Agent discovery feature exists but focused on deployed ML models, not dev tools. Public pricing: $0/$60/custom.
- **Calypso AI (F5)** — Acquired by F5. AI gateway/proxy in F5 network infra. No coding assistant coverage.
- **Robust Intelligence (Cisco)** — Acquired. Model validation, absorbed into Cisco AI Defense.

**The gap Kerios fills:**
No vendor covers the intersection of:
1. **CLI AI agents** (Claude Code terminal, Codex CLI, MCP agents) — not just IDE plugins or web apps
2. **Config standardization** across providers (Puppet for AI tools) — nobody does this at all
3. **Provider-agnostic** governance — each vendor partners with specific providers, none spans all
4. **Lightweight local daemon** — competitors use API proxies or browser extensions, not machine-level agents

**Pricing benchmarks:**
- GitGuardian (secret detection): ~$40/dev/month
- Snyk (code security): ~$25/dev/month
- Arthur AI: $60/month (not per-seat)
- Kerios target: $8-15/dev/month (undercuts security tools, accessible for mid-market)

### Risks
- **Platform dependency:** if Claude Code changes `~/.claude/` format, daemon breaks → mitigate with version-pinned adapters and rapid update cycle
- **Enterprise sales cycle:** 3-6 months for paid tier → mitigate with reseller channel and free-tier-to-paid conversion
- **Build vs Buy:** CISO might wait for native Copilot/Claude admin features → mitigate by being live now, cross-provider, and deeper
- **Prompt Security moves to CLI:** they're the closest — if they add a local daemon, they become direct competition → mitigate by shipping first and having OSS community lock-in

## Problem Statement

### Summary
Kerios is a lightweight Rust daemon that governs AI coding assistants (Claude Code, Codex, Copilot) in enterprise environments — syncing agent configurations across teams (free/OSS) and monitoring what developers send to AI for security compliance (paid). The daemon is dynamic — it pulls its config and policies from a central server, so IT can update behavior in real-time.

### Target Users
- **IT Admin / DevOps** — deploys Kerios across 100-300 developer machines, configures roles and policies
- **CISO / Security Lead** — needs visibility into what code/data employees send to AI, blocks sensitive leaks
- **Engineering Manager** — wants to understand how teams use AI, track ROI, standardize tools
- **Developer** — end-user whose AI tools are configured and monitored by Kerios (mostly transparent)
- **IT Reseller / VAR** — sells Kerios as part of their managed IT services to medium enterprises

### Pain Points
- **No visibility:** enterprises deploy Claude/Codex to 200 devs and have zero insight into what files, secrets, or proprietary code gets sent to LLMs
- **No standardization:** each dev configures their own AI tools — different skills, different rules, inconsistent quality. New hires start from scratch
- **Secret leakage:** API keys, .env files, credentials end up in LLM context with no guardrails
- **Compliance gap:** SOC2/GDPR require data governance, but AI tools operate outside existing DLP pipelines
- **No ROI tracking:** CTO can't answer "is our $50K/year AI spend actually helping?"
- **Multi-provider chaos:** company uses Claude + Codex + Copilot, each configured differently, no single pane of glass

## Tech Stack
- **Language:** Rust
- **Daemon:** tokio async runtime, lightweight (<10MB RSS)
- **Config format:** TOML for daemon config, YAML/MD for agent configs (same format as Claude Code .claude/, Codex .codex/)
- **Storage (local):** SQLite for metrics buffer + file cache
- **Storage (centralized):** PostgreSQL for the brain/dashboard backend
- **API:** axum HTTP server (admin API + metrics ingestion)
- **Dashboard:** Web UI (lightweight — Vite + React or just server-rendered HTML)
- **Distribution:** single binary (cargo install / brew / .deb / .rpm / MSI)
- **License:** GPL-3.0 (OSS core), private repo for paid features (feature-flagged at compile time)
- **CI/CD:** GitHub Actions

## Features

### Feature: Config Sync Daemon (Free/OSS)
**Priority:** P1
**Description:** A Rust daemon that runs on developer machines, pulls agent configurations (skills, rules, agents, tools) from a central source (git repo or HTTP endpoint) and writes them to the correct locations for each AI provider (~/.claude/, .codex/, etc.).

**Acceptance Criteria:**
- Daemon starts on boot, runs in background with <10MB memory
- Pulls config from a central server (HTTP API) or git repo as fallback
- Server tells the daemon WHAT to sync and HOW to behave (dynamic — no daemon update needed for policy changes)
- Writes configs to provider-specific paths (~/.claude/skills/, ~/.claude/rules/, ~/.claude/agents/, .codex/ equivalents)
- Supports layered config: org-wide base + team-specific overrides + role-based overrides + user-specific overrides
- Config changes are applied without restarting the AI tool
- Daemon exposes a local health endpoint (localhost:PORT/health)
- Works offline: caches last config, applies cached version if server unreachable

#### User Stories

##### US-001
- **As a:** IT Admin
- **I want:** deploy Kerios to all developer machines with a single command/package
- **So that:** every dev gets standardized AI tool configs without manual setup, reducing onboarding from hours to minutes
- **Priority:** P1

**Acceptance Scenarios:**
1. **Given:** a fresh macOS/Linux machine with Claude Code installed
   **When:** IT runs `curl -sSf https://kerios.dev/install.sh | sh` with a config URL
   **Then:** the daemon installs, starts, pulls configs, and writes them to ~/.claude/ within 60 seconds

2. **Given:** a machine with Kerios already installed
   **When:** IT pushes a new skill to the config repo
   **Then:** the daemon picks up the change within the sync interval and writes the skill to ~/.claude/skills/

**Testability:** Integration test: install script on clean Docker container, verify files written. Unit test: config merge logic.

##### US-002
- **As a:** IT Admin
- **I want:** define layered configs (org → team → user) that merge predictably
- **So that:** the security team gets security-focused rules while the frontend team gets React-focused skills, and individual devs can add personal overrides without losing team defaults
- **Priority:** P1

**Acceptance Scenarios:**
1. **Given:** org config has rule "no-secrets" and team "frontend" config has skill "react-patterns"
   **When:** a frontend dev's Kerios syncs
   **Then:** their ~/.claude/ contains both "no-secrets" rule AND "react-patterns" skill

2. **Given:** org config has agent "code-reviewer" and a dev has a personal override for that agent
   **When:** Kerios syncs
   **Then:** the personal override wins for that agent, but all other org configs are preserved

**Testability:** Unit test: merge function with org/team/user layers. Property test: merge is associative and user layer always wins.

##### US-003
- **As a:** Developer
- **I want:** Kerios to be invisible — no popups, no extra commands, no slowdown
- **So that:** I get better AI configs without changing my workflow
- **Priority:** P1

**Acceptance Scenarios:**
1. **Given:** Kerios daemon is running
   **When:** the developer uses Claude Code normally
   **Then:** they see improved skills/rules but never interact with Kerios directly

2. **Given:** Kerios daemon crashes or loses network
   **When:** the developer uses Claude Code
   **Then:** existing configs remain in place (no deletion), and the daemon auto-restarts

**Testability:** Test: kill daemon, verify configs untouched. Test: daemon starts with no network, graceful degradation.

##### US-004
- **As a:** IT Admin
- **I want:** support for multiple AI providers (Claude Code, Codex, Copilot)
- **So that:** we can standardize configs regardless of which AI tool each team uses
- **Priority:** P2

**Acceptance Scenarios:**
1. **Given:** a config repo with a "security-rules" rule
   **When:** Kerios syncs on a machine with Claude Code AND Codex installed
   **Then:** the rule is written to both ~/.claude/rules/ and the Codex equivalent path

**Testability:** Unit test: provider path resolution. Integration test: verify files in both locations.

### Feature: Enterprise Deployment & Management
**Priority:** P1
**Description:** Tools for IT admins to deploy, configure, and manage Kerios across a fleet of developer machines.

**Acceptance Criteria:**
- MDM-compatible installation (silent install, config via environment variables or config file)
- Central config source (git repo with branch-per-team or directory-per-team structure)
- Fleet health dashboard: which machines have synced, last sync time, errors
- Config validation before push (CI check that configs are valid before merge)

#### User Stories

##### US-005
- **As a:** IT Admin
- **I want:** deploy Kerios via our MDM (Jamf/Intune) with a pre-configured endpoint
- **So that:** 200 machines get Kerios without touching each one
- **Priority:** P1

**Acceptance Scenarios:**
1. **Given:** a Kerios .pkg with embedded config URL
   **When:** Jamf pushes it to 200 Macs
   **Then:** each machine starts syncing within 5 minutes of install

**Testability:** Build .pkg with embedded config, install on clean VM, verify sync.

##### US-006
- **As a:** IT Admin
- **I want:** a git repo structure that maps teams to config directories
- **So that:** I manage AI configs the same way I manage infrastructure — as code, with PRs and reviews
- **Priority:** P1

**Acceptance Scenarios:**
1. **Given:** repo structure: `org/base/`, `teams/frontend/`, `teams/backend/`, `users/adrien/`
   **When:** Kerios daemon syncs for user "adrien" in team "frontend"
   **Then:** configs merged: org/base + teams/frontend + users/adrien

**Testability:** Unit test: directory discovery + merge. Integration test: clone test repo, verify merge output.

### Feature: Security Proxy / Request Monitor (Paid)
**Priority:** P2 (post-MVP, feature-flagged)
**Description:** The daemon acts as a dual-layer security gate: filesystem watcher + local TLS proxy. It intercepts both file access by AI processes AND outbound HTTP requests to AI APIs, blocking sensitive content before it leaves the machine.

**Architecture:**
- **Layer 1 — Filesystem watcher:** monitors which files AI processes (claude, codex, copilot) read. Blocks access to forbidden paths before the file content enters the AI context.
- **Layer 2 — Local TLS proxy:** intercepts HTTPS requests to AI API endpoints (api.anthropic.com, api.openai.com, etc.). Inspects payloads for secrets, PII, and forbidden content. Covers CLI, browser, IDE, desktop apps — everything on the machine.
- **CA certificate:** installed by IT via MDM (standard corporate practice — same as Zscaler/Netskope). Required for TLS inspection.

**Acceptance Criteria:**
- Detects sensitive files before they're sent to AI (API keys, .env, credentials, certificates)
- **Path-based blocklist:** IT admin configures forbidden paths (e.g., `//fileserver/confidential/`, `/mnt/shared/hr/`, `~/Documents/contracts/`). ANY AI process attempting to read these paths is blocked. Applies to:
  - CLI agents reading files from context
  - Browser uploads to ChatGPT/Claude.ai/Copilot
  - IDE extensions sending file content
- **Pattern-based blocklist:** block by file pattern (`*.pem`, `*.env`, `*credentials*`, `*.pfx`) regardless of path
- **Content-based detection:** scan payload for secrets (regex + entropy detection), PII (emails, SSN, credit cards), and custom patterns defined by IT
- Configurable action per rule: **block** (hard stop) / **warn** (log + allow) / **redact** (strip sensitive content, forward the rest)
- Developer sees a clear message when content is blocked: "Kerios: file /mnt/shared/hr/salaries.xlsx blocked by policy — contact IT"
- All interceptions logged locally and sent to central brain (anonymized metadata only)
- Feature activates only with a valid paid license key
- Works offline: blocks based on cached policies, queues logs for later sync

#### User Stories

##### US-007
- **As a:** CISO
- **I want:** developers to be blocked from sending .env files and private keys to AI
- **So that:** we prevent credential leaks that could lead to a security incident
- **Priority:** P2

**Acceptance Scenarios:**
1. **Given:** policy blocks `*.env`, `*.pem`, `*credentials*`
   **When:** a developer's Claude Code session includes `.env` in context
   **Then:** Kerios blocks the file, shows the dev a message "Blocked by security policy: .env files cannot be sent to AI", and logs the event

2. **Given:** policy is set to "warn" for `*.sql` files
   **When:** a developer sends a SQL file to AI
   **Then:** Kerios allows it but logs a warning event to the central brain

**Testability:** Unit test: pattern matching engine. Integration test: mock AI request with blocked file, verify block + log.

##### US-011
- **As a:** IT Admin
- **I want:** block AI tools from accessing specific network paths (shared drives, confidential folders)
- **So that:** sensitive documents (HR files, financial reports, contracts on the file server) never enter an AI context, even accidentally
- **Priority:** P2

**Acceptance Scenarios:**
1. **Given:** policy blocks path `//fileserver/hr/` and `~/Documents/contracts/`
   **When:** a developer's Claude Code tries to read `//fileserver/hr/salaries.xlsx`
   **Then:** the file is blocked, the dev sees "Blocked by policy: this path is restricted", and the event is logged

2. **Given:** policy blocks `*.env` files globally
   **When:** a developer uploads `.env.production` to ChatGPT via browser
   **Then:** the proxy intercepts the upload, blocks it, shows a browser notification, and logs the event

3. **Given:** policy is set to "redact" for `*.sql` files
   **When:** a developer sends a SQL dump to Claude Code
   **Then:** Kerios strips email/phone patterns from the content before forwarding to the AI

**Testability:** Unit test: path matching engine (glob + exact). Integration test: mock AI process reads blocked file, verify block. Proxy test: upload with blocked pattern, verify interception.

##### US-012
- **As a:** CISO
- **I want:** a single policy engine that covers CLI agents, browser uploads, and IDE extensions
- **So that:** I define one blocklist and it applies everywhere — not a different tool per vector
- **Priority:** P2

**Acceptance Scenarios:**
1. **Given:** policy blocks `*credentials*` pattern
   **When:** a dev sends credentials.json via Claude Code CLI, AND another dev uploads credentials.yaml via ChatGPT browser
   **Then:** both are blocked by the same policy rule, both logged as the same event type

**Testability:** Integration test: trigger block via filesystem watcher + proxy in same test, verify same policy applies.

##### US-008
- **As a:** CISO
- **I want:** see a dashboard of what files and data types are being sent to AI across all developers
- **So that:** I can identify risky patterns and adjust policies before an incident happens
- **Priority:** P2

**Acceptance Scenarios:**
1. **Given:** 50 developers are using Kerios with the paid tier
   **When:** the CISO opens the dashboard
   **Then:** they see: top files sent to AI (by frequency), blocked events, sensitive content detections, usage by team, usage over time

**Testability:** Seed metrics database with sample data, verify dashboard renders correct aggregations.

##### US-009
- **As an:** Engineering Manager
- **I want:** see AI usage metrics per team (requests/day, files shared, tools used)
- **So that:** I can justify our AI tool spend and identify teams that need more training
- **Priority:** P2

**Acceptance Scenarios:**
1. **Given:** team "backend" has 10 devs using Claude Code
   **When:** the manager views the team dashboard
   **Then:** they see: avg requests/day, most used skills, most shared file types, blocked events

**Testability:** API test: metrics aggregation endpoint with test data.

### Feature: Centralized Brain / Metrics Backend
**Priority:** P2
**Description:** Server-side component that receives anonymized metrics from Kerios daemons, stores them, and serves dashboards.

**Acceptance Criteria:**
- Receives metrics via HTTPS from daemons (authenticated with org API key)
- Stores in PostgreSQL with per-org isolation
- Serves dashboard UI for CISO/manager personas
- Metrics are anonymized by default (no file contents, only patterns and metadata)
- Retention policies configurable per org

#### User Stories

##### US-010
- **As a:** CISO
- **I want:** metrics to be anonymized — I need to see patterns, not read my developers' code
- **So that:** Kerios doesn't become a surveillance tool that destroys developer trust
- **Priority:** P2

**Acceptance Scenarios:**
1. **Given:** a developer sends `src/auth/jwt.rs` to Claude Code
   **When:** the metric is recorded
   **Then:** the brain receives: `{file_type: "rs", path_pattern: "src/auth/*", contains_sensitive: false}` — NOT the file content

**Testability:** Unit test: anonymization function strips content, preserves metadata.

## Constraints
- **Performance:** daemon must use <10MB RSS, <1% CPU when idle, sync in <5s
- **Security:** daemon binary must be signed. Communication to brain over mTLS. No file contents sent to brain (metadata only).
- **Compliance:** SOC2-ready architecture (audit logs, access controls, data retention). GDPR-compatible (anonymization, data deletion).
- **Compatibility:** macOS (Apple Silicon + Intel), Linux (x86_64 + arm64). Windows P2.
- **License:** GPL-3.0 for OSS core. Paid features in a separate private repo, compiled as a feature flag.

## Integrations

### AI Providers (config targets)
- **Claude Code:** ~/.claude/ (skills/, rules/, agents/, CLAUDE.md, settings.json)
- **Codex CLI:** ~/.codex/ (instructions, agents)
- **Copilot:** (P2) ~/.config/github-copilot/
- **Purpose:** write standardized configs to each provider's expected paths

### Config Source
- **Git repo:** primary config source (org manages a repo with team/role directories)
- **HTTP endpoint:** alternative for orgs that don't want git (JSON/TOML API)
- **Purpose:** pull configs from central source of truth

### Centralized Brain (paid)
- **Type:** HTTPS REST API
- **Purpose:** receive anonymized metrics from daemons, serve dashboard
- **Auth:** org API key per daemon, JWT for dashboard users

## Non-Functional Requirements
- **Scalability:** brain must handle 1000+ daemons reporting metrics every 5 minutes
- **Availability:** daemon must work offline (queue metrics, sync when network returns)
- **Observability:** daemon logs to journald/syslog, exposes /health and /metrics endpoints
- **Upgrades:** daemon auto-updates from a release channel (or IT-managed pinned version)
- **Privacy:** no telemetry in OSS version. Paid version sends only what the org explicitly configures.
