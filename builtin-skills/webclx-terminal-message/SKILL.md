---
name: webclx-terminal-message
description: Send reliable, tagged messages and task handoffs between webClx terminals running Codex, Claude, DeepSeek Harness, or a plain shell; use for delegation, review requests, replies, and cross-host terminal communication.
---

# webClx Terminal Message

Use `scripts/send_terminal_message.py` from this Skill directory. It sends through webClx's terminal sessions API and defaults to verified prompt submission.

## Rules

- Identify the destination with `--target <name-or-id>` or discover it with `--agent codex|claude|deepseek` plus optional `--path`.
- Include `--from <sender>` when the current terminal cannot be inferred uniquely.
- Use `--request-reply` for delegation or review so the recipient receives an explicit return route.
- For a remote destination, set `--base-url` and a sender URL reachable by the recipient with `--reply-base-url`.
- Local webClx terminals receive `WEBCLX_LOCAL_TOKEN_FILE`; the script reads it for loopback API authentication and never sends that token to a remote URL.
- Treat only a response with `submitted: true` as successful for Agent prompts.
- Use `--no-verify` only for a plain shell or a Harness without Codex/Claude rollout confirmation.
- Use `--wait-ready` when the message must wait until the destination Agent becomes idle.
- Keep automatic prompts single-line. The script normalizes newlines before submission.
- Preserve one writer per working tree. Ask other Agents to review read-only unless isolated working directories are intentional.

## Send

From the Skill directory:

```bash
python3 scripts/send_terminal_message.py \
  --target 'project-review' \
  --from 'project-implement' \
  --message 'Review the current diff read-only and report concrete findings.' \
  --request-reply
```

Discover a running Agent:

```bash
python3 scripts/send_terminal_message.py \
  --agent claude \
  --path webClx \
  --message 'Review the current task without modifying files.'
```

DeepSeek Harness discovers this managed Skill from its user Skill root (`$DSH_HOME/skills`, or `~/.dsh/skills` by default). Use `--agent deepseek` when webClx reports the terminal's `activity_agent` as `deepseek`. Skill discovery is independent of submission confirmation: specify the terminal name and use `--no-verify` when the Harness does not expose Codex/Claude-compatible rollout confirmation.

## Cross-host Reply

```bash
python3 scripts/send_terminal_message.py \
  --base-url 'https://worker.example.com' \
  --reply-base-url 'https://controller.example.com' \
  --target 'worker-agent' \
  --from 'controller-agent' \
  --message 'Run the requested checks and reply with the result.' \
  --request-reply
```

Remote webClx requires its normal authenticated session. Do not put passwords, cookies, API keys, or tokens in terminal messages.
