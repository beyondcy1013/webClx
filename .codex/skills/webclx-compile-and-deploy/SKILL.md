---
name: webclx-compile-and-deploy
description: Use when compiling, deploying, restarting, or publishing builds through the webClx queue.
---

# webClx Compile And Deploy API

Use the running webClx service as the default compile/deploy coordinator for the active project. Do not run local compile, install, deploy, or restart commands directly unless the API is unavailable, the user explicitly asks for manual execution, or the task is fixing the build/deploy API itself.

The wrapper must submit the actual project identity and working directory. Do not assume the project is `webClx` unless the current project really is `/home/codes/tools/webClx`.

The `project_dir` is only the working directory for compile/install commands. webClx centrally retains build logs and install audit reports under its own `.webclx-compile-queue/runs/<run-id>/outputs/` directory and returns absolute paths in the completion callback. Do not look for or create `docs/logs` in the client project, and do not add client `.gitignore` entries for coordinator logs.

If an older worker left `webclx-build-*` or `webclx-install-report-*` files below `/home/codes/**/docs/logs`, use `/home/codes/tools/webClx/scripts/migrate-compile-api-logs.sh`: run it without arguments for a dry run, inspect the totals, then rerun with `--apply`. It preserves the original source hierarchy below `.webclx-compile-queue/legacy/`, updates retained run path references, and writes a SHA-256 manifest. Do not move legacy files by hand.

## Three Modes

This skill provides three scripts covering all compile/deploy scenarios:

| Mode | Script | Description |
|------|--------|-------------|
| Pure compile | `request-webclx-compile-api.sh` | Compile only through `/api/build/compile`; no install/restart fields are submitted. |
| One-step compile+deploy | `request-webclx-deploy-api.sh` | Compile and run install script in one API call via `/api/build/deploy`. |
| Two-stage compile-then-deploy | `request-webclx-compile-and-deploy.sh` | Compile first, then after success callback, deploy via `/api/service/deploy` with binary install and service restart. |

Choose the mode based on the task:

- **Pure compile / build check**: use `request-webclx-compile-api.sh`. No service restart, no binary install.
- **Compile and immediately deploy in one step**: use `request-webclx-deploy-api.sh`. The install script runs right after compile succeeds within the same API call.
- **Compile, wait for callback, then deploy separately**: use `request-webclx-compile-and-deploy.sh`. Stage 1 uses the pure compile endpoint; after the compile success callback, stage 2 (`--skip-compile`) installs the binary and restarts the service via `/api/service/deploy`. Known projects auto-detect service name and binary path.

## Pure Compile (Mode 1)

Queue a compile-only request from the project directory:

```bash
WEBCLX_BUILD_SKILL_DIR="/home/root/.codex/skills/webclx-compile-and-deploy"
# Claude Code sessions can use:
# WEBCLX_BUILD_SKILL_DIR="/home/root/.claude/skills/webclx-compile-and-deploy"
bash "$WEBCLX_BUILD_SKILL_DIR/scripts/request-webclx-compile-api.sh" \
  --note "当前代理请求编译当前项目，并在等待回调期间继续处理不依赖编译结果的其它工作。"
```

By default the wrapper uses `pwd -P` as `project_dir`, the directory basename as `project`, and infers the compile command in this order: `cargo build --release`, `npm run build`, `make`. The compile endpoint strips any legacy install command, so a pure compile cannot install files or restart services.

If the response includes `"queued": true`, continue with any work that does not depend on the build result. Wait for the callback message in the source terminal.

## Identify The Caller: CLI Versus Devin Desktop

Determine the caller's actual runtime before choosing a return channel. `devin-desktop` and `devin cli` are not interchangeable; a Devin session ID, IDE tab, `devin acp` process, or inbox availability alone does not identify a desktop caller.

