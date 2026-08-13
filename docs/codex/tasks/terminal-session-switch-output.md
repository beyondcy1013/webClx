# Terminal Session Switching Output Isolation

Date: 2026-05-18

## Symptoms

- Switching terminal conversations from the dropdown could leave the visible terminal showing another session's fast-scrolling output.
- Returning to a terminal session forced the replayed history to the bottom instead of preserving the last scroll position for that session.
- Long Codex output could appear to lose visible lines when output was written faster than the browser terminal could render.
- Typing several characters or moving the cursor could appear several seconds late when high-frequency terminal output or replay had built up small browser-side write/input units.
- Creating a new terminal could show text from an older terminal above the fresh prompt.

## Root Cause

`static/terminal.js` wrote each websocket binary message directly into xterm with `term.write()`. During high-volume output, chunks already queued inside xterm could continue rendering after the page had switched `state.activeSessionId` and opened a different websocket.

The backlog replay completion path also always called `scrollTerminalToBottom()`, so any per-session scroll position was overwritten on every reconnect or dropdown switch.

New terminal creation used the same tmux "ensure session exists" path as session restore. If the registry's next `sN` id matched an orphaned `webclx_sN` tmux session, the backend attached the new stored session to the old pane, and the initial websocket snapshot correctly replayed that stale tmux history.

Explicit new terminal creation can also leak old output entirely on the frontend. Clearing the page-level output queue and calling `term.reset()` does not cancel bytes already handed to xterm's internal write queue. A pending `term.write()` callback from the old terminal can render after the reset and make the fresh terminal appear to start at the end of the previous terminal display.

High-volume live output could also disappear from browser history before reconnect: the PTY reader broadcast 8KB chunks through a fixed 256-entry `tokio::sync::broadcast` channel, so a slow websocket sender could lag by more than ~2MB and receive `RecvError::Lagged`. The old socket loop only logged the skipped chunk count and resumed with newer chunks. The frontend also trimmed queued live output when the user typed, which traded away history for input responsiveness.

普通的十几行输出也可能在浏览器历史里缺行，即使 WebSocket 字节没有丢。Browser PTY 实际 attach 到 tmux，tmux 根据外层 `TERM=xterm-256color` 的 terminfo 看到 `indn/rin` 能力后，会把滚屏优化成 `CSI n S` / `CSI n T` 这类批量滚屏序列。xterm.js 对这些显式滚屏不会把被推出视口的内容作为普通输出行加入 scrollback，所以用户看到 1..9 这类早几行从历史里消失。可用 20 行编号输出验证：后端 WS 收到 20/20，但同版本 xterm 写入 tmux attach 流后只剩 10..20。

## Fix Pattern

