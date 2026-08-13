---
name: webclx-codex-api-terminal-ops
description: Use when working on webClx Codex/Claude APIs, terminals, presets, proxies, or UI deployment drift.
metadata:
  short-description: Work on webClx Codex/API presets and terminal session UI
---

# webClx Codex/API Terminal Ops

Use this skill for `webClx` issues around Codex/Claude API presets, the in-browser terminal, resume archives, local proxy behavior, mobile terminal controls, or deployed static files.

## First Reads

From `/home/codes/webClx`, read before broad searching:

```bash
sed -n '1,220p' AGENTS.md
sed -n '1,260p' docs/codex/index.md
```

Then choose the matching section in `docs/codex/index.md`:

- `Mobile Terminal Keyboard And Height`
- `Terminal Rich Paste`
- `Static Deployment Sync`
- `User Identity, HOME, And Codex Config`
- `Terminal Quick Commands`
- `Remote Preset Sync`
- `Terminal Resume Archives`
- `Terminal Session Switching And Scroll Restoration`

## Stable Rules

- Running service static assets are usually read from `/home/bin/webclx/static/*`, not directly from repo `static/*`.
- Backend changes require rebuilding and reinstalling `/home/bin/webclx/webClx`; frontend changes require syncing deployed static files.
- Confirm Cargo output with `cargo metadata --format-version 1 --no-deps`; do not assume `./target/release`.
- User identity controls terminal Linux user, HOME, shell, and Codex/Claude config paths. Resolve HOME from the system user database; do not hardcode `/root`, `/home/root`, or `/home/beyondcy`.
- Terminal quick commands are `program` plus raw `args`; do not store a full shell command in `program`.
- Codex_API env/startup snippets must end with a shell separator so auto-start commands do not concatenate with exports.
- Local proxy preset switching must not stop a proxy that already-running Codex/Claude sessions still depend on.
- For resume archives, preserve `cwd`; `codex resume <id>` must run from the original project directory or a newly created terminal for that directory.
- Do not print API keys, OAuth tokens, or copied preset secrets in reports. Mask or summarize.

## Common Files

- `static/app.js`, `static/index.html`, `static/styles.css`
- `static/terminal.js`, `static/terminal.html`
- `src/main.rs`, `src/terminal.rs`, `src/system.rs`, `src/preset_sync.rs`
- `src/terminal/manager.rs`
- `crates/settings_core/src/lib.rs`
- `crates/terminal_core/src/lib.rs`

## Debug Workflow

1. Reproduce against the actual running service when the user reports UI behavior.
2. Check whether the running binary/static bundle is stale before changing logic:

```bash
systemctl cat webclx.service
cargo metadata --format-version 1 --no-deps | jq -r '.target_directory'
node --check static/app.js
```

3. For preset/API issues, separate these layers:

- saved webClx settings and preset table
- generated Codex/Claude config file
- terminal user HOME and environment
- local proxy process state
- already-running terminal sessions versus newly-started sessions

4. For terminal display/copy/paste/cursor issues, prefer browser or Playwright geometry/state checks over CSS guessing.
5. After edits, run the narrowest relevant checks, then deploy local service assets if the user expects the live UI to change.

## Local Deploy Pattern

Use the project-documented path and verify live target directory:

```bash
cargo build --release
TARGET_DIR=$(cargo metadata --format-version 1 --no-deps | jq -r '.target_directory')
install -m 0755 "$TARGET_DIR/release/webclx" /home/bin/webclx/webClx
rsync -a --delete static/ /home/bin/webclx/static/
systemctl restart webclx.service
```

If only static files changed, syncing `static/` and restarting may still be useful to avoid browser/server mismatch.

## Verification

Use checks that match the change:

```bash
node --check static/app.js
curl -sS http://127.0.0.1:11111/api/update/check | jq .
curl -sS http://127.0.0.1:11111/api/settings | jq 'keys'
```

For terminal UI changes, use Playwright or browser geometry checks and inspect the deployed `/home/bin/webclx/static/*` copy if the page still shows old behavior.

## Documentation

When a reusable conclusion is found, update `docs/codex/index.md` or the matching `docs/codex/tasks/*.md` topic. Keep the note short: symptom, root cause, files, verification, and deployment caveat.
