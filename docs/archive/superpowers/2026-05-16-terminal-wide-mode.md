# Terminal Wide Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a mobile terminal function key that toggles a wider terminal column layout so long conversation history wraps less and can be reviewed with horizontal scrolling.

**Architecture:** Keep the feature client-side. The terminal host gets a persistent wide-mode width multiplier, `FitAddon` recalculates columns from that wider host, and the existing resize websocket sends the larger `cols` value to tmux/pty. The mobile function menu exposes a `toggle_terminal_width` action.

**Tech Stack:** xterm.js, existing terminal mobile key menu, CSS overflow layout, settings defaults in `settings_core`.

---

### Task 1: Add Wide-Mode UI Action

**Files:**
- Modify: `static/terminal.js`
- Modify: `crates/settings_core/src/lib.rs`
- Modify: `static/app.js`

- [ ] **Step 1: Add default function command**

Add `{ key: "toggle_width", label: "宽屏", action: "toggle_terminal_width", command: "", shortcut: "" }` to the browser and backend default terminal function command lists.

- [ ] **Step 2: Handle the function action**

In `runTerminalFunctionCommand()`, branch on `toggle_terminal_width` and call a new `toggleTerminalWideMode()` helper.

### Task 2: Implement Wide Terminal Layout

**Files:**
- Modify: `static/terminal.js`
- Modify: `static/styles.css`

- [ ] **Step 1: Persist mode**

Store state in `localStorage` under `webclx:terminal-wide-mode`.

- [ ] **Step 2: Apply layout**

When wide mode is enabled, add a body data attribute and set `#terminal-host` width to at least 180% of its scroll shell viewport width. When disabled, clear the inline width.

- [ ] **Step 3: Refit and resize**

After applying the width, call `fitTerminal({ force: true })`, update scroll helpers, and keep the existing websocket resize path.

### Task 3: Verify And Deploy

**Files:**
- Deploy: `/home/bin/webclx/static/terminal.js`
- Deploy: `/home/bin/webclx/static/styles.css`
- Deploy: `/home/bin/webclx/static/app.js`

- [ ] **Step 1: Run syntax and Rust checks**

Run `node --check static/terminal.js`, `node --check static/app.js`, `cargo fmt --check`, and `cargo test --workspace`.

- [ ] **Step 2: Sync static files**

Copy changed static files to `/home/bin/webclx/static/`.

- [ ] **Step 3: Browser sanity**

Verify the deployed terminal bundle contains the wide-mode action and the service remains active.
