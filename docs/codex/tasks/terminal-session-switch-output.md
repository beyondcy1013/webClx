# Terminal Session Switching Output Isolation

Date: 2026-05-18

## Symptoms

- Switching terminal conversations from the dropdown could leave the visible terminal showing another session's fast-scrolling output.
- Originally, replay always forced the bottom and lost manual reading positions. Current policy: selecting a different terminal intentionally opens at bottom; same-session reconnect/layout preserves the current reading intent.
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
- Selecting a different terminal resets its saved position and follow intent to
  bottom. Same-session reconnect and replay completion still respect the current
  saved intent, including manual history reading after selection.
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
- When a hidden/backgrounded terminal page becomes visible again, refresh layout and cursor correction while preserving scroll/focus intent as rendering settles; do not trim queued live output.
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
  when selected again. Selecting any different session now resets a scrolled-up
  position to bottom; manual reading remains preserved while it stays selected.
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
and one visible xterm on desktop and mobile. The running service reads the tracked
source `static/` directory through `/home/bin/webclx/static`; verify that absolute
symlink and bump changed script versions without copying assets or restarting.

## Cached Scroll Intent And Write Completion (2026-10-14)

Dropdown switching could save an intermediate viewport during layout or an
unfinished xterm write as a new reading position. Switching also cleared the
viewport listener reference before rebinding, leaving the previous terminal's
scroll listener attached. The write completion callback independently restored
its pre-write follow decision, overwriting input or scrolling performed while
the write was pending. Replay and input settle timers could then undo a newer
choice to read history.

Keep `followOutput` as the per-session follow intent. When switching during a
layout/write/replay, preserve that intent and the last saved reading position.
Write completion reads the current intent without assigning an old decision.
Explicit input and the bottom button enable following; wheel/touch/top-button
scrolling records the user's position even during layout suppression. Stamp
scroll snapshots and input settle callbacks with the session's intent revision
so newer gestures cancel older restores. Replay bottom anchors only run while
the selected session still follows output. Rebind scroll listeners using the
previous viewport reference and reject events from any other viewport.

Verification uses production helpers in `tests/terminal-scroll-intent.test.mjs`
and real xterm in `tests/terminal-session-cache.browser.py`. The browser holds a
real write completion, types, switches away, then releases it: the old code
turns following off; the fix preserves it. It also covers background output and
grid changes, top/bottom buttons during switching, input followed by manual
scrolling, native wheel/touch gestures, replay restoration, and agreement
between DOM and buffer positions.
Desktop, touch viewport, Android DOM renderer, and reconnect runs pass. Use
`WEBCLX_TEST_CHROMIUM=/opt/google/chrome/chrome` on this host. A separate existing
`terminal-touch-selection-policy.test.mjs` assertion expects the absent
`terminalCanScrollTouchPixels` helper and fails identically against HEAD; its
touch eligibility contract is outside this scroll-restoration fix.
Native touch also exposes the unchanged bundled xterm's passive-touchstart
`preventDefault` warning (`cancelEvents: true`). The browser test requires that
exact known warning and reports it separately; other console errors still fail.

### Switch Always Opens At Bottom (2026-10-14)

The latest user policy requires every dropdown switch to display the target's
latest output, even when it was previously left at the top or in the middle.
`activateTerminalSessionContext()` resets the target's saved position and
`followOutput` only when the active context changes. Its scroll intent revision
invalidates older layout/input restores, and a retained reconnect frame also
scrolls to bottom. Cached output, connections, and xterm instances stay intact.
Same-session reconnects preserve manual reading; scrolling up after selection
still disables follow until the next selection, input, or bottom button action.

`tests/terminal-session-cache.browser.py` covers immediate and settled bottom
positions, subsequent live output, native wheel/touch history followed by a
switch, and manual history reading during replay without a session switch.

## Bounded Background Preconnect (2026-08-14)

Symptom: the first dropdown switch to an unvisited active session still showed
`正在连接`, even though revisiting an already cached session was immediate.

Current pattern:

- After the session list selects its foreground target, and when a session selector is
  hovered, opened, or focused, schedule background WebSocket connections for likely targets.
- Create background contexts through `terminalSessionCache.getOrCreate()` without invoking the
  activation callback. Background creation must not change `activeTerminalContext` or send a
  false visibility update for the current foreground session.