- Keep a page-level terminal output queue with each item bound to the current connection token.
- Reset that queue when opening a new terminal connection, and re-check the token after async Blob conversion before queueing output.
- Queue the `terminal_backlog_replay` end marker behind already-received output bytes. Ending replay immediately when the text control message arrives can let cursor/status-line correction run against an intermediate xterm frame while backlog bytes are still queued.
- If the user types while a large replay is still draining, drop unrendered replay output and reveal the live terminal. Otherwise the PTY input is sent, but its echo and following live output can sit behind old history in xterm's write queue for a long time.
- Do not drop queued live output on the frontend. Live PTY output is user-visible history; if xterm/browser rendering lags, preserve the queued bytes and solve backend lag with recovery rather than trimming browser history.
- Live backend output chunks carry monotonically increasing sequence numbers. If a websocket receiver observes a sequence gap or `RecvError::Lagged`, recover `last_output_seq_sent + 1..` from the session's byte-capped chunk backlog before continuing with newer broadcast chunks. Logging lag without replaying the missing chunks reintroduces silent line loss.
- Configure each webClx tmux session with `terminal-overrides=xterm-256color:indn@:rin@`. This preserves xterm color support but prevents tmux attach from emitting `CSI n S` / `CSI n T` batch scroll operations that xterm.js cannot preserve as scrollback history. Do not replace this with `TERM=vt100` unless color loss is acceptable.
- Server-side websocket handling must read input independently from output/backlog sending. Sending the initial backlog before entering the socket receive loop can queue typed input behind a large history replay, even if the frontend interrupts its local replay queue.
- Initial websocket replay must use a bounded tmux snapshot, not `capture-pane -S -` over the full `history-limit`. Full 100000-line history with ANSI escapes can keep xterm in hidden replay for 10+ seconds; user input still reaches the PTY, but its echo cannot be shown until the initial write drains.
- Save scroll metrics per session id before changing sessions and on user scroll events.
- On backlog replay end, restore the selected session's saved scroll position; only default to bottom when no saved position exists.
- Wrap xterm layout fitting in scroll preservation: capture the active session's current position, suppress layout-induced scroll saves, then restore immediately and on the next frame. This prevents mobile IME/visualViewport resize events from overwriting a user's saved scroll location.
- If the page itself was at the bottom before a dropdown session switch, preserve that page-bottom state across asynchronous xterm replacement, websocket open, and backlog replay settle stages. Bind the restore to the selected session and new connection token, and cancel it only on explicit user scroll input.
- Treat bottom restoration as an idempotent request. If the xterm logical viewport or page is already within its bottom tolerance, synchronize its DOM projection and controls without calling `term.scrollToBottom()` or `window.scrollTo()` again. Cached dropdown switches can otherwise execute the replay anchor, layout preservation, and page restore paths repeatedly even though the selected terminal is already displayed correctly.
- Do not run page-level bottom restoration while hidden backlog replay is still active. During replay the terminal buffer and document height are still changing, so repeated `window.scrollTo(bottom)` calls can show a visible second scroll from above to the same bottom. Restore the xterm viewport first, reveal the terminal, then run the page-bottom settle once replay has ended.
- Do not let session switching show a blank terminal between xterm replacement and backlog replay completion, but do not use the previous session's visible xterm text as the placeholder. A previous-terminal snapshot looks like leaked or stale output when the target session is slow to replay. Use a neutral target-session placeholder such as `正在打开 <name>…`, keep it over the hidden/replaying xterm, and remove it only after the replay end marker has revealed the selected session. The backend should send replay start/end for every initial websocket sync, even when the backlog is empty, so the frontend always has a completion signal.
- Backend backlog replay control frames must serialize as `{"type":"terminal_backlog_replay",...}`. For Rust structs with `#[serde(tag = "type")]`, add an explicit `#[serde(rename = "terminal_backlog_replay")]`; otherwise the wire type can become the Rust struct name and the frontend will never see replay start/end.
- Do not mark terminal output as viewed on every live websocket chunk. High-volume terminal output can make many websocket tasks contend for the terminal manager write lock, delaying session switching and websocket connection preparation. Throttle viewed-state writes on the hot output path.
- A websocket HTTP upgrade should not wait for terminal session recovery. Upgrade first, then prepare the session inside the socket task; if recovery fails or times out, send `terminal_connection_error` and clear frontend replay/switching state.
- Treat an explicit session id as stronger than the current path. A stale refresh kicked off by a broken terminal path must not override the user's newer dropdown selection, and the websocket should use the registered session path before validating the URL path.
- During initial `fresh`/`quick_start`/`run` terminal loads, do not normalize browser history before that one-shot intent is consumed. Otherwise the URL can lose the create/quick-start flags, and a queued session refresh can select an older busy session for the same path. When a new terminal is created, pass the created session id as a forced preferred target through `loadSessions()` so background refreshes and stale URL session ids cannot steal the active terminal before quick start sends `codex`.
- WebSocket `open` only means the transport is ready; the initial tmux snapshot may still be in the browser/xterm replay queue. Gate `quick_start` and `run` startup commands on a replay-settled check that covers pending replay controls, active replay, queued replay output, the queued replay end marker, and the current xterm write. If user input arrives before the replay start control, mark the initial replay interrupted so stale snapshot output is discarded and live echo is not hidden behind it.
- While a new terminal is offering the `1`/`2` quick-start choice, buffer manual input received before the initial replay settles and flush it as one complete input frame afterward. A possible quick key may be held only for a short confirmation window; if more text follows, prepend the held key instead of consuming the first character of the user's command.
- Start the new-terminal quick-start countdown when the session is created and keep one fixed wall-clock deadline in visible and hidden/background tabs alike. Connection, reconnect, and initial xterm replay must not reset or postpone that deadline; when it expires, send the configured default command through the session-targeted auto-input API even if browser replay has not settled.
- Explicit terminal creation must use a fresh backend path. If `webclx_sN` already exists when creating stored session `sN`, kill that orphan tmux session before creating the new one. Keep restore/reconnect on the ordinary `ensure_tmux_session` path so registered sessions still survive service restarts.
- Both explicit new-terminal creation and ordinary dropdown switching must replace the xterm instance before connecting the target session. Rebuild `term` and `fitAddon`, re-register xterm event handlers, dispose the previous term after the new one is mounted, and invalidate pending write callbacks. `term.reset()` does not cancel bytes already handed to xterm's internal write queue; on a busy source terminal that queue can block WebSocket frame handling for several seconds even when the backend responds in under 200 ms.
- Batch small interactive input bursts before sending them over the websocket. Ordinary typed characters and cursor-key escape sequences can share a short client-side flush window; Enter, Ctrl+C/Ctrl+D, explicit flushes, and large paste payloads should still flush immediately.
- Merge adjacent websocket binary output chunks before each `term.write()` when they belong to the same connection token and replay/live phase. Do not merge across control frames, token changes, or the replay end marker, or session switching and scroll restoration can regress. Keep live-output merge chunks much smaller than replay chunks; large live `term.write()` batches can make normal input echo and Codex text-entry effects appear seconds late while xterm drains output.
- When a hidden/backgrounded terminal page becomes visible again, refresh layout, focus, bottom scroll, and cursor correction repeatedly while browser rendering settles, but do not trim queued live output.
- Coalesce terminal resize messages for a short window before sending them over the same websocket as input. Layout/visualViewport churn can otherwise enqueue resize frames ahead of user input and delay when the backend writes that input to the PTY.
- Do not ignore `session_list_changed: connected/opened` just because the event's session id already matches the active session. On fresh terminal creation, that is the normal success path; use it to clear a still-visible `正在连接...` status and trigger an immediate forced session-list refresh so connected/activity state does not wait for the slower passive poll or require a manual session switch.

