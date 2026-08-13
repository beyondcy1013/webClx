# API Manager Status Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore visible and functional Web UI preset switching in the `Codex_API` table by wiring a real manager status element and guarding status updates against missing DOM nodes.

**Architecture:** Keep the existing apply flow and backend API unchanged. Add the missing list-level status elements in the HTML where the table actions live, and harden the shared status helper so a missing element cannot abort button handlers before requests are sent.

**Tech Stack:** Static HTML, vanilla JavaScript

---

### Task 1: Add Manager Status Mount Points

**Files:**
- Modify: `static/index.html`

- [ ] **Step 1: Add the API manager status element below the API preset table**

```html
<div id="api-manager-status" class="inline-status" data-tone="muted" hidden></div>
```

- [ ] **Step 2: Add the Claude manager status element below the Claude preset table**

```html
<div id="claude-manager-status" class="inline-status" data-tone="muted" hidden></div>
```

### Task 2: Guard Shared Status Updates

**Files:**
- Modify: `static/app.js`

- [ ] **Step 1: Make `updateStatus` a no-op when the target element is missing**

```js
function updateStatus(element, message, tone) {
  if (!element) {
    return;
  }
  element.textContent = message;
  element.dataset.tone = tone;
}
```

- [ ] **Step 2: Leave existing `applyApiPreset()` and table actions unchanged so they continue using the new manager status element**

### Task 3: Verify And Deploy

**Files:**
- Modify: `/home/bin/webclx/static/index.html`
- Modify: `/home/bin/webclx/static/app.js`

- [ ] **Step 1: Run syntax and diff checks**

Run: `node --check static/app.js`
Expected: exit `0`

Run: `git diff --check`
Expected: no output

- [ ] **Step 2: Sync static assets to the live service directory**

Run: `install -m 0644 static/index.html /home/bin/webclx/static/index.html`
Expected: exit `0`

Run: `install -m 0644 static/app.js /home/bin/webclx/static/app.js`
Expected: exit `0`

- [ ] **Step 3: Reproduce with the existing API preset endpoints as a sanity check**

Run: `curl -s http://127.0.0.1:11111/api/auth/api-presets | jq '.presets | length'`
Expected: a positive preset count