- Keep at most four automatically managed preconnect targets and stagger handshakes by 300 ms.
  Recheck active, idle, deleted, hidden-page, and existing-socket state when each delayed task runs.
- Reuse `connectTerminal(context)` unchanged. Its foreground guards suppress status, fit, focus,
  and startup actions for background contexts; selecting an open context uses the existing cached
  switch path and sends the foreground resize then.
- Hidden sockets still receive their initial backlog and live output so their xterm buffers are
  ready when selected. The four-session bound therefore limits real WebSocket, DOM, replay, and
  render work, not merely heartbeat connections.
- The bound applies to automatic preconnect ownership. Contexts that the user has actually visited
  continue following the existing per-session cache lifecycle and are disposed only when the
  session is idled, removed, pruned, or the page unloads.

Verification:

```bash
node --test tests/terminal-background-preconnect.test.mjs tests/terminal-session-cache.test.mjs
python3 tests/terminal-session-cache.browser.py
WEBCLX_TEST_MOBILE=1 python3 tests/terminal-session-cache.browser.py
WEBCLX_TEST_RECONNECT=1 python3 tests/terminal-session-cache.browser.py
```

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

Top-level pages such as `/downloads` and `/agent-profiles` link to `/workspace`
without a `path` query. On that entry path, workspace startup must read the
browser's global current-terminal preference, query the same live-cwd endpoint,
and set `state.currentPath` before the first directory load. Keep the initial
URL terminal ID separate from mutable session-list selection state: the early
workspace-tab session refresh can otherwise replace it with a root-directory
terminal before live cwd restoration finishes. Explicit `path` links continue
to select their requested directory unless they also carry a terminal session.
Verify with `tests/workspace-current-terminal-directory.browser.py`.

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
- Each xterm uses the canonical PTY grid so ANSI cursor addressing remains
  consistent across clients. A single interactive process has one TUI layout.
- Clients whose canonical grid exceeds their local width keep native font size and
  use horizontal scrolling; desktop and Android therefore render at the same scale.
  When the canonical grid fits locally again, explicit width is cleared.
- Local fit measurement temporarily clears zoom and canonical width, then
  restores both. Only these native local dimensions are reported to the server,
  preventing scaling from feeding back into shared PTY size arbitration.
- `tests/terminal-mobile-scale.browser.py` verifies real xterm geometry, native
  dimension reporting, ANSI grid preservation, rotation, horizontal scrolling and reset.
  The shared-session live test additionally requires valid local session auth.

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

Fix: in `scheduleTerminalCursorCorrection` ([terminal-cursor-correction.js](/home/codes/tools/webClx/static/terminal-cursor-correction.js)), added a `terminalOutputWriteInFlight` guard. While a write is in flight, `onRender`-triggered cursor correction scheduling is skipped. `drainTerminalOutputQueue`'s write callback already calls `syncTerminalCursorCorrection()` synchronously once the write completes and `terminalOutputWriteInFlight` is false, so the final stable-state correction still happens exactly once. This eliminates the oscillation feedback loop without losing cursor correction accuracy.

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
- Limit only `capture_tmux_initial_pane_snapshot` to the latest 100 lines on page refresh or reconnect. Keep tmux `history-limit=100000`, full text capture (`-S -`), search/error diagnostics, and live backlog recovery unchanged.

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

## 工作流等待首屏超时（2026-09-11）

- 首屏就绪原先要求 `outputWriteInFlight=false`，把后续实时输出也算作未完成回放；繁忙终端可能始终错过就绪轮询。现在分别记录 `outputReplayWriteInFlight`，只等待回放写入、回放结束标记和待回放队列，不等待实时输出空闲。写入回调仍受 connection token / write id 保护，重置队列时同步重置回放写入状态。
- 服务端 `terminal_connection_error` 按当前连接 token 保存错误，工作流直接返回原始原因；新连接清除旧错误。超时提示区分上下文、连接、回放开始与回放写入阶段，未增加超时阈值。
- `node --test tests/terminal-tool-readiness.test.mjs` 覆盖繁忙实时写入、排队/中断回放、旧连接回调、服务端报错及阶段提示。backlog、session-cache、preset-command-matrix 检查通过；扩展检查中的 session-switch-output（旧全局 input queue 正则）和 switch-performance-contract（tmux list-panes 正则）仍有既有失败，不在本次修改范围。
- 静态模块与 HTML 缓存版本同步更新，运行目录 HTTP 内容逐文件核对。尚未做真实浏览器全链路超时复现；后端 10 秒阻塞任务超时、tmux 子进程期限及锁范围优化仍需单独处理，不能据此前端修复认定已解决。

