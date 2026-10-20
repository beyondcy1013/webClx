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

- **自动继续误触发／动态字符／工作中被追加继续（2026-10-18）**：反查 `docs/codex/tasks/terminal-continue-send-unification.md` 的 Working-status guard；共享 `terminal_error_keyword_match_from_snapshot` 应与活动列表一致，先检查末尾原生工作状态，再匹配错误，覆盖 `/auto-continue` 与到期任务。宽泛 URL/请求关键词可能命中正常工具输出；无触发快照时不要断言动画是错误。回归 `terminal_error_snapshot_does_not_retry_while_agent_is_working`；修复 `563c296`，本机部署验收 `222705-18df8229b141d59a`。
- **部署门禁静态链接方向过时**：`workspace icon gate: source static must link` 曾拦截已构建产物；按当前 AGENTS.md 校验“运行 static 绝对链接 → 源码实体”，不要迁移实体迎合旧脚本。反查 `docs/codex/tasks/static-deployment.md`、`scripts/check-workspace-icon-gate.sh`、修复 `807fca6`；构建成功而安装失败时，经队列复用已验证 SHA 产物，无需重复编译。
- Error recognition is split between `src/terminal/manager/error_detection.rs`, built-in settings merge in `crates/settings_core`, and browser defaults. Preserve the same keyword and action contract across all active owners and lock it with focused Rust and Node tests.
- An ordinary retry uses the canonical per-session auto-continue path. Cooldowns and timers must be keyed by stable session ID and cleared when that session leaves `error` or `retrying`; one terminal must never delay another.
- Context-limit failures can select `compact_then_continue` instead of ordinary continue. Keep action selection separate from keyword matching, submit `/compact` once, and treat an actual `Context compacted` marker as completion.
- For terminal scroll/focus defects, distinguish explicit user intent from layout events. IME, `visualViewport`, page resume, fit, and replay settle must preserve the captured scroll/focus state; only explicit input, terminal switching policy, or live-output follow mode may force a new position.

## 终端滚动修复子技能

终端切换未到底、输入/回答后跳顶部、新输出不跟随、回放/共享尺寸/IME 导致位置错乱、页面恢复后首次滚动回弹时，先读 [终端滚动子技能](subskills/webclx-terminal-scroll/SKILL.md)。按页面、逻辑缓冲区、DOM 滚动区域分别取证；主技能只保留入口。

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

## 终端输入与退格（Agent TUI 按读取批次解析）

- **连续退格删不动 / 退格无效（2026-10-20）**：ink 类 Agent TUI（CodeBuddy）把一次 stdin 读取当成一次按键，多于一个字节的读取按“粘贴文本”处理并丢弃退格字节（`0x7f`/`0x08`）。webClx 合并按键后，两次退格落进同一次读取就一个都删不掉。反查 `docs/codex/tasks/terminal-input-keystroke-delivery.md`；实现 `src/terminal/input_pacing.rs`（按退格切段 + 5ms 间隔），三条写入路径 `terminal/session.rs::write_input`、`terminal/manager.rs::write_session_input_inner`、`terminal/tmux.rs::send_tmux_input`；修复 `d06407f`。
- 取证用“读取分块探针”而非看画面：一次投递 `\x7f`×N，探针应输出 N 条 `len=1`。同类症状先怀疑控制字节与文本合并（Ctrl-A..Z 同理），不要先改渲染或前端批处理。

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
- 工作区有无关未提交改动时，先用 `preserve-dirty-deploy-state.sh save` 记录 stash OID，确认干净且 HEAD 为目标提交后入队；匹配回调后按 OID `restore`，不要把无关改动带入部署。整树含未跟踪文件时先存 `git diff | sha256sum` 与应用备份，恢复后必须比对指纹一致。
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
- **Qoder 识别（2026-10-20）**: `src/terminal/activity.rs::is_qoder_process` 按 comm/cmdline 识别 `qodercli`、`qoderclicn`、`qoder-cli`、`qoder` 及 node/bun 包装，输出 `activity_agent=Qoder`；旧终端需重建并重启后端后才显示。反查 `docs/codex/tasks/terminal-session-activity.md`；回归 `cargo test --bin webclx agent_detection_recognizes_qoder_launchers`。

## 结束终端卡顿、下拉闪烁与后台关闭

关键词：结束后无法点击、新开被禁用、pointerdown/pointerup 刷新丢点击、后台 toast 抢焦点。反查 `/home/codes/tools/webClx/docs/codex/tasks/terminal-session-switch-output.md` 的“结束终端后的下拉闪烁与点击丢失”及“关闭与后续工作并行”（2026-10-17；提交 `92c5f1d`、`dd0d9f2`）。

隐藏成功即用本地列表切换；按目标 session ID 去重，不能禁用全局动作。关闭结果使用不拦点击的 toast，不导航、不覆盖用户的新选择或对话框；保留隐藏失败不关闭、关闭失败可重试的边界。下拉无变化不重建，展开期间延迟更新以保留点击目标。回归入口：`tests/terminal-experience-save.browser.py`（延迟 close 时切换、新开、焦点保持）与 `tests/workspace_project_icons.browser.py`（按下/松开之间五次刷新）。

## CodeBuddy 账号导入凭据防丢失与格式兼容（2026-10-20）