- **All CLI agents (including Devin CLI, Codex, Claude Code, Gemini, and other CLIs)**: resolve the originating terminal through the webClx API and return completion directly to that terminal. First obtain `GET /api/terminal/sessions?all=true` with the local API token. Match the caller's `WEBCLX_TERMINAL_ID`, actual `webclx_<id>` tmux session, or uniquely matching connected `WEBCLX_TERMINAL_NAME` to the API response. Take the current terminal name from that response, not from a guessed/stale name or the newest active terminal. Submit `source_terminal_id`, `source_terminal_name` (a terminal name, not a URL), and `source_tmux_session` when resolved. The existing wrappers perform this resolution; use `--source-terminal-name '<API-resolved name>'` when explicit selection is needed.
- **Devin Desktop / non-terminal platform sessions**: when the runtime is confirmed not to be a CLI in a webClx terminal, use the external inbox callback described below. In this mode `source_terminal_name` is the full callback URL, and terminal ID/tmux fields are empty. Explicitly bind the originating session with `--callback-session <current-session-id>`.
- **Ambiguous or unresolved CLI identity**: inspect the caller's environment and tmux/parent-process context and query the terminal API before proceeding. Do not silently classify a CLI as Desktop, select another active terminal, or fall back to an inbox solely because terminal environment variables are absent. If no originating webClx terminal can be resolved, report the routing blocker; use an alternate channel only when explicitly agreed.
- Keep any user-required `--callback-session <current-session-id>` on CLI submissions too. That flag identifies a session for an inbox if used; it does not itself select Desktop mode or override a resolved terminal. Do not use `--third-party`, `--callback-url`, or an inherited `WEBCLX_CALLBACK_URL` to override a CLI's terminal route.
- Before accepting queue success, inspect the returned routing fields: CLI must point to its API-resolved terminal; Desktop must point to its session-bound callback URL. `callback_supported: true` alone does not prove the result will reach the correct caller. A wrong queued route is not a reason to submit a duplicate build; diagnose the existing request instead.

For example, if the caller's tmux session is `webclx_s9732` and the terminal API reports `{ "id": "s9732", "name": "thsRs_3" }`, its CLI source identity is `s9732` / `thsRs_3` / `webclx_s9732`, not an inbox URL. These example values must not be hard-coded for future sessions.

## Querying Results Without Terminal Callbacks (Third-Party Agents)

The following fallback applies only after caller classification above. CLI agents must first resolve their originating webClx terminal; these are not default CLI return paths. Confirmed non-terminal callers fall into two groups:

- **With an agent inbox (preferred)**: agents running on this host under a third-party platform IDE (e.g. Devin IDE with the `devin-inbox` extension on `127.0.0.1:11119`) receive the completion callback automatically. The submit wrapper detects the missing webClx terminal identity, resolves the inbox plus the current agent session, and submits the **full callback URL** in the `source_terminal_name` field (field extension: any `http(s)://` value is treated as an external callback target instead of a webClx terminal). webClx then POSTs the result JSON to that URL and the inbox forwards it into the originating agent session. For confirmed non-terminal callers, configure this mode with `--third-party`, `--callback-url <url>`, `--callback-session <sessionId>`, or env `WEBCLX_CALLBACK_URL` / `DEVIN_INBOX_URL` / `DEVIN_INBOX_SESSION_ID` / `DEVIN_INBOX_TOKEN`. `--wait-callback <secs>` makes the wrapper block on the inbox status endpoint (`GET {inbox}/webclx/callback/status?request_id=...`) so the result arrives as tool output within the same turn — use it only when no independent work remains and the platform cannot wake the agent from outside.
  - **Devin Desktop / confirmed non-terminal IDE sessions (verified 2026-09-26)**: such sessions are locked, so `devin --resume` delivery cannot enter. Delivery instead goes through user-level Devin hooks (`~/.config/devin/config.json` → `PostToolUse`/`UserPromptSubmit`/`Stop` run `/home/codes/plugins/vscode_devin_terminalMsg/hooks/inbox-pull.sh`), which pull `GET {inbox}/inbox/pending?sessionId=<id>` and inject the result as a `【收件箱消息】` block on the session's next tool call or turn end. Hook `session_id` equals the short Devin session id (e.g. `proud-shell`). The `UserPromptSubmit` hook injects `【当前 Devin 会话 id: <id>】` every turn; always pass that id as `--callback-session <id>` — auto-detection only picks the most recently active of all tabs held by the shared `devin acp` process and can misroute when several tabs are busy. A fully idle session receives it on its next interaction. So in a confirmed non-terminal Devin session: queue, keep doing independent work, and the callback appears in-context; do not poll. Devin CLI sessions in webClx terminals use direct terminal delivery instead.
  - **Visibility**: hook-injected context is only visible to the agent. The inbox therefore pops an IDE notification when a callback/message arrives or is claimed, and the hook script fires a desktop `notify-send` toast. A session whose IDE tab is closed is not locked: the inbox delivers through `devin --resume` (ACP `session/prompt`), which lands as a normal visible user message — the same effect as typing into the session input box.
