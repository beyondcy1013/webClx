# Codex API Per-Preset Local Proxy Option Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Codex_API local proxy routing owned by each saved preset, with compatibility providers recommending that preset option and warning when it is unchecked.

**Architecture:** Backend apply paths use the saved preset-local proxy decision and persist only the active preset ID for routing. Frontend removes the Codex_API global toggle and makes the preset checkbox auto-check only as an editable recommendation.

**Tech Stack:** Rust `auth_core` and axum handlers, static HTML/JS tests.

---

### Task 1: Backend Semantics

**Files:**
- Modify: `crates/auth_core/src/lib.rs`
- Modify: `crates/auth_core/src/tests.rs`
- Modify: `src/auth/apply.rs`
- Modify: `src/auth.rs`

- [x] Write failing Rust tests for saved preset local proxy option and active summary without global bool.
- [ ] Run `cargo test -p auth_core api_preset`.
- [x] Update helpers so provider keywords only recommend local proxy, and apply paths use the saved preset option instead of `codex_api_proxy_enabled`.
- [x] Re-run `cargo test -p auth_core api_preset`.

### Task 2: Frontend Contract

**Files:**
- Modify: `static/index.html`
- Modify: `static/app.js`
- Modify: `tests/api-claude-preset-test-actions.test.mjs`
- Modify: `tests/preset-table-shared-renderer.test.mjs`

- [ ] Write failing static tests that Codex_API no longer has the global toggle and still has the preset-level checkbox.
- [ ] Run the changed Node tests.
- [ ] Remove Codex_API global toggle UI/event wiring while leaving Claude global toggle unchanged.
- [x] Update preset editor checkbox rendering/saving, provider recommendation copy, unchecked warnings, and edit-save overwrite prompt.
- [x] Re-run the changed Node tests.

### Task 3: Deploy

**Files:**
- Build: release binary.
- Sync: `/home/bin/webclx/webClx`, `/home/bin/webclx/static/index.html`, `/home/bin/webclx/static/app.js`

- [ ] Run targeted Rust and Node tests.
- [ ] Build release and install binary.
- [ ] Sync changed static files.
- [ ] Restart `webclx.service`.
- [ ] Verify service status and API/static output.