### WebSocket 重连首屏异步快照（2026-09-11）

- `handle_registered_socket` 原先在 Tokio 工作线程直接执行同步 `tmux capture-pane`，慢子进程会拖住其它连接。重连首屏改用 Tokio 异步进程，最多 4 个并发，排队和执行共享 3 秒期限；超时或取消通过 `kill_on_drop` 子进程守卫终止并由 Tokio 回收，不留下 `spawn_blocking` 后台任务。
- 首屏读取失败向浏览器发送真实 `terminal_connection_error`，不再把失败伪装成成功回放。关闭的会话使用已有缓冲；存活会话快照成功但为空时保留有界缓冲逻辑。日志记录 session ID、快照耗时和字节数。
- 保持订阅 → 快照 → 序号边界 → 回放顺序、100 行首屏及完整历史查询不变。同步 attach 初始化和管理器全局锁仍待后续优化，此改动只覆盖 WebSocket 首屏读取。
- Node backlog/readiness 6 项通过；Rust 增加子进程返回码、执行超时、排队超时测试，并将已有首屏边界测试改为 async。编译 API 请求 `170641-18d438fab25ed3f8` 已完成：`cargo check --locked` 和 5 项 Rust 回归全部通过（存在编译警告）。部署请求 `171053-18d438fab25ed3fa` 于 17:11 完成：服务 active/running，运行进程、安装文件与 release 产物 SHA-256 一致，认证探针 HTTP 200。重启后 6 条首屏捕获日志成功（记录为 0ms，可能命中内存初始快照，不代表异步 tmux 或浏览器端到端耗时）。前次部署因缺少匹配的 Playwright Chromium 在替换二进制前中止；补齐浏览器后门禁通过，未跳过检查。

## DA1/DA2 响应残留成键盘输入（2026-09-12）

现象：终端输出中出现 `ESC[c` 或 `ESC>c` 查询后，随后键盘输入会残留 `0;276;0c`。根因是输入侧自动响应过滤器只识别 `ESC[?...c` 与 `ESC>...c`，而当前 xterm 对普通 DA1/DA2 返回无前缀的 `ESC[0;276;0c`；过滤器把它当作未匹配字符透传给 PTY。

修复：响应匹配改为允许普通 CSI 参数（`ESC[...c`），保留原有 `?` / `>` 形式；浏览器回归模拟查询 `ESC[c` 后逐字符输入 `ESC[0;276;0c`，确认只发送后续 `a`，并确认待过滤计数归零。已确认 `/home/codes/tools/webClx/static` 仍指向 `/home/bin/webclx/static`，静态文件即时生效，不需要重建或重启。

回归覆盖：

```bash
node tests/terminal-entry-script-syntax.test.mjs
python3 tests/terminal-device-attribute.browser.py
```

## 输入热路径与持久化锁（2026-09-15）