- **Without any callback target**: they never receive a callback and must poll. For them the submit response itself declares how to fetch the outcome:

- Every queue response contains `callback_supported` (true when the source terminal resolved OR an external callback URL was accepted) and a `query` object with the fields `url`, `suggested_poll_seconds`, and `note`. When `callback_supported` is `false`, the caller MUST poll `query.url` instead of waiting for any callback.
- The query endpoint is `GET /api/build/compile/requests/{request_id}` and returns a self-describing JSON snapshot:

  ```json
  {
    "ok": true,
    "found": true,
    "request_id": "131103-18d7d87bcc543af9",
    "state": "queued|running|stalled|unknown|succeeded|failed|timed_out|not_found",
    "done": false,
    "exit_codes": [],
    "progress": { "phase": "compile", "spec_index": 2, "spec_count": 2, "packages_completed": 37, "packages_total": 120, "current_package": "tokio", "...": "..." },
    "log_tail": ["last build log line", "..."],
    "log_path": "/home/bin/webclx/compile/runs/<run-id>/build-1.log",
    "suggested_poll_seconds": 15,
    "request": { "project": "...", "command": ["..."], "...": "..." },
    "run": { "run_id": "...", "dir_path": "...", "...": "..." }
  }
  ```

- `progress` carries the last reported build progress (phase, package counters); it stays available after completion. `log_tail` carries the final lines of the latest centralized build log (default 50, adjustable with `?tail_lines=N`, max 400) — on failure the error output is typically right there, so callers do NOT need filesystem access to diagnose.

- Poll until `done=true`, then read `state` (`succeeded` / `failed` / `timed_out` are terminal; `not_found` means the request never existed or aged out of retention), `exit_codes` for the last command result, and `run.log_path` for the centralized build log. `state=running/stalled/unknown` keeps `done=false` — keep polling at `suggested_poll_seconds`.
- Requests coalesced into one run keep their own `request_id`: each original request resolves through the query endpoint to the run that absorbed it, so every caller still gets its own verdict.
- Once `run.log_path` is known and the caller has filesystem access, it may tail the centralized log directly instead of continuing HTTP polling; both channels report the same run. Callers without filesystem access can rely on `log_tail` / `progress` in the query response itself.
- Submit responses for agents WITH callback support include the same `query` object; it is safe to ignore there and rely on the callback.

## APK Publication After A Successful Build

Treat webClx download-center publication as a required final stage of every successful APK build, including pure compile and verification-only tasks:

**Completion gate:** a successful compile callback does not complete an APK packaging task. The active agent must continue in the same task until the newly built APK is verified and published to the webClx download center. A previous upload or an older local APK never satisfies this requirement for the current build.

1. Before queueing, pass the expected final APK through `--required-artifact PATH` whenever its path is known.
2. Wait for the successful build callback. Do not publish an APK merely because a possibly stale file already exists.
3. Verify the callback belongs to the current request, then verify the final APK using the project's documented checks. At minimum, confirm the package/version and release signature; record a SHA-256 digest when the project workflow provides one.
4. Read and use `/home/root/.codex/skills/webclx-artifact-publisher/SKILL.md`, then publish the verified final APK with its `scripts/publish-artifact.sh` helper. Pass the verified application version through `--version` and name the download `<product>-<verified-version>.apk`, for example `lyyNote-0.1.8.apk`. Never publish a generic name such as `lyyNote.apk`, and never substitute a timestamp, build request ID, or guessed version for the authoritative application version.
5. Report the direct download URL and the webClx `/downloads` page URL. An APK build task is not complete until these URLs are reported.

Do not publish an APK when the build or verification fails. Skip publication only when the user explicitly says `不发布`, `不要上传`, or an equivalent instruction. `仅构建`, `仅测试`, `不部署`, and `不发布服务` do not suppress APK publication: registering a downloadable APK is not a service deployment and does not install or restart anything.

## One-Step Compile + Deploy (Mode 2)

Compile and run the install script in a single `/api/build/deploy` call:

```bash
WEBCLX_BUILD_SKILL_DIR="/home/root/.codex/skills/webclx-compile-and-deploy"
bash "$WEBCLX_BUILD_SKILL_DIR/scripts/request-webclx-deploy-api.sh" \
  --install-command 'bash scripts/deploy.sh' \
  --required-artifact target/release/my-service \
  --audit-path /home/bin/my-service \
  --note "编译并部署当前项目。"
```

