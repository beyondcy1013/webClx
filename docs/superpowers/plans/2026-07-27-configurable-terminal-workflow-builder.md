# Configurable Terminal Workflow Builder Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the existing terminal `利器` action tree into a user-editable `工作流` builder so Codex workflows such as `代理设置` can be created, nested, changed, and launched through configuration without editing or redeploying frontend source.

**Architecture:** Keep the existing `terminal_tool_entries` Settings API field and the persisted `root_key: "tools"` value for backward compatibility, while renaming its product-facing UI to `工作流`. Extend the typed action model with one structured `codex_launch` action, resolve presets through a shared deterministic resolver, and execute launches through the existing `executeSpecifiedPreset` and `launchTerminalSpecifiedPreset` boundary. Migrate the hard-coded proxy shortcut into a built-in, editable workflow entry and continue using the existing parent-ID tree for arbitrary menu depth.

**Tech Stack:** Rust (`settings_core` and Settings API), browser JavaScript, HTML/CSS, Node.js tests, Playwright browser QA, webClx compile/deploy queue.

## Global Constraints

- Preserve existing user changes in the dirty worktree; do not revert or rewrite unrelated files.
- Keep the serialized Settings field named `terminal_tool_entries` and keep persisted root key `tools` in this version.
- Existing saved folders, actions, IDs, sort order, and legacy action kinds must remain valid.
- User-facing terminology for this feature is `工作流`; compatibility storage names may remain `terminal_tool_*` internally.
- Nested folders must continue to support more than one level and reject missing parents, non-folder parents, self-parenting, and cycles.
- A workflow executes actions serially and stops on the first failure with a visible error status.
- `codex_launch` must use `executeSpecifiedPreset` with `launchTerminalSpecifiedPreset`; it must not duplicate session, preset-apply, or shell-quoting logic.
- A Codex launch must pass both `cwd` and `projectPath` explicitly.
- Preset matching must reject zero matches and ambiguous matches; it must never silently select the first substring match.
- The migrated `代理设置` workflow uses `cwd=/home/system`, `project_path=/home/system`, preset selector `miniMax`, unique substring matching, and task `$mihomo-proxy-ops 请检查当前代理配置，并根据当前环境完成代理设置。`.
- Configuration edits take effect after saving Settings and refreshing/reloading terminal state; they must not require compilation or deployment.
- Keep limits at 200 workflow entries and 20 actions per executable workflow unless a measured product requirement changes them.
- Follow repository cache-version conventions for every changed static script.
- Compile and deploy only through `webclx-compile-and-deploy` and the running webClx service API.

---

## File And Ownership Map

- `static/terminal-settings.js`: shared browser schema, defaults, normalization, compatibility migration, and action metadata.
- `crates/settings_core/src/lib.rs`: authoritative server-side workflow schema validation and built-in defaults.
- `crates/settings_core/src/tests.rs`: Rust compatibility, validation, and migration tests.
- `static/app-terminal-tools-settings.js`: workflow tree table and structured action editor.
- `static/app-settings-categories.js`, `static/index.html`, `static/styles-settings.css`, `static/styles-responsive.css`: dedicated 工作流 settings category and responsive builder UI.
- `static/terminal-tool-actions.js`: workflow tree rendering and action execution.
- `static/specified-preset-actions.js`: shared preset selector resolution used by dedicated and configured launches.
- `static/terminal.js`, `static/terminal.html`: soft-key binding, removal of the hard-coded proxy shortcut, and asset versioning.
- `static/app-settings-load-save.js`: Settings payload load/save and built-in entry merge.
- `tests/terminal-tools-config.test.mjs`: browser-side schema and migration contract.
- `tests/terminal-tools-workflows.test.mjs`: static integration and menu contract.
- `tests/terminal-tool-api-execution.test.mjs`: action execution order and launch payload contract.
- `tests/specified-preset-actions.test.mjs`: deterministic preset resolution contract.
- `tests/settings-categories.test.mjs`: settings category naming and navigation contract.
- `tests/terminal-tools.browser.py`: deployed multi-level builder/menu browser QA without creating real sessions.
- `docs/codex/tasks/terminal-soft-keyboard-deploy.md`: durable ownership and configuration documentation.

