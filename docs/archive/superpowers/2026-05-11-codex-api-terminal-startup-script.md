# Codex API Terminal Startup Script Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let Codex_API presets carry terminal startup environment variables and scripts, then create and apply a preset copy with `CODEX_RESPONSE_STYLE=caveman`.

**Architecture:** Persist startup data on `StoredApiPreset`, expose it through the existing auth API, render/edit it in the API preset UI, and inject it when creating new tmux-backed terminals. Environment variables are normalized as key/value pairs and also converted to `export` commands before the optional script.

**Tech Stack:** Rust/Axum backend, `auth_core`, tmux terminal manager, static HTML/JavaScript frontend, Node-based static tests, Cargo tests.

---

### Task 1: Persist API Preset Startup Fields

**Files:**
- Modify: `crates/auth_core/src/models.rs`
- Modify: `crates/auth_core/src/lib.rs`
- Modify: `crates/auth_core/src/storage.rs`
- Test: `crates/auth_core/src/tests.rs`

- [ ] Add failing tests that deserialize/save API presets with `terminal_env` and `terminal_startup_script`, sanitize invalid env entries, and include fields in `api_preset_summary`.
- [ ] Run `cargo test -p auth_core terminal` or the closest targeted auth_core tests and verify the tests fail because fields are missing.
- [ ] Add `PresetTerminalEnvVar`, `terminal_env`, and `terminal_startup_script` fields to stored/request/summary models.
- [ ] Add sanitizers and wire them into API preset normalization/save/update paths.
- [ ] Re-run the targeted Cargo tests and verify they pass.

### Task 2: Inject Current API Preset Startup Commands

**Files:**
- Modify: `src/auth.rs`
- Modify: `src/terminal.rs`
- Modify: `src/terminal/manager.rs`
- Modify: `src/terminal/tmux.rs`
- Test: `src/terminal/tests.rs` or `crates/terminal_core/src/lib.rs`

- [ ] Add a failing test for building startup script text from env entries and custom script.
- [ ] Run the targeted test and verify it fails because the helper does not exist.
- [ ] Add a helper that shell-quotes env values and builds newline-separated `export KEY='value'` commands followed by the custom script.
- [ ] Resolve the currently active API preset before creating a new terminal, pass its env to tmux environment, and send its startup script into new tmux sessions before attach.
- [ ] Re-run the targeted test and relevant terminal/auth tests.

### Task 3: Add API Preset UI Controls

**Files:**
- Modify: `static/index.html`
- Modify: `static/app.js`
- Test: `tests/preset-table-shared-renderer.test.mjs`

- [ ] Add failing static assertions that API preset rows/header/form include terminal startup fields.
- [ ] Run `node tests/preset-table-shared-renderer.test.mjs` and verify the new assertions fail.
- [ ] Add a foldout section with env textarea and script textarea to the API preset form.
- [ ] Include fields in edit snapshots, save payloads, reset, edit, and table rendering.
- [ ] Re-run the Node static tests.

### Task 4: Deploy Static Files And Add Preset Copy

**Files:**
- Modify runtime data: `webclx-api-presets.json`
- Sync static deploy files: `/home/bin/webclx/static/index.html`, `/home/bin/webclx/static/app.js`

- [ ] Copy static files to the deploy directory after frontend changes.
- [ ] Copy the currently active API preset in `webclx-api-presets.json`, give it a distinct name, add `terminal_env: [{ "key": "CODEX_RESPONSE_STYLE", "value": "caveman" }]`, and preserve the rest of the current preset fields.
- [ ] Apply that new preset by writing the matching `auth.json` and `config.toml`, using existing app behavior or matching backend logic.
- [ ] Run Cargo and Node verification commands and report any commands that could not run.