## Verification

```bash
node tests/terminal-session-switch-output.test.mjs
node tests/terminal-initial-path-session.test.mjs
node tests/terminal-backlog-replay.test.mjs
node tests/terminal-paste-safety.test.mjs
cargo test terminal::tests::
for test_file in tests/*.test.mjs; do node "$test_file" || exit 1; done
```

## Compact Codex Status Block (2026-07-26)

When the terminal contains a bordered block headed by `>_ OpenAI Codex (v...)`,
`terminal-codex-status-output.js` rewrites that native ANSI/CRLF byte stream
before it enters xterm. The resulting compact text is the real terminal buffer:
copy, search, scrollback, and redraw all use the same output, with no DOM overlay
and no values merged from `agent-session`.

Fields use `Label: value` without a padded label column. `Collaboration mode`
is shortened to a separate `Mode` row, and whitespace-only left indentation is
removed while ANSI controls are preserved. The border shrinks to the longest
actual row but never grows beyond the native Codex block width.
Comma-separated `Directory` and `Agents.md` paths use at most two rows: the
labelled first row ends with a comma and the remaining paths continue directly
on the second row. Unrelated or incomplete bordered output flushes unchanged.
Verify with `node tests/codex-status-output-transform.test.mjs` and
`python3 tests/codex_status_output_transform.browser.py`.

## Codex Startup Synchronized Redraw (2026-08-03)

On narrow Android WebViews, real Codex startup can emit a loading frame and a
final frame inside `CSI ?2026 h/l` synchronized-output regions. Rendering the
WebSocket fragments independently exposes Codex's intermediate clear/scroll
operations, while rewriting the live status block can invalidate the absolute
cursor and scroll-region coordinates and leave the browser blank even though
tmux still contains the complete prompt.

`terminal-synchronized-output.js` now holds split synchronized regions until
their closing marker and releases their bytes atomically. Live TUI bytes are
otherwise passed through unchanged; Codex status compaction is limited to the
initial backlog replay, where no later live cursor coordinates depend on the
rewritten block. Verify with `node tests/terminal-synchronized-output.test.mjs`
and a real legacy-Android WebView launch of `codex`, including visible typed
input after the final prompt appears.

## Per-Session Browser Cache (2026-07-18)

Symptom: switching back to a terminal rebuilt xterm, closed the previous WebSocket,
and replayed the server snapshot. The selected terminal could appear at the top or
blank while replay drained, and terminals hidden by the dropdown could not keep
their browser buffers current.

Root cause: xterm, WebSocket, replay flags, and the output queue were page-level
singletons. `connectTerminal()` always closed the current socket and replaced the
xterm instance, so a session had no browser-owned state after it lost selection.

Current pattern:

- A visited session owns one cached context containing its xterm, FitAddon,
  WebSocket, connection token, replay state, and output queue.
- Ordinary dropdown switches hide the old xterm and reveal the cached target;
  they do not close the old socket or discard its queued output.
- WebSocket callbacks capture their owning context, so async Blob conversion and
  xterm writes cannot be redirected by a later `activeSessionId` change.
- Background output continues writing to the hidden session xterm. A context that
  was following the bottom stays at the bottom and is restored there immediately
  when selected again; an intentional scrolled-up position remains preserved.
- Each cached socket reports whether its xterm is currently visible. Hidden
  sockets continue receiving output, but the backend must not acknowledge that
  output as viewed until the session is selected again. Legacy clients that do
  not send visibility continue to default to visible.
- Making a cached socket visible must also persist the session's
  `last_opened_at` timestamp. Cached switches do not reconnect the WebSocket, so
  relying on connection setup alone lets the next session-list refresh restore
  stale ordering after the frontend has moved the selected terminal to the top.
- Revealing a hidden xterm must call `term.refresh()` immediately and on the next
  animation frame. Buffer state can be correct while the canvas remains blank if
  a hidden renderer is only unhidden without an explicit refresh.
- Ending, idling, pruning, or unloading a session disposes its context. Normal
  switching does not.

Verification:

```bash
node tests/terminal-session-cache.test.mjs
node tests/terminal-session-switch-output.test.mjs
node tests/terminal-backlog-replay.test.mjs
python3 tests/terminal-session-cache.browser.py
```

The browser test loads the source static bundle with mocked terminal APIs and real
xterm.js. It verifies one WebSocket per visited session, background output capture,
DOM instance reuse, bottom restoration, visibility transitions, a nonblank canvas,
and one visible xterm on desktop and mobile. Deploy the whole `static/` directory;
the running service does not read frontend files directly from the source tree.

## Reconnect Replay Keeps The Last Complete Frame (2026-07-27)

Symptom: ordinary short terminal switches reused the browser cache correctly, but
switching back after a service restart or network interruption could visibly scroll
the selected terminal from the top again.

