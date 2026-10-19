import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const source = readFileSync(
  new URL("../static/terminal-dialogs.js", import.meta.url),
  "utf8",
);
const terminalSource = readFileSync(
  new URL("../static/terminal.js", import.meta.url),
  "utf8",
);

function createStorage() {
  const values = new Map();
  return {
    getItem(key) {
      return values.get(key) ?? null;
    },
    setItem(key, value) {
      values.set(key, String(value));
    },
    removeItem(key) {
      values.delete(key);
    },
  };
}

function loadDraftRuntime(sessionStorage) {
  const context = vm.createContext({
    console,
    sessionStorage,
    window: { sessionStorage },
  });
  vm.runInContext(source, context);
  return context;
}

test("document drafts survive a page script reload and stay scoped by session and path", () => {
  const storage = createStorage();
  let runtime = loadDraftRuntime(storage);

  vm.runInContext(
    'persistTerminalAgentsDocDraft("session-a", "AGENTS.md", "draft A");',
    runtime,
  );
  vm.runInContext(
    'persistTerminalAgentsDocDraft("session-a", "docs/plan.md", "draft plan");',
    runtime,
  );
  vm.runInContext(
    'persistTerminalAgentsDocDraft("session-b", "AGENTS.md", "draft B");',
    runtime,
  );

  runtime = loadDraftRuntime(storage);
  assert.equal(
    vm.runInContext('readTerminalAgentsDocDraft("session-a", "AGENTS.md")?.content', runtime),
    "draft A",
  );
  assert.equal(
    vm.runInContext('readTerminalAgentsDocDraft("session-a", "docs/plan.md")?.content', runtime),
    "draft plan",
  );
  assert.equal(
    vm.runInContext('readTerminalAgentsDocDraft("session-b", "AGENTS.md")?.content', runtime),
    "draft B",
  );
  assert.equal(
    vm.runInContext('latestTerminalAgentsDocDraftPath("session-a")', runtime),
    "docs/plan.md",
  );
});

test("saving a document removes only its matching draft", () => {
  const storage = createStorage();
  const runtime = loadDraftRuntime(storage);

  vm.runInContext(
    'persistTerminalAgentsDocDraft("session-a", "AGENTS.md", "draft A");',
    runtime,
  );
  vm.runInContext(
    'persistTerminalAgentsDocDraft("session-a", "docs/plan.md", "draft plan");',
    runtime,
  );
  vm.runInContext('clearTerminalAgentsDocDraft("session-a", "AGENTS.md");', runtime);

  assert.equal(
    vm.runInContext('readTerminalAgentsDocDraft("session-a", "AGENTS.md")', runtime),
    null,
  );
  assert.equal(
    vm.runInContext('readTerminalAgentsDocDraft("session-a", "docs/plan.md")?.content', runtime),
    "draft plan",
  );
});

test("document editor persists input, restores drafts, and clears them after save", () => {
  assert.match(
    terminalSource,
    /terminalAgentsDocEditorEl\.addEventListener\("input"[\s\S]*persistTerminalAgentsDocDraft\(/,
  );
  assert.match(
    source,
    /async function loadTerminalAgentsDoc[\s\S]*readTerminalAgentsDocDraft\([\s\S]*terminalAgentsDocEditorEl\.value = draft\.content/,
  );
  assert.match(
    source,
    /async function saveTerminalAgentsDoc[\s\S]*await requestJson\([\s\S]*clearTerminalAgentsDocDraft\(sessionId, documentPath\)/,
  );
});
