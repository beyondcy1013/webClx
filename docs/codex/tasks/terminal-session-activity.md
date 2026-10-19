# Terminal Session Activity Indicators

Date: 2026-05-23

## Goal

Terminal session selectors and the home-page `活动终端` table should expose a compact status prefix from the backend activity signal.

## Terminal Dropdown Sort Cycle

The terminal tools `切换终端排序` action and `Ctrl+Alt+O` cycle through workspace, Agent type, and status ordering. The selected mode lives in terminal-page state and must continue to apply after background session refreshes, renames, and local activity updates. Each mode uses the other dimensions plus the natural terminal name as tie breakers so groups remain deterministic. Status priority is error, retrying, working, building, completed, recent output, idle Agent, then plain idle.

## Signals

- `/api/terminal/sessions` includes `busy`, `activity_state`, `activity_label`, `activity_agent`, `activity_error_keyword`, `activity_error_signature`, `activity_error_continue_sent`, and `last_output_at`.
- `activity_state = "working"` takes priority when the last 10 terminal lines above unused trailing pane rows contain an active agent status. Supported signals include Codex's `Working (5m 31s • esc to interrupt)`, Claude/CodeBuddy spinner lines, CodeBuddy's `∴ Thinking...`, and AGY's `Thinking... (31s)` / `esc to cancel` hints. Once this matches, do not fall through to the recent-output window; the prefix must remain `[工作中]` even while visible output pauses or is already viewed. Read statuses newest first so a later completion supersedes an old working line.
- A Codex completion line like `Worked for 7m 10s` in the last 10 readable terminal lines is treated as completion: it shows `completed`/`[完成]` even after the latest output has been viewed. This check runs before generic recent-output and agent-process labels so a completed Codex session does not keep showing `[输出中]`, `[运行中]`, or `[空闲]`.
- A queued compile/deploy request is an explicit outstanding business task for its source terminal. The compile API persists `request_id -> source_terminal_id`; while that request is queued or running, terminal activity reports `building`/`[编译中]` after working/error checks and before `completed`, so an agent that stops after submitting a build is not shown as `待查看`.
- Build lifecycle state is owned by the compile coordinator, not by terminal text or the agent callback. After each request writes its final success, failure, or timeout status, the worker calls `POST /api/build/compile/complete` with the exact request id. This idempotently removes only that request from the terminal's active-build set before the toast/prompt notification path. Missing or failed terminal notification must not leave the terminal in `编译中`; the existing `completed_build_request_id` on verified prompt delivery remains a compatibility fallback during rolling upgrades.
- A terminal stays in `building` while any of its request ids remain active, so concurrent requests finish independently. Markers survive a webClx restart and expire after 24 hours as a last-resort stale-state guard.
- `activity_state = "error"` takes priority when the configured terminal tail text contains an error keyword.
- `activity_state = "retrying"` means the same matched error is still visible and a submitted or queued `继续` line already appears after it. Dropdowns show `[重试中]` and auto-continue treats it as part of the same handled error lifecycle.
- Context-window exhaustion uses the legacy `compact_then_continue` action key for compatibility, but its runtime action is compact-only: send `/compact` through the slash-command path and wait for Codex to finish. Do not append `继续`; compaction is the recovery action, and starting another turn would misclassify the compact operation as an ordinary retry. The compact sender shares the per-session automatic-action cooldown so browser and backend scanners cannot inject duplicates while compaction is running.
- `activity_state = "completed"` replaces the old `ready` state. For non-Codex output it means the session produced output, is no longer in the recent-output window, and no connected terminal view has viewed the latest output timestamp since it stopped.
- Error matching is controlled by settings fields `terminal_error_match_line_limit` and `terminal_error_keywords`.
- Keep `terminal_error_match_line_limit` large enough for Codex status/tool output. The default is 12 lines.
- Default error keywords use the generic `stream disconnected before completion:` prefix so any reported disconnect reason, including wrapped `Upstream request failed`, enters the retry flow. They also include common retry/status failures such as `exceeded retry limit`, `last status: 429`, `last status: 503`, `last status: 404`, `unexpected status 502 Bad Gateway: Upstream service temporarily unavailable, url:`, and the model-busy 529 body `该模型当前访问量过大，请您稍后再试`. Match the stable response body because 529 is nonstandard and Codex may render its reason phrase as `<unknown status code>`.
- OpenRouter server-tool failures are matched with the raw terminal fragment `Server tool \"openrouter:web_search\" failed`; retain the backslash-escaped quotes exactly as Codex prints the JSON error body. Add the keyword to both Rust settings-core defaults and the `static/app.js` settings fallback, then let the normal continue action send `继续`.
- `MCP startup incomplete` is a nonfatal startup summary, not a terminal error. When it appears at or after a generic transport match such as `sending request for url`, discard that candidate entirely: do not mark the session as error and do not auto-send `继续`. A genuinely newer error after the MCP summary remains eligible.
- Default error keywords include quota-limit 429 variants such as `API Error: Request rejected (429)` and `已达到 5 小时的使用上限`, so reset-time messages are handled by the same terminal error state.
- `terminal_auto_continue_time_patterns` extracts reset times from terminal output with `{time}` placeholders, for example `限额将在 {time} 重置`. The backend exposes the parsed value as `activity_error_auto_continue_at`; the terminal page schedules `继续` for that time instead of sending immediately.
- `terminal_auto_continue_interval_seconds` is the per-terminal minimum cooldown between automatic `继续` sends. It defaults to `60`, is clamped to `1..=86400`, applies even when the error signature changes, and is enforced atomically by the backend for browser, scanner, and reset-time cron sources. Manual `继续` bypasses the wait but starts a fresh automatic cooldown.
- Repeated retries for the same error use `terminal_auto_continue_backoff_factor` (default `1.5`) and stop growing at `terminal_auto_continue_backoff_max_minutes` (default `20`, range `1..=1440`). The scanner copies both values into the runtime atomics on every pass, so saving either setting changes existing sessions and reset-time tasks without recreating them. The cap limits only exponential growth and never shortens `terminal_auto_continue_interval_seconds` when the base interval is larger.
- `terminal_auto_continue_active_window` (`HH:MM-HH:MM`, supports midnight crossing) is an unattended-hours gate for immediate no-reset retries. Reset-time quota tasks must still be collected outside the window, otherwise a captured GLM/Zhipu reset time can be hidden from the frontend and the error falls back to ordinary `继续`. Empty (default) disables the time-window gate (existing behavior).
- Codex drops the body of a 429 response and only prints `exceeded retry limit, last status: 429 ...`, so the terminal tail lacks the `限额将在 {time} 重置` text the time patterns rely on. Every local proxy adapter that can carry a Zhipu request, including OpenAI upstream conversion and GLM 5.3's Anthropic conversion, passes non-success responses through `QuotaResetCache::capture_zhipu_quota_response`; the shared helper validates HTTP 429 plus the Zhipu host and parses the authoritative `code:1308` reset time. The scanner then backfills a session's `activity_error_auto_continue_at` before collecting schedules. This backfill must apply to both `error` and `retrying`; an earlier submitted `继续` line only means the session is retrying, not that quota-reset scheduling should be skipped. This reset time is shared and non-consuming because one preset/account quota window applies to every terminal using that preset; per-session task dedupe uses the session id and error signature. The base URL key is the fallback when preset metadata is stale or missing. Capture applies only to upstreams whose `base_url` matches the Zhipu/BigModel filter reused from the quota dialog and the Codex_API auto-proxy provider list (`bigmodel.cn`, GLM 5.3's `zcode.z.ai`, or `/zhipu/`); see `src/quota_reset_cache.rs`. The session is matched to its preset via `update_api_preset_snapshot` (refreshed from the HTTP session-list path) by preset name or base_url. Quota queries for `zcode.z.ai` model presets must still use `https://open.bigmodel.cn/api/monitor/usage/*`: the new model host returns `404` for those monitoring paths while the existing BigModel monitor host remains authoritative.
- The backend error auto-continue runner must refresh `api_preset_snapshot` from `AuthPresetManager` before scanning. `/api/terminal/sessions` must refresh the same snapshot before building session infos, and both paths must backfill proxy-captured Zhipu reset times before collecting schedules. Otherwise a browser-open terminal can see `activity_error_auto_continue_at=null` and the frontend will treat quota 429s as ordinary retry errors.
- A terminal can start under one API preset and later launch a GLM Codex process after the global preset changes. Its persisted `codex_api_preset_name` and `codex_api_base_url` then describe terminal creation, not the current model. Resolve the live `webclx run api <preset-id>` process first so a third-party GLM-compatible upstream is not mistaken for official Zhipu. Before retrying an official Zhipu quota-like 429, query `/api/monitor/usage/quota/limit`: a `TOKENS_LIMIT.percentage` below 100 permits immediate `继续`; an exhausted limit caches `nextResetTime` and creates the persistent scheduled retry; an unavailable or invalid response remains blocked.
- The synchronous session-list path must apply the same recent-GLM/shared-reset fallback after the background scanner fills the cache. Otherwise the backend can own a valid reset-time cron task while `/api/terminal/sessions` still returns `activity_error_auto_continue_at=null`; the browser then keeps calling the immediate auto-continue endpoint and repeatedly shows cooldown toasts. Scanner and HTTP listings must expose the same reset time.
- The browser can observe a new GLM quota 429 before the five-second background scan has populated the shared reset cache. The `POST .../auto-continue` endpoint is therefore the final send gate: it rechecks the current error plus raw 429 tail text, identifies Zhipu from the persisted session base URL or a recent GLM model marker, refreshes the Zhipu plan cache, and returns without sending `继续`. Do not rely only on the matched keyword because a line containing `last status: 429` may be recorded under the generic `exceeded retry limit` keyword. Do not rely only on the last model lines because a long reconnect display can displace them. The scanner then persists the reset-time task. A future `activity_error_auto_continue_at` is rendered as `等待重置`, not the misleading generic `重试中` label.
- Add model-capacity retries such as `Selected model is at capacity. Please try a different model.` to the shared default keyword list in both `crates/settings_core/src/lib.rs` and `static/app.js`, so fresh installs and settings-page fallbacks recognize them consistently.
- Error matching compares raw tail text, whitespace-collapsed tail text, and whitespace-free tail text, so wrapped output like `Too Many` followed by `Requests` or mid-word terminal wraps like `servi` followed by `ce` can still match one keyword.
- Error state is not sticky: every session list refresh captures the current readable tmux tail and recomputes the state. When the configured tail no longer contains a keyword, the API returns the normal `输出中`/`空闲` state and the auto-continue handled marker is cleared.
- A matched error is stale after a later agent completion line such as `Worked for 7m 28s`. Do not keep reporting that old 429/quota line just because it remains inside the tail window; otherwise the frontend can see changing signatures and repeatedly auto-send `继续` even when no 429 is near the current prompt.
- `terminal_auto_continue_respect_manual_interrupt` defaults to `true`. When enabled, a matched error is stale after a later Codex manual-interruption marker such as `Conversation interrupted - tell the model what to do differently.` or `<turn_aborted>`; when disabled, the earlier error remains eligible for automatic retry after the cooldown. Match these markers across terminal hard wraps because Codex commonly renders the sentence on multiple lines. A genuinely newer error after the interruption remains eligible in either mode.
- When several keywords match, the backend chooses the last matching error occurrence in original terminal tail order. Cross-line whitespace-collapsed matches keep an index back to the original line order, so a wrapped error does not incorrectly outrank a later single-line error.
- On Linux/tmux, the backend resolves the tmux pane PID and walks the `/proc` descendant process tree plus the session systemd scope to detect `codex`, `claude`, DeepSeek Harness (`dsh`), AGY/Antigravity, CodeBuddy, Grok, and Devin launchers. Devin matches a process whose own name is `devin`, including `~/.local/bin/devin` and the versioned binary under `~/.local/share/devin/cli/`. A search, an echoed argument, or `/usr/local/bin/devin` when that path is the Windsurf executable (`comm` is `windsurf`) stays unlabeled as Devin. Grok matches the `grok` command, the official `grok-linux-*` / `grok-darwin-*` / `grok-macos-*` binaries, a `node` wrapper whose script basename is `grok`, and the installer's `~/.grok/bin/agent` path. A bare `agent` command, `systemd-inhibit --who=grok`, or a Codex process whose model flag contains `grok` stays unlabeled as Grok. Executable tokens must match the launcher; searching for its package name in arguments or echoing its name must not create a false positive. When no newer error/output/completed signal wins, `activity_state = "agent"` is displayed as `[运行中]`; frontend status prefixes should not render backend agent names such as `Codex`.
- Settings field `terminal_activity_agent_display` controls whether detected agent/program names such as `Codex`, `Claude`, `DeepSeek`, `AGY`, `CodeBuddy`, `Grok`, or `Devin` are hidden, shown before the status prefix, or shown after the terminal name. The default is `hidden` so agent names do not replace `[运行中]`. This setting must not control whether `activity_agent` is recorded; backend session listings should attach the detected agent name to `working`, `error`, `completed`, and `recent_output` states too, so the soft-keyboard 智能体 checkbox can reveal it without showing `未记录`.
- Backend session listing compares recent tmux pane output snapshots and updates `last_output_at` when the output bytes change, including background sessions that are not the current websocket target.
- When a tmux pane snapshot is available, raw PTY timestamps only advance activity if the visible pane fingerprint also changes. Idle TUI cursor/control redraws can emit bytes without changing the pane and must not turn an already-viewed session back into `输出中` or `待查看`; raw PTY timestamps remain the fallback when snapshot capture fails.
- Frontend websocket output must not decide dropdown activity by itself; the frontend trusts backend `working` to show `[工作中]`, `completed` to show `[待查看]`, and `recent_output` to show `[输出中]`. Do not infer `[输出中]` from `last_output_at` alone because that timestamp can belong to output already viewed in the selected terminal.
- Backlog replay is not counted as new output, so switching sessions does not falsely mark old history as activity.
- Opening or switching to a session marks the latest observed output as viewed, and live WebSocket output sent to the connected terminal view advances the viewed timestamp. Recent output is `[输出中]` only while `last_output_at > last_viewed_output_at`; after it is viewed, the session falls back to `[空闲]` immediately instead of waiting for the recent-output window to expire.
- Session selection must also apply that viewed transition to the frontend's local session record before rendering: convert only `completed`/`recent_output` to `agent` or `idle`, and leave `working`, `error`, and `retrying` untouched. The subsequent session-list response remains authoritative, but the selected option must not keep showing a stale `[待查看]` while that response is in flight.
- Live-output viewed updates remain throttled to avoid terminal-manager lock contention, but every delivered output burst needs a trailing viewed update and pending updates must flush when its WebSocket closes. A session switch/resize can produce several tmux redraw chunks inside one throttle window; acknowledging only the first chunk leaves the final redraw timestamp unviewed and incorrectly changes the already-opened terminal to `待查看` after the recent-output window.
- `/api/terminal/completion-bell.wav` returns the built-in `audio/wav` notification sound. The terminal page plays it once per observed `session id + last_output_at` transition into `completed`; already-completed sessions on first page load are recorded as the baseline and do not ring retroactively.
- Settings field `terminal_completion_bell_enabled` defaults to `true` and controls only automatic terminal-page playback for Codex completion. The settings-page test button and the WAV API remain available even when automatic playback is disabled.
- Output activity polling and ordinary `loadSessions()` refreshes are throttled and must not rebuild the native session `<select>` while the user is interacting with it; defer until change/blur and then flush the pending update.
- Already-open terminal pages must treat settings broadcasts as a session-scope change, not just a visual settings reload. If `show_all_workspace_sessions` changes, reload `/api/terminal/sessions` with the new scope; always refresh scheduled paste and auto-continue tasks afterward. Otherwise an old terminal page can keep a path-filtered `state.sessions` list and fail to show a newly detected background terminal such as `ZCode_1`, even though `/api/terminal/sessions?all=true` and `/api/terminal/auto-continue-tasks` are correct.
- The terminal `定时 current/total` chip is derived from two independent streams: the scheduled-task list and the active-session selection. After `selectSession()` updates `state.activeSessionId`, immediately retick the chip; otherwise a task already loaded for `ZCode_1` can stay displayed as `定时 0/1` until the next scheduled refresh. Matching should consider the active state id plus the session picker value so transient switch timing cannot undercount the current session.
- The "检测到 ... 个自动继续定时任务" toast must be deduped by a stable business key such as `session id + task kind + due time`, not by cron marker or error signature. Quota reset tasks can keep the same reset time while the visible terminal error signature changes during repeated scans, and marker-based detection will report the same real schedule as "new" every refresh. This toast-level dedupe must not affect the chip count or backend schedule execution.

## UI Pattern

- Active terminal dropdown options are prefixed with `[工作中]`, `[编译中]`, `[错误]`, `[重试中]`, `[完成]`, `[输出中]`, `[空闲]`, or `[运行中]` for `activity_state = "agent"`.
- The same labels are used in the idle-session dropdown and option titles.
- Frontend label/status helper code shared by the home page and terminal page lives in `static/terminal-session-activity.js`, loaded before `static/app.js` and `static/terminal.js`.
- Shared terminal session localStorage preference and mutation-event helpers live in `static/terminal-session-storage.js`, also loaded before `static/app.js` and `static/terminal.js`.
- If Codex or Claude keeps running but no new terminal bytes arrive, the dropdown should show `[运行中]` instead of `[Codex]` or `[空闲]`.
- If `terminal_activity_agent_display` is `prefix` or `suffix`, the detected program tag is placed around the session option text, while the status label itself remains `[运行中]`.
- The terminal page `继续` checkbox lives beside `详细`; when enabled, it persists `terminal_auto_continue_on_error` through `/api/settings`. The backend scans all sessions and sends immediate no-reset retries only while this setting is enabled. If the readable tail already has a submitted or queued `继续` after the latest matched error (`› 继续`, `↳ 继续`, or a standalone `继续` line), the scanner records that event instead of immediately sending again. If `activity_error_auto_continue_at` is present, the scanner persists a scheduled reset-time task even when `terminal_auto_continue_on_error=false`, because the checkbox only gates immediate retries. Browser-triggered retries and cron tasks call `POST /api/terminal/sessions/{session_id}/auto-continue`; this endpoint revalidates the current error, queued input, manual-interruption policy, and cooldown. The manual `POST .../continue` endpoint remains unthrottled. A reset-time due task may ignore an older submitted `继续` line for the same error, but queued user input still wins.
- Auto-continue sends through the backend session-id input path, not the current websocket. This matters because background terminals can enter error state while the browser is closed or the user is viewing another terminal; they must not wait for a manual switch before receiving `继续`.
- The workspace embeds the full active-terminal management table below the directory browser and editor; there is no separate `活动终端` top-level page. Legacy `/sessions` and `#sessions` links resolve to the workspace.
- The embedded terminal table shows the same state as a small badge before the terminal name; do not color the terminal name itself for error state.
- `#sessions-status` is lightweight title-adjacent text and should auto-hide for non-sticky updates such as `终端会话列表已更新。`.
- Avoid forced option-list rebuilds while the dropdown is open, because native selects can jump or close during user selection. If the dropdown closes immediately after click, instrument `focusin`/`focusout` plus `Node.textContent` or child-list mutations on `#session-switcher`; the usual root cause is `renderSessions()` deleting and recreating options during an in-flight session refresh, not xterm stealing focus.
- Also inspect page-resume focus restoration. `window.focus`, `pageshow`, or `visibilitychange` can run `refreshTerminalInputVisibilityAfterPageResume()`; if its delayed callbacks call `focusTerminalForUserInput()` while `#session-switcher` has focus, xterm's helper textarea steals focus and closes the native select. Keep those callbacks gated by the same session-dropdown interaction guard.

## 2026-09-22 Devin 下拉识别

部署请求 `181142-18d79be04c32b39a` 于 18:12:01 完成，status=0。`webclx.service` 主进程 139966，运行二进制与 release 产物 SHA-256 同为 `a941a75944ae6eeb303dcc81e47609643b13b6f195e15cc577de07317a2c73e1`，`/api/auth/session` 返回 HTTP 200。会话列表中 s8018（webClx_7）的 `activity_agent` 为 `Devin`。

## Verification

```bash
node tests/terminal-error-status.test.mjs
node tests/terminal-session-activity.test.mjs
node tests/terminal-session-details.test.mjs
node tests/terminal-archives-and-idle.test.mjs
node tests/terminal-session-switch-output.test.mjs
cargo test terminal -- --nocapture
cargo check
```

## Slow Session List Refresh (2026-07-19)

Symptom: opening the active-terminal view or refreshing a terminal selector could wait more than
10 seconds. With 32 sessions, `/api/terminal/sessions?all=true` took 2.78 seconds in an otherwise
healthy sample, and the 10-second terminal task timeout appeared during heavier contention.

Root cause: every six-second terminal-page activity refresh scanned every session serially. Each
session captured the same tmux pane once for its output fingerprint and up to three more times for
working, error, and completion detection, then started another tmux process to read that pane's PID.
Several open terminal pages could therefore keep the tmux server busy and make an interactive list
request hit the global terminal-task timeout.

Fix: capture one readable recent pane snapshot per session and reuse it for the fingerprint and all
status checks. The snapshot keeps at least 200 lines and expands to the configured error line limit.
Load all pane PIDs with one `tmux list-panes -a` command for the process-tree detector. Full-history
capture (`-S -`) remains unchanged for terminal search and diagnostics. Identical concurrent scans
use a one-second shared cache behind a dedicated single-flight lock, so synchronized browser polls
wait for one tmux scan and then reuse its result instead of multiplying tmux work.

Regression check: `node tests/terminal-switch-performance-contract.test.mjs`.

## Viewed Session Returns To Pending (2026-07-19)

Symptom: an already viewed, inactive terminal could return to `待查看` after revisiting the page or
opening/closing the mobile keyboard, even though the terminal produced no new visible content.

Root cause: the viewed acknowledgement fingerprinted `tmux capture-pane -e` output including ANSI
escape sequences, while the optimized activity scan reused a plain-text pane snapshot. Alternating
between those two representations made identical pane content look changed. In addition, ordinary
capture output reflows when the pane width changes, so a soft-keyboard resize could also change the
hash without adding output.

Fix: both paths now fingerprint plain tmux text with `capture-pane -J`, which joins soft-wrapped
rows. Initial terminal replay and full-history search retain their original capture modes. The
activity scan still captures each pane only once.

A second restart-specific cause was that output observations lived only in `TerminalState`. A
service deploy discarded the viewed fingerprint, so the first post-restart scan treated the same
pane as newly observed output. The terminal registry now persists output observations, restores
only entries whose sessions survive restart, and keeps the process-local probe sequence out of the
serialized record. High-frequency live-output acknowledgements remain memory-only; selecting a
terminal, moving a visible terminal into the background, or closing a visible WebSocket provides a
stable persistence boundary.

Another restart-specific cause was ordering: the restore path called `observe_terminal_output_locked`
before marking the observation with `rebaseline_after_restore`, so a tmux attach/redraw could advance
`last_output_at` and turn an already viewed `[空闲]` session into `[待查看]`. Normal session restore and
explicit shutdown/save restore now capture the restored fingerprint as a baseline first and let the
first activity scan consume the rebaseline flag without advancing the output timestamp.

Regression checks: `node tests/terminal-output-fingerprint-stability.test.mjs` and
`node tests/terminal-output-observation-persistence.test.mjs`.

### Codex idle composer animation with quota footer (2026-09-26)

`zcodeRs_2` had finished its turn but periodically appeared as `输出中` in the terminal
selector. The output fingerprint already ignored Codex's idle-composer braille particles, but
its Codex footer recognizer required `Context … used · Fast`; Codex now inserts dynamic quota
text such as `5h 9% left` between those markers, so the idle-composer normalization was skipped.
The recognizer now accepts intervening footer items and excludes the volatile Codex status line
from the output fingerprint while keeping actual visible conversation output significant.
Regression covers the quota-bearing footer, particle-only changes, changing quota, and new reply text. Codex TUI animations are also disabled in the owning user's Codex
configuration with `tui.animations = false` (effective on subsequently started Codex processes).

### Codex idle composer animation (2026-09-16)

`quoteGateway_1` kept returning to pending after viewing because Codex animates
single-dot braille particles around `› Ask Codex to do anything`, even after
completion/compaction. Plain-text pane snapshots therefore still changed.
`manager/output_fingerprint.rs` canonicalizes only that empty composer and its
two particle-only neighboring rows, gated by the Codex Context/Fast footer.
Both viewed acknowledgements and activity scans use this fingerprint. Reply
text, typed input, braille in output, and other programs remain significant.
Regression: `cargo test --bin webclx output_fingerprint`; backend deployment is
required (a browser refresh alone does not install the fix).

## 终端工作现场恢复（2026-09-18）

项目管理菜单提供 **恢复终端** 入口。后端把终端工作现场保存在
`.webclx-terminal-workspace-restores.json`，记录包含稳定 session ID、Linux 用户、
工作目录、终端名、分组 token/名称/颜色/顺序、输入历史、以及可探测到的 Codex /
Claude resume ID 和恢复命令。创建、改名、分组成员更新、删除、终端消息发送、
“保存会话并关机/重启”和服务启动导入都会更新该文件；其中分组更新会把新成员和旧组
被移出的成员一起落盘，避免留下半更新状态。

`GET /api/terminal/workspace-restores` 列出记录；`PUT` 先捕获当前全部现场再返回；
`POST {"session_ids":[]}` 复用既有 shutdown restore 机制：存活 tmux 会话直接重连，
缺失会话按原目录/用户/名称重建，有 resume 命令时注入对应 Agent 恢复命令，并恢复输入
历史与分组字段。对话框按 Agent、终端名、分组、目录和更新时间展示，支持单条、勾选和
全部恢复；目录缺失标记不可恢复，成功恢复后记录次数并阻止同一次记录重复恢复。

普通 Shell 终端保存的是结构、名称、分组和输入历史，不伪造 resume ID。tmux pane 已
消失且没有可探测 Agent 身份的记录，只能恢复到原目录 Shell；这是可用边界。写盘使用
同目录临时文件和原子 rename，不保证进程崩溃瞬间的多文件事务。回归：
`cargo test --bin webclx terminal_workspace_restore`、
`node --test tests/terminal-workspace-restores.test.mjs`、
`python3 tests/terminal-workspace-restores.browser.py`。前端运行目录改动以
`patches/terminal-workspace-restores-static.patch` 留存。

## 终端分组跟随（2026-09-16）

入口为 **终端工具 → 终端分组**。勾选当前终端的搭档后保存；取消其余成员的勾选即可解除分组。
允许多个成员。组内按稳定终端 ID 顺序显示，激活任一成员会按该组最近激活成员的位置整体移动，
不会改写其他成员的激活时间。工作区、Agent、状态排序先保留自身排序语义，再把组折叠为连续块。
过滤掉的成员不会被补回列表。

`PUT /api/terminal/sessions/{session_id}/group` 接收 `member_ids` 数组，当前 ID 隐式加入。
服务端先校验全部 ID，再在一次状态写锁内更新成员、清除旧组与残留单成员组，持久化并广播
`group_updated`。普通 rename 不接收分组参数，不会清除分组或把分组操作误标记为手动改名。
对话框冻结打开时的 session ID；保存失败保留选项供重试。

验证：`tests/terminal-session-groups.test.mjs`、`tests/terminal-session-groups.browser.py`、
Rust `grouped_sessions_` 和 `recent_opened_sessions_sort_first`。浏览器测试拦截会话 API，
覆盖桌面与 375px 手机视口，不操作用户终端。

### 群组管理显示与成员改名（2026-09-19）

终端下拉的统一名称在项目图标后追加 ` · 分组名`；原始 `<option>` 不重复拼接，由图标
选择器用 `dataset.groupName` 渲染，避免改名后留下旧后缀。`终端分组` 对话框新增
“管理分组”切换，可用于管理当前会话所在组或其他已存在群组；“修改成员终端名”走专用
改名弹窗，保存后用既有终端消息 API 只通知同组其他成员，排除被改名终端本人。该能力
复用会话推导分组，不新增后端分组注册中心；静态改动留存于
`scripts/patches/terminal-group-manager-static.patch`，版本 `20260919manager1`。

静态目录仍是 `/home/bin/webclx/static` 的符号链接。Git 不跟踪链接目标内的文件；本次前端
分组专属改动另以 `patches/terminal-session-groups-static.patch` 与 `scripts/patches/terminal-group-async-notify-static.patch` 留存。保存分组与修改成员名时，配置持久化与列表刷新完成后立即关闭对话框窗口返回，群组变动通知与建队须知消息发送动作在后台异步执行并向状态栏汇报，避免因等待终端投递确认导致分组窗口阻塞。后端改动需要编译部署队列成功后才可使用保存接口。


### 有界并行活动探测（2026-09-17）

独立 tmux socket 使每个会话仍需自己的快照和 pane PID 查询。活动扫描在既有
`activity_probe_scan_lock` 内以最多 4 个 scoped 线程处理连续输入分片，线程间共享
同一份只读进程表；空列表不读进程表，单会话直接处理。按输入顺序预分配探测序列号，
按分片顺序合并结果，再统一写缓存与会话状态。线程创建失败记录告警并原地处理该片；
探测 panic 传播，不能发布部分缓存。生产辅助函数与并发/保序/错误保留回归共用。

此改动不增加 tmux 调用次数，不改变缓存 TTL 或扩展 manager 写锁。同步 tmux
命令原有的本地超时缺口仍存在，不能把本批描述为完整的超时治理或死锁修复。线上比较需要保留会话数、采样间隔和负载波动说明。

### 空闲模型标识复用活动快照（2026-09-17）

空闲分支的模型标识由 probe 收集阶段从同一份可读快照计算，状态写锁内仅合并
预计算字段，不再为此启动第二个 tmux 子进程。保留既有末 8 个逻辑行、大小写
不敏感及 DeepSeek 优先于 GLM 的规则；更早的历史不参与匹配。无需另外复制末
200 行：tmux `-S -200` 表示历史屏幕行，不能等同于 `-J` 输出的 200 个逻辑行。

模型标识现在与其它活动字段对应同一采样时间，并跟随既有一秒缓存；此前是在
状态合并时重新采样。首轮捕获失败时模型标识为空，不再通过锁内重试恢复该字段。
这不扩大 TTL，也不改变工作中、错误、编译中、未读输出及 Agent 状态的优先级。
行为回归覆盖生产模型提取及状态合并；性能契约防止合并阶段重新引入直接子进程调用。


### 活动 tmux 子进程本地边界（2026-09-17）

Unix 活动快照与 pane PID 查询共用同步非阻塞双管道执行器：每条命令预算 2 秒，
stdout/stderr 合计上限分别为 8 MiB 和 64 KiB。超时、超限、读取失败均报错，
ChildGuard 在错误及 panic 时 kill+wait；成功仅返回完整输出，保留退出状态。
活动快照错误沿原有路径处理，PID 查询错误记录告警并返回 None。初始回放、
全历史搜索等其它命令不经过此封装；非 Unix 保留原执行路径。

每轮限制读取次数，EOF 管道退出 poll，避免持续输出或 HUP 饿死 deadline。
本边界约束直接 tmux 客户端，不终止 tmux server 或用户终端，也不承诺整轮扫描
在 10 秒内完成；spawn 和内核 kill/wait 不构成硬实时保证。缓存、并发及序列号不变。
回归直接执行生产 helper，覆盖完整双管道、合计超限、持续输出、超时精确 PID
回收与 panic 回收；性能契约检查两个生产入口的接入。


### 定时输入整轮阻塞隔离（2026-09-17）

调度循环将取任务、避让窗口重排、注册表保存和批次发送放入同一个 awaited
spawn_blocking。只迁移发送仍会让任务表锁竞争及同步写盘阻塞 Tokio worker。
保持每轮串行、现有任务顺序和 notify_one permit，不增加发送并发或后台队列。
此改动不移除任务表的持久化锁，也不改变写盘失败仅告警的既有语义。
current_thread 回归分别覆盖发送屏障及真实任务锁竞争，后者需在独立锁持有线程
的兜底释放前观察到心跳；RAII 释放及有限等待避免失败时卡死。

## Cline 空闲状态与输入光标（2026-09-29）

后续验证：空闲输入提示修正后，s9149 不再显示 `工作中`，但进程检测仍落入
`activity_state = "agent"` / `activity_label = "Cline"`。该状态的历史含义是
“检测到空闲 Agent 进程”，前端本就渲染为 `空闲`；后端却把 `busy` 和 label
设为运行语义，导致仍读取 `activity_label` 的调用方显示 `运行中`。现在
`agent` 快照统一为 `busy=false`、label=`空闲`，同时保留 `activity_agent`
供智能体类型、排序和展示配置使用；孤儿 Agent 仍保持异常状态。

追加复现：开启“自动压缩”的 Cline 终端首轮完成时，进程类型可能刚被后端识别。
前端把它当作 Codex 兼容会话发送 `/compact` 后，Cline 不会输出 Codex 的
`Context compacted` 完成标记，`autoCompactCompactingSessionIds` 因此永久残留，
下拉状态持续显示 `压缩中`。自动压缩现在按 `activity_agent` 排除 Cline 和
AGY；若发送后才识别出 Cline，状态同步也会立即撤出 compacting 集合。静态
资源更新为 `terminal-auto-continue.js?v=20260930-cline-compact`。

再复现：Cline 静止时 pane 内容和输出时间均不变，但 15 秒 recent-output 窗口
到期后从 `输出中` 变为 `待查看`。Cline 的 `❯ Plan something` / `❯ Ask anything`
本身就是明确的空闲输入 footer，不该再走通用“未被查看输出”规则。后端现在
为 probe 记录 `cline_idle_prompt`；命中时立即把该输出水位标记为已查看，并
返回空闲 Agent 状态。真实工作中（Working/spinner）仍优先，因此不会掩盖任务
运行。

补充复现：system_2__当前硬盘分区情况 的 Act 输入提示为 ❯ Ask anything...。
旧规则仅排除 Plan something，而 anything 也以 ing 结尾，被当作工作动词。
现同时排除两种占位提示，并要求 Claude spinner 带实际状态符号前缀，避免
普通思考正文误匹配。排除仅作用于当前行，其他行的真实工作指示仍然有效。
回归覆盖 Act/Plan、两种省略号、历史思考正文与真实工作指示共存。

症状：Cline 终端已完成回答并显示 `❯ Plan something...`，会话仍显示
`[工作中] Cline`；同时浏览器把底部状态行当作 Codex 式可纠偏光标，导致可见
按键位置在底部状态与输入框之间跳动，用户难以判断输入会落到哪里。

根因有两层：

- 活动 matching 的 Claude spinner 规则是“任意行含 gerund + 省略号”。Cline
  输出内容里保留 `Thinking: ...处理中...` 这类历史/思考文本时满足该规则，
  覆盖了后面的空闲输入 footer。
- 前端 application cursor 检查只看固定倒数第三行；Cline 在输入行右侧绘制
  inverse-video 光标/竖条，但逻辑光标在底部状态行，纠偏逻辑误认为应用没有
  自绘光标。

修复：

- `line_is_cline_idle_prompt()` 识别 `❯ Plan something` 空闲 footer；命中时该行
  明确不是 working 状态，避免历史思考文本把空闲会话置为 busy。
- `terminalViewportEditableApplicationCursorRows()` 扫描整个 viewport 的
  inverse-video 单元格，把 Cline 输入行右侧的自绘光标传给 cursor guard；检测器
  对应用自绘光标返回 null，不隐藏原生光标，也不再把纠偏标记画到错误行。

验证：

```bash
cargo test terminal_working_status_detection_recognizes_cline_idle_prompt
node tests/terminal-cursor-guard.test.mjs
python3 tests/cline-input-activity.browser.py
```

## CodeBuddy / AGY 输出停顿误判（2026-10-11）

根因在后端 pane 状态匹配：CodeBuddy 的 star 动画包含 `✶ ✸ ✹ ✺ ✹ ✷`，旧规则漏掉
`✸` 和 `✷`，动画切到这两帧时工作信号会消失；自定义中文进度只有运行时长、尚无
token 时也会漏判。AGY 的 `Thinking...` 和取消提示未被覆盖。此外，tmux 快照保留
屏幕底部空白行，旧的最后 10 行窗口可能完全落在空白区域。工作信号漏判后，未查看
输出超过 15 秒窗口就进入 `completed`，造成任务中途突然显示已结束。

匹配现在覆盖完整 CodeBuddy star 帧、思考指示、带时长的自定义进度及 AGY 活动提示，
并在截取尾部前剔除末尾空白行。工作/完成按新旧顺序判断，新的完成行会清除历史工作
提示；普通正文、输入框和 `Thought for` 历史摘要不作为活动信号。输出时间戳、查看
水位和通用 15 秒窗口保持原有含义。

回归用例 `terminal_working_status_detection_*` 与
`terminal_agent_activity_preserves_working_during_output_pauses` 覆盖所有动画帧、空白
屏幕行、无 token 工具等待、超过输出窗口的重复快照、已查看输出和真正完成后的状态。
`terminal_error_snapshot_ignores_unused_pane_rows_and_stale_errors` 同时约束错误窗口与
完成/MCP 启动摘要的旧错误抑制行为。后端改动经编译队列上线后才对实时会话生效。

本次运行现有 Node 契约测试时，`terminal-session-activity.test.mjs:376` 的 WebSocket
输出代码形状断言与 `terminal-error-status.test.mjs:295` 的 compact guard 形状断言均
失败；已核对原提交同样不满足这两个断言，属于既有测试漂移。实际状态判定由上述
Rust 回归覆盖，不以这两个 Node 用例冒充通过。

部署验收：请求 `224358-18dd5fa94a8282b9` 于 2026-10-11 22:47:06 完成，
`state=succeeded`、退出码 0。`terminal_work` 9 项、停顿状态回归 1 项、
`terminal_error` 17 项全部通过；workspace check、Clippy 与 release 构建成功
（保留现有编译告警），安装前图标门禁另 5 项通过。集中证据位于
`/home/bin/webclx/compile/runs/20261011T224403-1186397-27819/` 的
`build-1.log` 和 `install-report-1.json`。

上线后服务为 active/running，PID `1248729`，`/api/auth/session` HTTP 200；
release 产物、安装文件及 `/proc/1248729/exe` 的 SHA-256 均为
`711a05ce9610748aa95f8bbbdbfb7a9b9a0b379be3ea777e953d8c3eb2ad4ba0`。
静态资源未改动，运行链接仍指向源码 `static/`。只读现场核验中，CodeBuddy
`s9919` 的完成行对应待查看状态，AGY `s9921` 的启动空闲输入框对应空闲状态；
当时无活动中的 CodeBuddy/AGY 任务，故不宣称完成了真实任务停顿观察。
唯一补查唤醒 `paste-1791729911474-9933` 已取消。

## CodeBuddy / AGY 的 /compact 禁令（2026-10-11）

复核发现此前 AGY 仅在浏览器完成后自动压缩时被排除，后端上下文错误恢复没有
AGY 门禁；CodeBuddy 后端只有可关闭的跳过开关，前端自动压缩未排除它。
现在 CodeBuddy、AGY（含 Antigravity 别名）及 Cline 不接收 webClx 生成的
`/compact` 或带参数的 `/compact`：前端候选、倒计时、软键盘斜杠发送、快捷命令
和 auto-typed 入口共用 Agent 判断，后端错误恢复、斜杠发送及 silent 自动输入
以真实 Agent 进程检测再次拦截。上下文错误保持错误状态，不伪装压缩成功。

禁令不依赖 `terminal_auto_compact_skip_codebuddy`；旧字段保留兼容但不能绕过
禁令，设置页显示固定保护，相关会话自动压缩复选框禁用。已排程后才识别出
不支持的 Agent 时，发送前会撤销；`/compact` 软键盘动作冻结目标 session ID，
切换终端不会把该命令发给新选择的 Agent。Codex / Claude 保留原有压缩流程。
普通 WebSocket / input API 的原始键盘字节仍按终端 I/O 规则转发，不拦截或改写
用户亲自键入的内容；禁令范围是 webClx 的命令与自动化入口，不是 Agent 原生命令权限。

回归：`terminal_compact_agent_guard_*`、`tests/terminal-compact-agent-guard.test.mjs`
和 `tests/terminal-compact-agent-guard.browser.py` 调用生产规则，覆盖大小写、混合
Agent、延迟识别、禁止发送、禁用控件、切换终端与 Codex / Claude 正例。
既有 Node 测试漂移另有三项：`terminal-tool-api-execution.test.mjs:421` 仍要求
当前 HEAD 已不存在的 compact 前置 Esc；`specified-preset-actions.test.mjs:110`
和 `terminal-specified-task.test.mjs:331` 仍期望迁移前 `webClx` 路径，实际为
`tools/webClx`。前者已用原 HEAD 源码复现同一失败，后两项涉及的 launcher
源文件本次未改动；不以这些用例冒充通过，也不新增 Esc 打断运行中的任务。
本次相关 Node 检查 20/20 通过，375px 与 1440px 浏览器隔离测试通过且零真实
终端 I/O。所改 Rust 文件的格式检查仍有三处原有格式漂移（活动检测两处、
manager import 一处），本次新增代码格式已对齐。

部署验收：修复提交 `b65516d`，请求 `230636-18dd8113d8c9be7f` 于
2026-10-11 23:09:33 完成，`state=succeeded`、退出码 0。
compact Agent 门禁 1 项、Agent 进程识别 10 项、compact 错误去重 1 项通过；
workspace check、Clippy 和 release 构建成功，安装前图标门禁另 5 项通过
（保留现有编译告警）。集中证据位于
`/home/bin/webclx/compile/runs/20261011T230641-1532916-7975/` 的
`build-1.log` 和 `install-report-1.json`，审计仅二进制修改，无缺失文件。

上线核验：服务 active/running，PID `1590956`，`/api/auth/session` HTTP 200；
release、安装文件及 `/proc/1590956/exe` 的 SHA-256 一致：
`f395654f863e8b2a1717918bcd50e1a0f3a62bcbb15e209809f538df86f5c55f`。
运行静态链接仍指向源码 `static/`；5 个改动脚本及 `/assets/terminal.html`、
`/assets/index.html` 的 HTTP 哈希均与源码一致，HTML 中对应 loader 已更新为
`20261011-compact-agent-guard`。此次未获取登录后的页面路由，未向用户终端
发送 `/compact` 做破坏性探测；发送禁止行为由上述隔离回归验证。
补查唤醒 `paste-1791731337494-9935` 已移除，复查无同 ID 待办。
