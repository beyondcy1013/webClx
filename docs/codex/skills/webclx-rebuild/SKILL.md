---
name: webclx-rebuild
description: "Rebuild webClx from source, deploy release binary and static files, restart systemd service. Use when the user asks to recompile, rebuild, deploy, update, or restart webClx, or says 重新编译/重新构建/部署/更新 webClx."
---

# webClx Rebuild & Deploy

Project-local fallback for direct webClx rebuilds when the compile API is unavailable or the task is specifically about the build/deploy script itself. Rebuild the webClx Rust backend, install the release binary, sync frontend static files, and restart the systemd service.

## Codex Deploy API

When Codex needs to deploy this project during a webClx terminal conversation, prefer the HTTP deploy queue instead of running the rebuild script directly:

```bash
bash /home/root/.codex/skills/webclx-compile-api/scripts/request-webclx-deploy-api.sh \
  --source-terminal-name "<current webClx terminal name>" \
  --project webClx \
  --project-dir /home/codes/webClx \
  --command 'cargo build --release' \
  --install-command 'bash scripts/rebuild-and-deploy.sh --skip-build' \
  --audit-path /home/bin/webclx/webClx \
  --audit-path /home/bin/webclx/static \
  --note "Codex 请求部署 webClx，并将在收到回调后继续原任务。"
```

If the terminal environment has `WEBCLX_TERMINAL_NAME`, the source terminal name can be omitted:

```bash
bash /home/root/.codex/skills/webclx-compile-api/scripts/request-webclx-deploy-api.sh \
  --project webClx \
  --project-dir /home/codes/webClx \
  --command 'cargo build --release' \
  --install-command 'bash scripts/rebuild-and-deploy.sh --skip-build' \
  --audit-path /home/bin/webclx/webClx \
  --audit-path /home/bin/webclx/static \
  --note "Codex 请求部署 webClx，并将在收到回调后继续原任务。"
```

Do not use compatibility aliases such as `--target` or `--path`; compile requests use `source_terminal_name` for the requesting terminal and `project_path` for the project/workspace label.
If the current shell lacks `WEBCLX_TERMINAL_ID` and `WEBCLX_TERMINAL_NAME`, the wrapper can resolve the current webClx-managed tmux session name `webclx_s<number>` through `/api/terminal/sessions?all=true` and still submit the terminal name. The server persists `source_terminal_id`, `source_terminal_name`, and `source_tmux_session`; completion notification prefers the stable id, then the tmux-derived id, then the terminal name.
Use Chinese `--note` text. The wrapper normalizes known legacy English notes to Chinese before submitting the request.

After a successful queue response, do not poll or begin result-dependent verification. Independent work may continue; the worker sends both a browser-level toast and a completion prompt to the source terminal, so a reconnecting or hung model turn cannot hide build completion from the user. The `/api/terminal/sessions/message` prompt remains the join point for deployment verification. Compile logs and deploy audit reports live under the webClx-owned `compile/runs/<run-id>/` directory (below the app dir), never in the client project's source tree. Callbacks include their absolute paths and a compact file-difference summary.

If an older worker left `webclx-build-*` or `webclx-install-report-*` files below client `docs/logs` directories, run `bash scripts/migrate-compile-api-logs.sh` first, inspect the dry-run totals, then rerun with `--apply`. The script archives them below `compile/legacy/`, updates retained run path references, and writes a SHA-256 manifest. Do not move those files by hand.

Status:

```bash
curl -fsS http://127.0.0.1:11111/api/build/compile/status | jq .
```

Requests with the same complete build specification are continuously coalesced
while waiting for a shared Cargo target or other build resource. One waiting
owner absorbs later matching requests immediately before execution, so the
command runs once against the latest workspace state and each original request
still receives its own callback. A command that is already running is not
cancelled. Requests with different commands, environments, install parameters,
audit paths, or required artifacts remain independent.

## Quick Start

Run the bundled script for a one-command rebuild:

```bash
bash scripts/rebuild-and-deploy.sh
```

Options:

- `--skip-build` — skip `cargo build --release`; use this when the deploy API already ran the compile command
- `--port <port>` — locate the running webClx process on a non-default port

## Manual Steps

If not using the script, follow these steps in order:

1. Determine Cargo target directory:
   ```bash
   cd /home/codes/webClx
   TARGET_DIR=$(cargo metadata --no-deps --format-version 1 | jq -r '.target_directory')
   ```

2. Build release binary:
   ```bash
   cargo build --release
   ```

3. Install binary to deployment directory:
   ```bash
   install -m 0755 "$TARGET_DIR/release/webclx" /home/bin/webclx/webClx
   ```

4. Sync static files (frontend changes will not take effect without this):
   ```bash
   cp -a /home/codes/webClx/static/ /home/bin/webclx/static/
   ```

5. Restart service and verify:
   ```bash
   systemctl restart webclx.service
   systemctl status webclx.service --no-pager -l
   ```

## Key Paths

| Item | Path |
|------|------|
| Source repo | `/home/codes/webClx` |
| Cargo target | Determined dynamically via `cargo metadata` (currently `/home/codes/webClx/target`) |
| Deploy binary | `/home/bin/webclx/webClx` |
| Deploy static | `/home/bin/webclx/static/` |
| Service | `webclx.service` |

## Pitfalls

- The running service reads static files from `/home/bin/webclx/static/`, not the repo `static/` directory. Only modifying repo files without syncing will have no visible effect.
- Do not hardcode `./target/release/webclx`; always resolve `target_directory` from `cargo metadata` because `~/.cargo/config.toml` can redirect it.
- After frontend-only changes, `--skip-build` is acceptable, but skipping the script entirely is wrong because the sync step is what makes static changes visible.