Root cause: background WebSocket reconnects called `resetTerminalContextInstance()`
before the replacement snapshot arrived. That immediately disposed the session's
complete cached xterm. Selecting the session while the new snapshot was still
draining therefore exposed the partial replacement buffer.

Current pattern:

- Retain the last complete xterm as a read-only visible frame when reconnect starts.
- Write the replacement snapshot into a separate hidden xterm owned by the same
  session context.
- If the user selects that session during replay, show the retained frame without
  exposing the hidden buffer's incremental writes.
- On the queued replay-end marker, dispose the retained xterm, reveal the completed
  replacement in the same task, and then restore the saved viewport or bottom state.
- A second disconnect during replay replaces only the incomplete hidden xterm; it
  must not discard the retained complete frame.

Verification:

```bash
WEBCLX_TEST_RECONNECT=1 python3 tests/terminal-session-cache.browser.py
WEBCLX_TEST_MOBILE=1 WEBCLX_TEST_RECONNECT=1 python3 tests/terminal-session-cache.browser.py
```

## Terminal Navigation Uses The Live Cwd (2026-07-26)

A terminal session's registered `path` is the directory where the session was
created. Running `cd` changes tmux `#{pane_current_path}` but does not rewrite that
registered path. Terminal-to-workspace navigation must therefore keep the session
ID as identity and query `GET /api/terminal/sessions/{session_id}/current-directory`
before choosing the history-workspace directory. Do not infer the current directory
from the navigation URL's `path` suffix or from `TerminalSessionInfo.display_path`.

The workspace stores the returned absolute display path in
`state.currentWorkspaceDirectoryPath`. History grouping includes that directory
without changing normal activity ordering; entering the history tab moves its
existing option to the top and selects it. Verify the full `cd` round trip with
`python3 tests/terminal_live_cwd_workspace_history.browser.py`.

## Shared Session Viewport Arbitration (2026-07-20)

Symptom: when desktop and mobile browsers viewed the same terminal session, the
mobile browser's `resize` frame resized the one shared PTY/tmux pane. Codex and
other TUI programs then redrew at the mobile column count, changing the desktop
browser's output layout as well.

Current pattern:

- Each terminal WebSocket registers its own last reported `cols`/`rows` and
  visibility state with `TerminalSession`.
- The shared PTY uses the complete size of the widest visible client. A narrower
  phone cannot shrink a concurrently visible desktop terminal; the selected
  client's row count stays paired with its column count.
- Hidden cached sessions do not participate. When the widest client hides,
  disconnects, or leaves the session, the next widest visible client takes over.
- If no sized visible client remains, keep the last PTY size. This avoids a
  disconnect/reconnect window resetting the pane to a default size.
- Each xterm instance still fits its own browser viewport. Only PTY size is
  shared. A single interactive process cannot generate two independent TUI
  layouts at once, so mobile may show the desktop-width layout while both are
  open, but it no longer changes the desktop layout.

Verification:

```bash
cargo test terminal::session::viewport_tests::widest_visible_client_controls_shared_pty_size -- --exact
python3 tests/terminal-shared-viewport.browser.py
```

## Broadcast Buffer History Leak (2026-07-06)

Symptom: switching terminals still showed history scrolling top-to-bottom even after all the replay/placeholder/scroll fixes above were in place. The scrolling output was NOT the backlog replay (that was correctly hidden via `terminal-host-replaying`), but live WebSocket binary chunks.

Root cause: `handle_socket` did `let mut receiver = session.subscribe()` then `let mut last_output_seq_sent = 0_u64`. tokio `broadcast::subscribe()` returns a receiver that **does** receive the values already buffered in the channel (up to capacity). Because `last_output_seq_sent` started at 0, every historical chunk (seq 1..N) still in the 4096-capacity broadcast buffer satisfied `chunk.seq > last_output_seq_sent` and was sent to the frontend as live output. A session with 1.6MB of buffered history replayed all of it after the backlog snapshot.

Fix: in `handle_socket`, order the calls as `subscribe -> current_output_seq -> initial_backlog_for_socket`, and initialize `last_output_seq_sent = live_output_start_seq`. Sequence guarantees:
- chunks with `seq <= start_seq` (subscribe-time buffered history) are skipped; their content is already covered by the later tmux snapshot, so nothing is lost.
- chunks with `seq > start_seq` (true new output after subscribe) are sent normally.
- the read_seq-to-snapshot window may double-show a chunk (sent live AND in snapshot), which xterm renders idempotently.

`TerminalSession::current_output_seq()` returns `next_output_seq.load(SeqCst)`; since seq is `fetch_add(1)+1`, the loaded value equals the max seq assigned so far.

Verification: Playwright headless switch between two idle sessions showed `liveBytes=0, liveChunks=0` (was 1.6MB / 720 chunks). Busy-session switches show only genuinely new output. The backlog replay path and `terminal-host-replaying` opacity gate were already correct; the leak was entirely in the live output path.

