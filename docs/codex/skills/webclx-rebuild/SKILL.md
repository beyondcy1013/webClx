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
  --project-dir /home/codes/tools/webClx \
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
  --project-dir /home/codes/tools/webClx \
  --command 'cargo build --release' \
  --install-command 'bash scripts/rebuild-and-deploy.sh --skip-build' \
  --audit-path /home/bin/webclx/webClx \
  --audit-path /home/bin/webclx/static \
  --note "Codex 请求部署 webClx，并将在收到回调后继续原任务。"
```

Do not use compatibility aliases such as `--target` or `--path`; compile requests use `source_terminal_name` for the requesting terminal and `project_path` for the project/workspace label.
If the current shell lacks `WEBCLX_TERMINAL_ID` and `WEBCLX_TERMINAL_NAME`, the wrapper can resolve the current webClx-managed tmux session name `webclx_s<number>` through `/api/terminal/sessions?all=true` and still submit the terminal name. The server persists `source_terminal_id`, `source_terminal_name`, and `source_tmux_session`; completion notification prefers the stable id, then the tmux-derived id, then the terminal name.
Use Chinese `--note` text. The wrapper normalizes known legacy English notes to Chinese before submitting the request.

After a successful queue response, do not poll or begin result-dependent verification. Independent work may continue; the worker sends both a browser-level toast and a completion prompt to the source terminal, so a reconnecting or hung model turn cannot hide build completion from the user. The completion callback remains the join point for deployment verification. Compile logs and deploy audit reports live under the webClx-owned `compile/runs/<run-id>/` directory (below the app dir), never in the client project's source tree. Callbacks include their absolute paths and a compact file-difference summary.

If an older worker left `webclx-build-*` or `webclx-install-report-*` files below client `docs/logs` directories, run `bash scripts/migrate-compile-api-logs.sh` first, inspect the dry-run totals, then rerun with `--apply`. The script archives them below `compile/legacy/`, updates retained run path references, and writes a SHA-256 manifest. Do not move those files by hand.

Status:

```bash
curl -fsS http://127.0.0.1:PORT/api/build/compile/status | jq .
```

### 单请求结果查询（无回调智能体）

不支持源终端回调的第三方智能体（在 webClx 终端体系之外提交请求）不能依赖回调消息。提交响应因此始终包含 `callback_supported`（仅当来源终端可解析时为 true）与 `query` 对象（`url` / `suggested_poll_seconds` / `note`）；当 `callback_supported=false` 时，调用方必须轮询 `query.url` 获取结果：

```bash
curl -fsS http://127.0.0.1:11111/api/build/compile/requests/<request_id> | jq .
```

响应字段：`state`（`queued|running|stalled|unknown|succeeded|failed|timed_out|not_found`）、`done`（true 后不再变化，可停止轮询）、`exit_codes`（run 内各 spec 的命令退出码）、`progress`（progress.json 最后上报的进度：阶段、命令、包计数，结束后保留最后已知值）、`log_tail`（最新集中日志的末尾行，默认 50，可用 `?tail_lines=N` 调节至 400，失败时通常直接包含报错内容）、`log_path`（日志来源绝对路径）、`request`（原始请求摘要）、`run`（含 `run_id`、目录 `dir_path`）。`not_found` 表示请求不存在或已超出保留期被清理。合并运行（见下）中每个原始 `request_id` 仍通过自己的请求文件解析到吸收它的 run，各自拿到独立结论。服务端对应的实现见 `src/compile_service.rs` 的 `compile_request_query` / `find_compile_request_in_queue` / `read_log_tail`。

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
   cd /home/codes/tools/webClx
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
   cp -a /home/codes/tools/webClx/static/ /home/bin/webclx/static/
   ```

5. Restart service and verify:
   ```bash
   systemctl restart webclx.service
   systemctl status webclx.service --no-pager -l
   ```

## Key Paths

| Item | Path |
|------|------|
| Source repo | `/home/codes/tools/webClx` |
| Cargo target | Determined dynamically via `cargo metadata` (currently `/home/codes/tools/webClx/target`) |
| Deploy binary | `/home/bin/webclx/webClx` |
| Deploy static | `/home/bin/webclx/static/` |
| Service | `webclx.service` |

## Pitfalls

- The running service reads static files from `/home/bin/webclx/static/`, not the repo `static/` directory. Only modifying repo files without syncing will have no visible effect.
- Do not hardcode `./target/release/webclx`; always resolve `target_directory` from `cargo metadata` because `~/.cargo/config.toml` can redirect it.
- After frontend-only changes, `--skip-build` is acceptable, but skipping the script entirely is wrong because the sync step is what makes static changes visible.