---

### Task 1: Extend The Workflow Configuration Contract

**Files:**
- Modify: `tests/terminal-tools-config.test.mjs`
- Modify: `crates/settings_core/src/tests.rs`
- Modify: `static/terminal-settings.js`
- Modify: `crates/settings_core/src/lib.rs`
- Modify: `static/app-settings-load-save.js`

**Interfaces:**
- Produces: normalized action `{ kind: "codex_launch", value, preset_selector, preset_match, cwd, project_path, terminal_name, session_action }`.
- Produces: built-in entry ID `proxy_settings_workflow` beneath the compatible `tools` root.
- Consumes: existing `normalizeTerminalToolEntries(entries)` and `ensureBuiltInTerminalToolEntries(entries)` contracts.

- [ ] **Step 1: Add failing browser schema tests**

  Extend `tests/terminal-tools-config.test.mjs` with a configured launch:

  ```js
  {
    id: "proxy_settings_workflow",
    root_key: "tools",
    parent_id: null,
    kind: "action",
    label: "代理设置",
    sort_order: 20,
    actions: [{
      kind: "codex_launch",
      value: "$mihomo-proxy-ops 请检查当前代理配置，并根据当前环境完成代理设置。",
      preset_selector: "miniMax",
      preset_match: "unique_contains",
      cwd: "/home/system",
      project_path: "/home/system",
      terminal_name: "代理设置",
      session_action: "new",
    }],
  }
  ```

  Assert trimming without case destruction, preservation through normalization, rejection of empty task/selector/path/name, rejection of unsupported `preset_match` and `session_action`, and deduplication of the built-in entry by stable ID.

- [ ] **Step 2: Add failing Rust validation tests**

  Add matching `TerminalToolAction` fixtures in `crates/settings_core/src/tests.rs`. Assert that valid `codex_launch` survives save/load, legacy actions deserialize with empty new fields, invalid enums fail with HTTP bad-request semantics, and the default proxy workflow has the exact values in Global Constraints.

- [ ] **Step 3: Run the schema tests to verify RED**

  Run:

  ```bash
  node --test tests/terminal-tools-config.test.mjs
  cargo test -p settings_core terminal_tool -- --nocapture
  ```

  Expected: failures because `codex_launch` and its structured fields are not accepted.

- [ ] **Step 4: Implement the browser schema and compatibility default**

  Add `codex_launch` to `TERMINAL_TOOL_ACTION_TYPES` with parameter type `codex_launch`. Normalize the fields exactly as follows:

  ```js
  {
    kind: "codex_launch",
    value: normalizeTerminalToolActionValue(rawAction.value, 4096),
    preset_selector: normalizeTerminalQuickText(rawAction.preset_selector, 128),
    preset_match: ["id", "exact_name", "unique_contains"].includes(rawAction.preset_match)
      ? rawAction.preset_match
      : "",
    cwd: normalizeAbsoluteWorkflowPath(rawAction.cwd),
    project_path: normalizeAbsoluteWorkflowPath(rawAction.project_path),
    terminal_name: normalizeTerminalQuickText(rawAction.terminal_name, 64),
    session_action: rawAction.session_action === "new" ? "new" : "",
  }
  ```

  Reject the whole configuration when a required field normalizes to empty. Add the canonical `proxy_settings_workflow` entry through `ensureBuiltInTerminalToolEntries`; preserve a saved entry with that ID so users can edit its label, location, sort order, and launch fields.

- [ ] **Step 5: Mirror the schema in Rust**

  Add serde-defaulted strings to `TerminalToolAction`:

  ```rust
  preset_selector: String,
  preset_match: String,
  cwd: String,
  project_path: String,
  terminal_name: String,
  session_action: String,
  ```

  For legacy actions, clear fields they do not own. For `codex_launch`, require the exact enums above, absolute `cwd` and `project_path`, and bounded non-control text. Add the same canonical built-in proxy entry to `default_terminal_tool_entries()` and preserve user-saved entries during merge.