- 普通键盘输入原先在异步 WebSocket 任务中同步取得管理器写锁，提交历史时还持锁序列化整个 registry 并写盘。磁盘或其他会话操作变慢时，会阻塞输入并占住 Tokio 工作线程；现场曾记录会话路径查询耗时 2741 ms。
- 所有 registry 保存入口现在先释放管理器锁。独立写入互斥量串行化保存，获得写入权后再读取最新内存快照，随即释放状态读锁，再编码和原子替换文件，避免乱序保存覆盖新状态。
- WebSocket 输入在 blocking 线程中记录内存历史，落盘使用每个管理器至多一个合并任务，输入不等待磁盘。正常关闭仍同步保存最新状态；异常强杀可能丢失尚未落盘的最近输入。
- 会话路径查询、已读状态的 tmux 探测和稳定边界保存移到 blocking 线程。超过 100 ms 的输入记录 `capture_ms`、`pty_write_ms` 和 `total_ms`，不记录输入内容。通用任务超时提示同时列出锁、磁盘与 tmux，避免误导为 tmux 未安装；此次未改变 blocking 任务的取消语义。
- 输入历史达到 500 条后，新增条目会挤出最旧条目，不能用长度变化判断是否需要保存；改为使用实际提交事件。
- 回归入口：`cargo test terminal_persistence_`（阻塞写入时输入继续、状态锁可用、最新历史保存，以及容量上限提交检测）、`cargo test terminal_input_history_`；Node 输出状态持久化、已读确认、会话切换测试。
- 这次覆盖持久化及异步运行线程的阻塞。部分旧的 tmux 清理/恢复路径仍在管理器锁内执行，主机资源竞争和浏览器网络/渲染也需结合新耗时日志继续定位，不能将本次改动当作所有卡顿来源均已消除。
- 部署验收：请求 `214923-18d5818caf07fa20` 于 2026-09-15 21:52 完成。`cargo check` 与 7 项输入历史/持久化 Rust 回归此前通过；默认级别 Clippy 和 release 构建通过，既有告警保留（严格 `-D warnings` 未通过）。运行进程、安装文件与 release 产物 SHA-256 均为 `f4019da66f0ccccdfd23f407557f1d835a88745554c64ef76eecbe5af41178e8`，服务 active/running，`/api/auth/session` 返回 HTTP 200。
- 部署后通过真实会话 API 创建 `s5805`，确认 pane 为 `runuser -u root -- /bin/bash -l`。回放结束后连续发送 15 条带唯一输出标记的命令，loopback WebSocket 输入至命令输出中位数 40.98 ms、最大 111.07 ms；会话删除成功，API 与隔离 tmux 均无残留。该测量不包含远端网络及浏览器渲染，也没有可比较的旧版本延迟基线。

## 长文本粘贴完整性（2026-09-16）

`terminal-input-transport.js` 原先对已连接输入也使用 65,536 字符重连队列，
截断长文本时丢掉 bracketed paste 的结束标记，导致 Agent 停留在粘贴状态。
`terminal-paste.js` 忽略发送结果，又显示成功、关闭弹窗或追加回车。

现在连接正常的大输入先发送既有按键队列，再通过单个 WebSocket 消息完整发送；
断线队列容量不足时整条拒绝，不截断任何输入。粘贴弹窗使用要求在线的即时发送，
失败保留草稿、不入重连队列、不追加回车；若只有回车失败，提示勿重复粘贴。
单行文本保留 xterm 的 bracketed-paste 模式，多行仍强制包裹结束标记。
发送成功只表示浏览器 WebSocket 接受了完整消息，不表示 Agent 已执行；网络在此之后
断开仍需核对终端实际内容，不应自动重发。

验证：`node tests/terminal-input-reconnect.test.mjs` 覆盖超过 65 KB 的中文/emoji/多行、
旧输入顺序、离线溢出、socket 异常及弹窗失败路径；
`node tests/terminal-paste-safety.test.mjs` 检查粘贴入口约束。
线上两个 JS 的 HTTP 内容已与实体文件逐字节核对，HTTP 静态 HTML 引用 `20260916a` 版本（页面路由需浏览器登录）。
已打开页面需刷新加载新脚本，无需重启服务。

`static` 是指向 `/home/bin/webclx/static` 的已跟踪符号链接，内部实体文件不在 Git 中。
本次运行时改动存于 `scripts/patches/terminal-paste-integrity.patch` 以便审查和重放，
不创建第二份静态目录。重放前先确认符号链接和补丁适用性，勿覆盖较新的运行时修改。

## 会话清理的锁外探测（2026-09-17）

