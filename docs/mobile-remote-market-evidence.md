# Mobile remote coding-agent market evidence

Captured 2026-08-14 from public GitHub Search and Hacker News Algolia results.
This is product-research evidence, not a claim that the linked projects endorse
webClx. Counts and issue state can change after capture.

## Repeated jobs to be done

1. Leave the workstation while a long coding task continues.
2. Check progress and determine whether the Agent is blocked.
3. Approve, reject, interrupt, or send a short steering message from a phone.
4. Return to the same live CLI session without forking or rebuilding context.
5. Reach sessions that were already running, rather than remembering to enable
   a remote mode before leaving.
6. Retrieve generated files and inspect build output remotely.

The phone is usually described as a companion or control surface. Users still
expect the workstation, terminal, and full keyboard to remain the primary
coding environment.

## Public issue evidence

- OpenAI Codex issue 27565 asks for Claude-style remote control with simple
  phone pairing and synchronized terminal/mobile messages:
  https://github.com/openai/codex/issues/27565
- OpenAI Codex issue 37967 describes the workstation-primary workflow and asks
  to attach to an in-progress CLI session for monitoring, approvals, and short
  steering without quit/resume or fork:
  https://github.com/openai/codex/issues/37967
- OpenAI Codex issue 33358 asks to download files created by a remote task
  directly from mobile:
  https://github.com/openai/codex/issues/33358
- Command Code issue 674 asks for a `/remote` URL or QR code to continue the
  current session, view progress, and send prompts:
  https://github.com/CommandCodeAI/command-code/issues/674
- Freebuff issue 982 asks to follow a desktop task, receive decision/approval
  prompts, and steer it from a phone while the Agent keeps working:
  https://github.com/CodebuffAI/freebuff/issues/982
- mux-pod issue 71 asks for an Agent-aware mobile UI over an existing SSH/tmux
  foundation rather than raw slash commands on a terminal keyboard:
  https://github.com/moezakura/mux-pod/issues/71
- Cindy issue 1373 asks to discover and continue external Codex or Claude Code
  sessions even when the user did not prepare remote access before leaving:
  https://github.com/makecindy/cindy/issues/1373
- Anthropic Claude Code issue 78246 asks to start additional sessions remotely
  instead of being limited to sessions prepared on the computer:
  https://github.com/anthropics/claude-code/issues/78246
- Atrium issue 43 asks for notifications, read-only oversight, and mobile
  decisions when Agents run overnight or away from the desk:
  https://github.com/jonnyasmar/atrium-issues/issues/43

## Market shape and positioning consequence

Hacker News search results include Claude Code Remote, Pocodex, Pocket Agent,
Polpo, Zedra, Detach, Clsh, and other mobile control projects. Most sampled
Show HN submissions had only zero to five points/comments. This supports two
conclusions:

- the problem is recurring and multiple builders are trying to solve it;
- generic "control an AI Agent from your phone" positioning is crowded and
  does not produce strong organic distribution by itself.

webClx should therefore lead with a narrower proof:

> Keep the same native tmux-backed CLI alive, supervise it from a phone, return
> to the same live session, and hand work between Codex, Claude, and DeepSeek
> Harness terminals for review.

Do not claim that mobile replaces the desktop, that every external session is
automatically discovered, or that hosted access is generally available.
