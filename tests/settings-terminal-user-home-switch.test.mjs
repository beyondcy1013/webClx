import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const formattersSource = readFileSync(
  new URL("../static/app-settings-formatters.js", import.meta.url),
  "utf8",
);
const eventBindingsSource = readFileSync(
  new URL("../static/app-settings-event-bindings.js", import.meta.url),
  "utf8",
);

function createHarness({ confirmResult = true, workspaceDir = "/home/codes" } = {}) {
  const listeners = new Map();
  const confirmations = [];
  const terminalUserSelectEl = {
    value: "root",
    addEventListener(type, listener) {
      listeners.set(type, listener);
    },
  };
  const workspaceDirInputEl = {
    value: workspaceDir,
    scrollLeft: 12,
  };
  const context = vm.createContext({
    DEFAULT_TERMINAL_USER: "root",
    normalizeTerminalUser: (value) => String(value || "").trim(),
    state: {
      availableUsers: [
        { name: "root", uid: 0, gid: 0, home: "/home/root", shell: "/bin/bash" },
      ],
    },
    terminalUserSelectEl,
    workspaceDirInputEl,
    window: {
      confirm(message) {
        confirmations.push(message);
        return confirmResult;
      },
    },
  });

  vm.runInContext(formattersSource, context);
  vm.runInContext(eventBindingsSource, context);
  vm.runInContext("bindTerminalUserHomeSuggestion()", context);

  return { confirmations, listeners, workspaceDirInputEl };
}

test("switches the workspace to the dynamically resolved terminal user home after confirmation", () => {
  const harness = createHarness();

  harness.listeners.get("change")();

  assert.equal(harness.workspaceDirInputEl.value, "/home/root");
  assert.equal(harness.workspaceDirInputEl.scrollLeft, 0);
  assert.equal(harness.confirmations.length, 1);
  assert.match(harness.confirmations[0], /root/);
  assert.match(harness.confirmations[0], /\/home\/root/);
});

test("keeps the current workspace when the user declines the home switch", () => {
  const harness = createHarness({ confirmResult: false });

  harness.listeners.get("change")();

  assert.equal(harness.workspaceDirInputEl.value, "/home/codes");
  assert.equal(harness.workspaceDirInputEl.scrollLeft, 12);
  assert.equal(harness.confirmations.length, 1);
});

test("does not prompt when the workspace already matches the selected user home", () => {
  const harness = createHarness({ workspaceDir: "/home/root/" });

  harness.listeners.get("change")();

  assert.equal(harness.workspaceDirInputEl.value, "/home/root/");
  assert.equal(harness.confirmations.length, 0);
});