- 列表、自动继续扫描、改名、删除及创建前清理统一使用 `manager/cleanup.rs`：短锁快照、锁外 tmux 状态探测、短锁合并；移除的 PTY 客户端在释放全局状态锁后 kill/wait。
- `has-session` 复用同步有界命令封装，单命令预算 2 秒、stdout+stderr 合计 64 KiB，超时或未知错误返回 Unknown 并保留会话。扫描复用最多 4 个工作线程，不将单命令期限表述为整轮期限。
- 清理使用既有每会话启动锁的 try_lock，跳过创建/重连中的会话。新建会话在发布 reservation 前取得同一启动锁；合并前核对创建时间、路径和 live Arc 身份，避免过期 Missing 结果删除替换后的会话。
- 回归 `terminal::manager::cleanup::tests` 覆盖阻塞探测期间状态锁/输入可用、Unknown 保留、Missing 删除、启动跳过及过期结果丢弃；有界子进程回收沿用 `terminal::tmux::command_output::tests`。
- 此项修复清理路径的全局锁放大。旧的无指定 ID 创建/attach、显式删除及其它 tmux 调用仍需独立审计；外层 10 秒 timeout 不取消已运行 blocking 任务，主机调度/磁盘压力仍可能造成超时。部署及测试结果以对应队列回调为准。
- 上线验收：修复提交 `3aba800`，部署请求 `122328-18d5ffa787aad963` 于 2026-09-17 12:26:53 完成并重启。3 项清理回归、6 项有界子进程测试及 5 项部署 Rust 门禁通过；安装审计仅修改二进制，无缺失文件。构建/安装/运行 SHA256 均为 `6651602ace60f16d67267c50c5c3d12d1927e873039f6ff040941eda3d20126a`。服务 active/running，认证、全会话列表和当前目录接口均 HTTP 200；列表单次 267 ms（23 会话），不作为受控性能收益证据；验收时重启后无 error 级 journal。集中证据目录 `/home/bin/webclx/compile/runs/20260917T122333-4127425-1743/`。

## 按会话锁定移动端重排（2026-09-18）

共享会话的 PTY 宽度仍由最宽可见客户端决定。手机单独查看某个曾被桌面撑宽的终端时，
服务端 canonical 宽度会继续触发 `terminal-shared-grid-overflow`，历史输出只能横向拖动；
TUI 的 ANSI 坐标也不能在 xterm 内自动重新换行。

终端工具菜单中的“重排当前终端”现在不只是单次动作：该会话会记入浏览器本地的
`webclx:terminal-session-reflow-sessions`。切换、重连或收到新的 canonical 宽度后，
只要仍启用本地 fit，就按该终端上下文的手机视口重新 resize xterm 并发送同一尺寸给 PTY，
随后清掉 overflow 布局，避免后端 canonical 覆盖用户选择。其他终端和桌面客户端不受影响；
删除会话上下文后如需恢复共享宽度，可在桌面端开启宽屏模式重建该终端的本地偏好。

验证：

```bash
node --test tests/terminal-session-reflow.test.mjs
node --test tests/terminal-session-switch-output.test.mjs
```

## xterm 自动应答被当作用户输入截断回放（2026-09-24）

症状：打开或刷新长期运行的 Agent 终端后，屏幕停在陈旧中间帧：输入框不可见（在
可视区之下），或输入框跑到顶部、下方大片空白；`tmux refresh-client` 后立即正常。

根因：接入回放的 backlog 中含有 TUI 发出的终端查询（OSC 10/11 颜色、DA、焦点上报
模式等）。xterm.js 通过与按键相同的 `onData` 自动应答（`\e[O`、`\e]10;rgb:…\e\\`、
`\e]11;rgb:…\e\\`），`sendTerminalInput` 把它们视为用户输入并调用
`interruptTerminalBacklogReplayForInput()`，回放被截断（实测 62 段中丢 58 段），陈旧
应答还被写入 Agent PTY。

修复：`static/terminal-report-filter.js` 识别纯终端报告序列；`terminal.js` 的 onData
对这类序列不记用户输入、不打断回放，初始回放未稳定前直接丢弃，稳定后以
`sendTerminalInput(data, { terminalReport: true })` 转发；`terminal-input-transport.js`
仅在非 `terminalReport` 时打断回放和刷新输入可见性。

验证：

```bash
node tests/terminal-report-filter.test.mjs
node tests/terminal-input-reconnect.test.mjs
node tests/terminal-backlog-replay.test.mjs
```

后续：接入主回放已改为每次连接即时抓取 tmux `capture-pane` 快照 + 抓取水位线之后的
live chunk + 强制 `tmux refresh-client` 整屏重绘兜底，不再整段重放 attach 以来的字节流
（src/terminal/websocket.rs、src/terminal/tmux.rs）。

## 断线重连 seq 水位增量续传（2026-09-28）

症状：手机浏览器/PWA 切到其它 App 再切回时，页面恢复可见会立刻"终端清空并
拉取历史消息"，同时系统键盘状态跳动（hide/show）。回归路径：`visibilitychange`/
`focus` → `refreshSessionsAfterPageResume()` → socket 已被系统挂起或关闭 →
`scheduleReconnect()` → `connectTerminal()` 对 `disconnected && hasLoadedOutput`
的上下文调用 `resetTerminalContextInstance()` 重建 xterm（helper textarea 重建导致
焦点与系统键盘抖动），服务端对每个新 socket 又总是即时抓 tmux 快照做全量回放。

