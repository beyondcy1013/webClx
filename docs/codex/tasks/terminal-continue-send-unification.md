# Terminal Continue Send Unification

Date: 2026-07-05

## Implemented

2026-07-05: `继续` submission now has a canonical backend path.

Stable conclusions:

- Backend canonical function: `TerminalManager::send_session_continue`.
- Public endpoint: `POST /api/terminal/sessions/{session_id}/continue`.
- Submission semantics intentionally match the slash soft-keyboard `继续`: write `继续`, wait briefly, then send a standalone Enter.
- Manual `继续`, `send_session_message` with `submit_enters=1`, and server-side scheduled paste/input tasks whose text is exactly `继续` call the canonical unthrottled backend sender.
- Automatic browser retries, backend immediate retries, reset-time due tasks, and cron scripts use the cooldown-aware automatic sender. The public automatic endpoint is `POST /api/terminal/sessions/{session_id}/auto-continue`; it validates the current error and applies the configured per-terminal cooldown before calling the canonical command writer.
- Reset-time cron scripts call the local `/auto-continue` endpoint instead of duplicating `tmux send-keys` semantics. The generated script uses `WEBCLX_BASE_URL` when set, otherwise `http://127.0.0.1:${WEBCLX_PORT:-11111}`.
- Persisted reset-time tasks refresh missing or old cron scripts when the scheduler sees the same task again, so tasks created before this change are upgraded to the `/continue` endpoint template.
- Reinstalling a reset-time cron entry removes both old command marker lines and old `webclx-auto-continue-due` comments for the same session.
- The auto-continue runner also checks already-loaded persisted tasks for stale cron scripts/comments, because an existing task may not flow through new schedule collection again.
- Frontend auto-continue calls `/auto-continue`; manual soft-keyboard `继续` calls `/continue`, and generic `/input` remains available for raw terminal input.
- `GET /api/terminal/sessions/{session_id}/input-history` filters standalone `继续` entries after selecting the rollout or keystroke-history source. This keeps automatic and manual continuation turns out of “对话史” without removing normal prompts such as `继续处理这个问题`.
- Terminal-page scheduled-task chips must aggregate both `/api/terminal/scheduled-inputs` and `/api/terminal/auto-continue-tasks`. The chip text is `定时 当前终端数/全部任务数`; newly detected auto-continue tasks should also emit a toast so server-side detection is visible even when the paste dialog did not create the task.
- Regression coverage lives in `src/terminal/tests.rs`, `tests/terminal-error-status.test.mjs`, and `tests/terminal-session-details.test.mjs`.

## Starting Points

- Frontend slash command path: `static/terminal.js` (`sendContinueCommand`, `sendTextCommand`, slash-command dispatch)
- Session input API: `POST /api/terminal/sessions/{session_id}/input`
- Backend send path: `TerminalManager::send_session_input` and `write_session_input_inner`
- Auto-continue paths: `TerminalManager::run_due_auto_continue_task`, `maybe_send_error_auto_continue`, and generated auto-continue cron script
