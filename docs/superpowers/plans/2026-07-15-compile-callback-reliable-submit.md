# Compile Callback Reliable Submit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make compile callbacks reach Codex/Claude as submitted user messages exactly once instead of occasionally remaining as composer text with trailing newlines.

**Architecture:** Keep the generic terminal message API backward compatible, but add opt-in agent-prompt framing and submission verification for compile callbacks. Agent prompts use bracketed-paste boundaries, send Enter separately, and confirm the unique request id against authoritative rollout history; retries send only Enter. The compile worker emits the browser toast immediately, then prefers a connected, non-busy terminal for prompt delivery.

**Tech Stack:** Rust/Axum terminal backend, PTY input, Bash compile worker, Codex/Claude rollout history, Rust and Node regression tests.

## Global Constraints

- Do not change raw terminal input semantics for existing callers.
- Do not resend callback text after an unconfirmed submission; retry only Enter.
- Treat rollout history, not HTTP 200 or reconstructed keystroke history, as the submission acknowledgement.
- Keep the existing browser toast as an independent immediate notification.
- Do not modify unrelated dirty worktree files.

---

### Task 1: Add Regression Coverage

**Files:**
- Modify: `src/terminal/tests.rs`
- Create: `tests/compile-callback-delivery.test.mjs`

**Interfaces:**
- Consumes: current `TerminalManager::send_session_message` and compile worker source.
- Produces: failing tests for bracketed prompt framing, rollout acknowledgement matching, non-busy readiness, immediate toast ordering, and verified delivery payloads.

- [ ] **Step 1: Write Rust framing and acknowledgement tests**

Add tests that require `prepare_terminal_message_body("line1\nline2", true)` to return `\x1b[200~line1\rline2\x1b[201~`, while raw mode remains unchanged. Add a test that requires delivery acknowledgement to match the unique delivery id in `TerminalInputHistoryEntry` values.

- [ ] **Step 2: Write compile worker contract test**

Require `notify_terminal` to send `bracketed_paste:true`, `verify_submission:true`, and `delivery_id`; require readiness to include `busy == false`; require toast notification to occur before waiting for prompt readiness; require a false `submitted` response to make notification fail.

- [ ] **Step 3: Run tests and verify RED**

Run:

```bash
cargo test terminal_message --lib
node tests/compile-callback-delivery.test.mjs
```

Expected: failures caused by missing framing/acknowledgement helpers and missing compile-worker delivery fields.

- [ ] **Step 4: Commit RED checkpoint**

```bash
git add docs/superpowers/plans/2026-07-15-compile-callback-reliable-submit.md src/terminal/tests.rs tests/compile-callback-delivery.test.mjs
git commit -m "test: 添加编译回调可靠提交复现"
```

### Task 2: Implement Verified Agent-Prompt Delivery

**Files:**
- Modify: `src/terminal.rs`
- Modify: `src/terminal/manager.rs`
- Modify: `docs/codex/skills/webclx-rebuild/scripts/compile-worker.sh`

**Interfaces:**
- Consumes: `TerminalMessageRequest`, `TerminalManager::session_input_history`, and request ids already embedded in callback text.
- Produces: optional `bracketed_paste`, `verify_submission`, and `delivery_id` request fields; response fields `submitted` and `submit_attempts`.

- [ ] **Step 1: Implement prompt framing**

Normalize CRLF/LF to CR and wrap opted-in prompt bodies as `\x1b[200~<body>\x1b[201~`. Keep raw mode byte-for-byte unchanged. Continue sending the body and every Enter as separate PTY writes.

- [ ] **Step 2: Implement rollout acknowledgement and Enter-only retry**

After the initial send, poll authoritative session input history for `delivery_id`. When absent, wait with bounded backoff and send one standalone `\r`; never resend the body. Return whether acknowledgement was observed and the total Enter attempt count.

- [ ] **Step 3: Update compile worker ordering and payload**

Send the toast before readiness waiting. Restore `connected && !busy` as the preferred readiness condition. Submit callbacks with bracketed framing, verification, and request id, then reject a response whose `submitted` field is not true.

- [ ] **Step 4: Run focused tests and verify GREEN**

Run:

```bash
cargo test terminal_message --lib
node tests/compile-callback-delivery.test.mjs
bash -n docs/codex/skills/webclx-rebuild/scripts/compile-worker.sh
```

Expected: all commands exit 0.

- [ ] **Step 5: Commit GREEN checkpoint**

```bash
git add src/terminal.rs src/terminal/manager.rs docs/codex/skills/webclx-rebuild/scripts/compile-worker.sh src/terminal/tests.rs tests/compile-callback-delivery.test.mjs
git commit -m "fix: 编译回调使用可确认的终端提交"
```

### Task 3: Verify And Deploy

**Files:**
- Modify: `docs/codex/tasks/agent-api.md`

**Interfaces:**
- Consumes: verified delivery response and compile API deployment callback.
- Produces: documented message-delivery contract and deployed webClx behavior.

- [ ] **Step 1: Document the verified delivery contract**

Record the opt-in fields, rollout acknowledgement semantics, Enter-only retry behavior, and the distinction between immediate toast and deferred prompt delivery.

- [ ] **Step 2: Run project verification**

Run focused Rust tests, affected Node tests, `cargo check`, `git diff --check`, and shell syntax validation.

- [ ] **Step 3: Deploy through the webClx deploy API**

Queue the documented webClx deploy request. After the callback, verify the running service and exercise a disposable terminal with a long callback-shaped prompt, confirming the exact delivery id appears once in rollout history.

- [ ] **Step 4: Commit documentation if changed after GREEN**

```bash
git add docs/codex/tasks/agent-api.md
git commit -m "docs: 记录编译回调确认投递机制"
```

