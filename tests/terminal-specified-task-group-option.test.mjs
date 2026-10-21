import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const specifiedTaskSource = readFileSync(
  new URL("../static/terminal-specified-task.js", import.meta.url),
  "utf8",
);

function loadSpecifiedTask(overrides = {}) {
  const elements = new Map(Object.entries(overrides.elements || {}));
  const context = {
    document: {
      getElementById: (id) => elements.get(id) || null,
      querySelector: () => null,
    },
    state: { activeSessionId: "s1", currentPath: "project", sessions: [] },
    terminalMessageGroups: (sessions) => [{
      token: "group-1",
      name: "提审组",
      color: "#ff5500",
      members: sessions.filter((session) => session.group_token === "group-1"),
    }],
    terminalMessageGroupLabel: (group) => group.name || "GROUP",
    terminalMessageGroupMemberIds: (group) => group.members.map((member) => member.id),
    ...overrides,
  };
  vm.createContext(context);
  vm.runInContext(specifiedTaskSource, context);
  return context;
}

test("new terminal dialog exposes an optional current-group selector", () => {
  assert.match(terminalHtml, /id="terminal-specified-task-group-field"/);
  assert.match(terminalHtml, /id="terminal-specified-task-join-group"[^>]*type="checkbox"/);
  assert.match(terminalHtml, /<span>加入当前群组<\/span>/);
  assert.match(terminalHtml, /id="terminal-specified-task-group"/);
  assert.match(terminalHtml, /terminal-specified-task\.js\?v=/);
});

test("launch joins the selected group without replacing existing members", async () => {
  const calls = [];
  const existing = [
    { id: "s1", name: "lead", group_token: "group-1", group_fixed_order: true },
    { id: "s2", name: "worker", group_token: "group-1" },
  ];
  const created = { id: "created", name: "created", path: "project" };
  const context = loadSpecifiedTask({
    state: { activeSessionId: "s1", currentPath: "project", sessions: existing },
    async createSession() {
      calls.push(["create"]);
      return created;
    },
    async renameTerminalForTool(sessionId, name) {
      calls.push(["rename", sessionId, name]);
      return { ...created, name };
    },
    async requestJson(url, options) {
      calls.push(["group", url, JSON.parse(options.body)]);
      return { ok: true };
    },
    async loadSessions(options) {
      calls.push(["reload", options]);
    },
    async waitForTerminalToolSessionReady(sessionId) {
      calls.push(["ready", sessionId]);
    },
    async sendTerminalAutoTypedInput(command, options) {
      calls.push(["send", command, options.sessionId]);
      return true;
    },
    runCommand: undefined,
  });
  await context.launchTerminalSpecifiedPreset("project", {
    runCommand: "codex",
    groupId: "group-1",
    groupRole: "executor",
  });
  assert.deepEqual(calls.find((item) => item[0] === "group")[2], {
    member_ids: ["s1", "s2", "created"],
    fixed_order: true,
    group_color: "#ff5500",
    group_name: "提审组",
    group_roles: [{ session_id: "created", role: "executor" }],
  });
  assert.equal(
    calls.findIndex((item) => item[0] === "group") < calls.findIndex((item) => item[0] === "ready"),
    true,
  );
});

test("legacy generated hsl group colors are not submitted to the hex-only API", async () => {
  const calls = [];
  const existing = [
    { id: "s1", name: "lead", group_token: "group-1" },
    { id: "s2", name: "worker", group_token: "group-1" },
  ];
  const context = loadSpecifiedTask({
    terminalMessageGroups: () => [{
      token: "group-1",
      name: "",
      color: "hsl(120 76% 52%)",
      members: existing,
    }],
    state: { activeSessionId: "s1", currentPath: "project", sessions: existing },
    async createSession() {
      return { id: "created", name: "created", path: "project" };
    },
    async renameTerminalForTool(sessionId, name) {
      return { id: sessionId, name };
    },
    async requestJson(url, options) {
      calls.push(JSON.parse(options.body));
      return { ok: true };
    },
    async loadSessions() {},
    async waitForTerminalToolSessionReady() {},
    async sendTerminalAutoTypedInput() {
      return true;
    },
  });
  await context.launchTerminalSpecifiedPreset("project", {
    runCommand: "codex",
    groupId: "group-1",
  });
  assert.equal(calls[0].group_color, undefined);
});

test("selected group is ignored unless the optional checkbox is enabled", () => {
  const joinGroup = { checked: false };
  const group = { value: "group-1" };
  const context = loadSpecifiedTask({
    elements: {
      "terminal-specified-task-join-group": joinGroup,
      "terminal-specified-task-group": group,
    },
  });
  assert.equal(context.terminalSpecifiedTaskSelectedGroup(), "");
  joinGroup.checked = true;
  assert.equal(context.terminalSpecifiedTaskSelectedGroup(), "group-1");
});