For argument-safe commands, use repeated argv flags:

```bash
WEBCLX_BUILD_SKILL_DIR="/home/root/.codex/skills/webclx-compile-and-deploy"
bash "$WEBCLX_BUILD_SKILL_DIR/scripts/request-webclx-deploy-api.sh" \
  --cmd cargo --arg build --arg --release \
  --install-cmd bash --install-arg scripts/deploy.sh \
  --required-artifact target/release/my-service \
  --audit-path /home/bin/my-service
```

Every deploy request must explicitly submit a deployment script. Prefer a project-local script such as `scripts/deploy.sh`, `scripts/install-service.sh`, or `scripts/rebuild-and-deploy.sh` through `--install-cmd bash --install-arg <script>` / `--install-command 'bash <script> ...'`. The coordinator resolves quoted shell arguments and rejects the request before queueing when the referenced relative or absolute script does not exist. If the project does not have a suitable deployment script yet, create or update one first. For pure compile checks, use Mode 1 instead.

Pass every expected build output through repeated `--required-artifact PATH` flags. Relative paths resolve from `project_dir`; absolute paths are checked as written. The worker verifies them after compilation and before installation, so a command that exits successfully without producing its deliverable cannot trigger deployment.

## Two-Stage Compile Then Deploy (Mode 3)

### Stage 1: Compile

In the project directory (known projects need no parameters):

```bash
bash ~/.codex/skills/webclx-compile-and-deploy/scripts/request-webclx-compile-and-deploy.sh
```

The script auto-detects the project directory and name, matches `--service-name`, `--binary-path`, and compile command from the built-in service registry, resolves terminal identity for callback, and generates a deploy script.

After queueing, continue with work that does not depend on the compile result. Wait for the callback.

### Stage 2: Deploy

After the compile success callback, install the binary and restart the service:

```bash
bash ~/.codex/skills/webclx-compile-and-deploy/scripts/request-webclx-compile-and-deploy.sh --skip-compile
```

Or call `/api/service/deploy` directly:

```bash
curl -fsS --noproxy '*' -X POST 'http://127.0.0.1:11111/api/service/deploy' \
  -H 'Content-Type: application/json' \
  -d '{
    "service_name": "webclx.service",
    "script": "#!/bin/bash\nset -euo pipefail\ninstall -m 0755 /home/codes/tools/webClx/target/release/webclx /home/bin/webclx/webClx",
    "binary_path": "/home/bin/webclx/webClx",
    "source_terminal_name": "<当前终端名>"
  }'
```

## Project Parameters

Pass explicit project parameters when the current shell is not in the intended repo or the project needs its own deploy command/options:

```bash
bash ~/.codex/skills/webclx-compile-and-deploy/scripts/request-webclx-compile-api.sh \
  --project signIn \
  --project-dir /home/codes/apps/signIn \
  --command 'cargo build --release' \
  --project-path signIn \
  --note "当前代理修改后编译 signIn。"
```

For argument-safe commands, use `--cmd` plus repeated `--arg`:

```bash
bash ~/.codex/skills/webclx-compile-and-deploy/scripts/request-webclx-compile-api.sh \
  --project signIn \
  --project-dir /home/codes/apps/signIn \
  --cmd cargo --arg build --arg --release
```

Prefer direct argv for commands that do not need shell syntax. When pipes, redirects, environment expansion, `&&`, or other shell syntax are required, use `--command '<one complete shell string>'`. Never encode `bash -lc` as `--command-json '["bash","-lc","bash","scripts/build.sh"]'`: the item after `-lc` is the complete command string, so the correct form is `--command 'bash scripts/build.sh'` or `--command-json '["bash","-lc","bash scripts/build.sh"]'`. The wrappers and API reject the split form before queueing.

For project-specific deploy scripts, use the Mode 2 deploy wrapper with `--install-command 'bash scripts/deploy.sh --target 127.0.0.1'` or `--install-cmd bash --install-arg scripts/deploy.sh --install-arg --skip-restart`. Keep compile/build arguments in `--command`, `--cmd/--arg`, or `--command-json`.

Before queueing, inspect the exact payload when project detection or command options are uncertain:

```bash
bash ~/.codex/skills/webclx-compile-and-deploy/scripts/request-webclx-compile-api.sh --dry-run
```