- [ ] **Step 6: Run the schema tests to verify GREEN**

  Run the commands from Step 3.

  Expected: all targeted Node and Rust tests pass; existing legacy fixtures require no serialized-field changes because empty new fields use serde defaults.

- [ ] **Step 7: Commit the configuration contract**

  ```bash
  git add static/terminal-settings.js static/app-settings-load-save.js crates/settings_core/src/lib.rs crates/settings_core/src/tests.rs tests/terminal-tools-config.test.mjs
  git commit -m "feat: add configurable Codex workflow launch action"
  ```

### Task 2: Build The Structured Workflow Editor TAB

**Files:**
- Modify: `tests/settings-categories.test.mjs`
- Modify: `tests/terminal-tools-workflows.test.mjs`
- Modify: `static/app-settings-categories.js`
- Modify: `static/index.html`
- Modify: `static/app-terminal-tools-settings.js`
- Modify: `static/styles-settings.css`
- Modify: `static/styles-responsive.css`

**Interfaces:**
- Consumes: `TERMINAL_TOOL_ACTION_TYPES` and normalized `codex_launch` from Task 1.
- Produces: a dedicated `工作流` category and editor controls whose values map one-to-one to the persisted action fields.

- [ ] **Step 1: Write failing category and editor tests**

  Assert the settings registry exposes category/tab key `tools` with label `工作流`, the panel heading is `工作流搭建`, and the action editor contains controls for task, preset matching, preset selector, working directory, project path, terminal name, and session mode.

- [ ] **Step 2: Run tests to verify RED**

  Run:

  ```bash
  node --test tests/settings-categories.test.mjs tests/terminal-tools-workflows.test.mjs
  ```

  Expected: failures because the current product label is `利器` and the editor only renders one generic parameter control.

- [ ] **Step 3: Rename the product-facing category**

  Keep category key and panel key `tools`, but change visible copy to:

  ```text
  Category: 工作流
  Tab: 工作流
  Section label: 动作树
  Heading: 工作流搭建
  Buttons: 新建目录 / 新建工作流
  ```

  Keep internal DOM IDs unchanged to avoid unnecessary migration risk.

- [ ] **Step 4: Render structured `codex_launch` controls**

  In `createTerminalToolActionParameter`, render a field group bound directly to the action draft:

  ```text
  预设匹配: ID / 精确名称 / 唯一包含
  预设: preset_selector
  工作目录: cwd
  项目路径: project_path
  终端名称: terminal_name
  会话: 新建
  初始任务: value
  ```

  Populate known preset names and IDs from `/api/auth/api-presets` as suggestions, but retain typed values that are not in the current list. Do not silently rewrite a name selector to an ID selector.

- [ ] **Step 5: Improve tree management ergonomics**

  Add `复制` beside `编辑` and `删除`. Copying an entry generates a new ID, appends ` 副本` to the label within the 64-character limit, retains the same parent and action configuration, and increments `sort_order` by one up to 10000. Continue using the existing parent selector to support arbitrary depth.

- [ ] **Step 6: Add responsive layout rules**

  Use a two-column grid for structured launch fields on desktop and one column below the existing settings mobile breakpoint. Ensure long paths wrap or scroll inside their input and that editor controls do not overlap at 360px width.

- [ ] **Step 7: Run tests to verify GREEN**

  Run the commands from Step 2 plus:

  ```bash
  node --check static/app-terminal-tools-settings.js
  ```

  Expected: all tests pass and JavaScript syntax is valid.

- [ ] **Step 8: Commit the builder UI**

  ```bash
  git add static/app-settings-categories.js static/index.html static/app-terminal-tools-settings.js static/styles-settings.css static/styles-responsive.css tests/settings-categories.test.mjs tests/terminal-tools-workflows.test.mjs
  git commit -m "feat: turn terminal tools settings into workflow builder"
  ```

### Task 3: Resolve Presets And Execute Configured Launches

**Files:**
- Modify: `tests/specified-preset-actions.test.mjs`
- Modify: `tests/terminal-tool-api-execution.test.mjs`
- Modify: `static/specified-preset-actions.js`
- Modify: `static/terminal-tool-actions.js`

