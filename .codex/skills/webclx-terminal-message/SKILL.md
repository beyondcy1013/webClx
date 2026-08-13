---
name: webclx-terminal-message
description: Use when messaging another webClx terminal through the terminal sessions API.
---

# webClx Terminal Message

Use this skill to send text into a target webClx terminal by terminal name or session id. By default, messages are submitted with Enter so Codex/Claude treats them like a normal user prompt.

## Rules

- Always include the sender terminal name in the text sent to the target terminal.
- Send the intended message text, not an `echo ...` wrapper, unless the user explicitly asks to run a shell command.
- To simulate a Codex/Claude prompt submission, send bracketed-paste text, wait for TUI paste processing to settle, and then send one initial Enter. The API allows rollout persistence time and retries Enter with bounded backoff without duplicating the body when confirmation is missing.
- Keep auto-submitted prompts single-line by default. Multi-line text can stay in the Codex/Claude composer instead of submitting, even when Enter is appended.
- When asking the recipient to reply, explicitly tell it to use the `webclx-terminal-message` skill, the sender terminal, and the sender's reachable webClx URL. Pass that URL with `--reply-base-url` or `WEBCLX_REPLY_URL`; do not assume the recipient knows the sender's IP. Avoid literal `$webclx-terminal-message` in auto-submitted terminal text because `$...` can trigger a TUI mention/skill picker and prevent Enter from submitting.
- Preflight reply requests through the reply endpoint's sessions API. Require a unique sender match, reject loopback reply URLs for remote destinations, and infer an omitted sender from the reply endpoint rather than the destination.
- Normalize auto-submitted text to one line; preserve multiline text only for `--no-enter` insertion.
- On this server, use the recorded public reply endpoint `http://fpsq.xyz:11112` unless the user provides a newer endpoint.
- Use real newline characters (`\n`) when a newline is needed. Do not send the literal typo `/n`.
- Prefer `scripts/send_terminal_message.py` so JSON quoting, sender tagging, and Windows curl quoting mistakes are avoided.
- The script sends immediately by default, including to a busy target. Browser foreground/background state does not change the server-side PTY path. Use `--wait-ready` only when delivery should be delayed until the target is idle.
- The script exits nonzero unless Codex/Claude rollout submission is confirmed. Use `--no-verify` only for plain shell targets.
- Treat only `submitted: true` as success and allow at least 30 seconds for reliable delivery; the bundled script uses a 45-second HTTP timeout.
- If the terminal name is unknown, use `--agent codex|claude|deepseek` and optionally `--path`; the script discovers sessions through `GET /api/terminal/sessions?all=true` and rejects ambiguous matches.
- If the resolved terminal has no Agent, use `--start-if-needed`; the script starts it through `/api/terminal/auto-typed-input`, waits for `activity_agent`, then sends the message. It never replaces a different running Agent.
- If the sender terminal cannot be uniquely inferred, pass `--from <sender-terminal-name>` explicitly.
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
  --base-url 'http://remote-webclx:11111' \
  --reply-base-url 'http://sender-webclx:11111' \
  --message '你好 Claude，这是终端通讯测试，请回复一句确认收到。' \
  --request-reply
```

This appends:

```text
；请使用名为 webclx-terminal-message 的 skill 回复，回复端点为 http://sender-webclx:11111，目标终端为 webClx#2，不要只在你自己的终端里回答。
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
- Use `--base-url http://host:11111` for a non-local webClx service.
- Use `--reply-base-url http://sender-host:11111` (or `WEBCLX_REPLY_URL`) with remote `--request-reply` calls. The script rejects missing or loopback remote reply endpoints and endpoints that cannot resolve the sender terminal.
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

For Windows `cmd.exe`, use double quotes and escape JSON quotes:

```bat
curl -X POST "http://192.168.3.2:11111/api/terminal/sessions/message" -H "Content-Type: application/json" -d "{\"target\":\"webClx#1\",\"data\":\"[from webClx#1] webClx-message-api-ok-from-codex\",\"submit_enters\":1,\"bracketed_paste\":true,\"verify_submission\":true,\"delivery_id\":\"[from webClx#1] webClx-message-api-ok-from-codex\"}"
```
