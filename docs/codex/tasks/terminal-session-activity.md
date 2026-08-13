# Terminal Session Activity Indicators

Date: 2026-05-23

## Goal

Terminal session selectors and the home-page `活动终端` table should expose a compact status prefix from the backend activity signal.

## Terminal Dropdown Sort Cycle

The terminal tools `切换终端排序` action and `Ctrl+Alt+O` cycle through workspace, Agent type, and status ordering. The selected mode lives in terminal-page state and must continue to apply after background session refreshes, renames, and local activity updates. Each mode uses the other dimensions plus the natural terminal name as tie breakers so groups remain deterministic. Status priority is error, retrying, working, building, completed, recent output, idle Agent, then plain idle.

## Signals

- `/api/terminal/sessions` includes `busy`, `activity_state`, `activity_label`, `activity_agent`, `activity_error_keyword`, `activity_error_signature`, `activity_error_continue_sent`, and `last_output_at`.
- `activity_state = "working"` takes priority when the last 10 readable terminal lines contain a Codex status line like `Working (5m 31s • esc to interrupt)`. Once this matches, do not fall through to the recent-output window; the prefix must remain `[工作中]`, not `[输出中]`.
- A Codex completion line like `Worked for 7m 10s` in the last 10 readable terminal lines is treated as completion: it shows `completed`/`[完成]` even after the latest output has been viewed. This check runs before generic recent-output and agent-process labels so a completed Codex session does not keep showing `[输出中]`, `[运行中]`, or `[空闲]`.
- A queued compile/deploy request is an explicit outstanding business task for its source terminal. The compile API persists `request_id -> source_terminal_id`; while that request is queued or running, terminal activity reports `building`/`[编译中]` after working/error checks and before `completed`, so an agent that stops after submitting a build is not shown as `待查看`.
- Build lifecycle state is owned by the compile coordinator, not by terminal text or the agent callback. After each request writes its final success, failure, or timeout status, the worker calls `POST /api/build/compile/complete` with the exact request id. This idempotently removes only that request from the terminal's active-build set before the toast/prompt notification path. Missing or failed terminal notification must not leave the terminal in `编译中`; the existing `completed_build_request_id` on verified prompt delivery remains a compatibility fallback during rolling upgrades.
- A terminal stays in `building` while any of its request ids remain active, so concurrent requests finish independently. Markers survive a webClx restart and expire after 24 hours as a last-resort stale-state guard.
- `activity_state = "error"` takes priority when the configured terminal tail text contains an error keyword.
- `activity_state = "retrying"` means the same matched error is still visible and a submitted or queued `继续` line already appears after it. Dropdowns show `[重试中]` and auto-continue treats it as part of the same handled error lifecycle.
- `activity_state = "completed"` replaces the old `ready` state. For non-Codex output it means the session produced output, is no longer in the recent-output window, and no connected terminal view has viewed the latest output timestamp since it stopped.
- Error matching is controlled by settings fields `terminal_error_match_line_limit` and `terminal_error_keywords`.
- Keep `terminal_error_match_line_limit` large enough for Codex status/tool output. The default is 12 lines.
- Default error keywords use the generic `stream disconnected before completion:` prefix so any reported disconnect reason, including wrapped `Upstream request failed`, enters the retry flow. They also include common retry/status failures such as `exceeded retry limit`, `last status: 429`, `last status: 503`, `last status: 404`, and `unexpected status 502 Bad Gateway: Upstream service temporarily unavailable, url:`.
- `MCP startup incomplete` is a nonfatal startup summary, not a terminal error. When it appears at or after a generic transport match such as `sending request for url`, discard that candidate entirely: do not mark the session as error and do not auto-send `继续`. A genuinely newer error after the MCP summary remains eligible.
- Default error keywords include quota-limit 429 variants such as `API Error: Request rejected (429)` and `已达到 5 小时的使用上限`, so reset-time messages are handled by the same terminal error state.
- `terminal_auto_continue_time_patterns` extracts reset times from terminal output with `{time}` placeholders, for example `限额将在 {time} 重置`. The backend exposes the parsed value as `activity_error_auto_continue_at`; the terminal page schedules `继续` for that time instead of sending immediately.
- `terminal_auto_continue_interval_seconds` is the per-terminal minimum cooldown between automatic `继续` sends. It defaults to `60`, is clamped to `1..=86400`, applies even when the error signature changes, and is enforced atomically by the backend for browser, scanner, and reset-time cron sources. Manual `继续` bypasses the wait but starts a fresh automatic cooldown.
- Repeated retries for the same error use `terminal_auto_continue_backoff_factor` (default `1.5`) and stop growing at `terminal_auto_continue_backoff_max_minutes` (default `20`, range `1..=1440`). The scanner copies both values into the runtime atomics on every pass, so saving either setting changes existing sessions and reset-time tasks without recreating them. The cap limits only exponential growth and never shortens `terminal_auto_continue_interval_seconds` when the base interval is larger.
- `terminal_auto_continue_active_window` (`HH:MM-HH:MM`, supports midnight crossing) is an unattended-hours gate for immediate no-reset retries. Reset-time quota tasks must still be collected outside the window, otherwise a captured GLM/Zhipu reset time can be hidden from the frontend and the error falls back to ordinary `继续`. Empty (default) disables the time-window gate (existing behavior).
- Codex drops the body of a 429 response and only prints `exceeded retry limit, last status: 429 ...`, so the terminal tail lacks the `限额将在 {time} 重置` text the time patterns rely on. The upstream proxy captures the authoritative Zhipu `code:1308` reset time from the 429 body into the shared `QuotaResetCache` (keyed by preset id and normalized base URL), and the scanner backfills a session's `activity_error_auto_continue_at` from it before collecting schedules. This backfill must apply to both `error` and `retrying`; an earlier submitted `继续` line only means the session is retrying, not that quota-reset scheduling should be skipped. This reset time is shared and non-consuming because one preset/account quota window applies to every terminal using that preset; per-session task dedupe uses the session id and error signature. The base URL key is the fallback when preset metadata is stale or missing. This capture applies only to upstreams whose `base_url` matches the Zhipu/BigModel filter reused from the quota dialog and the Codex_API auto-proxy provider list (`bigmodel.cn` / `zhipu`); see `src/quota_reset_cache.rs`. The session is matched to its preset via `update_api_preset_snapshot` (refreshed from the HTTP session-list path) by preset name or base_url.
- The backend error auto-continue runner must refresh `api_preset_snapshot` from `AuthPresetManager` before scanning. `/api/terminal/sessions` must refresh the same snapshot before building session infos, and both paths must backfill proxy-captured Zhipu reset times before collecting schedules. Otherwise a browser-open terminal can see `activity_error_auto_continue_at=null` and the frontend will treat quota 429s as ordinary retry errors.
- Add model-capacity retries such as `Selected model is at capacity. Please try a different model.` to the shared default keyword list in both `crates/settings_core/src/lib.rs` and `static/app.js`, so fresh installs and settings-page fallbacks recognize them consistently.
- Error matching compares raw tail text, whitespace-collapsed tail text, and whitespace-free tail text, so wrapped output like `Too Many` followed by `Requests` or mid-word terminal wraps like `servi` followed by `ce` can still match one keyword.
- Error state is not sticky: every session list refresh captures the current readable tmux tail and recomputes the state. When the configured tail no longer contains a keyword, the API returns the normal `输出中`/`空闲` state and the auto-continue handled marker is cleared.
- A matched error is stale after a later agent completion line such as `Worked for 7m 28s`. Do not keep reporting that old 429/quota line just because it remains inside the tail window; otherwise the frontend can see changing signatures and repeatedly auto-send `继续` even when no 429 is near the current prompt.
- `terminal_auto_continue_respect_manual_interrupt` defaults to `true`. When enabled, a matched error is stale after a later Codex manual-interruption marker such as `Conversation interrupted - tell the model what to do differently.` or `<turn_aborted>`; when disabled, the earlier error remains eligible for automatic retry after the cooldown. Match these markers across terminal hard wraps because Codex commonly renders the sentence on multiple lines. A genuinely newer error after the interruption remains eligible in either mode.
- When several keywords match, the backend chooses the last matching error occurrence in original terminal tail order. Cross-line whitespace-collapsed matches keep an index back to the original line order, so a wrapped error does not incorrectly outrank a later single-line error.
- On Linux/tmux, the backend resolves the tmux pane PID and walks the `/proc` descendant process tree to detect `codex` and `claude` processes. When no newer error/output/completed signal wins, `activity_state = "agent"` is displayed as `[运行中]`; frontend status prefixes should not render backend agent names such as `Codex`.
- Settings field `terminal_activity_agent_display` controls whether detected agent/program names such as `Codex` or `Claude` are hidden, shown before the status prefix, or shown after the terminal name. The default is `hidden` so agent names do not replace `[运行中]`. This setting must not control whether `activity_agent` is recorded; backend session listings should attach the detected agent name to `working`, `error`, `completed`, and `recent_output` states too, so the soft-keyboard 智能体 checkbox can reveal it without showing `未记录`.
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
