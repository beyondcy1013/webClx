# Terminal Session Output Search

Date: 2026-05-23

## Goal

The home-page `活动终端` tab can search across readable output text from all non-idle terminal sessions and use the result to locate the right terminal.

## Implementation

- Frontend controls live in `static/index.html` with IDs `sessions-search-form`, `sessions-search-input`, `sessions-search-submit`, and `sessions-search-clear`.
- `static/app.js` calls `/api/terminal/sessions/search?q=...`, filters the active terminal table to matching sessions, and shows the matched line plus occurrence count in the `匹配` column.
- Search results are also a fallback session data source, so a fast search can render rows even if the normal session list has not finished loading.
- Backend route `/api/terminal/sessions/search` delegates to `TerminalManager::search_active_session_output`.
- Search reads full tmux pane text through `capture_tmux_text_pane_snapshot`, which intentionally omits ANSI escape sequences. If tmux capture is unavailable, live backlog is used as a fallback.
- Idle sessions are excluded to match the active terminal tab.

## Verification

```bash
node tests/terminal-session-search.test.mjs
cargo check
cargo test terminal -- --nocapture
```