- **症状表现**：
  1. CLI 抛出 `401 Authentication required`，排查发现账号的 `refresh_token` 丢失，导致 AccessToken 过期后无法自动刷新；
  2. 重新导入包含更新 token 的 JSON 时，已有账号的有效 `refresh_token` 反被空值冲掉；
  3. 导入不同来源的 JSON（如 `workbuddy2api-rs` 账号、`wb-switch` 导出）时，因字段被包裹在 `auth`/`auth_raw`、`account`/`profile_raw` 或使用 `expiresIn` 秒级偏移而解析失败；
  4. 预设管理中通过剪贴板导出 `codebuddy_presets` 时，导出内容仅含脱敏元数据（`meta()`），无法用于跨机器迁移或备份恢复。
- **根因分析**：
  1. `src/codebuddy.rs::upsert_account` 匹配到已有账号时直接全量替换结构体，未对 `refresh_token` 等敏感凭据做空值防覆盖保护；
  2. `src/codebuddy.rs::parse_codebuddy_import_account` 仅读取顶层扁平字段，不支持嵌套结构，且未从 `expiresIn`/`refreshExpiresIn` 自动推算绝对毫秒时间戳；
  3. `src/preset_sync.rs::export_account_presets_to_clipboard` 对 CodeBuddy 调用了面向 UI 展示的 `list_accounts()`，导致 `access_token` 与 `refresh_token` 被过滤。
- **治理与规范**：
  - **凭据继承保护**：更新已有账号时，新记录中为空的 `refresh_token`、`refresh_expires_at`、`access_token`、`enterprise_id`、`nickname` 必须继承已有记录，积分缓存与 `switch_count` 不得清零；
  - **嵌套与格式兼容**：导入解析兼容顶层和 `auth`/`auth_raw`、`account`/`profile_raw`，并按 `createdAt + expiresIn * 1000` 自动换算绝对毫秒；
  - **剪贴板导出全量凭据**：在 `CodeBuddyManager` 实现 `export_accounts(&HashSet<&str>)`，剪贴板导出保留完整凭据；
  - **核心文件**：[`src/codebuddy.rs`](file:///home/codes/tools/webClx/src/codebuddy.rs)、[`src/preset_sync.rs`](file:///home/codes/tools/webClx/src/preset_sync.rs)；
  - **回归验证**：
    ```bash
    cargo test --bin webclx codebuddy
    # 覆盖：
    # - imports_nested_auth_and_account_json_correctly
    # - imports_auth_raw_with_refresh_expires_in
    # - upsert_account_preserves_existing_refresh_token_when_incoming_empty
    bash scripts/rebuild-and-deploy.sh
    ```

## Devin 账号库重复 id 与导入新旧判定（2026-10-20）

关键词：同 id 两条账号、同邮箱不同 token、删一条删掉两条、导入把新 token 冲掉、旧凭据覆盖新凭据、Devin 账号去重、导入跳过提醒。

账号 id 只由邮箱派生，`DevinManager::load` 在当前 `credentials.toml` token 未入库时直接 push，于是同一 id 出现第二条；导入解析曾把 `saved_at` 硬编码为当前时间，新旧无法比较。现加载按 `(client_type,id)` 收敛（`dedupe_accounts`）、登录凭据登记改为 `upsert_current_login` 原位更新；导入按来源 `saved_at` 判定：新的覆盖，旧的跳过并返回 `skipped`，剪贴板导入转成 `warnings`。直改 `webclx-devin-accounts.json` 后必须重启服务，否则内存态回写覆盖磁盘。反查 `docs/codex/tasks/devin-desktop-auth.md`「重复账号记录收敛」「导入新旧判定」；提交 `d74d4bc`、`b4a7677`；回归 `cargo test --bin webclx devin::tests`。

## 终端“压缩中”状态误判（2026-10-20）

关键词：下拉显示压缩中、只发送了 /compact 并未执行、状态不退出、后台终端永久压缩中。
前端曾在 `/compact` 字节发送成功即本地置 `压缩中`；命令未真正执行（停在 shell 提示符、Agent 忙）或后台终端没有本地 xterm 缓冲时该状态永不退出。现由后端活动探针按 pane 真实进度行判定（`Compacting context`、`✻ Compacting conversation…`、`正在压缩`，最近 15 行且其后无完成行），`activity_state=compacting` 优先于 `working`；前端不再本地伪造，`已压缩` 改为离开 compacting 后写入。反查 `docs/codex/tasks/terminal-session-activity.md`「压缩中状态按真实压缩输出判定」；回归 `cargo test --bin webclx compacting`、`tests/terminal-compact-agent-guard.test.mjs`；提交 `2ad45b7`。

## Codex 旁白型忙碌行被误判为已结束（2026-10-20）

关键词：Codex 明明还在跑却显示完成、旁白型状态行、计时加 esc to interrupt、只认 Working ( 漏检、deepseek 预设状态不准。

Codex 会把即时旁白（如 `Let's inspect environment, ... (38m 21s • esc to interrupt)`）渲染成底部忙碌行；旧规则只匹配 `Working (` 前缀，旁白换词就漏检，会话列表回落成 `completed`。现判据是同一行尾括号同时含有效耗时与 `esc to interrupt`，排除队列提示，并按自下而上的最新 turn 信号裁决（较新 `Worked for`/完成行覆盖较早忙碌行）。实现 `src/terminal/manager/error_detection.rs::line_is_codex_working_status`；回归 `cargo test --bin webclx terminal_working_status_detection`（8 项）；提交 `e3f6a17`，部署 `222400-18e042e1659cfed5` 后生效，改静态资源无效。反查 `docs/codex/tasks/terminal-session-activity.md`「Codex 旁白型工作状态」。

## Documentation

When a reusable conclusion is found, update `docs/codex/index.md` or the matching `docs/codex/tasks/*.md` topic. Keep the note short: symptom, root cause, files, verification, and deployment caveat.