**Interfaces:**
- Produces: `resolveSpecifiedPreset(presets, { selector, match }) -> preset` or throws a user-readable error.
- Consumes: configured `codex_launch` fields from Task 1 and `executeSpecifiedPreset`/`launchTerminalSpecifiedPreset`.

- [ ] **Step 1: Write failing resolver tests**

  Cover these exact cases:

  ```text
  id: exact preset ID only
  exact_name: case-insensitive exact trimmed name
  unique_contains: prefer one exact-name match; otherwise require exactly one case-insensitive substring match
  no match: throw with selector in message
  two substring matches: throw an ambiguity error and include both display names
  ```

- [ ] **Step 2: Write failing execution tests**

  Execute one `codex_launch` in the VM harness and assert this payload:

  ```js
  {
    action: "launch",
    agent: "codex",
    presetId: "api-1776989731419",
    cwd: "/home/system",
    projectPath: "/home/system",
    sessionAction: "new",
    task: "$mihomo-proxy-ops 请检查当前代理配置，并根据当前环境完成代理设置。",
    terminalName: "代理设置",
    quickStart: false,
    launchTerminal: launchTerminalSpecifiedPreset,
  }
  ```

  Assert resolution or launch failure stops subsequent workflow actions and restores the running/disabled UI state.

- [ ] **Step 3: Run tests to verify RED**

  Run:

  ```bash
  node --test tests/specified-preset-actions.test.mjs tests/terminal-tool-api-execution.test.mjs
  ```

  Expected: failures because there is no shared resolver or configured launch executor.

- [ ] **Step 4: Implement the deterministic resolver**

  Add `resolveSpecifiedPreset` to the shared specified-preset module. Compare normalized IDs only for `id`; compare trimmed lower-case names for `exact_name`; use exact-name preference followed by unique substring matching for `unique_contains`. Throw before applying a preset or creating a terminal when resolution is invalid.

- [ ] **Step 5: Implement the `codex_launch` executor**

  In `executeTerminalToolAction`, load Codex presets from `specifiedPresetListEndpoint("codex")`, resolve the configured selector, and call `executeSpecifiedPreset` with the exact payload above using action fields rather than constants. Store the launched session ID in `executionContext.sessionId` when returned so later rename/send actions target the launched terminal.

- [ ] **Step 6: Run tests to verify GREEN**

  Run the commands from Step 3 plus:

  ```bash
  node --check static/specified-preset-actions.js
  node --check static/terminal-tool-actions.js
  ```

  Expected: all resolver, payload, serial-order, error, and syntax checks pass.

- [ ] **Step 7: Commit the runtime**

  ```bash
  git add static/specified-preset-actions.js static/terminal-tool-actions.js tests/specified-preset-actions.test.mjs tests/terminal-tool-api-execution.test.mjs
  git commit -m "feat: execute configured terminal workflows"
  ```

### Task 4: Migrate The Proxy Shortcut Into The Workflow Tree

**Files:**
- Modify: `tests/terminal-tools-workflows.test.mjs`
- Modify: `static/terminal.html`
- Modify: `static/terminal.js`
- Modify: `static/terminal-tool-actions.js`

**Interfaces:**
- Consumes: built-in `proxy_settings_workflow` from Task 1 and configured executor from Task 3.
- Produces: one `工作流` soft key whose menu contains `代理设置`; no hard-coded proxy launch path remains.

- [ ] **Step 1: Write the failing migration test**

  Assert `工作流` remains immediately after `后退`, `terminal-proxy-settings-button` is absent, `launchProxySettingsWorkflow` and `PROXY_SETTINGS_WORKFLOW_*` constants are absent, and the configured menu still renders `proxy_settings_workflow`.

- [ ] **Step 2: Run the migration test to verify RED**

  Run:

  ```bash
  node --test tests/terminal-tools-workflows.test.mjs
  ```

  Expected: failure because the dedicated button and hard-coded launcher still exist.

- [ ] **Step 3: Remove the dedicated shortcut implementation**

  Remove `terminal-proxy-settings-button`, its DOM lookup/listener, the proxy constants, and `launchProxySettingsWorkflow`. Keep `terminal-workflows-button` opening the compatible `tools` root. Do not change existing saved tree entries.

