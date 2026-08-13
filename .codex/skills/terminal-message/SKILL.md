---
name: terminal-message
description: Use when sending messages or notifications between webClx terminal sessions.
---

# Terminal Message

Use this skill to send text into a webClx terminal by terminal name or session id. It is a cross-project skill: the API is implemented by the local webClx service, but the skill can be invoked from any repository or terminal.

## Quick Start

Prefer the bundled script so JSON quoting, sender tagging, busy-terminal delivery, and Codex/Claude Enter behavior stay consistent:

```bash
python3 /home/root/.codex/skills/terminal-message/scripts/send_terminal_message.py \
  --target 'webClx#1' \
  --message 'message text'
```

By default this sends a bracketed-paste prompt followed by one initial Enter:

```text
[from <sender-terminal-name>] message text
```

The API confirms that the text became a real Codex/Claude rollout user message. The backend waits for bracketed paste processing to settle before the first Enter, allows rollout persistence time after each attempt, and retries Enter with bounded backoff without duplicating the message body. The script uses a 45-second HTTP timeout and exits nonzero unless the submission is confirmed.

## Required Habits

- Include the sender terminal name in messages sent to another agent terminal.
- Send the intended text directly; do not wrap it in `echo ...` unless the user explicitly wants a shell command executed.
- Keep auto-submitted Codex/Claude prompts single-line by default. Multi-line composer text can remain unsubmitted in some TUI states.
- Use real newline characters (`\n`) when needed, never the literal typo `/n`.
- Send immediately by default even when the target is busy. Browser foreground/background state does not change the server-side PTY input path. Use `--wait-ready` only when the caller explicitly wants delivery delayed until the target becomes idle.
- Require rollout-confirmed submission for Codex/Claude targets. Use `--no-verify` only for plain shells or other programs without a Codex/Claude rollout file.
- Treat `submitted: true` as the only success result. Reliable delivery can take several seconds; do not wrap this script or API in a timeout shorter than 30 seconds.
- When the terminal name is unknown, use `--agent codex|claude` (and usually `--path`) so the script resolves it through `GET /api/terminal/sessions?all=true`. Never guess among multiple candidates.
- When the resolved terminal may only contain a shell, add `--start-if-needed`. The script starts the requested agent through `/api/terminal/auto-typed-input`, waits for `activity_agent` to confirm it, and only then sends the message. It refuses to replace a different running agent.
- If asking the recipient to reply, explicitly instruct it to use the `terminal-message` skill and target the sender terminal.
- For cross-server replies, pass `--reply-base-url <sender-webclx-url>` or set `WEBCLX_REPLY_URL`. This URL is the sender's reachable webClx endpoint, not the destination passed to `--base-url`. Never guess a LAN IP that the recipient may not be able to reach.
- Before sending a reply request, the script queries the reply endpoint and requires it to resolve the sender terminal uniquely. A remote destination cannot use a loopback reply URL. If `--from` is omitted, infer the sender from the reply endpoint, not the destination.
- Auto-submitted messages are normalized to one line so embedded newlines cannot remain in the Agent composer; `--no-enter` preserves intentional multiline input.
- On this server, the recorded public reply endpoint is `http://fpsq.xyz:11112`; use it unless the user provides a newer endpoint.
- Avoid literal `$terminal-message` in auto-submitted terminal text because `$...` can trigger TUI mention/skill pickers and prevent Enter from submitting.

## Script Options

- `--target <terminal-name-or-session-id>` selects the destination.
- `--agent codex|claude` verifies the requested Agent and can discover its terminal when `--target` is omitted.
- `--start-if-needed` starts `--agent` in the uniquely resolved shell terminal before delivery. Use `--agent-start-timeout 30` to adjust the wait.
- `--from <sender-terminal-name>` sets the sender when it cannot be inferred from the current cwd.
- `--path <relative-workspace-path>` disambiguates duplicate terminal names.
- `--base-url http://host:11111` targets a non-local webClx service. The default is `WEBCLX_URL` or `http://127.0.0.1:11111`.
- `--reply-base-url http://sender-host:11111` tells a remote recipient where to send its reply. It defaults to `WEBCLX_REPLY_URL`; loopback messages reuse `--base-url`. Remote `--request-reply` calls fail unless the URL is reachable, non-loopback, and uniquely resolves the sender terminal.
- `--no-enter` inserts text without submitting.
- `--submit-enters 1` is the default initial submission attempt. The API retries Enter only when rollout confirmation is still missing.
- `--no-verify` disables rollout confirmation for plain shell targets.
- `--dry-run` prints the JSON payload without writing to a terminal.
- `--request-reply` appends a reply instruction that names this skill and the sender terminal.
- `--wait-ready` waits for the target to become idle before sending; `--wait-ready-timeout 120` controls the timeout.
- `--no-wait-ready` is retained for compatibility and is no longer needed because immediate sending is the default.

Example with an explicit sender and reply request:

```bash
python3 /home/root/.codex/skills/terminal-message/scripts/send_terminal_message.py \
  --target 'webClx#3' \
  --from 'webClx#2' \
  --base-url 'http://remote-webclx:11111' \
  --reply-base-url 'http://sender-webclx:11111' \
  --message '请确认收到这条终端通讯测试。' \
  --request-reply
```

Discover a Codex terminal in one project without knowing its terminal name:

```bash
python3 /home/root/.codex/skills/terminal-message/scripts/send_terminal_message.py \
  --agent codex \
  --path 'webClx' \
  --message '请检查当前任务。'
```

Start Claude in a known shell terminal when needed, wait for readiness, then send:

```bash
python3 /home/root/.codex/skills/terminal-message/scripts/send_terminal_message.py \
  --target 'project-shell' \
  --agent claude \
  --start-if-needed \
  --message '请处理这个任务。'
```

## Manual API

The webClx route is:

```text
POST /api/terminal/sessions/message
```

Route registration: `/home/codes/webClx/src/main.rs`.
Request parsing and send behavior: `/home/codes/webClx/src/terminal.rs`.

Payload shape:

```json
{
  "target": "webClx#1",
  "path": "optional/relative/path",
  "data": "[from webClx#2] message text",
  "submit": true,
  "submit_enters": 1,
  "bracketed_paste": true,
  "verify_submission": true,
  "delivery_id": "[from webClx#2] message text"
}
```

Accepted target aliases include `target`, `session_id`, `terminal_name`, or `name`. Accepted message aliases include `data` or `message`. `submit_enters` is capped at 4; if it is omitted, legacy `enter: true` or `submit: true` sends one Enter. Treat only `submitted: true` as successful verified delivery.

For Windows `cmd.exe`, quote the JSON with escaped double quotes:

```bat
curl -X POST "http://192.168.3.2:11111/api/terminal/sessions/message" -H "Content-Type: application/json" -d "{\"target\":\"webClx#1\",\"data\":\"[from webClx#2] message text\",\"submit\":true,\"submit_enters\":1,\"bracketed_paste\":true,\"verify_submission\":true,\"delivery_id\":\"[from webClx#2] message text\"}"
```
