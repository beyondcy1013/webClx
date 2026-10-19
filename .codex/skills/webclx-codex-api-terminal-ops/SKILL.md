---
name: webclx-ops
description: Comprehensive operational and maintenance skill for webClx — covers Codex/Claude APIs, presets, terminal session UI, mobile soft keyboard & IME isolation, proxies, build/deploy, and static asset sync.
metadata:
  short-description: webClx operations, terminal UI, mobile soft keys, and API presets
---

# webClx Operations (webclx-ops)

Use this skill for `webClx` operational issues, including Codex/Claude API presets, in-browser terminal, mobile soft keyboard and IME isolation, resume archives, local proxy behavior, and deployed static files.

## First Reads

From `/home/codes/tools/webClx`, read before broad searching:

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

For the recurring terminal paths, read the owning topic directly:

- `docs/codex/tasks/terminal-soft-keyboard-deploy.md` for preset launch, resume, fork, and stable terminal routing.
- `docs/codex/tasks/terminal-continue-send-unification.md` for manual continue and automatic retry ownership.
- `docs/codex/tasks/terminal-mobile-scroll-settle.md` and `terminal-session-switch-output.md` for IME, resize, replay, and scroll restoration.
- `docs/codex/tasks/compile-coordinator-reliability.md` for queue identity, coalescing, callback, and worker failures.

## Stable Rules

- `/home/codes/tools/webClx/static` is the tracked physical source; `/home/bin/webclx/static` must be an absolute symlink to it. Verify the link before editing; do not copy or rsync between these equivalent paths.
- Static-only changes take effect without rebuilding or restarting the service, but every changed browser module must receive a matching cache-version bump in its HTML loader.
- Backend changes require the repository's queued compile/deploy workflow. Do not run local Cargo install or service restart commands unless the compile API is unavailable, the user requests manual execution, or the task repairs that API.
- Confirm Cargo output with `cargo metadata --format-version 1 --no-deps`; do not assume `./target/release`.
- User identity controls terminal Linux user, HOME, shell, and Codex/Claude config paths. Resolve HOME from the system user database; do not hardcode `/root`, `/home/root`, or `/home/beyondcy`.
- Terminal quick commands are `program` plus raw `args`; do not store a full shell command in `program`.
- Terminal startup only forwards native shell/Agent input; never inject preset environment, HOME/config directories, credentials, or provider/model identity. Follow AGENTS.md and the preset-switching topic rather than legacy startup snippets.
- Local proxy preset switching must not stop a proxy that already-running Codex/Claude sessions still depend on.
- For resume archives, preserve `cwd`; `codex resume <id>` must run from the original project directory or a newly created terminal for that directory.
- Do not print API keys, OAuth tokens, or copied preset secrets in reports. Mask or summarize.
- `/home/bin/webclx/webclx-api-presets.json` is the runtime authority for API presets. When retiring or updating a model identifier, cascade the removal across downstream services (`autoApi`, `sub2api`) to eliminate dead fallback pricing, obsolete aliases, and unneeded whitelist entries.
- In shared multi-agent repositories, never use blanket `git add .` or stage unrelated modified/untracked files. If a file contains concurrent in-flight edits, isolate hunks with `git apply --cached <patch>` and verify with `git diff --cached --stat` before committing.
- Do not treat compile API completion callbacks as matching the current task unless the callback `request_id` and project match the active terminal's submitted job. Ignore callbacks from concurrent tasks.

## Recurring Terminal Automation

- Error recognition is split between `src/terminal/manager/error_detection.rs`, built-in settings merge in `crates/settings_core`, and browser defaults. Preserve the same keyword and action contract across all active owners and lock it with focused Rust and Node tests.
- An ordinary retry uses the canonical per-session auto-continue path. Cooldowns and timers must be keyed by stable session ID and cleared when that session leaves `error` or `retrying`; one terminal must never delay another.
- Context-limit failures can select `compact_then_continue` instead of ordinary continue. Keep action selection separate from keyword matching, submit `/compact` once, and treat an actual `Context compacted` marker as completion.
- For terminal scroll/focus defects, distinguish explicit user intent from layout events. IME, `visualViewport`, page resume, fit, and replay settle must preserve the captured scroll/focus state; only explicit input, terminal switching policy, or live-output follow mode may force a new position.

## 终端滚动修复子技能

