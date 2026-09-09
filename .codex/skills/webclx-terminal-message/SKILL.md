---
name: webclx-terminal-message
description: Use when messaging another webClx terminal through the terminal sessions API.
---

# webClx Terminal Message

Use this skill to send text into a target webClx terminal by terminal name or session id. By default, messages are submitted with Enter so Codex/Claude treats them like a normal user prompt.

## Rules

- Always include the sender terminal name in the text sent to the target terminal.
- Send the intended message text, not an `echo ...` wrapper, unless the user explicitly asks to run a shell command.
- To simulate a Codex/Claude prompt submission, inject bracketed-paste text through `/api/terminal/sessions/message`, then make a separate API call to `/api/terminal/sessions/<id>/enter`. The Enter endpoint uses the same semantic key path as the soft keyboard's `快捷 > Enter`, allows rollout persistence time, and retries Enter with bounded backoff without duplicating the body.
- Keep auto-submitted prompts single-line by default. Multi-line text can stay in the Codex/Claude composer instead of submitting, even when Enter is appended.
- When asking the recipient to reply, explicitly tell it to use the `webclx-terminal-message` skill, the sender terminal, and the sender's reachable webClx URL. Pass that URL with `--reply-base-url` or `WEBCLX_REPLY_URL`; do not assume the recipient knows the sender's IP. Avoid literal `$webclx-terminal-message` in auto-submitted terminal text because `$...` can trigger a TUI mention/skill picker and prevent Enter from submitting.
- Resolve the sender through the reply endpoint's sessions API on every call, never from memory. The script resolves in priority order: an exact `--from`/`WEBCLX_TERMINAL_NAME` match, then the webClx tmux pane it runs in (`TMUX` socket `webclx_s<session-id>` verified against the API), then the unique terminal matching the working directory. Also reject loopback reply URLs for remote destinations.
- Normalize auto-submitted text to one line; preserve multiline text only for `--no-enter` insertion.
- For a remote reply, use the operator-provided reachable endpoint `REPLY_BASE_URL`; never publish or hard-code a server address.
- Use real newline characters (`\n`) when a newline is needed. Do not send the literal typo `/n`.
- Prefer `scripts/send_terminal_message.py` so JSON quoting, sender tagging, and Windows curl quoting mistakes are avoided.
- The script sends immediately by default, including to a busy target. Browser foreground/background state does not change the server-side PTY path. Use `--wait-ready` only when delivery should be delayed until the target is idle.
- `--wait-ready` must trust the backend `busy` flag and only block on a genuine turn/build/error state (`working`, `building`, `error`, `retrying`, `recent_output`, or `orphaned_agent`). Do not treat `agent` as busy: it means an idle Agent process is present. A completed turn may retain `completed`/待查看 until viewed and is ready.
- The script exits nonzero unless Codex/Claude rollout submission is confirmed. Use `--no-verify` only for plain shell targets.
- Treat only `submitted: true` as success and allow at least 30 seconds for reliable delivery; the bundled script uses a 45-second HTTP timeout.
- Judge delivery from the API response fields, never from HTTP status alone or from the target terminal's apparent UI state:
  - An HTTP 2xx response only means the API request was accepted; it is not proof that the prompt was delivered.
  - For Codex/Claude prompts, require both `ok == true` and `submitted == true`. A missing, false, or non-boolean `submitted` value is a delivery failure.
  - If `--request-reply` is used, the sender must have been resolved against the reply endpoint before sending; the appended reply instruction carries the sender name plus its `s<id>` session id so the recipient can target either. If the script cannot resolve a sender it exits nonzero without delivering anything; a sent prompt without a valid return route is not a successful request-reply operation.
  - For plain shell targets using `--no-verify`, report the API result as insertion only unless a separate target-side acknowledgement or history query confirms receipt.
  - On any failed or ambiguous result, preserve the complete status code and JSON/error body, report the failure, and do not claim the message was sent. Check the target input history with `GET /api/terminal/sessions/<id>/input-history` when receipt must be independently confirmed.
- If the terminal name is unknown, use `--agent codex|claude|deepseek` and optionally `--path`; the script discovers sessions through `GET /api/terminal/sessions?all=true` and rejects ambiguous matches.
- If the resolved terminal has no Agent, use `--start-if-needed`; the script starts it through `/api/terminal/auto-typed-input`, waits for `activity_agent`, then sends the message. It never replaces a different running Agent.
- Never invent or recall a terminal name from memory. Terminal names must come from the live sessions API (`GET /api/terminal/sessions?all=true`) in the same session. Inside a webClx terminal, omit `--from` entirely: the script auto-detects the current terminal. Only pass `--from <name-or-session-id>` with a value just fetched from the API; an unresolvable value is demoted to a warning and replaced by auto-detection instead of blocking delivery.
- If the sender terminal cannot be auto-detected (for example when calling from outside any webClx terminal), fetch the exact name or session id from the sessions API first, then pass it as `--from`.
- Local managed terminals receive `WEBCLX_LOCAL_TOKEN_FILE`. The script reads it for loopback authentication and never sends the token to a remote URL.

## Quick Start

Send a tagged message and press Enter:

```bash
python3 /home/codes/webClx/.codex/skills/webclx-terminal-message/scripts/send_terminal_message.py \
  --target 'webClx#1' \
  --message 'webClx-message-api-ok-from-codex'
```

This sends:

```text
[from <sender-terminal-name>] webClx-message-api-ok-from-codex
```

Ask the recipient to reply through the same skill:

