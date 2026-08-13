# Historical Workspace Terminal Name Restore Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist each active terminal's name with its agent resume ID and restore that name through the terminal rename API before resuming the conversation.

**Architecture:** Reuse the existing resume archive registry as the durable `resume_id -> terminal_name` store. The historical-workspace refresh already resolves active terminal resume IDs, so it will upsert archive metadata only when the stored name or directory differs; restore links will pass the stored name into the concrete-session creation flow, which calls the existing rename endpoint before navigation.

**Tech Stack:** Browser JavaScript, existing webClx terminal/resume REST APIs, Node.js contract tests.

## Global Constraints

- Preserve unrelated dirty-worktree changes and touch only files required by this feature.
- Use the existing `POST /api/terminal/resume-archives` and `PUT /api/terminal/sessions/{session_id}` APIs.
- Keep modified-click/native link behavior unchanged.
- Follow RED/GREEN TDD and commit each validated stage.

---

### Task 1: Add the terminal-name persistence and restore contract

**Files:**
- Create: `tests/workspace-history-terminal-name-restore.test.mjs`
- Modify: `static/app-workspace-history.js`
- Modify: `static/app.js`
- Modify: `static/index.html`
- Modify: `docs/codex/tasks/terminal-rename-presets.md`

**Interfaces:**
- Consumes: `POST /api/terminal/resume-archives`, `PUT /api/terminal/sessions/{session_id}`, `openFreshTerminalRunLink(event, path, command, options)`.
- Produces: `persistWorkspaceHistoryTerminalArchive(session, detected)`, plus a `terminalName` option on `openFreshTerminalSession` and `openFreshTerminalRunLink`.

- [ ] **Step 1: Write the failing test**

Create a Node contract test that requires the history refresh to post `resume_id`, `cwd`, and `terminal_name`, and requires restore handlers to pass the recorded name into a fresh-session flow that calls the rename API before navigation.

- [ ] **Step 2: Run test to verify it fails**

Run: `node tests/workspace-history-terminal-name-restore.test.mjs`

Expected: FAIL because `persistWorkspaceHistoryTerminalArchive` and the `terminalName` restore option do not exist.

- [ ] **Step 3: Write minimal implementation**

Add a persistence helper that skips an unchanged archive, preserves existing archive fields, posts changed metadata, and merges the returned archive into `state.terminalArchives`. Extend the fresh-session helper as follows:

```js
async function openFreshTerminalSession(
  path,
  { runCommand = "", quickStart = !runCommand, terminalName = "" } = {},
) {
  const session = await requestJson("/api/terminal/sessions", { method: "POST" });
  const restoredName = String(terminalName || "").trim();
  if (restoredName && restoredName !== session.name) {
    await requestJson(`/api/terminal/sessions/${encodeURIComponent(session.id)}`, {
      method: "PUT",
      body: JSON.stringify({ path: session.path, name: restoredName }),
    });
  }
  window.location.assign(buildTerminalUrl(session.path, session.id, { runCommand }));
}
```

Pass `item.activeTerminalName` from both archive and conversation restore rows.

- [ ] **Step 4: Run test to verify it passes**

Run: `node tests/workspace-history-terminal-name-restore.test.mjs`

Expected: PASS with exit code 0.

- [ ] **Step 5: Verify related behavior and syntax**

Run:

```bash
node --check static/app.js
node --check static/app-workspace-history.js
node tests/workspace-terminal-fresh-link.test.mjs
node tests/terminal-archives-and-idle.test.mjs
```

Expected: all commands exit 0.

- [ ] **Step 6: Commit**

```bash
git add tests/workspace-history-terminal-name-restore.test.mjs static/app-workspace-history.js static/app.js static/index.html docs/codex/tasks/terminal-rename-presets.md
git commit -m "feat: 恢复历史终端原名称"
```