修复（seq 水位续传协议）：

- `TerminalSession` 持有随机 `output_instance_id`（限 2^53 内，JSON 精度安全），
  每个输出 chunk 本来就有单调递增 `seq`；backlog 是连续流。
- 新客户端在 `/api/terminal/ws` 查询参数里带 `seq_framed=1`；服务端先回
  `terminal_output_framing` 控制帧再发任何二进制，之后每条二进制输出帧带
  8 字节大端 seq 前缀。旧客户端不带参数即收不到协商帧，服务端保持裸帧；
  新客户端连旧服务端时永远等不到协商帧，也就不会剥帧——双向兼容。
- 客户端在 backlog_replay `start` 控制帧里记录 `instance`，并累计
  `lastOutputSeq`；重连且缓冲区可信（`hasLoadedOutput && !outputStreamGap`）
  时带 `resume_seq`+`resume_instance`。
- 服务端 `delta_replay_chunks()` 仅在实例匹配、`resume_seq <= current_output_seq`
  且 backlog 首块恰为 `resume_seq+1`（连续无缺口）时接受续传；此时跳过 tmux
  快照抓取与 `refresh-client`，只按 seq 补发缺失 chunk（`start` 带
  `"delta":true`）。任一条件不满足回退全量快照回放。
- 客户端把"是否重建终端"推迟到 `start` 到达：`pendingDeltaReplay` 时收到
  `delta:true` 则保留原缓冲区，缺失字节按 live 输出路径追加（不清屏、不强制
  滚底、不换 textarea，键盘不跳）；收到非 delta 的 `start` 才调用
  `resetTerminalContextInstance()`，与旧路径等价（retainedTerm 保旧画面至回放
  结束再切换）。
- `interruptTerminalBacklogReplayForInput()` 丢弃回放字节时置
  `outputStreamGap`，缓冲不再可信，后续重连回退全量；一次未打断的回放结束
  后清除该标记。delta 开始时必须清掉 `backlogReplayInterrupted`——delta 字节
  是缺失输出的唯一副本，`queueTerminalOutput` 的打断丢弃会永久丢内容。
- delta 回放不置 `backlogReplayActive`：缺失 chunk 走 live 输出语义（用户滚动
  位置保留、自动应答按已稳定通道发送），回放 `end` 走轻量收尾。

验证：

```bash
cargo test delta_replay_chunks
node tests/terminal-backlog-replay.test.mjs
```

## 直开新终端后的输入焦点（2026-09-29）

症状：终端页“新开”配置为直接创建时，点击按钮后按钮获得焦点；新终端连接、首屏
回放和默认命令启动完成后，焦点仍留在“新开”按钮上。用户直接按方向键/输入键时，
键盘事件先到页面控件而不是 xterm 输入区。

根因：普通会话切换有 `focusTerminalAfterSessionSwitch()` 在 select 关闭后反复把焦点
交还 xterm；直开新终端走 `createSession({ enableQuickStart: true })`，连接 open 阶段
的 `focusTerminalIfAllowed()` 可能仍被按钮点击后的移动端 action-focus gate 拦住，
而回放结束路径没有针对“新开按钮仍持有焦点”的恢复逻辑。

修复：新增 `focusTerminalAfterNewTerminalLaunch(sessionId, sourceControl)`。创建请求
成功并启动目标会话连接后登记目标 session id；仅当该 id 仍是活动会话、socket 已打开、
首屏回放已完全 settle 时，释放 action-focus gate 并调用 `term.focus()`。恢复窗口覆盖
 0/80/280/800ms，可穿过字体 fit、回放队列和 3 秒快速启动链路；若期间用户聚焦了其它
控件，则保留用户焦点。浏览器测试同时验证新会话 helper textarea 获得焦点且方向键不切
换会话。

验证：

```bash
node --test tests/terminal-new-launch-focus.test.mjs
python3 tests/terminal-new-launch-focus.browser.py
python3 tests/terminal-session-switch-focus.browser.py
node tests/terminal-session-cache.test.mjs
```

## 滚动几何与用户意图区分（2026-10-15）