For the frontend xterm replacement fix, browser verification should open the deployed `/terminal` page, click `#create-session`, and confirm the pre-click `#terminal-host .xterm` node is disconnected, a different xterm node is mounted, and the newly created session is selected.

## Live Output Merge Ceiling Too Small Caused Torn Codex Frames (2026-07-07)

Symptom: during fast Codex output the visible text intermittently garbled, with the bottom status-line text (`Working…`, `Compiling`, `Running`) bleeding into the scrolling output region as fragments like `Wo───ng`, `Won rng`, `Working h4gh47`. The garble self-healed after a short delay. Confirmed by the user that the PTY/stream content was correct and only the browser display was wrong.

Root cause: `TERMINAL_LIVE_OUTPUT_MERGE_MAX_BYTES` was `8 * 1024`, which is **exactly equal** to the backend PTY read buffer size (`src/terminal/session.rs`: `let mut buffer = [0_u8; 8192]`). In `mergeQueuedTerminalOutputItem` the loop guard is `totalBytes + candidate.bytes.length > maxBytes`; since the very first live chunk is already 8192 bytes, the guard trips on the first candidate and **no live chunks ever merge**. Each 8 KiB backend read becomes its own `term.write()` call.

A single Codex TUI redraw is a byte stream (clear + redraw scrolled region + reposition/redraw the bottom status line) typically 20–40 KiB. It arrives as 3–5+ 8 KiB chunks, and because they never merge, xterm.js paints a frame between each `term.write()`. If the canvas renders mid-redraw — e.g. between scrolling the old frame and repainting the status line — it shows the torn intermediate frame, which is exactly the garbled text observed. It self-heals because once all chunks of the full frame have drained, the next render shows the correct final state.

Fix: raise `TERMINAL_LIVE_OUTPUT_MERGE_MAX_BYTES` to `256 * 1024` and give the active visible terminal an 8 ms live-output coalescing window before draining. The original 64 KiB ceiling covered ordinary terminals, but a 204-column production Codex redraw was observed arriving as 65,536 + 63,000 bytes. Raising the ceiling alone was insufficient because the first WebSocket message started `term.write()` immediately, before the adjacent message reached the queue. The short timer lets the pieces of one TUI update accumulate, then the existing token/replay-aware merge sends them to xterm atomically. Replay remains immediate behind its opacity gate, while hidden pages and background cached terminals also drain immediately to avoid timer-throttled queue growth.

Verification: `node tests/terminal-session-switch-output.test.mjs` (the test was also repaired: it now reads the terminal frontend as the concatenated global scope, and the moved output-queue/blob assertions are retargeted to their real files after the extraction). The live-merge assertion verifies the new `64 * 1024` ceiling and that the merge logic stays intact.

Reuse note: the live-merge ceiling must fit a complete redraw at the widest supported terminal size, not merely exceed the backend PTY read buffer in `src/terminal/session.rs` (`8192`). The coalescing delay must also be long enough to collect adjacent WebSocket messages without making input echo feel delayed. If the read buffer, terminal width limit, browser delivery timing, or TUI output format changes, re-measure a real wide-screen redraw before changing either value.

## Large Merged Write Caused Cursor Correction Oscillation Flicker (2026-07-07)

Symptom: after the live-merge ceiling fix (64 KiB), the terminal started flickering every 2-3 seconds on desktop during Codex output. Mobile was unaffected. The flicker was irregular, not tied to a fixed interval.

Root cause: the 64 KiB merge ceiling coalesced a full Codex TUI redraw (20-40 KiB) into a single atomic `term.write()`. xterm's default canvas renderer paints such a large write across multiple animation frames. Each intermediate frame fires `onRender`, which triggers `scheduleTerminalCursorCorrection` → `syncTerminalCursorCorrection`. During those intermediate frames the buffer is in a half-updated state, so `terminalCursorCorrectionTarget()` detection oscillates between "found" (target != null) and "not found" (target == null). Each flip calls `setTerminalCursorHiddenForCorrection()`, which sets `term.options.theme = ...` and triggers a full xterm canvas repaint — a self-reinforcing feedback loop: write → multi-frame render → onRender → detection oscillation → theme toggle → full repaint → more onRender. This loop fires once per Codex TUI redraw cycle, matching the observed irregular 2-3s interval.

Mobile was unaffected because it typically renders with a smaller viewport and the same write completes in fewer frames, so the oscillation window is much shorter.

Fix: in `scheduleTerminalCursorCorrection` ([terminal-cursor-correction.js](/home/codes/webClx/static/terminal-cursor-correction.js)), added a `terminalOutputWriteInFlight` guard. While a write is in flight, `onRender`-triggered cursor correction scheduling is skipped. `drainTerminalOutputQueue`'s write callback already calls `syncTerminalCursorCorrection()` synchronously once the write completes and `terminalOutputWriteInFlight` is false, so the final stable-state correction still happens exactly once. This eliminates the oscillation feedback loop without losing cursor correction accuracy.

Reuse note: any future change that increases `term.write()` batch sizes (merge ceilings, coalescing timers) must account for multi-frame rendering. The `terminalOutputWriteInFlight` guard pattern should remain on any code path where `onRender` can trigger DOM or theme mutations based on buffer-state inspection.

