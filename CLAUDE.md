# Kerios — Agent Governance Platform

## What is Kerios?

A lightweight Rust daemon that syncs AI coding assistant configs (Claude Code, Codex, Copilot) across an engineering team — like Puppet, but for AI tools.

## Quick Reference

```bash
# Build
cargo build

# Test
cargo test

# Lint
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

## Crate Structure

```
kerios-core/   — config merge, policy engine, provider adapters (shared library)
kerios/        — the single dev-machine binary; subcommands: daemon, status, sync, validate
```

**One binary per dev machine.** The `kerios` crate produces a single executable that bundles the daemon (`kerios daemon`) and admin CLI (`kerios status / sync / validate`). Modeled after `tailscale` / `rustup` / `kubectl`. See `docs/architecture.md` for diagrams.

## Architecture

See `.claude/output/architecture.md` for full architecture, data model, API surface, ADRs.
See `.claude/output/backlog.md` for implementation tasks (18 tasks, prioritized).
See `.claude/output/problem.md` for problem definition, WHY, competitive landscape.

## Key Decisions

- **Rust everywhere** — daemon + CLI in one binary. Shared kerios-core crate.
- **Config merge happens on daemon** — fetches raw layers from your config source, merges locally. Enables offline mode.

## Testing Philosophy (non-negotiable)

- **TDD strict** for all code stories — RED → GREEN → REFACTOR. See `.claude/rules/testing.md`.
- **E2E tests must simulate a real user.** Real clicks in the real UI, real CLI subprocess invocations, real signals to the real daemon, real config files on a real filesystem. Never "spin the stack with env vars and curl the backend" and call it an E2E — that is an integration test wearing a costume.
- Each E2E test maps to a named user journey from the backlog / PRD. If no journey exists yet, write the journey first.
- Full rules: `.claude/rules/testing.md` → section "End-to-End Tests — Real User Simulation".

## Communication

- After every batch of edits, produce a short summary table: file path, role in the system, this change. Helps the reader hold the mental map across many edits.
- See `.claude/rules/post-edit-summary.md` for the rule.

## Previous Work (artybot)

This project is a pivot from ~/workspace/claw/artybot (Kerios v1 — AI team orchestration platform).
Reusable from artybot: Rust expertise, axum patterns, tokio runtime, provider abstractions.
NOT reused: the agent runtime, dashboard components, multi-team orchestration. Clean start.

## Git

- Conventional commits: `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`
- Never push without being asked
- Never amend unless explicitly asked
