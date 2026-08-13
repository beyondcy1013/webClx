# Terminal Fork Tool Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `fork` button under the terminal soft keyboard's `利器` menu that runs `/fork` in the source terminal, extracts the newly printed resume command, opens and resumes it in a new terminal, then renames the new terminal to `<source-name>_fork`.

**Architecture:** Model the operation as one typed `fork_session` tool action because later steps depend on output produced by the first step and cannot be represented by independent static commands. Freeze the source session ID, path, and name in the tool execution context; wait on the source xterm buffer for a newly rendered resume command; then use stable session IDs for create, input, and rename operations. Merge one canonical built-in entry into saved tool configuration so existing user entries remain intact.

**Tech Stack:** Browser JavaScript, xterm.js, Node.js tests, Rust `settings_core`, Playwright browser QA.

## Global Constraints

- Preserve all existing dirty-worktree changes and edit only the files listed below.
- Do not use arbitrary workflow sleeps to guess when `/fork` output is ready.
- All command sends must use `/api/terminal/auto-typed-input` with an explicit stable `session_id`.
- A failed source command, resume extraction, terminal creation, resume send, or rename stops the workflow and surfaces the existing `利器` warning status.
- The built-in button label is exactly `fork`; the renamed terminal is exactly `<old terminal name>_fork`.

---

### Task 1: Register The Built-In Fork Tool

**Files:**
- Modify: `tests/terminal-tools-config.test.mjs`
- Modify: `crates/settings_core/src/tests.rs`
- Modify: `static/terminal-settings.js`
- Modify: `static/terminal-settings-loader.js`
- Modify: `static/app-settings-load-save.js`
- Modify: `crates/settings_core/src/lib.rs`

**Interfaces:**
- Produces: `DEFAULT_TERMINAL_TOOL_ENTRIES`, `ensureBuiltInTerminalToolEntries(entries)`, and the validated action kind `fork_session`.
- Consumes: the existing `normalizeTerminalToolEntries` tree validator and Settings API payload.

- [ ] **Step 1: Write the failing configuration tests**

  Assert that `ensureBuiltInTerminalToolEntries` preserves configured entries, appends one canonical `fork` action entry, deduplicates it by ID, and that Rust defaults/validation accept `fork_session` without a value.

- [ ] **Step 2: Run tests to verify RED**

  Run: `node --test tests/terminal-tools-config.test.mjs`

  Run: `cargo test -p settings_core terminal_tool -- --nocapture`

  Expected: failure because the built-in entry, merge helper, and action kind do not exist.

- [ ] **Step 3: Implement the minimal configuration contract**

  Add the canonical entry:

  ```js
  {
    id: "fork_session",
    root_key: "tools",
    parent_id: null,
    kind: "action",
    label: "fork",
    sort_order: 30,
    actions: [{ kind: "fork_session", value: "", seconds: 0 }],
  }
  ```

  Treat `fork_session` like `create_terminal` during normalization, merge the canonical entry into terminal and Settings state, and mirror the same default/validation rules in `settings_core`.

- [ ] **Step 4: Run tests to verify GREEN**

  Run the same Node and Rust commands and expect zero failures.

### Task 2: Execute Fork Against Stable Session Contexts

**Files:**
- Modify: `tests/terminal-tool-api-execution.test.mjs`
- Modify: `tests/terminal-tools-workflows.test.mjs`
- Modify: `static/terminal-focus-selection.js`
- Modify: `static/terminal-tool-actions.js`
- Modify: `static/terminal-sessions.js`
- Modify: `static/terminal.html`

**Interfaces:**
- Produces: `readTerminalBufferTailTextFrom(terminalInstance)`, `waitForTerminalToolResumeCommand(context, baselineCommand)`, and `forkTerminalSessionForTool(executionContext)`.
- Consumes: `extractLatestResumeCommand`, `sendTerminalAutoTypedInput`, `createSession`, `waitForTerminalToolSessionReady`, and the session rename API.

- [ ] **Step 1: Write the failing workflow test**

  Build a VM harness with source/new cached terminal contexts. Assert this exact order:

  ```text
  send /fork to source ID
  wait while the source buffer still contains only the baseline resume command
  accept a newly rendered codex resume command
  create a terminal at the frozen source path
  send the extracted command to the created ID
  rename the created ID to source-name_fork
  ```

  Also assert timeout/error behavior creates no terminal when no new resume command appears.

- [ ] **Step 2: Run the workflow test to verify RED**

  Run: `node --test tests/terminal-tool-api-execution.test.mjs tests/terminal-tools-workflows.test.mjs`

  Expected: failure because `fork_session` has no executor and buffer waiting is unavailable.

- [ ] **Step 3: Implement the fork action**

  Freeze `{ sourceSessionId, sourceSessionName, sourcePath }` when the menu entry starts. Subscribe to the source xterm render event before sending `/fork`, resolve only when `extractLatestResumeCommand` returns a non-empty command different from the baseline, dispose the listener on success/error/timeout, create the new session using `sourcePath`, send the resume command using the created ID, and rename that same ID.

- [ ] **Step 4: Run tests to verify GREEN**

  Run the same workflow tests and expect zero failures.

- [ ] **Step 5: Run JavaScript syntax and focused regression checks**

  Run:

  ```bash
  node --check static/terminal-settings.js
  node --check static/terminal-focus-selection.js
  node --check static/terminal-tool-actions.js
  node --check static/terminal-sessions.js
  node --test tests/terminal-resume-extract.test.mjs tests/terminal-tool-api-execution.test.mjs tests/terminal-tools-config.test.mjs tests/terminal-tools-workflows.test.mjs tests/terminal-session-switch-output.test.mjs
  ```

  Expected: zero syntax errors and zero test failures.

### Task 3: Verify The Live UI And Document The Contract

**Files:**
- Modify: `tests/terminal-tools.browser.py`
- Modify: `docs/codex/tasks/terminal-soft-keyboard-deploy.md`

**Interfaces:**
- Consumes: the deployed Settings API, terminal page tool menu, and fork executor.
- Produces: browser evidence that `fork` is visible and the stable-ID sequence executes with stubbed terminal output.

- [ ] **Step 1: Extend browser coverage**

  Assert `fork` appears after the two configured entries at mobile/tablet/desktop widths. In an isolated in-page harness, stub command transport, terminal contexts, create, and rename; emit a new resume hint and assert the source/new IDs and `<source-name>_fork` name.

- [ ] **Step 2: Update the reusable terminal documentation**

  Add a short section covering the `fork_session` ownership boundary, output-driven wait, stable IDs, and failure behavior.

- [ ] **Step 3: Build and deploy through the webClx queue**

  Use `webclx-compile-and-deploy` with the project deployment script because `settings_core` and static assets both changed. Wait for the source-terminal callback instead of polling or submitting duplicates.

- [ ] **Step 4: Run browser QA against the deployed service**

  Run: `python tests/terminal-tools.browser.py`

  Expected: no page errors, no critical console errors, `fork` visible at all three widths, menu remains in the viewport, and the stubbed workflow uses stable source/new session IDs.

- [ ] **Step 5: Inspect final scope and verification evidence**

  Run `git diff --check` and a path-limited `git diff` for every file in this plan. Confirm no unrelated user changes were reverted.