## FAB Backdrop Blocked Terminal Scroll (2026-07-08)

Symptom: terminal could not be scrolled after opening the FAB (floating action button) quick menu.

Root cause: the deployed binary (v1.6.26) did not include the `terminal_fab_auto_expand` field in the `/api/settings` response. The terminal settings loader called `applyTerminalFabAutoExpand(settings.terminal_fab_auto_expand)` with `undefined`, and `applyTerminalFabAutoExpand` computed `Boolean(undefined)` = `false`. With auto-expand off, opening the FAB menu created a full-screen backdrop (`position: fixed; inset: 0; z-index: 39; pointer-events: auto`) that intercepted all pointer events over the terminal, blocking scroll, click, and keyboard input.

Fix: `applyTerminalFabAutoExpand` and its call site in `terminal-settings-loader.js` now treat `undefined`/`null` as the default `true` value (`enabled !== false`), matching the intended default and the `app-settings-load-save.js` path. This ensures the backdrop is permanently disabled even when the backend binary lacks the field.

Verification: Playwright CDP `Input.dispatchMouseEvent` (type `mouseWheel`) at the terminal screen center scrolls correctly with the FAB menu open, on both desktop and mobile viewports. The backdrop reports `pointer-events: none` and `is-visible` class absent.

## Session Switch Page Scroll Went to Top Instead of Bottom (2026-07-09)

Symptom: switching terminals via the dropdown always left the page at the top instead of the bottom. The user expected the new terminal to appear scrolled to the bottom after switching.

Root cause: `beginSessionPageScrollRestore` (terminal-output-scroll.js) only created a page-scroll restore state when the page was **already at the bottom** at the moment of switching (`capturePageScrollSnapshotForLayout().atBottom === true`). If the user had scrolled up at all — even by a few pixels beyond the 8 px tolerance — `activeSessionPageScrollRestore` was set to `null` and no restoration occurred. Then `connectTerminal()` called `term.reset()`, which cleared the xterm buffer and collapsed the document `scrollHeight`. The browser adjusted `scrollTop` downward toward 0, and with no active restore state, nothing pulled the page back to the bottom. The page stayed stranded at the top.

Fix: `beginSessionPageScrollRestore` now always creates a restore state with `snapshot: { atBottom: true }` when given a valid sessionId, regardless of the current scroll position. This ensures the page-bottom restoration fires through the same async chain (`term.reset()` → WebSocket open → backlog replay end → scheduled retries) and brings the page to the bottom after the new terminal's content has settled. User-initiated scroll during the restore window still cancels the restore via `cancelSessionPageScrollRestoreForUserScrollIntent`, so the user can scroll up immediately after switching without fighting the restoration.

The function `capturePageScrollSnapshotForLayout()` is still used by `preservePageScrollDuringLayout` and `pageIsAtBottomForLayout` for layout-time scroll preservation; only the session-switch entry point changed.

Verification: `node tests/terminal-session-switch-output.test.mjs` — the assertion for `beginSessionPageScrollRestore` now verifies `snapshot: { atBottom: true }` instead of `capturePageScrollSnapshotForLayout()`.

## F5 Refresh Left Terminal At Top (2026-07-09)

Symptom: after scrolling the terminal to the bottom and pressing F5, the page reloaded and the terminal viewport ended up at the top instead of the bottom.

Root cause: two independent issues.

1. The previous session-switch fix (commit 101959e) modified `terminal-output-scroll.js` but did not bump its `?v=` cache-bust query in `terminal.html`. The browser kept serving the old cached file, so the fix never reached the browser. (Fixed by bumping the version string.)

2. Browser native scroll restoration: `history.scrollRestoration` defaults to `"auto"`. On F5, the browser attempts to restore the previous document/element scroll position. Because terminal content loads asynchronously via WebSocket backlog replay, the browser restores the scroll before xterm content has settled, stranding the viewport at the top. Additionally, `fitAddon.fit()` (called during `scheduleTerminalSizeSettle`, 3 frames × 100 ms) can reflow the xterm buffer when rows/cols change, which resets the buffer viewportY to 0 — a post-replay reflow that the single `restoreTerminalScrollPositionForSession` call in `endTerminalBacklogReplay` cannot survive.

Fix:
- Set `history.scrollRestoration = "manual"` early in `terminal.js` (before `mountTerminalInstance()`), so the code owns scroll restoration entirely and the browser does not interfere with async content loading.
- Added `scheduleTerminalBottomAnchorAfterReplay(sessionId)` in `terminal-output-scroll.js`, called from `endTerminalBacklogReplay`. It schedules delayed `scrollTerminalToBottom` + `saveTerminalScrollPositionForSession` retries at [0, 80, 180, 360, 720, 1200] ms, covering the size-settle reflow window. These retries force the viewport back to bottom even if a post-replay reflow already pushed it to top. Explicit user scroll intent (`wheel` / `touchmove`) and the terminal "jump to top" button call `cancelTerminalBottomAnchor()`, so the user can still scroll away immediately. Session switches naturally cancel the anchor because each timer checks `state.activeSessionId`.

