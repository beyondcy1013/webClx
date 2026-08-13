# Soft Keyboard Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a configurable soft-keyboard settings tab, keep the existing slash-command dropdown configurable, and add a separate "功能命令" dropdown with explicit system-keyboard controls.

**Architecture:** Extend the existing settings JSON/API model with `terminal_function_commands`, reuse current table editor patterns for settings UI, and render the terminal dropdown from settings. IME suppression stays in `terminal-ime-policy.js` so it is unit-testable outside the browser.

**Tech Stack:** Rust settings core, vanilla HTML/CSS/JS, Node-based static tests.

---

### Task 1: Settings Model

**Files:**
- Modify: `crates/settings_core/src/lib.rs`
- Modify: `crates/settings_core/src/api.rs`
- Modify: `crates/settings_core/src/manager.rs`
- Modify: `crates/settings_core/src/storage.rs`
- Test: `crates/settings_core/src/tests.rs`

- [ ] Add `TerminalFunctionCommand` with `key`, `label`, `action`, and `command`.
- [ ] Add `terminal_slash_commands` defaults for slash commands and `terminal_function_commands` defaults for show/disable system keyboard.
- [ ] Add sanitization that deduplicates by key, drops invalid entries, and falls back to defaults when missing.
- [ ] Include function commands in load, save, response, and manager update paths.
- [ ] Add Rust tests for defaults and sanitization.

### Task 2: Settings UI

**Files:**
- Modify: `static/index.html`
- Modify: `static/app.js`
- Modify: `static/styles.css`

- [ ] Add the "软键盘" settings tab.
- [ ] Move existing terminal quick command and soft-keyboard fields into the new tab.
- [ ] Add editable slash-command and function-command settings.
- [ ] Save and reset `terminal_slash_commands` and `terminal_function_commands`.

### Task 3: Terminal Runtime

**Files:**
- Modify: `static/terminal.html`
- Modify: `static/terminal.js`
- Modify: `static/terminal-ime-policy.js`
- Test: `tests/terminal-ime-policy.test.mjs`

- [ ] Render slash command options from settings.
- [ ] Render separate function command options from settings.
- [ ] Implement `show_system_keyboard`, `disable_system_keyboard`, and slash command dispatch.
- [ ] Add a 60-second suppression window for disabled system keyboard mode.
- [ ] Add JS tests for suppression and explicit show override.

### Task 4: Verification And Deployment

**Files:**
- Modify: `/home/bin/webclx/static/index.html`
- Modify: `/home/bin/webclx/static/app.js`
- Modify: `/home/bin/webclx/static/styles.css`
- Modify: `/home/bin/webclx/static/terminal.html`
- Modify: `/home/bin/webclx/static/terminal.js`
- Modify: `/home/bin/webclx/static/terminal-ime-policy.js`

- [ ] Run targeted Rust settings tests.
- [ ] Run JS static tests.
- [ ] Syntax-check changed JS.
- [ ] Sync changed static files to `/home/bin/webclx/static`.
- [ ] Verify `/terminal` references updated cache-busted assets.
