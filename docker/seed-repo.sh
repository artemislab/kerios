#!/usr/bin/env bash
# Build the initial bare git repo that the git-daemon will serve.
# Run before `docker compose up`.

set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$HERE/gitserver/repos"
WORK="$HERE/.seed-work"

rm -rf "$ROOT/configs.git" "$WORK"
mkdir -p "$ROOT" "$WORK"

cd "$WORK"
git init -q -b main

# Org-wide rules: everyone gets this
mkdir -p org/claude/agents org/claude/rules org/codex
cat > org/claude/agents/security.md <<'EOF'
You are the security reviewer.
Refuse to look at .env, *.pem, *.key.
EOF
cat > org/claude/rules/style.md <<'EOF'
- Use snake_case in Python, camelCase in TS.
- 2-space indent for YAML, 4 for Python.
EOF
cat > org/codex/config.toml <<'EOF'
model = "o1"
EOF

# Backend team: extra backend-specific agents
mkdir -p teams/backend/claude/agents
cat > teams/backend/claude/agents/api-reviewer.md <<'EOF'
You are the backend API reviewer.
Check REST conventions, status codes, error envelopes.
EOF

# Frontend team: extra frontend-specific agents
mkdir -p teams/frontend/claude/agents
cat > teams/frontend/claude/agents/a11y.md <<'EOF'
You are the accessibility reviewer.
Check WCAG AA contrast, keyboard nav, ARIA roles.
EOF

# Per-user overrides
mkdir -p users/alice/claude/agents
cat > users/alice/claude/agents/me.md <<'EOF'
Alice's personal agent: pedantic about test coverage.
EOF

mkdir -p users/bob/claude/agents
cat > users/bob/claude/agents/me.md <<'EOF'
Bob's personal agent: pedantic about migrations.
EOF

git add .
git -c user.email=demo@kerios -c user.name=demo commit -qm "seed: org rules + backend / frontend teams + alice / bob users"

# Push to a bare repo that git-daemon will serve.
git clone --bare -q "$WORK" "$ROOT/configs.git"
echo true > "$ROOT/configs.git/git-daemon-export-ok"

cd "$ROOT"
rm -rf "$WORK"
commits=$(git --git-dir="$ROOT/configs.git" rev-list --count HEAD)
files=$(git --git-dir="$ROOT/configs.git" ls-tree -r HEAD | wc -l | tr -d ' ')
echo "seeded $ROOT/configs.git ($commits commits, $files files)"