Verification: Playwright reload test confirmed `history.scrollRestoration === "manual"` and the terminal stayed at the bottom across all sampled time points (0 times stranded at top). A stronger browser test wrote 260 deterministic lines into xterm, scheduled the post-replay anchor, forcibly called `term.scrollToTop()` 40 ms later to simulate the suspected reflow, and verified the anchor returned to bottom; after `cancelTerminalBottomAnchor()`, the same forced top scroll stayed at top. `node tests/terminal-session-switch-output.test.mjs` passes. `.smoke/smoke.py` passes.

## Slow Active-Terminal Switching (2026-07-15)

Symptom: selecting another active terminal could keep `正在打开…` visible for several seconds. Browser timing showed WebSocket handshakes took only 2–29 ms, while the server's first replay control could wait 0.2–3.1 seconds and xterm could spend another 0.06–2.2 seconds rendering a 49–235 KiB initial snapshot.

Root cause: `list_sessions`, `list_all_sessions`, and the background auto-continue scan held the terminal manager's global write lock while running tmux and process activity probes. Periodic/all-workspace scans therefore blocked the connection path from reading or updating the selected session. After the lock wait, every switch also rendered up to 2000 tmux history lines before revealing xterm.

Fix:

- Snapshot session ids/live handles under a short read or write section, run tmux `/proc`/working/error probes without the manager lock, then reacquire the write lock only to merge observations and build response DTOs.
- Capture viewed-state fingerprints outside the manager write lock. Use a monotonic probe sequence so a slower, older concurrent scan cannot overwrite a newer fingerprint observation.
- Limit only `capture_tmux_initial_pane_snapshot` to the latest 800 lines. Keep tmux `history-limit=100000`, full text capture (`-S -`), search/error diagnostics, and live backlog recovery unchanged.

Regression coverage:

```bash
node tests/terminal-switch-performance-contract.test.mjs
node tests/terminal-session-switch-output.test.mjs
node tests/terminal-backlog-replay.test.mjs
cargo test terminal::tests::
```

Deployed browser verification after the lock, replay, and xterm replacement fixes measured four busy-session dropdown switches at 127 ms, 300 ms, 568 ms, and 765 ms (previously up to 5.36 seconds). Every switch disconnected the previous `.xterm` node, restored the original selected session after the probe, and produced no console errors, failed requests, or HTTP 4xx/5xx responses.

## 新建终端被后台列表刷新抢回焦点（2026-07-16）

现象：从老终端点新建按钮创建同工作区终端后，前端先正确切到新终端，但随后某次会话列表刷新会把活动会话选中抢回旧终端，新建终端被丢到后台、其快捷命令仍在后台运行。

根因（用路由劫持确定性复现）：新建终端选中后，后端尚未把该会话列入 `/api/terminal/sessions` 时，任意一次 `loadSessions` 刷新（不仅 `forcePreferredSession` 的，也包括 `scheduleSessionActivityRefresh` 等不带该标志的刷新）会用新列表覆盖 `state.sessions`，而新列表不含当前 `activeSessionId`。此时 `renderSessions` 的 `activeSession()` 命中失败、`displaySession` 回退到 `sessions[0]`，`selectSession` 随之选中另一个终端，焦点被抢走。

修复（覆盖所有刷新路径，`static/terminal-sessions.js`）：本页通过创建 API 得到新会话后，把 ID 放入 `pendingCreatedSessionIds`。在 `state.sessions = sortSessionsByRecentActivity(...)` 之前，只有该集合中的活动会话不在本次拉取列表里时，才从旧 `state.sessions` carry-over 一次，并立即消费待确认标记；后端列表已经包含该 ID 时也清除标记。这样仍能覆盖创建响应与在途列表刷新的竞态，同时普通缺失 ID 以服务端列表为准，避免其它浏览器已删除会话后旧页面继续保留它。

跨浏览器删除还要求后端遵守严格连接语义：WebSocket 带显式 `session_id` 时只能连接注册表中的原会话；ID 不存在必须返回连接错误，不能按目录隐式调用 `create_session_locked`。不带 ID 的连接和显式创建 API 才允许选择或创建会话。已登记但 tmux 进程丢失的会话仍由 `ensure_live_session_locked` 按原 ID 恢复。

线上验证：路由劫持强制把新会话从每次刷新列表剔除，3/3 次新建后新终端均稳定保持选中、快捷命令前台运行；正常无劫持路径 3/3 也通过。

## 旧列表刷新把 A 切回 B（2026-07-19）

现象：用户已经从终端 B 切到 A 并在 A 中操作，页面稍后却无用户输入地重新选中 B。

根因：会话下拉切换发生时，`syncQueuedSessionRefresh` 会把 `pushHistoryOnSelect` 和当时的 `preferredSessionId` 合并进待执行的列表刷新。若该刷新请求以 B 为目标发出后，用户在响应返回前再次切到 A，请求完成路径会因为 `pushHistoryOnSelect=true` 跳过 `stableCurrentSessionId` 防护，并按旧 preferred ID 重新执行 `selectSession(B)`。旧请求还会写入一条过期的浏览器历史记录。

