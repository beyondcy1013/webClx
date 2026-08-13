# Unified Preset Row Click Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the `auth`, `api`, and `claude` preset tables behave consistently so clicking a preset row triggers the same apply flow as the row's `切换` button.

**Architecture:** Reuse the existing preset table renderer and add one shared row-decoration helper that wires row click and keyboard activation while ignoring nested action controls. Keep the current per-table apply handlers and status updates intact. Preserve the current visual pattern by adding one shared row style instead of per-table special cases.

**Tech Stack:** Rust backend unchanged, vanilla JavaScript, CSS, existing static deployment sync.

---

### Task 1: Add a shared preset-row activation helper

**Files:**
- Modify: `static/app.js:3118-3158`

- [ ] **Step 1: Inspect the current row rendering helpers**

```bash
sed -n '3118,3158p' static/app.js
```

- [ ] **Step 2: Replace the API-only helper with a shared helper that accepts an activation callback**

```javascript
function isPresetRowClickIgnored(event) {
  return Boolean(event?.target instanceof Element && event.target.closest("button, a, input, select, textarea, label"));
}

function makePresetRowClickable(row, preset, activate) {
  if (!row || !preset || typeof activate !== "function") {
    return;
  }

  row.classList.add("clickable-preset-row");
  row.tabIndex = 0;
  row.setAttribute("aria-label", `点击切换到 ${preset.name}`);
  row.title = `点击切换到 ${preset.name}`;

  row.addEventListener("click", (event) => {
    if (isPresetRowClickIgnored(event)) {
      return;
    }
    activate();
  });

  row.addEventListener("keydown", (event) => {
    if (isPresetRowClickIgnored(event) || (event.key !== "Enter" && event.key !== " ")) {
      return;
    }
    event.preventDefault();
    activate();
  });
}
```

- [ ] **Step 3: Keep the helper in a reusable location near the preset table renderer**

```bash
node --check static/app.js
```

### Task 2: Wire auth, api, and claude rows to the shared helper

**Files:**
- Modify: `static/app.js:5366-5586`

- [ ] **Step 1: Update `renderPresetTable` usage in the three preset renderers**

```javascript
renderPresetTable({
  listEl: authPresetListEl,
  presets,
  emptyText: "还没有保存任何 auth 预设。",
  emptyColspan: 14 + configKeys.length,
  buildCells: (preset) => { /* unchanged cells */ },
  decorateRow: (row, preset) => makePresetRowClickable(row, preset, () => applyAuthPreset(preset.id)),
});

renderPresetTable({
  listEl: apiPresetListEl,
  presets,
  emptyText: "还没有保存任何 API 预设。",
  emptyColspan: 11 + configKeys.length,
  buildCells: (preset) => { /* unchanged cells */ },
  decorateRow: (row, preset) => makePresetRowClickable(row, preset, () => applyApiPreset(preset.id)),
});

renderPresetTable({
  listEl: claudePresetListEl,
  presets,
  emptyText: "还没有保存任何 Claude 预设。",
  emptyColspan: 8 + configKeys.length,
  buildCells: (preset) => { /* unchanged cells */ },
  decorateRow: (row, preset) => makePresetRowClickable(row, preset, () => applyClaudePreset(preset.id)),
});
```

- [ ] **Step 2: Leave the per-row action buttons in place so nested clicks still work**

```bash
rg -n "createActionButton\\(\"切换\"" static/app.js
```

- [ ] **Step 3: Re-run a syntax check after the wiring change**

```bash
node --check static/app.js
```

### Task 3: Add a single shared row style and sync deployment assets

**Files:**
- Modify: `static/styles.css:1203-1215`
- Modify: `docs/codex/index.md:89-97`
- Sync: `/home/bin/webclx/static/app.js`
- Sync: `/home/bin/webclx/static/styles.css`

- [ ] **Step 1: Add one pointer/keyboard focus style for all clickable preset rows**

```css
.clickable-preset-row {
  cursor: pointer;
}

.clickable-preset-row:focus-visible {
  outline: 2px solid var(--accent-color);
  outline-offset: -2px;
}
```

- [ ] **Step 2: Update the project memory note to cover all preset tables, not only Codex_API**

```markdown
- Auth, Codex_API, and Claude preset rows are clickable outside the action buttons; clicking a preset row should invoke the same apply flow as the `切换` button.
```

- [ ] **Step 3: Sync the deployed static files**

```bash
install -m 0644 static/app.js /home/bin/webclx/static/app.js
install -m 0644 static/styles.css /home/bin/webclx/static/styles.css
cmp -s static/app.js /home/bin/webclx/static/app.js
cmp -s static/styles.css /home/bin/webclx/static/styles.css
```

- [ ] **Step 4: Verify the runtime endpoint still serves the updated behavior**

```bash
curl -s --max-time 5 http://127.0.0.1:11111/api/auth/api-presets | jq '.presets | map(select(.name|ascii_downcase|contains("deepseek"))) | .[0] | {id,name,active}'
curl -s --max-time 5 -X PUT http://127.0.0.1:11111/api/auth/api-presets/api-1778249626614/apply | jq '{ok,name}'
```

