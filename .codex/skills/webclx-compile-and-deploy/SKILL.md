---
name: webclx-compile-and-deploy
description: Use when compiling, deploying, restarting, or publishing builds through the webClx queue.
---

# webClx Compile And Deploy API

Use the running webClx service as the default compile/deploy coordinator for the active project. Do not run local compile, install, deploy, or restart commands directly unless the API is unavailable, the user explicitly asks for manual execution, or the task is fixing the build/deploy API itself.

The wrapper must submit the actual project identity and working directory. Do not assume the project is `webClx` unless the current project really is `/home/codes/webClx`.

The `project_dir` is only the working directory for compile/install commands. webClx centrally retains build logs and install audit reports under its own `.webclx-compile-queue/runs/<run-id>/outputs/` directory and returns absolute paths in the completion callback. Do not look for or create `docs/logs` in the client project, and do not add client `.gitignore` entries for coordinator logs.

If an older worker left `webclx-build-*` or `webclx-install-report-*` files below `/home/codes/**/docs/logs`, use `/home/codes/webClx/scripts/migrate-compile-api-logs.sh`: run it without arguments for a dry run, inspect the totals, then rerun with `--apply`. It preserves the original source hierarchy below `.webclx-compile-queue/legacy/`, updates retained run path references, and writes a SHA-256 manifest. Do not move legacy files by hand.

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
    "script": "#!/bin/bash\nset -euo pipefail\ninstall -m 0755 /home/codes/webClx/target/release/webclx /home/bin/webclx/webClx",
    "binary_path": "/home/bin/webclx/webClx",
    "source_terminal_name": "<当前终端名>"
  }'
```

## Project Parameters

Pass explicit project parameters when the current shell is not in the intended repo or the project needs its own deploy command/options:

```bash
bash ~/.codex/skills/webclx-compile-and-deploy/scripts/request-webclx-compile-api.sh \
  --project signIn \
  --project-dir /home/codes/signIn \
  --command 'cargo build --release' \
  --project-path signIn \
  --note "当前代理修改后编译 signIn。"
```

For argument-safe commands, use `--cmd` plus repeated `--arg`:

```bash
bash ~/.codex/skills/webclx-compile-and-deploy/scripts/request-webclx-compile-api.sh \
  --project signIn \
  --project-dir /home/codes/signIn \
  --cmd cargo --arg build --arg --release
```

Prefer direct argv for commands that do not need shell syntax. When pipes, redirects, environment expansion, `&&`, or other shell syntax are required, use `--command '<one complete shell string>'`. Never encode `bash -lc` as `--command-json '["bash","-lc","bash","scripts/build.sh"]'`: the item after `-lc` is the complete command string, so the correct form is `--command 'bash scripts/build.sh'` or `--command-json '["bash","-lc","bash scripts/build.sh"]'`. The wrappers and API reject the split form before queueing.

For project-specific deploy scripts, use the Mode 2 deploy wrapper with `--install-command 'bash scripts/deploy.sh --target 192.168.3.38'` or `--install-cmd bash --install-arg scripts/deploy.sh --install-arg --skip-restart`. Keep compile/build arguments in `--command`, `--cmd/--arg`, or `--command-json`.

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
| `/home/codes/webClx` | `webclx.service` | `/home/bin/webclx/webClx` | `cargo build --release` |
| `/home/codes/signIn` | `signin.service` | `/home/bin/signIn/signIn` | `cargo build --release` |
| `/home/codes/feishuFwd` | `feishu-fwd-web.service` | `/home/bin/feishuFwd/feishu_fwd_web` | `cargo build --release` |
| `/home/codes/quoteGateway` | `quoteGateway.service` | `/home/bin/quoteGateway/quoteGateway` | `cargo build --release` |
| `/home/codes/stockJiepan` | `stockJiepan.service` | `/home/bin/stockJiepan/stock-jiepan` | `cargo build --release` |
| `/home/codes/stockScreener` | `stockScreener.service` | `/home/bin/stockScreener/stockScreener` | `cargo build --release` |
| `/home/codes/stockInfo` | `stockInfo.service` | `/home/bin/stockInfo/stock-info` | `cargo build --release` |
| `/home/codes/stockF10` | `stockF10-web.service` | `/home/bin/stockF10/stockF10` | `cargo build --release` |
| `/home/codes/stockAgent` | `stock-agent.service` | `/usr/local/bin/stock-agent` | `cargo build --release` |
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

## Missing Callback And Self-Repair

When a queued request has no callback beyond its expected build/deploy duration, inspect the status endpoint, `webclx.service` journal, transient systemd unit, and request/run JSON once. Do not blindly poll or submit repeated requests.

If evidence points to the wrapper, this skill, or the webClx coordinator, repairing the owning skill/script and `/home/codes/webClx` source is explicitly in scope. Add a regression test and a concise incident note, then use the documented manual fallback while the compile API itself is broken. A worker launch failure must not be accepted as `queued: true` and must not leave a request that another worker can consume.

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
are enrolled by `/home/codes/webClx/scripts/unify-cargo-targets.sh`: their
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