终端切换未到底、输入/回答后跳顶部、新输出不跟随、回放/共享尺寸/IME 导致位置错乱时，先读 [终端滚动子技能](subskills/webclx-terminal-scroll/SKILL.md)。按页面、逻辑缓冲区、DOM 滚动区域分别取证；主技能只保留入口。

## 中文输入法问题子技能

遇到中文输入法消失、Windows 文档/新开 AGY/恢复/其他弹窗返回终端后不能输入中文，或移动软键盘焦点异常时，读取 [中文输入法子技能](subskills/webclx-chinese-ime/SKILL.md)。含已验证根因、统一恢复入口、移动策略边界、回归命令和 Windows 原生候选窗待验说明。

## 网络安全与 API 鉴权子技能

遇到 API 鉴权漏洞、公网端口扫描、FRP 穿透风险、本地回环假象或接口防枚举加固时，读取 [网络安全子技能](subskills/webclx-network-security/SKILL.md)。含 FRP 透传 127.0.0.1 陷阱、全局前置门禁规范、零知识响应及回归验证。

## 新开对话框与会话动作子技能

遇到新开终端/指定预设执行弹窗高度不一、Tab 切换跳动、控件宽度闪烁、工作目录丢失或 AGY 恢复最后一次会话（`-c`）联动问题时，读取 [新开对话框子技能](subskills/webclx-new-session/SKILL.md)。含弹窗高度统一锁定、防闪烁稳定滚动槽、工作目录保留规范与自动化断言验证。


## Performance And Delivery Checks

- Read `docs/codex/tasks/terminal-session-activity.md` before activity/scheduler changes. Trace the entire async path: task collection, mutex waits and disk writes can block even when sending already uses `spawn_blocking`. Await one bounded batch; moving work does not make IO faster or cancel an already-running blocking task.
- Reuse existing snapshots, command builders and persistence workers first. Keep subprocess IO outside the manager state lock; distinguish that lock from a dedicated ordered-writer lock. Never remove write ordering or move snapshots across locks without proving stale writes cannot win.
- Bound subprocess time and combined stdout/stderr size; drain both pipes fairly, and use RAII kill+wait for errors/panic. An outer async timeout alone does not terminate a synchronous child; per-command bounds are not a whole-scan deadline.
- Tests must invoke production helpers. For blocking isolation, use current-thread runtime, a real lock/barrier, causal signals and finite RAII release; assert responsiveness before the safety timeout. Distinguish queued, tested, deployed and health-verified status.
- Queue once, wait for the exact callback, and keep at most one persistent wake per check. Verify focused tests, install audit, artifact/installed/running hashes and HTTP health; cancel the wake. Never equate one batch completion with all roadmap items, or uncontrolled timing samples with measured speedup.

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

- saved webClx settings and preset table (`/home/bin/webclx/webclx-api-presets.json`)
- generated Codex/Claude config file
- terminal user HOME and environment
- downstream gateway and fallback pricing tables (`sub2api`, `autoApi`)
- local proxy process state
- already-running terminal sessions versus newly-started sessions

4. For terminal display/copy/paste/cursor issues, prefer browser or Playwright geometry/state checks over CSS guessing.
5. After edits, run the narrowest relevant checks, then deploy local service assets if the user expects the live UI to change.

## Preset & Model Retirement Cascading

When retiring an obsolete model identifier across the ecosystem (e.g. `gpt-6-sol` -> `gpt-6.1-sol`):

1. **Preset Definition**: Update `/home/bin/webclx/webclx-api-presets.json`.
2. **Downstream Gateway & Pricing (sub2api/autoApi)**:
   - Remove dead fallback prices in `billing_service.go` (`fallbackPrices`) and `pricing_service.go`.
   - Remove model alias prefixes in `openai_model_alias.go` (`normalizeKnownOpenAICodexModel`) and `openai_codex_transform.go`.
   - Update model catalog constants (`constants.go`) and pricing resources (`model_prices_and_context_window.json`).
   - Remove deprecated model entries from frontend whitelists (`useModelWhitelist.ts`) and client config templates (`UseKeyModal.vue`).
3. **Risks**:
   - Stale fallback mappings in billing/pricing cause deprecated models to match silently rather than failing fast.
   - Preserving deprecated models in test cases masks broken schema or reasoning validation.
4. **Verification**:
   ```bash
   git grep -n "<deprecated_model>" # Ensure 0 matches in target repo
   go test ./...                    # Backend tests pass
   pnpm test:run                    # Frontend tests pass
   ```