切换到不同终端必须立即到底并恢复跟随；之后手动阅读历史应保持位置，输入或跳底按钮重新跟随。同会话重连、IME 和布局变化不改变用户意图。运维入口为 `.agents/skills/webclx-ops/subskills/webclx-terminal-scroll/SKILL.md`。

本次真实 Chromium/xterm 排障复现两条独立故障链：

- 桌面接到共享 canonical 80 行时，本地只能显示约 33 行。旧代码仅限制触屏端行数，造成 canvas 1920px 高于 viewport 806px；即使 `viewportY === baseY`，最后一行仍不可见。`terminal-layout-connection.js` 的尺寸通知与 fit 两处统一限制到本地可见行数，保留 canonical 元数据和列数。测试用 ANSI 定位 canonical 最后一行，确认最后一行实际可见。
- xterm 异步 scroll area 刷新短暂缩小时，浏览器原生夹紧 `scrollTop`，随后 xterm 把夹紧读成用户滚动，webClx 又保存 `followOutput=false`。旧代码在实际 DOM 高度缩到 1px 后，逻辑位置从底部变为 0，后续输出继续停顶。新增 capture 监听 `repairTerminalViewportScrollArea`，在 xterm 消费事件前按缓冲区修复不足的滚动区域；原来的 bubble 监听继续在 xterm 更新位置后保存意图，保留真实滚动条操作。

另在 `captureTerminalScrollSnapshotForSession` 校验活动 context 所属 session，避免选择入口已更新页面 ID、旧 context 尚未切换时错误标记快照。没有 ANSI 或布局事件证据时，不把“回答后跳顶”直接归因于 CodeBuddy 或消息对话框。

回归入口 `tests/terminal-session-cache.browser.py` 同时断言逻辑位置、follow、DOM scrollTop/max 和 canvas/viewport 几何，覆盖活动共享尺寸、最后一行、快速切换、清历史重绘、DOM 收缩、原生滚动条阅读和后续输出。矩阵包括桌面 canvas、触屏、Android DOM 和同会话重连；这是浏览器模拟数据回归，不等于真实登录 CodeBuddy 的交互验收。

复现证据：集中日志请求 `205123-18deb48836feaeb3`（共享行数裁剪）、`205330-18deb48836feaeb4`（DOM 收缩丢失跟随）、`205548-18deb48836feaeb7`（capture 修复恢复跟随）。静态资源从运行目录链接即时读取，需刷新页面以获取新缓存键。

最终验证：请求 `210007-18deb48836feaebc`（2026-10-15 21:00:12–21:01:36）通过三模块语法检查、七项 Node 回归及上述四类浏览器矩阵。四类均在 DOM 收缩修复、手动阅读保持和跳底恢复后满足 `viewportY=baseY=123`、`scrollTop=domMax=2952`；HTTP 三模块哈希与源码一致。滚动子技能已随整个 `webclx-ops` 同步至 Codex、Claude、Cursor、Gemini、ZCode、Devin、WorkBuddy、Kiro 和 Grok。

### 后台输出与显示前定位

2026-10-16 用户明确要求后台新增输出也相应滚到底部，替代 2026-10-15 的“后台不强制滚动”策略。`drainTerminalOutputQueue` 写入完成及 `endTerminalBacklogReplay` 结束后，非活动 context 通过 `followBackgroundTerminalOutput` 滚动所属 xterm、启用 `followOutput` 并保存底部意图；不得调用依赖全局 term 的前台滚动函数。前台手动阅读仍保持位置，`scheduleTerminalRenderRefresh` 仍只为活动 context 安排额外刷新。

`activateTerminalContext` 保留显示前定位：在 `setTerminalContextRenderVisibility(context, true)` 前将新 term 和 retained term 定位到底部；显示后再同步 DOM。没有新输出的后台终端切回也到底；同会话前台重连不重置阅读意图。

浏览器回归三轮各 200 行后台输出：每轮先在 A 阅读历史，再切到 B 阅读历史；等待末行标记进入 A 缓冲区后，在切回前断言 A 已到底、follow 与保存意图均为底部，B 的历史位置不变；继续检查解除 hidden 前与切回后的底部状态。旧代码复现请求 `205100-18debef697f8f6aa`：后台 `viewportY=9`、`baseY=320`、`followOutput=false`。之前请求 `211116-18deb5f45ecf1533` 只验旧策略，不能作为新语义的验收证据。缓存键更新为 `20261016-background-bottom`。

