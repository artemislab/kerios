#!/usr/bin/env bash
# End-to-end demo: bring up the compose stack, watch the agents converge,
# then push a new role to the git repo and watch them re-sync.

set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
COMPOSE="docker compose -f $HERE/compose.yaml"
REPO="$HERE/gitserver/repos/configs.git"

note() { printf '\n\033[1;36m=== %s ===\033[0m\n' "$*"; }

list_claude() {
    local container=$1
    docker exec "$container" sh -c 'find /root/.claude -type f 2>/dev/null | sort' || true
}

cat_status() {
    local container=$1
    docker exec "$container" kerios status 2>&1 || true
}

note "1) seed the bare repo"
"$HERE/seed-repo.sh"

note "2) bring the stack up"
$COMPOSE up -d --build

note "3) wait 10 s — agents should complete their first sync (interval_secs=5)"
sleep 10

for who in alice bob charlie; do
    note "4) state on kerios-demo-$who"
    cat_status "kerios-demo-$who"
    echo
    echo "files written to /root/.claude/:"
    list_claude "kerios-demo-$who"
done

note "5) push a brand-new org rule to the repo"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
git clone -q "$REPO" "$WORK/configs"
cd "$WORK/configs"
cat > org/claude/agents/incident-response.md <<'EOF'
You are the incident response coach. Help the on-call walk through
detection, scope, mitigation, comms, post-mortem.
EOF
git add .
git -c user.email=demo@kerios -c user.name=demo commit -qm "feat: add incident-response agent"
git push -q origin main

note "6) wait 8 s — agents re-pull"
sleep 8

for who in alice bob charlie; do
    echo
    echo "[$who] $(docker exec kerios-demo-$who ls /root/.claude/agents/ | tr '\n' ' ')"
done

note "7) simulate drift: alice hand-edits org/claude/rules/style.md"
docker exec kerios-demo-alice sh -c 'echo "LOCAL HACK by alice" > /root/.claude/rules/style.md'
docker exec kerios-demo-alice cat /root/.claude/rules/style.md

note "8) wait 8 s — alice's next sync should detect drift + restore"
sleep 8

echo
echo "[alice] state.toml after drift cycle:"
docker exec kerios-demo-alice cat /root/.kerios/state.toml
echo
echo "[alice] daemon log (last 20 lines):"
docker logs --tail 20 kerios-demo-alice
echo
echo "[alice] rules/style.md restored?"
docker exec kerios-demo-alice cat /root/.claude/rules/style.md

note "Demo done — `$COMPOSE down -v` to clean up"