## Project-Owned Build And Deploy Scripts

Keep project-specific compatibility logic in the project, not in webClx coordinator profiles:

- The build script owns toolchain selection, cross-compilation environment, working-directory setup, target flags, and any generated-code prerequisites. It must exit nonzero if its final artifact is absent.
- The deploy script owns its project-directory default, artifact installation, backup policy, restart behavior, and runtime health verification. It should work when invoked from the project root without caller-supplied project environment variables.
- The Codex caller selects the appropriate wrapper, passes the build/deploy script as direct argv or one complete shell string, declares expected outputs with `--required-artifact`, and declares installed/runtime files with `--audit-path`.
- Do not add a coordinator-side project profile merely to supply toolchain variables, repair a project-relative path, or infer a project artifact. Put those details in the owning script and document only the generic submission convention here.

## Source Terminal Selection

The wrapper must refresh the current source terminal identity immediately before every request. Do not reuse a previously observed terminal name unless the user explicitly passed `--source-terminal-name`; terminal names can change after reconnect/resume.

The wrapper chooses the source terminal identity in this order:

- Explicit `--source-terminal-name '<terminal name>'`
- Fresh lookup by `WEBCLX_TERMINAL_ID` against `/api/terminal/sessions?all=true`
- Fresh lookup of the current tmux session name when it has the webClx-managed form `webclx_s<number>`
- Fresh lookup by `WEBCLX_TERMINAL_NAME` when exactly one connected session has that name

Pass `--source-terminal-name` when the current shell was not created by webClx. Do not use compatibility aliases such as `--target`, `--path`, or `--cwd`.

## Built-in Service Registry (Mode 3)

Known projects auto-match `--service-name` and `--binary-path` when omitted:

| Project Directory | systemd Service | Binary Path | Default Build |
|-------------------|-----------------|-------------|---------------|
| `/home/codes/tools/webClx` | `webclx.service` | `/home/bin/webclx/webClx` | `cargo build --release` |
| `/home/codes/apps/signIn` | `signin.service` | `/home/bin/signIn/signIn` | `cargo build --release` |
| `/home/codes/apps/feishuFwd` | `feishu-fwd-web.service` | `/home/bin/feishuFwd/feishu_fwd_web` | `cargo build --release` |
| `/home/codes/lyyStock/quoteTdx` | `quoteTdx.service` | `/home/bin/quoteGateway/quoteGateway` | `cargo build --release` |
| `/home/codes/lyyStock/stockJiepan` | `stockJiepan.service` | `/home/bin/stockJiepan/stock-jiepan` | `cargo build --release` |
| `/home/codes/lyyStock/stockScreener` | `stockScreener.service` | `/home/bin/stockScreener/stockScreener` | `cargo build --release` |
| `/home/codes/lyyStock/stockInfo` | `stockInfo.service` | `/home/bin/stockInfo/stock-info` | `cargo build --release` |
| `/home/codes/lyyStock/stockF10` | `stockF10-web.service` | `/home/bin/stockF10/stockF10` | `cargo build --release` |
| `/home/codes/lyyStock/stockAgent` | `stock-agent.service` | `/usr/local/bin/stock-agent` | `cargo build --release` |
| `/home/codes/newsKB` | `newsKB-web-rs-frontdoor.service` | `/home/bin/newsKB-web-rs/news-kb-web` | *(auto-inferred)* |
| `/home/codes/systemGuard` | `systemGuard.service` | `/home/bin/systemGuard/systemGuard` | `cargo build --release` |
| `/home/third_party/sub2api` | `sub2api.service` | `/home/third_party/bin/sub2api/sub2api` | `make` |
| `/home/third_party/sub2freeApi` | `sub2freeApi.service` | `/home/third_party/bin/sub2freeApi/sub2freeApi` | `make` |

Registry is maintained in the `KNOWN_SERVICES` array at the top of `request-webclx-compile-and-deploy.sh`.

## Status

Use status only for inspection, not for polling after a successful queue response:

```bash
curl -fsS --noproxy '*' http://127.0.0.1:11111/api/build/compile/status | jq .
```

## Request Coalescing Semantics

Use `docs/codex/tasks/compile-coordinator-reliability.md` when diagnosing duplicate work or suspicious merged-request counts.