test("new dialog can specify manager, executor, or reviewer for a joined group", () => {
  assert.match(terminalHtml, /id="terminal-specified-task-group-role"/);
  assert.match(terminalHtml, /<option value="manager">管理者<\/option>/);
  assert.match(terminalHtml, /<option value="executor">执行者<\/option>/);
  assert.match(terminalHtml, /<option value="reviewer">复核者<\/option>/);
  const context = loadSpecifiedTask({
    elements: {
      "terminal-specified-task-join-group": { checked: true },
      "terminal-specified-task-group-role": { value: "manager" },
    },
  });
  assert.equal(context.terminalSpecifiedTaskSelectedGroupRole(), "manager");
});

test("new dialog exposes a self-assembled group with a terminal count", () => {
  assert.match(terminalHtml, /id="terminal-specified-task-new-group-field"/);
  assert.match(terminalHtml, /id="terminal-specified-task-new-group"[^>]*type="checkbox"/);
  assert.match(terminalHtml, /<span>自组群<\/span>/);
  assert.match(terminalHtml, /id="terminal-specified-task-new-group-count"[^>]*type="number"/);
  assert.match(terminalHtml, /min="2"[\s\S]*?max="8"/);
  assert.match(terminalHtml, /value="2"/);

  const context = loadSpecifiedTask({
    elements: {
      "terminal-specified-task-join-group": { checked: false },
      "terminal-specified-task-new-group": { checked: true },
      "terminal-specified-task-new-group-count": { value: "9" },
    },
  });
  assert.equal(context.terminalSpecifiedTaskCreateGroup(), true);
  assert.equal(context.terminalSpecifiedTaskCreateGroupCount(), 8);
});

test("self-assembled launch creates, renames, groups, and starts only the final terminal", async () => {
  const calls = [];
  const sessions = [
    { id: "created-1", name: "backend_1", path: "project" },
    { id: "created-2", name: "backend_2", path: "project" },
  ];
  let next = 0;
  const context = loadSpecifiedTask({
    state: { activeSessionId: "source", currentPath: "project", sessions: [] },
    async createSession(options) {
      calls.push(["create", options.autoSelect]);
      const session = sessions[next];
      next += 1;
      return session;
    },
    async renameTerminalForTool(sessionId, name) {
      calls.push(["rename", sessionId, name]);
      const session = sessions.find((item) => item.id === sessionId);
      return { ...session, name };
    },
    async requestJson(url, options) {
      calls.push(["group", url, JSON.parse(options.body)]);
      return { ok: true };
    },
    async loadSessions(options) {
      calls.push(["reload", options]);
    },
    async waitForTerminalToolSessionReady(sessionId) {
      calls.push(["ready", sessionId]);
    },
    async sendTerminalAutoTypedInput(command, options) {
      calls.push(["send", command, options.sessionId]);
      return true;
    },
  });
  const finalSession = await context.launchTerminalSpecifiedPreset("project", {
    runCommand: "codex",
    terminalName: "stockTask",
    createGroup: true,
    createGroupCount: 2,
  });
  assert.deepEqual(calls.find((item) => item[0] === "group")[2], {
    member_ids: ["created-1", "created-2"],
    group_name: "stockTask_自组群",
    group_roles: [
      { session_id: "created-1", role: "manager" },
      { session_id: "created-2", role: "executor" },
    ],
  });
  assert.deepEqual(calls.filter((item) => item[0] === "send"), [
    ["send", "codex", "created-2"],
  ]);
  assert.equal(finalSession.id, "created-2");
});

test("self-assembled launch failure cleans up every newly created terminal", async () => {
  const calls = [];
  let next = 0;
  const context = loadSpecifiedTask({
    state: { activeSessionId: "source", currentPath: "project", sessions: [] },
    async createSession() {
      const session = { id: `created-${next + 1}`, name: `backend_${next + 1}` };
      next += 1;
      return session;
    },
    async renameTerminalForTool(sessionId, name) {
      return { id: sessionId, name };
    },
    async requestJson(url, options) {
      calls.push([url, options.method]);
      if (url.endsWith("/group")) {
        throw new Error("group failed");
      }
      return { ok: true };
    },
    async loadSessions(options) {
      calls.push(["reload", options.preferredSessionId]);
    },
    async waitForTerminalToolSessionReady() {
      throw new Error("should not start");
    },
  });
  await assert.rejects(
    context.launchTerminalSpecifiedPreset("project", {
      createGroup: true,
      createGroupCount: 2,
    }),
    /自组群启动失败/,
  );
  assert.deepEqual(calls.filter((item) => item[1] === "DELETE").length, 2);
});
