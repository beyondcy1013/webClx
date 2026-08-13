# Editing Preset Direct Apply Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a direct "switch to current editing preset" action to the auth, API, and Claude preset editors.

**Architecture:** Extend each editor toolbar with one extra button that is only enabled while editing an existing preset. Reuse the existing `applyAuthPreset`, `applyApiPreset`, and `applyClaudePreset` flows so the new action stays behaviorally identical to the preset table switch action.

**Tech Stack:** Static HTML, vanilla JavaScript

---

### Task 1: Extend Editor Toolbars

**Files:**
- Modify: `static/index.html`

- [ ] **Step 1: Add one direct-apply button after the existing save-as-new button in each preset editor toolbar**

```html
<button id="auth-apply-edited-preset" class="button secondary" type="button" disabled>切换到当前编辑预设</button>
<button id="api-apply-edited-preset" class="button secondary" type="button" disabled>切换到当前编辑预设</button>
<button id="claude-apply-edited-preset" class="button secondary" type="button" disabled>切换到当前编辑预设</button>
```

### Task 2: Wire Editing State And Apply Actions

**Files:**
- Modify: `static/app.js`

- [ ] **Step 1: Read the new button elements and toggle them from each `set*PresetEditingState()` helper**

```js
authApplyEditedPresetButton.disabled = !presetId;
apiApplyEditedPresetButton.disabled = !presetId;
claudeApplyEditedPresetButton.disabled = !presetId;
```

- [ ] **Step 2: Add one click handler per editor that applies the currently edited preset ID through the existing apply helpers**

```js
await applyAuthPreset(state.editingAuthPresetId, authFormStatusEl);
await applyApiPreset(state.editingApiPresetId, apiFormStatusEl);
await applyClaudePreset(state.editingClaudePresetId, claudeFormStatusEl);
```

- [ ] **Step 3: Disable and restore the new buttons alongside the existing save/clear buttons during save flows**

### Task 3: Verify And Deploy

**Files:**
- Modify: `docs/codex/index.md`
- Modify: `/home/bin/webclx/static/index.html`
- Modify: `/home/bin/webclx/static/app.js`

- [ ] **Step 1: Run static verification**

Run: `node --check static/app.js`
Expected: exit `0`

Run: `git diff --check`
Expected: no output

- [ ] **Step 2: Sync deployed static assets**

Run: `install -m 0644 static/index.html /home/bin/webclx/static/index.html`
Expected: exit `0`

Run: `install -m 0644 static/app.js /home/bin/webclx/static/app.js`
Expected: exit `0`

- [ ] **Step 3: Update durable context**

Add a short note that all three preset editors now expose a direct apply action while editing an existing preset.
