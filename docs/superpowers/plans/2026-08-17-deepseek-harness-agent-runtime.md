# DeepSeek Harness Agent Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the user-facing webClx native Agent runtime with the deployed DeepSeek Harness web application while retaining webClx terminal profiles, preset execution, Codex task APIs, and historical native session data.

**Architecture:** `/agent` becomes a server-side redirect to a configurable DeepSeek Harness URL. Terminal profile management moves to `/agent/profiles`; Codex and Claude profiles continue launching managed webClx terminals, while legacy `native` profiles remain visible but cannot create new native sessions. Native chat/session routes are removed from the protected router, but existing session JSON is not deleted.

**Tech Stack:** Rust 2024, Axum, static HTML/JavaScript, Node.js contract tests, systemd-hosted DeepSeek Harness.

## Global Constraints

- Work only in the primary checkout at `/home/codes/webClx`; do not create a Git worktree.
- Preserve unrelated dirty changes in terminal session tests.
- Do not delete `/home/bin/webclx/.webclx-agent-sessions.json` or mutate existing runtime session data.
- Keep `/api/codex/tasks`, `/api/agent/exec-with-preset`, terminal profiles, preset listing, Skill discovery, and terminal-message behavior.
- Use `WEBCLX_DEEPSEEK_HARNESS_URL` when configured; otherwise derive `http://<request-host>:3080/` without trusting a request-supplied port.
- Submit compile/deploy through the running webClx API.

---

### Task 1: Harness Routing Contract

**Files:**
- Create: `tests/deepseek-harness-agent-routing.test.mjs`
- Modify: `src/routes/agent.rs`
- Modify: `src/agent.rs`

**Interfaces:**
- Consumes: request `Host` and optional `WEBCLX_DEEPSEEK_HARNESS_URL`.
- Produces: `GET /agent` temporary redirect and `GET /agent/profiles` HTML management page.

- [ ] **Step 1: Write the failing static contract test**

Assert that `/agent` is handled by `deepseek_harness_redirect`, `/agent/profiles` is handled by `agent_profiles_page`, and native session/chat/tool/config mutation routes are absent from `src/routes/agent.rs`.

- [ ] **Step 2: Run the test to verify RED**

Run: `node --test tests/deepseek-harness-agent-routing.test.mjs`

Expected: FAIL because the redirect and profile route do not exist and native routes are still registered.

- [ ] **Step 3: Implement minimal routing behavior**

Add URL normalization and request-host fallback helpers. Keep terminal profile, preset, Skill listing, and `exec-with-preset` routes. Remove native session/chat, approval, checkpoint, background-command, model/config mutation routes from the router.

- [ ] **Step 4: Run the focused test to verify GREEN**

Run: `node --test tests/deepseek-harness-agent-routing.test.mjs`

Expected: PASS.

### Task 2: Compact Terminal Profile Management Page

**Files:**
- Create: `static/agent-profiles.html`
- Modify: `static/agent-terminal-profiles.js`
- Modify: `tests/agent-terminal-profiles.test.mjs`

**Interfaces:**
- Consumes: existing terminal profile, preset, Skill, and terminal session APIs.
- Produces: profile CRUD and terminal launch UI at `/agent/profiles` without native chat/session dependencies.

- [ ] **Step 1: Update tests for the replacement UI**

Require a DeepSeek Harness command in the profile page, forbid a `native` option in the editor, and require legacy native profiles to render a migration status instead of calling `/api/agent/sessions`.

- [ ] **Step 2: Run the test to verify RED**

Run: `node --test tests/agent-terminal-profiles.test.mjs`

Expected: FAIL against the current mixed native-chat page and controller.

- [ ] **Step 3: Implement the compact page and controller**

Build a compact operational page using existing top navigation and global styles. Remove native session state/controller calls from `agent-terminal-profiles.js`; render legacy native profiles read-only with an `Open DeepSeek Harness` command, while Codex/Claude profiles keep open/new/edit/delete actions.

- [ ] **Step 4: Run the test to verify GREEN**

Run: `node --test tests/agent-terminal-profiles.test.mjs`

Expected: PASS.

### Task 3: Navigation And Documentation Migration

**Files:**
- Modify: `static/index.html`
- Modify: `static/terminal.html`
- Modify: `src/artifacts.rs`
- Modify: `tests/top-navigation-order.test.mjs`
- Modify: `tests/backend-architecture.test.mjs`
- Modify: `README.md`
- Modify: `docs/codex/tasks/agent-api.md`

**Interfaces:**
- Consumes: `/agent` Harness redirect and `/agent/profiles` management page.
- Produces: consistent `DeepSeek Agent` navigation and an accurate ownership contract.

- [ ] **Step 1: Update navigation and architecture tests for the new ownership**

Require visible navigation text `DeepSeek Agent`, retain `/agent` as the route, and remove native Agent API paths from the expected backend route catalog while retaining control-plane paths.

- [ ] **Step 2: Run affected tests to verify RED**

Run: `node --test tests/top-navigation-order.test.mjs tests/backend-architecture.test.mjs tests/deepseek-harness-agent-routing.test.mjs`

Expected: FAIL until production navigation and routes match.

- [ ] **Step 3: Update production navigation and docs**

Change navigation labels and links, expose `/agent/profiles` from the profile page, and document that DeepSeek Harness owns agent loops/sessions/tools while webClx owns terminal profiles, presets, tasks, and messaging.

- [ ] **Step 4: Run affected tests to verify GREEN**

Run: `node --test tests/top-navigation-order.test.mjs tests/backend-architecture.test.mjs tests/deepseek-harness-agent-routing.test.mjs`

Expected: PASS.

### Task 4: Verification And Deployment

**Files:**
- Verify only; deploy through existing scripts.

**Interfaces:**
- Consumes: completed source changes.
- Produces: verified deployed webClx behavior with DeepSeek Harness still healthy.

- [ ] **Step 1: Run focused Node tests**

Run all Agent, navigation, architecture, static-entry, and terminal-profile contract tests affected by the change.

- [ ] **Step 2: Queue Rust and Node verification through webClx**

Submit a compile command with a bounded targeted Rust test/check and affected Node tests through `request-webclx-compile-api.sh`; wait for the matching callback.

- [ ] **Step 3: Queue deployment through webClx**

Commit only task files, preserve unrelated dirty state using the compile/deploy skill helper, then submit `scripts/rebuild-and-deploy.sh` through `request-webclx-deploy-api.sh` with runtime audit paths.

- [ ] **Step 4: Verify runtime behavior**

Confirm webClx is healthy, `/agent` redirects to the configured or derived Harness URL, `/agent/profiles` loads under webClx authentication, native chat APIs are unavailable, control-plane APIs remain available, and `deepseek-harness.service` remains active.