验证请求 `205201-18debef697f8f6ad`（2026-10-16 20:52:06–20:53:42）通过两模块语法检查、五项 Node 回归及桌面、触屏、Android DOM、重连四类浏览器矩阵，覆盖上述后台新增输出滚底与前台历史隔离断言。两模块 HTTP 哈希与源码一致；更新后的滚动子技能已同步至各 Agent 配置目录。

## 终端结束改为隐藏专区落盘与直接关闭选项（2026-10-13）

需求演进：
1. 隐藏的终端不应混在“闲置终端”中，增加独立的“隐藏专区”下拉菜单；会话模型增加 `hidden: bool`，从闲置（`idle`）中完全分离。
2. 结束终端对话框逻辑规范：
   - **点“否”或倒计时结束**：正常关闭终端（调用 `POST /api/terminal/sessions/{id}/close` 销毁会话与进程，不进入闲置或隐藏）。
   - **点“是”**：需要后台 Agent 处理经验落盘，才将终端转入“隐藏专区”（`PUT /api/terminal/sessions/{id}/hide`），落盘 prompt 末尾附带自毁关闭命令（`webclx terminal close ...` 或调用 `/close` API），落盘完成后终端自动自毁退出。
   - **点“取消”或按 Esc**：中止结束操作，保持当前终端状态不变。
3. 后端增加终端自毁与关闭 API：`POST/DELETE /api/terminal/sessions/{session_id}/close`，对 loopback 访问免除外部鉴权令牌，支持终端内脚本与 CLI（`webclx close <id>` / `webclx terminal close <id>`）调用。

验证：

```bash
node --test tests/terminal-experience-save.test.mjs
node --test tests/terminal-session-delete-safety.test.mjs
node --test tests/terminal-archives-and-idle.test.mjs
python3 tests/terminal-experience-save.browser.py
cargo test --bin webclx cli::tests session_hide_tests
```


## 结束终端后的下拉闪烁与点击丢失（2026-10-17）

“结束”先隐藏再关闭的流程曾先清空活动选择，串行等待两次 `loadSessions`；叠加连接与会话事件刷新，造成数秒重复渲染。`workspace-project-icons.js` 的每次 `sync` 又无条件重建触发器、图标和菜单按钮；刷新夹在 pointerdown/pointerup 之间时，原按钮已移除，点击无法提交。

隐藏成功后直接从本地现有会话选择同工作区优先的下一个终端，不等待列表请求；关闭完成仅更新本地列表。隐藏请求完成时重新检查当前选择，保留用户期间主动切换的终端。下拉渲染按内容签名跳过无变化刷新；展开期间延迟重建直到关闭，选择时核对目标仍存在且可用。

验证：`python3 tests/workspace_project_icons.browser.py` 在同一次鼠标按下/松开之间插入五次刷新，旧代码失败于按钮被移除，新代码保持焦点并成功切换；`python3 tests/terminal-experience-save.browser.py` 在桌面 1440×1000、移动 375×812 覆盖隐藏失败、关闭失败、落盘和延迟关闭期间通过真实下拉切换至第三终端，关闭完成后仍保持选择。另通过 selection-race、delete-safety、experience-save、workspace-project-icons 与 idle-session-icons Node 回归及两模块语法检查。运行目录静态链接保持指向源码，HTML 缓存键已更新，无需重启服务；浏览器拦截 API 验证不等同于操作真实用户终端。

### 关闭与后续工作并行（2026-10-17）

结束事务不再禁用全局“新开 / 结束 / 落盘并结束”按钮，仅按目标 session ID 防止重复提交。同一终端隐藏与关闭仍按顺序执行；用户可以在请求期间操作其他终端。后台成功或失败通过现有 `terminal-status-toast` 提示（`pointer-events: none`），不获取焦点；结束完成不再自动返回上一页，避免打断用户已经展开的新工作。隐藏失败不关闭目标；关闭失败保留隐藏终端供重试。

验证：`tests/terminal-experience-save.browser.py` 在桌面与移动尺寸故意挂起 close 响应，检查三个动作按钮可用、重复目标不重复发送、真实下拉切换和真实“新开”对话框可在关闭完成前操作；释放响应后对话框和焦点保持不变。另通过 selection-race、delete-safety、experience-save Node 回归及语法检查。静态缓存键为 `20261017-background-close`，无需重建或重启。