## Codex TUI Queued Input Hanging & Session Rescue

- **消息发送确认 / 补刀回车 / 原生队列（2026-10-18 修正）**：新正文写入后必须执行首次语义 Enter；只有本消息原生历史已确认才可跳过，不能拿已有队列证明新正文已提交（否则正文滞留、后续消息拼接）。首次提交后，队列已接收按用户语义记为 `confirmed`，停止历史确认与额外 Enter；未落历史不等于发送失败，不自动按 Esc 或 Alt+M。反查 `/home/codes/tools/webClx/docs/codex/tasks/terminal-scheduled-message-delivery.md` 的 Codex、AGY、CodeBuddy 队列段及边界修复 `11891f3`、上线核验 `ca87b04`。排障区分输入框草稿、队列已接收、原生历史已提交和上游 503 无响应；不得因修复提交 helper 就宣称上游无响应也解决。
- **识别关键词**：Codex `Messages to be submitted at end of turn` / `Queued follow-up inputs`；AGY `Press up to edit queued messages` / `▸ … (alt+m to send immediately)`；CodeBuddy v2.168.2 实测 `Messages to be submitted after next tool call (press esc to interrupt and send immediately)`，提示可折行。共享检测位于 `src/terminal/manager/error_detection.rs`，补发与定时提交位于 `src/terminal/manager.rs`；回归 `submission_`、`queued_input_detection`。只读实时可见 tmux 画面，避免旧 scrollback 或普通草稿误判；队列存在本身不证明 Agent 卡死，以下救援仅适用于另外确认的卡死。

- **Symptom**: Codex terminal hangs with prompt `› Ask Codex to do anything` and shows `• Queued follow-up inputs: ↳ <input>`, neither accepting new turns nor executing queued input. Attempting manual `/compact` fails with `cannot steer a compact turn`.
- **Root Cause**: Upstream error (429 Rate Limit / 502 Bad Gateway / steering conflict) aborted the active turn back to idle prompt. Codex TUI does not automatically dequeue follow-up inputs when idle, causing queued text to hang.
- **Rescue Workflow**:
  1. Interrupt hanging process cleanly via its isolated tmux socket:
     ```bash
     tmux -L webclx_<session_id> send-keys -t webclx_<session_id> C-c
     ```
  2. Codex safely exits and prints:
     ```text
     To continue this session, run:
       codex resume <session-uuid>
     ```
  3. Resume the session in the shell:
     ```bash
     tmux -L webclx_<session_id> send-keys -t webclx_<session_id> "codex resume <session-uuid>" Enter
     ```
- **Defensive Automation**:
  - `static/terminal-auto-continue.js` detects both legacy `Messages to be submitted at end of turn` and modern `Queued follow-up inputs`. When queued input is detected, automated `/compact` is withheld to prevent steering deadlocks.
  - Regression tests: `node tests/terminal-auto-compact.test.mjs`.

## Terminal Group Color Collisions

- Keywords: 分组同色、紫色重复、自动配色去重、手动改色。自动颜色由 `src/terminal/manager.rs` 的 `default_terminal_group_color` 及分组更新逻辑生成，选色前检查其他分组已用颜色；显式选色和同组继承不受此去重规则影响。已有重复颜色不会自动重染。
- 手动调整：终端菜单 → 终端分组 → 分组颜色 → 保存；控件与提交逻辑见 `static/terminal-sessions.js`。
- 回归入口：`src/terminal/tests.rs::new_group_default_color_skips_colors_already_used_by_other_groups`，构造默认候选色已被占用的输入。修复提交 `29d1a60`（2026-10-18）；后台变更须经编译部署队列上线，测试通过不代表运行服务已更新。

## Dialog Layout & Mobile Pointer Interception

- **Symptom**: Long descriptive text (>50 chars) placed directly as a grid/form child inside modals pushes scroll lists below the viewport on mobile screens (e.g. 375x812), causing Playwright errors like `<p> intercepts pointer events`.
- **Resolution**:
  - Move explanatory text into the modal header using `.card-title-help` (the `?` icon) + `.card-desc`, collapsible via click/Enter/Space.
  - Combine tightly-coupled single-row attributes (e.g. group name and color picker) into `.terminal-group-name-color-row`, putting the compact picker at the end.
  - Allocate the recovered vertical height directly to the flexible scroll list container (`minmax(0, 1fr)`).