- A command that has started is immutable and cannot absorb a later request. Equivalent later requests may converge only while their follower run is still waiting for build resources.
- Coalescing requires the complete build/deploy specification to match. Different commands, environments, install parameters, audit paths, or required artifacts remain separate.
- A waiting follower reads the worktree when it actually starts, so it observes the latest state accumulated while waiting. Do not assume an already-running build contains later edits.
- Count only request JSON objects containing valid request IDs. Progress, install, and status JSON files are run artifacts, not merged requests.
- A single-request callback must not claim a merge. If a count looks wrong, inspect the run's valid request IDs and worker version before changing scheduler behavior or submitting another request.

## Missing Callback And Self-Repair

When a queued request has no callback beyond its expected build/deploy duration, inspect the status endpoint, `webclx.service` journal, transient systemd unit, and request/run JSON once. Do not blindly poll or submit repeated requests.

If evidence points to the wrapper, this skill, or the webClx coordinator, repairing the owning skill/script and `/home/codes/tools/webClx` source is explicitly in scope. Add a regression test and a concise incident note, then use the documented manual fallback while the compile API itself is broken. A worker launch failure must not be accepted as `queued: true` and must not leave a request that another worker can consume.

After deploying and verifying the coordinator repair, retry the original project request once. If it still fails, diagnose that new failure from its own evidence instead of assuming the coordinator fix was ineffective.

## Static Asset Sync Gotcha (webClx)

`webClx` embeds `static/` into the release binary via `include_dir!` AND serves from disk at runtime. The runtime path is resolved by `main.rs::resolve_static_dir` in this order:

1. `$WEBCLX_STATIC_DIR` env var (absolute path)
2. `<process-cwd>/static/`
3. Ancestor directories of the executable, up to 4 levels

The first candidate with `index.html` wins. The running service's CWD is typically `/home/bin/webclx`, NOT the source repo. So `cargo build --release` does NOT copy new `static/` files to the runtime static dir on disk.

**Fix: use the project-local `scripts/rebuild-and-deploy.sh`** which handles both binary build and static rsync. For webClx deploy, pass this script through Mode 2:

```bash
bash ~/.codex/skills/webclx-compile-and-deploy/scripts/request-webclx-deploy-api.sh \
  --install-cmd bash --install-arg scripts/rebuild-and-deploy.sh \
  --audit-path /home/bin/webclx
```

Or through Mode 3 with `--deploy-script`.

## Binary Backup Policy

When the auto-generated deploy script (Mode 3) or the webClx project script `scripts/rebuild-and-deploy.sh` installs a new binary, the previous binary is first moved aside to a **single** fixed-name backup `<binary_path>.bak` (overwritten on every deploy). Only one backup is ever kept — no timestamped archive accumulates. The API fallback below follows the same rule. Supply a custom `--deploy-script` / `--install-command` if you need a different scheme.

## Cargo Target Ownership

Direct Cargo commands and webClx compile/deploy requests must use the Cargo
workspace's own resolved `target_directory`. The worker must not inject a
private `CARGO_TARGET_DIR`; resolve artifacts with
`cargo metadata --format-version 1 --no-deps`. Workspaces below `/home/codes`
are enrolled by `/home/codes/tools/webClx/scripts/unify-cargo-targets.sh`: their
normal `target` path is a compatibility symlink to a stable per-workspace
directory below `/data`, while an existing explicit target such as
`/data/cargo-target/stockScreener` remains authoritative.

The coordinator allows different Cargo target directories to build concurrently.
Builds sharing one resolved `target_directory` remain serialized. The global
limit is `compile_max_concurrency` in Settings > System (default `5`, range
`1..=32`). Non-Cargo builds are serialized per project directory.
Deploy requests also take a per-project lock around compile and install so two
target variants cannot install or restart the same project concurrently.

## Scope And Fallbacks

- If a deploy/update/restart task has no suitable project deployment script, add one before queueing. Do not submit inline shell fragments unless actively creating/repairing the deployment script.
- Deploy audit paths come from explicit `--audit-path` values, inferred Cargo binary outputs, and known webClx runtime paths.
- If the API is unavailable, state the reason and use the project-specific documented fallback.

## Fallback (API Unavailable)

```bash
cargo build --release
mv /home/bin/webclx/webClx /home/bin/webclx/webClx.bak
cp target/release/webclx /home/bin/webclx/webClx
/usr/bin/systemd-run --quiet --collect --on-active=1s /bin/systemctl restart webclx.service
```
