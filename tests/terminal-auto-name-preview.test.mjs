import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const source = readFileSync(
  new URL("../static/terminal-specified-task.js", import.meta.url),
  "utf8",
);

function createSandbox(sessions) {
  const sandbox = {
    document: {
      getElementById() { return null; },
      querySelector(selector) {
        if (selector.includes("terminal-specified-task-session-action")) {
          return { value: "new" };
        }
        return null;
      },
    },
    state: {
      activeSessionId: sessions[0]?.id || "",
      currentPath: "/home/codes/tools/webClx",
      sessions,
    },
    normalizeTerminalPath: (value) => String(value || "").replace(/\/+$/, ""),
    sessionPath: (session) => String(session?.path || "").replace(/\/+$/, ""),
    specifiedPresetSessionAction: (action) => action || "new",
    specifiedPresetTerminalName: () => "",
  };
  vm.createContext(sandbox);
  vm.runInContext(source, sandbox);
  return sandbox;
}

test("new terminal preview follows backend automatic numbering without submitting a name", () => {
  const sandbox = createSandbox([
    { id: "s1", name: "webClx_1", path: "/home/codes/tools/webClx" },
    { id: "s3", name: "webClx_3", path: "/home/codes/tools/webClx" },
  ]);

  assert.equal(sandbox.terminalSpecifiedTaskAutoNamePreview(), "webClx_4");
  assert.equal(sandbox.terminalSpecifiedTaskFinalTerminalName(), "");
});

test("new terminal preview wraps after 100 and skips occupied indices", () => {
  const sandbox = createSandbox([
    { id: "s100", name: "webClx_100", path: "/home/codes/tools/webClx" },
    { id: "s10", name: "webClx_10", path: "/home/codes/tools/webClx" },
  ]);

  assert.equal(sandbox.terminalSpecifiedTaskAutoNamePreview(), "webClx_11");
});
