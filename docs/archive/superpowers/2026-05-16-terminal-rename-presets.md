# Terminal Rename Presets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add configurable terminal rename preset buttons that append `_预设名` to the current terminal name.

**Architecture:** Persist `terminal_rename_presets` in settings_core and send it through `/api/settings`. Render the setting in the System tab, then consume it in the terminal rename editor.

**Tech Stack:** Rust settings_core, static HTML/CSS/JS, Node static tests, Cargo tests.

---

### Task 1: Settings Contract

**Files:**
- Modify: `crates/settings_core/src/lib.rs`
- Modify: `crates/settings_core/src/storage.rs`
- Modify: `crates/settings_core/src/manager.rs`
- Modify: `crates/settings_core/src/api.rs`
- Test: `crates/settings_core/src/tests.rs`

- [ ] Write tests for default and sanitized `terminal_rename_presets`.
- [ ] Run `cargo test -p settings_core terminal_rename` and confirm the tests fail.
- [ ] Add the setting field, default values, sanitizer, manager getter/update path, storage persistence, and API request/response fields.
- [ ] Re-run `cargo test -p settings_core terminal_rename`.

### Task 2: Frontend Settings And Rename UI

**Files:**
- Modify: `static/index.html`
- Modify: `static/app.js`
- Modify: `static/terminal.html`
- Modify: `static/terminal.js`
- Modify: `static/styles.css`
- Test: `tests/terminal-rename-presets.test.mjs`

- [ ] Write a Node static test requiring settings UI wiring and terminal rename preset behavior.
- [ ] Run `node tests/terminal-rename-presets.test.mjs` and confirm it fails.
- [ ] Add the System tab textarea and save/load normalization in `app.js`.
- [ ] Add the rename preset button container and click handling in `terminal.js`.
- [ ] Add compact CSS for rename preset buttons.
- [ ] Re-run `node tests/terminal-rename-presets.test.mjs`.

### Task 3: Verification And Deployment Sync

**Files:**
- Verify: settings_core tests and frontend static tests.
- Sync: `/home/bin/webclx/static/index.html`, `/home/bin/webclx/static/app.js`, `/home/bin/webclx/static/terminal.html`, `/home/bin/webclx/static/terminal.js`, `/home/bin/webclx/static/styles.css`

- [ ] Run targeted Rust and Node tests.
- [ ] Sync changed static files to `/home/bin/webclx/static/`.
- [ ] Verify deployed static files contain the rename preset UI and JS behavior.