- [ ] **Step 4: Bump static asset versions**

  Update query versions in `static/terminal.html` and `static/index.html` for each changed script or stylesheet so deployed browsers load the workflow schema, editor, and runtime together.

- [ ] **Step 5: Run focused regression tests**

  Run:

  ```bash
  node --check static/terminal.js
  node --check static/terminal-tool-actions.js
  node --test tests/terminal-tools-config.test.mjs tests/terminal-tools-workflows.test.mjs tests/terminal-tool-api-execution.test.mjs tests/terminal-specified-task.test.mjs tests/specified-preset-actions.test.mjs tests/settings-categories.test.mjs
  ```

  Expected: zero failures and no references to the removed dedicated button.

- [ ] **Step 6: Commit the migration**

  ```bash
  git add static/terminal.html static/terminal.js static/terminal-tool-actions.js static/index.html tests/terminal-tools-workflows.test.mjs
  git commit -m "refactor: migrate proxy shortcut to configured workflow"
  ```

### Task 5: Add Import, Export, And Non-Mutating Browser QA

**Files:**
- Modify: `tests/terminal-tools-workflows.test.mjs`
- Modify: `tests/terminal-tools.browser.py`
- Modify: `static/index.html`
- Modify: `static/app-terminal-tools-settings.js`
- Modify: `docs/codex/tasks/terminal-soft-keyboard-deploy.md`

**Interfaces:**
- Produces: versioned workflow JSON export `{ version: 1, terminal_tool_entries: [...] }` and validated replace-on-import behavior.
- Consumes: `normalizeTerminalToolEntries`, Settings save flow, and deployed workflow menu.

- [ ] **Step 1: Write failing import/export tests**

  Assert export contains only version and workflow entries, import rejects unknown versions or invalid trees without changing state, valid import previews entry counts, and replacement occurs only after explicit confirmation. Secrets from API presets must never be exported because workflow entries contain selectors, not preset credentials.

- [ ] **Step 2: Implement import/export controls**

  Add `导入` and `导出` icon/text controls in the 工作流 panel toolbar. Export formatted UTF-8 JSON. Parse imports with `JSON.parse`, require `version === 1`, normalize before confirmation, replace `state.terminalToolEntries` only after validation and confirmation, then require the existing global `保存设置` action to persist.

- [ ] **Step 3: Extend browser QA without mutating sessions**

  In `tests/terminal-tools.browser.py`:

  - Open the 工作流 settings TAB at desktop and 360px widths.
  - Verify a two-level folder tree can be edited and all structured fields fit without overlap.
  - Open the terminal 工作流 menu and navigate at least two directory levels.
  - Intercept preset listing and preset application/launch calls.
  - Return a controlled launch failure before session creation.
  - Assert `MiniMax3` was resolved uniquely and request data contains both `cwd=/home/system` and `project_path=/home/system`.
  - Assert no terminal create/delete endpoint was called.

- [ ] **Step 4: Document the durable contract**

  Add a concise `Configurable Workflows` section to `docs/codex/tasks/terminal-soft-keyboard-deploy.md` covering the compatibility root key, schema fields, deterministic preset resolution, multi-level parent tree, serial failure behavior, import/export version, and the rule that dedicated soft keys are reserved for fixed product semantics.

- [ ] **Step 5: Run static and browser verification**

  Run:

  ```bash
  node --test tests/terminal-tools-config.test.mjs tests/terminal-tools-workflows.test.mjs tests/terminal-tool-api-execution.test.mjs tests/terminal-specified-task.test.mjs tests/specified-preset-actions.test.mjs tests/settings-categories.test.mjs
  python tests/terminal-tools.browser.py
  git diff --check
  ```

  Expected: all tests pass; browser QA reports no overlap, correct nesting, correct intercepted launch payload, and zero real session mutations.

- [ ] **Step 6: Commit management and documentation**

  ```bash
  git add static/index.html static/app-terminal-tools-settings.js tests/terminal-tools-workflows.test.mjs tests/terminal-tools.browser.py docs/codex/tasks/terminal-soft-keyboard-deploy.md
  git commit -m "feat: add workflow configuration portability"
  ```