```bash
python3 /home/codes/webClx/.codex/skills/webclx-terminal-message/scripts/send_terminal_message.py \
  --target 'webClx#3' \
  --from 'webClx#2' \
  --base-url 'http://REMOTE_HOST:PORT' \
  --reply-base-url 'http://SENDER_HOST:PORT' \
  --message '你好 Claude，这是终端通讯测试，请回复一句确认收到。' \
  --request-reply
```

This appends:

```text
；请使用名为 webclx-terminal-message 的 skill 回复，回复端点为 REPLY_BASE_URL，目标终端为 webClx#2，不要只在你自己的终端里回答。
```

To insert a newline inside the message body, use shell quoting that produces a real newline. Avoid this for automatic Codex/Claude prompt submission unless you have verified the target TUI submits multi-line composer content correctly:

```bash
python3 /home/codes/webClx/.codex/skills/webclx-terminal-message/scripts/send_terminal_message.py \
  --target 'webClx#1' \
  --message $'webClx-message-api-ok-from-codex\n'
```

## Target Selection

- Use `--target <terminal-name-or-session-id>`.
- Use `--agent codex|claude` to verify or discover the destination Agent.
- Use `--start-if-needed` with `--agent` to start it in the uniquely resolved shell; `--agent-start-timeout` controls the wait.
- Add `--path <relative-workspace-path>` when terminal names may be duplicated.
- Use `--base-url http://REMOTE_HOST:PORT` for a non-local webClx service.
- Use `--reply-base-url http://SENDER_HOST:PORT` (or `WEBCLX_REPLY_URL`) with remote `--request-reply` calls. The script rejects missing or loopback remote reply endpoints and endpoints that cannot resolve the sender terminal.
- Use `--no-enter` only when the text should be inserted without submitting. Do not use it for Codex/Claude command simulation.
- Use the default `--submit-enters 1`; verified delivery supplies later Enter retries only when the first attempt was not committed to the rollout.
- Use `--no-verify` only when the target is a plain shell or another application without a Codex/Claude rollout file.
- Use `--dry-run` to print the JSON payload without writing to a terminal.
- Use `--request-reply` whenever the message asks the recipient to respond back to the sender.

Discover a Codex terminal by project path:

```bash
python3 /home/codes/webClx/.codex/skills/webclx-terminal-message/scripts/send_terminal_message.py \
  --agent codex \
  --path 'webClx' \
  --message '请检查当前任务。'
```

Start Claude in a known shell and send after it is detected:

```bash
python3 /home/codes/webClx/.codex/skills/webclx-terminal-message/scripts/send_terminal_message.py \
  --target 'project-shell' \
  --agent claude \
  --start-if-needed \
  --message '请处理这个任务。'
```

## Manual API Shape

When calling the API directly:

```json
{
  "target": "webClx#1",
  "path": "webClx",
  "data": "[from webClx#1] webClx-message-api-ok-from-codex",
  "submit": true,
  "submit_enters": 1,
  "bracketed_paste": true,
  "verify_submission": true,
  "delivery_id": "[from webClx#1] webClx-message-api-ok-from-codex"
}
```

Send immediately through the operator-provided webClx endpoint. Use
`--noproxy '*'` so a shell-level HTTP proxy cannot reroute the request:

```bash
curl -sS --noproxy '*' -X POST \
  'http://REMOTE_HOST:PORT/api/terminal/sessions/message' \
  -H 'Content-Type: application/json' \
  -d '{
    "target": "TARGET_SESSION",
    "path": "tools/webClx",
    "data": "[from webClx_14] terminal-name self-test",
    "submit": true,
    "submit_enters": 1,
    "bracketed_paste": true,
    "verify_submission": true,
    "delivery_id": "terminal-name-self-test-001"
  }'
```

## Scheduled Terminal Messages

`POST /api/terminal/scheduled-inputs` can persist a terminal message and inject
it later. Unlike the immediate-message endpoint, the current scheduled-input
API targets the stable `session_id` (`s...`) in its `session_id` field. Resolve
a terminal name first through `GET /api/terminal/sessions?all=true` when only
the display name is known.

`due_at` is an absolute Unix timestamp in milliseconds and must be in the
future. The server may move it to the end of a configured avoidance window, so
use the response's `task.due_at` as the authoritative execution time. Keep
auto-submitted Agent prompts on one line.

```bash
due_at="$(( $(date +%s) * 1000 + 60000 ))"

jq -n \
  --arg session_id 's5233' \
  --arg text '[from webClx_14] 检查 webClx 请求 142245 回调；成功后用 release parity 运行 business-ts 竞价报告并核对匹配数。' \
  --argjson due_at "$due_at" \
  '{session_id: $session_id, due_at: $due_at, send_enter: true, text: $text}' | \
curl -sS --noproxy '*' -X POST \
  'http://REMOTE_HOST:PORT/api/terminal/scheduled-inputs' \
  -H 'Content-Type: application/json' \
  --data-binary @-
```

List or cancel persisted scheduled messages:

```bash
curl -sS --noproxy '*' \
  'http://REMOTE_HOST:PORT/api/terminal/scheduled-inputs'

curl -sS --noproxy '*' -X DELETE \
  'http://REMOTE_HOST:PORT/api/terminal/scheduled-inputs/TASK_ID'
```

For Windows `cmd.exe`, use double quotes and escape JSON quotes:

```bat
curl -X POST "http://HOST:PORT/api/terminal/sessions/message" -H "Content-Type: application/json" -d "{\"target\":\"TARGET_SESSION\",\"data\":\"[from SENDER_SESSION] MESSAGE\",\"submit_enters\":1,\"bracketed_paste\":true,\"verify_submission\":true,\"delivery_id\":\"DELIVERY_ID\"}"
```
