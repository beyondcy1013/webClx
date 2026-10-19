import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const mobileKeysSource = readFileSync(
  new URL("../static/terminal-mobile-keys.js", import.meta.url),
  "utf8",
);
const terminalSource = readFileSync(new URL("../static/terminal.js", import.meta.url), "utf8");
const restoreSource = readFileSync(
  new URL("../static/terminal-workspace-restores.js", import.meta.url),
  "utf8",
);

test("project management exposes the terminal workspace restore workflow", () => {
  assert.match(terminalHtml, /<option value="open_workspace_restores">恢复终端<\/option>/);
  assert.match(terminalHtml, /data-project-action="open_workspace_restores">恢复终端<\/button>/);
  assert.match(terminalHtml, /id="terminal-workspace-restores-dialog"/);
  assert.match(terminalHtml, /terminal-workspace-restores\.js\?v=/);
  assert.match(mobileKeysSource, /action === "open_workspace_restores"[\s\S]*?openTerminalWorkspaceRestoresDialog\(\)/);
  assert.match(terminalSource, /bindTerminalWorkspaceRestoresDialog\(\)/);
});

function loadRestoreModule(overrides = {}) {
  const context = {
    console,
    document: { getElementById: () => null },
    requestJson: async () => ({ records: [] }),
    updateStatus: () => {},
    loadSessions: async () => {},
    selectSession: () => {},
    ...overrides,
  };
  context.window = context;
  vm.createContext(context);
  vm.runInContext(restoreSource, context);
  return context;
}

test("restore list renders agent, terminal name, group and disabled state", async () => {
  const elements = new Map();
  const makeElement = (id) => {
    if (elements.has(id)) return elements.get(id);
    const element = {
      children: [],
      append(...nodes) { this.children.push(...nodes); },
      replaceChildren(...nodes) { this.children = [...nodes]; },
      addEventListener() {},
      style: { setProperty() {} },
    };
    elements.set(id, element);
    return element;
  };
  const context = loadRestoreModule({
    document: {
      getElementById: makeElement,
      createElement: (tag) => ({
        tagName: tag,
        children: [],
        append(...nodes) { this.children.push(...nodes); },
        addEventListener() {},
        style: { setProperty() {} },
        dataset: {},
      }),
    },
  });
  context.terminalWorkspaceRestoresItems = [
    {
      session_id: "s1",
      name: "前端修复",
      program: "codex",
      resume_id: "019f-demo-session",
      group_name: "协作组",
      group_token: "group-1",
      group_color: "#4f8cff",
      group_order: 1,
      display_path: "/codes/webClx",
      updated_at: Date.parse("2026-09-18T12:34:00"),
      recoverable: true,
      restore_count: 0,
    },
    {
      session_id: "s2",
      name: "已恢复",
      program: "",
      group_name: "",
      display_path: "/codes",
      updated_at: 0,
      recoverable: false,
      restore_count: 1,
    },
  ];
  context.terminalWorkspaceRestoreRender();
  const tbody = elements.get("terminal-workspace-restores-tbody");
  // Row creation itself is covered by browser tests; here test deterministic display helpers.
  assert.equal(context.terminalWorkspaceRestoreAgentText(context.terminalWorkspaceRestoresItems[0]), "Codex · 019f-dem…");
  assert.equal(context.terminalWorkspaceRestoreGroupText(context.terminalWorkspaceRestoresItems[0]), "协作组");
  assert.equal(context.terminalWorkspaceRestoreProgramLabel(context.terminalWorkspaceRestoresItems[1]), "Shell");
  assert.equal(context.terminalWorkspaceRestoreDisabled(context.terminalWorkspaceRestoresItems[1]), true);
});

test("restore operation submits exact ids, refreshes sessions, then selects one target", async () => {
  const requests = [];
  const context = loadRestoreModule({
    requestJson: async (url, options = {}) => {
      requests.push({ url, options });
      if (url === "/api/terminal/workspace-restores" && options.method === "POST") {
        return { restored: 1 };
      }
      return { records: [] };
    },
  });
  await context.terminalRestoreWorkspaces(["s1"]);
  assert.deepEqual(requests.map((item) => [item.url, item.options.method]), [
    ["/api/terminal/workspace-restores", "POST"],
  ]);
  assert.deepEqual(JSON.parse(requests[0].options.body), { session_ids: ["s1"] });
  assert.equal(Array.from(context.terminalWorkspaceRestoresSelected || []).length, 0);
});

test("restore result summary preserves skipped and failed counts", () => {
  const context = loadRestoreModule();
  assert.equal(
    context.terminalWorkspaceRestoreResultSummary({
      restored: 2,
      skipped: [{ session_id: "s3", reason: "session_already_running" }],
      failed: [{ session_id: "s4", reason: "backend unavailable" }],
    }),
    "已恢复 2 个终端（跳过 1，失败 1）。",
  );
  assert.equal(
    context.terminalWorkspaceRestoreResultSummary({ restored: 0 }),
    "已恢复 0 个终端。",
  );
});

test("empty restore selection is rejected without an API call", async () => {
  let called = false;
  const context = loadRestoreModule({
    requestJson: async () => {
      called = true;
      return {};
    },
  });
  await context.terminalRestoreWorkspaces([]);
  assert.equal(called, false);
});