修复：`loadSessions` 只负责同步会话元数据。只要当前 `activeSessionId` 仍存在且不是闲置会话，它就在所有 URL、preferred ID 和历史标志之前成为刷新目标。只有当前会话确实已删除或闲置时，才允许原有回退逻辑选择其它会话。刷新最终仍选择当前会话时，不再传递旧的 `pushHistoryOnSelect`，避免重复或过期的历史项。

回归覆盖：

```bash
node tests/terminal-session-selection-race.test.mjs
node tests/terminal-session-switch-output.test.mjs
node tests/terminal-deleted-session-reconnect.test.mjs
node tests/terminal-session-cache.test.mjs
python3 tests/terminal-session-cache.browser.py
WEBCLX_TEST_MOBILE=1 python3 tests/terminal-session-cache.browser.py
```

## 浏览器 QA 误删已有终端（2026-07-19）

现象：浏览器 QA 用错误的绝对 `path` 打开终端页后回退到已有会话，清理代码又把 `state.activeSessionId` 当成本轮测试会话直接发送 DELETE，连续结束了多个真实 tmux 会话。服务重启发生在删除之后，并非根因。

修复与约束：

- `DELETE /api/terminal/sessions/{session_id}` 必须携带 `X-WebClx-Confirm-Session`，且值必须与 URL 中的目标 ID 完全一致；缺失或不匹配时后端拒绝删除。
- 终端页和首页活动终端表同时发送确认 ID，并用 `X-WebClx-Delete-Source` 标记请求入口。
- 后端对请求、拒绝/失败、成功三个阶段写审计日志，包含请求者、客户端地址、User-Agent、Referer、请求来源以及目标终端 ID；成功日志再记录终端名和路径。
- 浏览器测试只能清理由本轮创建 API 响应返回并保存的 ID。不得从当前活动项、下拉选中项、最终 URL、列表顺序或页面回退结果推断清理目标；创建失败或身份不一致时不执行清理。
- webClx 终端浏览器测试使用工作区相对 `path`，并在交互前断言创建响应 ID、活动 ID、URL session 和预期路径一致。

回归覆盖：

```bash
node --test tests/terminal-session-delete-safety.test.mjs
```

## 顶级导航往返丢失当前会话（2026-07-25）

现象：终端 A 进入“工作区”后目录显示正确，但再点顶部“终端管理”会打开另一个默认终端。shell 中的 `cd` 没有修改会话选择；变化发生在页面导航阶段。

根因：终端页生成工作区链接时只携带 `path`，工作区页顶部终端链接又固定为 `/terminal`。返回终端页时 URL 没有 `session`，只能依赖浏览器本地偏好或列表首项回退，因此可能选中其它会话。

修复：终端页用 `terminal_session` 把当前稳定会话 ID 传给工作区；工作区的目录和 TAB 导航持续保留该参数，并把顶部“终端管理”解析成带明确 `path` 与 `session` 的终端 URL。只有用户主动选择其它终端，或原会话已从服务端列表消失时，返回目标才允许变化。

回归覆盖：

```bash
node --test tests/terminal-navigation-session-return.test.mjs
python3 tests/terminal_navigation_session_return.browser.py
```

## 工作区目录与终端管理选择器不同步（2026-07-26）

现象：工作区 Tab 已同时展示目录浏览和终端管理，但目录进入 `newapi` 等项目后，终端管理选择器仍可能保留另一个目录的全局偏好终端，导致上下工作区不一致。

规则：终端管理选择器只自动选中与 `state.currentPath` 精确匹配的活动终端，优先沿用同目录的返回终端和目录偏好；当前目录没有终端时保持未选择。用户在终端管理选择器主动选择其它目录的终端时，目录浏览同步进入该终端工作区。两种方向都更新 `terminal_session` URL 状态，目录选择器与终端管理选择器保持同一会话。

回归覆盖：

```bash
node --test tests/workspace-terminal-current-path-sync.test.mjs
python3 tests/workspace-terminal-current-path-sync.browser.py
```

## 进入终端管理误建根目录终端（2026-07-27）

现象：从工作区顶部进入“终端管理”时，偶尔会自动新建一个 cwd 为 `/home/codes` 的终端。

根因：终端页把普通 `?path=...` 也当作新建意图，而工作区顶部链接在没有可返回会话时又生成了
`fresh=true` URL。位于工作区根目录时，空相对路径由后端解析为配置的工作区根
`/home/codes`，所以误创建集中表现为这个 cwd。

规则：`path` 只负责会话筛选和恢复上下文，不创建终端；只有显式 `fresh` 或 `run` 参数可以进入
初始化创建分支。顶部“终端管理”无返回会话时打开普通 `/terminal?path=...`，工作区内明确的
“新建终端/打开终端”动作继续使用 `fresh=true`。

回归覆盖：

```bash
node --test tests/terminal-initial-path-session.test.mjs
node --test tests/terminal-navigation-session-return.test.mjs
node --test tests/workspace-terminal-fresh-link.test.mjs
```
