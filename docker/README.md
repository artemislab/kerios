# Kerios demo — docker compose

A self-contained, end-to-end demo that proves the OSS daemon does what
the architecture doc claims:

1. **One git "source"** — a small `git-daemon` container serving a bare
   repo on `git://gitserver/configs.git`.
2. **Three "agents"** — three independent `kerios daemon` containers,
   one per identity:
   - `kerios-demo-alice`   — team `backend`, user `alice`
   - `kerios-demo-bob`     — team `backend`, user `bob`
   - `kerios-demo-charlie` — team `frontend`, no user
3. **`demo.sh`** drives the scenarios:
   - first sync converges each agent to a different bundle
     (org + their team + maybe their user)
   - a new role pushed to the repo propagates to all three after a
     5-second tick
   - a hand-edit on one agent is detected as drift and overwritten

## Run it

```sh
docker compose -f docker/compose.yaml up --build       # in one shell
docker/demo.sh                                          # in another
```

When you are done:

```sh
docker compose -f docker/compose.yaml down -v
rm -rf docker/gitserver/repos docker/.seed-work
```

## What you should see

- alice has `org/...`, `teams/backend/...`, **and** `users/alice/me.md`
- bob has the same as alice but with `users/bob/me.md` instead of alice's
- charlie has `org/...` and `teams/frontend/...` (no user-layer files)
- After the new commit, all three pick up `incident-response.md` from
  `org/claude/agents/` within ~5 s
- After alice clobbers `rules/style.md` by hand, her next sync logs
  `drift detected … rules/style.md` and restores the upstream content