### Task 6: Deploy And Verify The Live Contract

**Files:**
- Verify: all files modified by Tasks 1-5
- Verify deployed copies under: `/home/bin/webclx/static/`

**Interfaces:**
- Consumes: passing source tests and project-owned `scripts/rebuild-and-deploy.sh`.
- Produces: deployed Settings schema, workflow builder, and terminal menu with source/deployed asset parity.

- [ ] **Step 1: Run the complete focused verification set**

  Run:

  ```bash
  node --check static/terminal-settings.js
  node --check static/app-terminal-tools-settings.js
  node --check static/specified-preset-actions.js
  node --check static/terminal-tool-actions.js
  node --check static/terminal.js
  node --test tests/terminal-tools-config.test.mjs tests/terminal-tools-workflows.test.mjs tests/terminal-tool-api-execution.test.mjs tests/terminal-specified-task.test.mjs tests/specified-preset-actions.test.mjs tests/settings-categories.test.mjs
  cargo test -p settings_core terminal_tool -- --nocapture
  git diff --check
  ```

  Expected: zero failures and no whitespace errors.

- [ ] **Step 2: Queue deployment through webClx**

  Read and follow `webclx-compile-and-deploy`, then request:

  ```bash
  bash /home/root/.codex/skills/webclx-compile-and-deploy/scripts/request-webclx-deploy-api.sh \
    --install-cmd bash \
    --install-arg scripts/rebuild-and-deploy.sh \
    --audit-path /home/bin/webclx \
    --note 'deploy configurable terminal workflow builder'
  ```

  Expected: the API accepts or queues one request. If queued, wait for its callback; do not poll logs or submit a duplicate.

- [ ] **Step 3: Verify deployed asset parity**

  Compare SHA-256 hashes for each changed static asset between `static/` and `/home/bin/webclx/static/`. Expected: every pair matches.

- [ ] **Step 4: Run browser QA against the deployed service**

  Run `python tests/terminal-tools.browser.py` against the configured local URL.

  Expected: 工作流 settings and nested terminal menu render correctly on desktop/mobile, proxy settings resolves the intended preset in the intercepted request, and no real terminal is created.

- [ ] **Step 5: Inspect final scope**

  Run path-limited `git diff --stat` and `git diff` for the files named in this plan. Confirm no unrelated user changes were reverted and report any pre-existing unrelated test failure separately with its exact test name.

---

## Acceptance Criteria

- A user can create directories and executable workflows from the dedicated 工作流 settings TAB.
- A user can nest directories to at least two levels; the schema continues to support arbitrary acyclic depth within the 200-entry limit.
- A user can configure a Codex launch with preset strategy, working directory, project path, terminal name, and initial `$skill-name` task.
- Saving Settings is sufficient for the workflow to appear under the terminal 工作流 soft key; no source edit, compile, or deploy is needed for later workflow changes.
- `代理设置` is represented by persisted/default configuration and no longer has a dedicated hard-coded launch function or soft key.
- The proxy workflow resolves one compatible miniMax preset, launches from `/home/system`, applies project configuration for `/home/system`, and invokes `$mihomo-proxy-ops`.
- Missing or ambiguous presets and failed launches are visible, stop subsequent actions, and do not create partial sessions before resolution succeeds.
- Existing saved 利器 configuration remains readable and appears under the renamed 工作流 UI.
- Workflow export contains no API credentials; invalid import cannot modify current Settings state.
- Source tests, Rust tests, deployed browser QA, and source/deployed static hash checks pass.

## Out Of Scope

- Conditional branches, loops, parallel branches, variables, secrets, schedules, retry policies, or arbitrary shell interpolation.
- Renaming the persisted `terminal_tool_entries` field or `tools` root key.
- A second workflow engine or a second Settings payload.
- Supporting agents other than Codex in the new structured launch action; add agent-neutral launch only after a concrete Claude/ZCode workflow requires it.
- Drag-and-drop tree sorting; explicit parent and numeric sort controls remain authoritative in this iteration.
