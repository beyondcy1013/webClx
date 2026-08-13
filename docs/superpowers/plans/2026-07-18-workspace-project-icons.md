# Workspace Project Icons Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add separately configurable project-relative icons to the workspace browser and active-terminal selectors.

**Architecture:** Persist two relative icon paths in the existing settings schema. Serve validated image files through a workspace-scoped endpoint; exact lookup is used by workspace rows and nearest-ancestor lookup is used by terminals started in project subdirectories. Enhance existing native terminal selects with an accessible custom image menu while retaining the native select as the source of truth.

**Tech Stack:** Rust/Axum, Serde settings storage, plain JavaScript, CSS, Node test runner, Cargo tests.

## Global Constraints

- Workspace browser and terminal icons are separate settings.
- Defaults are `icon.ico` for workspace browsing and `static/favicon.svg` for terminals.
- Icon paths are relative to a project and cannot escape it.
- Existing terminal selection event handlers continue to receive native `change` events.
- Existing unrelated dirty worktree changes must be preserved; do not create checkpoint commits.

---

### Task 1: Settings schema

**Files:**
- Modify: `crates/settings_core/src/lib.rs`
- Modify: `crates/settings_core/src/manager.rs`
- Modify: `crates/settings_core/src/storage.rs`
- Modify: `crates/settings_core/src/api.rs`
- Test: `crates/settings_core/src/tests.rs`

**Interfaces:**
- Produces: `workspace_browser_icon_path: String` and `terminal_workspace_icon_path: String` in settings responses and saves.

- [ ] Add tests asserting defaults, normalization of project-relative paths, and rejection/fallback for absolute or parent-traversing paths.
- [ ] Run `cargo test -p settings_core workspace_icon -- --nocapture` and confirm RED for missing fields/helpers.
- [ ] Add fields to load, save, persistence, remote-tab merge, and manager state using the existing settings pipeline.
- [ ] Rerun the focused settings tests and confirm GREEN.

### Task 2: Workspace-scoped image endpoint

**Files:**
- Modify: `src/filesystem.rs`
- Modify: `src/routes/workspace.rs`

**Interfaces:**
- Consumes: query fields `path`, `icon_path`, and `search`.
- Produces: `GET /api/workspace-icon` with a validated image response or 404/400.

- [ ] Add resolver tests for exact lookup, nearest-ancestor lookup, traversal rejection, unsupported extensions, and symlink escape.
- [ ] Run the focused filesystem tests and confirm RED.
- [ ] Implement bounded image reads, MIME detection, project containment, and nearest-ancestor search.
- [ ] Rerun focused filesystem tests and confirm GREEN.

### Task 3: Appearance settings controls

**Files:**
- Modify: `static/index.html`
- Modify: `static/app.js`
- Modify: `static/app-settings-load-save.js`
- Modify: `static/app-settings-event-bindings.js`
- Modify: `static/styles-settings.css`
- Test: `tests/settings-categories.test.mjs`

**Interfaces:**
- Consumes/produces the two settings fields through `/api/settings`.

- [ ] Add source assertions for two independent inputs in the Appearance panel and save/reset wiring.
- [ ] Run the settings test and confirm RED.
- [ ] Add labeled relative-path inputs with defaults and concise help text.
- [ ] Load, normalize, save, and reset both values through existing settings state.
- [ ] Rerun the settings test and confirm GREEN.

### Task 4: Workspace and terminal rendering

**Files:**
- Create: `static/workspace-project-icons.js`
- Modify: `static/index.html`
- Modify: `static/terminal.html`
- Modify: `static/app-workspace-browser.js`
- Modify: `static/app-session-controls.js`
- Modify: `static/terminal-session-render.js`
- Modify: `static/terminal-settings-loader.js`
- Modify: `static/styles-base.css`
- Modify: `static/styles-responsive.css`
- Test: `tests/workspace-project-icons.test.mjs`

**Interfaces:**
- Produces: `workspaceProjectIconUrl`, `createWorkspaceProjectIcon`, and `enhanceWorkspaceIconSelect` browser helpers.
- Consumes: option `data-workspace-path` attributes and configured relative icon paths.

- [ ] Add unit/source tests for URL construction, path normalization, native-select synchronization, and table column markup.
- [ ] Run the new Node test and confirm RED.
- [ ] Implement reusable image creation and an accessible combobox enhancement that keeps the native select authoritative.
- [ ] Add the workspace icon column using exact lookup.
- [ ] Add project path metadata to terminal options and enhance activity selectors using nearest lookup.
- [ ] Rerun the Node test and confirm GREEN.

### Task 5: Verification and deployment

**Files:**
- Modify only if verification exposes a defect.

- [ ] Run focused Cargo and Node tests.
- [ ] Run JavaScript syntax checks for modified scripts.
- [ ] Run the relevant full settings/filesystem test targets.
- [ ] Review `git diff` for only task-scoped edits and preservation of unrelated changes.
- [ ] Queue the webClx compile/deploy request through the documented service API and wait for its callback.