- **Verification**: `python3 tests/terminal-session-groups.browser.py`.

## Multi-Model Relay Gateway & 502 Diagnostics

- A single relay gateway base URL (e.g., `http://HOST:PORT`) can multiplex across different models (GPT, GLM, DeepSeek, Claude) under the same API key.
- A transient `502 Upstream access forbidden` or `502 Bad Gateway` usually indicates upstream account rotation or Cloudflare challenge on a specific provider worker, not necessarily a broken preset configuration.
- Do not immediately change global presets in an active session upon 502, which risks splitting conversation context. Allow the gateway account pool to rotate, or cleanly resume via `codex resume <session-uuid>`.

## Apply And Deploy

- For static-only work, confirm `readlink -f /home/bin/webclx/static` is `/home/codes/tools/webClx/static`, bump the changed module's HTML cache version, and verify the live HTTP asset hash. No binary rebuild, copy, or restart is required.
- For backend work, use `webclx-compile-and-deploy` and the project-owned `scripts/rebuild-and-deploy.sh`; wait for the callback instead of polling or submitting duplicates.
- Before diagnosing apparent deployment drift, compare the source/link target, the asset requested by `terminal.html` or `index.html`, and the asset returned by the running HTTP service.

## Verification

Use checks that match the change:

```bash
node --check static/app.js
node --check static/terminal.js
curl -sS http://127.0.0.1:11111/api/update/check | jq .
curl -sS http://127.0.0.1:11111/api/settings | jq 'keys'
```

For terminal UI changes, use Playwright or browser geometry checks and inspect the deployed `/home/bin/webclx/static/*` copy if the page still shows old behavior.

## Codex Model Catalog & Context Window Tuning

- **权威路径**: `~/.codex/model_catalog.json`（root/beyondcy/cy 多用户通过硬链接共享同一 inode）。验证命令：`codex debug models | jq '.models[] | select(.slug | test("<model>")) | {slug, context_window, max_context_window, auto_compact_token_limit}'`。
- **上下文与自动压缩阈值**: 调整 `context_window` 与 `max_context_window`（例如 400K 设为 400000），`auto_compact_token_limit` 遵循 80% 比例（400K 对应 320000）。
- **webClx 同步兜底**: 并在 `crates/auth_core/src/storage.rs` 的 `custom_model_context_window` 维护映射，防止 webClx 同步 API 预设或初始化目录时被覆盖为 128K 默认值。

## Agent Running/Working Status Recognition (AGY / Antigravity)

- **核心判定位置**: `src/terminal/manager/error_detection.rs`（`terminal_tail_has_working_status` / `line_is_agy_working_status`）。
- **工具调用与状态栏特征**:
  - 工具调用中行：`● Running (...)`（含 `(ctrl+o to expand)` / `collapse`），完成会被 AGY 原地改写为过去式 `● Ran (...)`；
  - 底部后台任务栏：`● [HH:MM:SS] ... running`（含时间戳/耗时括号与活跃动词）；
  - 排除过去式思考 `○ Thought for ...` 与已完成 `● Ran (...)`。
- **回归测试**: `cargo test terminal_working_status_detection`。

## 结束终端卡顿、下拉闪烁与后台关闭

关键词：结束后无法点击、新开被禁用、pointerdown/pointerup 刷新丢点击、后台 toast 抢焦点。反查 `/home/codes/tools/webClx/docs/codex/tasks/terminal-session-switch-output.md` 的“结束终端后的下拉闪烁与点击丢失”及“关闭与后续工作并行”（2026-10-17；提交 `92c5f1d`、`dd0d9f2`）。

隐藏成功即用本地列表切换；按目标 session ID 去重，不能禁用全局动作。关闭结果使用不拦点击的 toast，不导航、不覆盖用户的新选择或对话框；保留隐藏失败不关闭、关闭失败可重试的边界。下拉无变化不重建，展开期间延迟更新以保留点击目标。回归入口：`tests/terminal-experience-save.browser.py`（延迟 close 时切换、新开、焦点保持）与 `tests/workspace_project_icons.browser.py`（按下/松开之间五次刷新）。

## Documentation

When a reusable conclusion is found, update `docs/codex/index.md` or the matching `docs/codex/tasks/*.md` topic. Keep the note short: symptom, root cause, files, verification, and deployment caveat.
