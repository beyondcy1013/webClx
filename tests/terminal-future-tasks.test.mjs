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
const specifiedTaskSource = readFileSync(
  new URL("../static/terminal-specified-task.js", import.meta.url),
  "utf8",
);
const futureTasksSource = readFileSync(
  new URL("../static/terminal-future-tasks.js", import.meta.url),
  "utf8",
);
const presetActionsSource = readFileSync(
  new URL("../static/specified-preset-actions.js", import.meta.url),
  "utf8",
);

test("project management menu exposes the future tasks workflow", () => {
  assert.match(terminalHtml, /<option value="open_future_tasks">未来任务<\/option>/);
  assert.match(terminalHtml, /data-project-action="open_future_tasks">未来任务<\/button>/);
  assert.match(terminalHtml, /id="terminal-future-tasks-dialog"/);
  assert.match(terminalHtml, /terminal-future-tasks\.js\?v=/);
  assert.match(
    mobileKeysSource,
    /action === "open_future_tasks"[\s\S]*?openTerminalFutureTasksDialog\(\)/,
  );
});

test("specified task dialog gains the schedule field and future mode wiring", () => {
  assert.match(terminalHtml, /id="terminal-specified-task-schedule-field"[\s\S]*?type="datetime-local"/);
  assert.match(terminalHtml, /id="terminal-specified-task-mode-field"/);
  assert.match(
    specifiedTaskSource,
    /terminalSpecifiedTaskFutureMode = options\.futureTask === true/,
  );
  assert.match(
    specifiedTaskSource,
    /if \(terminalSpecifiedTaskFutureMode\) \{[\s\S]*?submitTerminalSpecifiedFutureTask/,
  );
  assert.match(
    specifiedTaskSource,
    /if \(dom\.scheduleField\) \{\s*dom\.scheduleField\.hidden = !terminalSpecifiedTaskFutureMode;/,
  );
  assert.match(terminalSource, /bindTerminalFutureTasksDialog\(\)/);
});

test("future task command builder and parser preserve the scheduled preset", () => {
  const context = loadFutureTasks();
  const command = context.buildTerminalFutureTaskRunCommand(
    "codex",
    "api",
    "preset-9",
    "继续修复",
  );
  const parsed = context.parseTerminalFutureTaskCommand(command);
  assert.equal(parsed.agent, "codex");
  assert.equal(parsed.presetKind, "api");
  assert.equal(parsed.presetId, "preset-9");
  assert.equal(parsed.prompt, "继续修复");
});

function loadFutureTasks(overrides = {}) {
  const context = {
    console,
    document: { getElementById: () => null },
    requestJson: async () => ({ tasks: [] }),
    updateStatus: () => {},
    ...overrides,
  };
  context.window = context;
  vm.createContext(context);
  vm.runInContext(presetActionsSource, context);
  vm.runInContext(futureTasksSource, context);
  return context;
}

test("future task run commands round-trip through build and parse", () => {
  const context = loadFutureTasks();
  const command = context.buildTerminalFutureTaskRunCommand(
    "codex",
    "api",
    "preset-1",
    "修复登录 bug\n注意单引号 it's",
  );
  assert.match(command, /^webclx run api 'preset-1' -- codex /);
  const parsed = context.parseTerminalFutureTaskCommand(command);
  assert.equal(parsed.agent, "codex");
  assert.equal(parsed.presetKind, "api");
  assert.equal(parsed.presetId, "preset-1");
  assert.equal(parsed.prompt, "修复登录 bug\n注意单引号 it's");

  const claudeCommand = context.buildTerminalFutureTaskRunCommand("claude", "claude", "claude-9", "总结进展");
  assert.match(claudeCommand, /^webclx run claude 'claude-9' -- claude '总结进展'$/);
  const claudeParsed = context.parseTerminalFutureTaskCommand(claudeCommand);
  assert.equal(claudeParsed.agent, "claude");
  assert.equal(claudeParsed.presetKind, "claude");
  assert.equal(claudeParsed.prompt, "总结进展");
});

test("parse rejects commands that are not future task launches", () => {
  const context = loadFutureTasks();
  assert.equal(context.parseTerminalFutureTaskCommand("继续"), null);
  assert.equal(context.parseTerminalFutureTaskCommand("ls -la"), null);
  assert.equal(
    context.parseTerminalFutureTaskCommand("webclx run api 'x' -- bash -c 'rm'"),
    null,
  );
  assert.equal(
    context.parseTerminalFutureTaskCommand("webclx run other 'x' -- codex '任务'"),
    null,
  );
});

test("schedule input conversion is a local-time round trip", () => {
  const context = loadFutureTasks();
  const millis = new Date(2026, 8, 5, 12, 0).getTime();
  const text = context.terminalFutureTaskMillisToScheduleInput(millis);
  assert.equal(text, "2026-09-05T12:00");
  assert.equal(context.terminalFutureTaskScheduleInputToMillis(text), millis);
  assert.equal(context.terminalFutureTaskScheduleInputToMillis(""), 0);
  assert.equal(context.terminalFutureTaskMillisToScheduleInput(0), "");
});

test("future task list keeps only pending future_task entries", async () => {
  const now = Date.now();
  const context = loadFutureTasks({
    requestJson: async (url) => {
      if (url === "/api/terminal/scheduled-inputs") {
        return {
          tasks: [
            {
              id: "f1",
              task_type: "future_task",
              due_at_millis: now + 7200_000,
              text: "webclx run api 'p1' -- codex '任务A'",
              working_dir: "/w/a",
            },
            {
              id: "old",
              task_type: "future_task",
              due_at_millis: now - 1000,
              text: "webclx run api 'p1' -- codex '过期任务'",
              working_dir: "/w/b",
            },
            {
              id: "paste",
              task_type: "paste",
              due_at_millis: now + 999,
              text: "hello",
              working_dir: "/w/c",
            },
          ],
        };
      }
      if (url === "/api/auth/codex-accounts") {
        return { accounts: [{ id: "p1", name: "主力", model: "gpt-5" }] };
      }
      if (url === "/api/auth/claude-presets") {
        return { presets: [] };
      }
      throw new Error(`unexpected endpoint ${url}`);
    },
  });
  await context.refreshTerminalFutureTasks();
  assert.deepEqual(
    vm.runInContext("terminalFutureTasksItems.map((task) => task.id)", context),
    ["f1"],
  );
});

function loadIntegrated(overrides = {}) {
  const elements = new Map([
    [
      "terminal-specified-task-preset",
      {
        value: "preset-1",
        selectedOptions: [{ dataset: { kind: "api" }, textContent: "主力 · gpt-5" }],
      },
    ],
    ["terminal-specified-task-text", { value: "修复登录" }],
    ["terminal-specified-task-schedule", { value: "" }],
  ]);
  const context = {
    console,
    state: { sessions: [], currentPath: "/w/a" },
    document: {
      getElementById: (id) => elements.get(id) || null,
      querySelector: () => ({ value: "codex" }),
      querySelectorAll: () => [],
    },
    updateStatus: () => {},
    setTerminalSpecifiedTaskStatus: () => {},
    reportTerminalSpecifiedTaskPageStatus: () => {},
    closeTerminalSpecifiedTaskDialog: () => {},
    setTerminalSpecifiedTaskBusy: () => {},
    terminalSpecifiedTaskAgent: () => "codex",
    terminalSpecifiedTaskSelectedPresetKind: () => "api",
    terminalSpecifiedTaskSourcePath: "",
    terminalSpecifiedTaskCurrentPath: () => "/w/a",
    requestJson: async () => ({ ok: true }),
    ...overrides,
  };
  context.window = context;
  vm.createContext(context);
  vm.runInContext(presetActionsSource, context);
  vm.runInContext(specifiedTaskSource, context);
  vm.runInContext(futureTasksSource, context);
  return { context, elements };
}

test("editing a future task in place patches it without recreating the terminal", async () => {
  const calls = [];
  const { context, elements } = loadIntegrated({
    requestJson: async (url, options = {}) => {
      calls.push({ url, method: options.method || "GET", body: options.body ? JSON.parse(options.body) : null });
      if (url === "/api/terminal/scheduled-inputs") {
        return { tasks: [] };
      }
      return { ok: true };
    },
  });
  // datetime-local 只到分钟，取整便于断言精确相等。
  const dueAt = Math.floor((Date.now() + 3600_000) / 60_000) * 60_000;
  elements.get("terminal-specified-task-schedule").value =
    context.terminalFutureTaskMillisToScheduleInput(dueAt);
  vm.runInContext(
    "terminalSpecifiedTaskSourcePath = '/w/a'; terminalSpecifiedTaskFutureMode = true;"
      + "terminalSpecifiedTaskFutureEdit = { taskId: 't1', workingDir: '/w/a/', dueAt: 0 };",
    context,
  );
  await context.submitTerminalSpecifiedFutureTask();

  const mutations = calls.filter((call) => call.method !== "GET");
  assert.equal(mutations.length, 1);
  assert.equal(mutations[0].method, "PATCH");
  assert.equal(mutations[0].url, "/api/terminal/scheduled-inputs/t1");
  assert.equal(mutations[0].body.task_type, "future_task");
  assert.equal(mutations[0].body.due_at, dueAt);
  assert.match(mutations[0].body.text, /^webclx run api 'preset-1' -- codex '修复登录'$/);
  assert.match(mutations[0].body.label, /^未来任务 · Codex · /);
});

test("changing the working directory recreates the future task", async () => {
  const calls = [];
  const { context, elements } = loadIntegrated({
    requestJson: async (url, options = {}) => {
      calls.push({ url, method: options.method || "GET", body: options.body ? JSON.parse(options.body) : null });
      if (url === "/api/terminal/scheduled-inputs") {
        return { tasks: [] };
      }
      return { ok: true };
    },
  });
  const dueAt = Math.floor((Date.now() + 3600_000) / 60_000) * 60_000;
  elements.get("terminal-specified-task-schedule").value =
    context.terminalFutureTaskMillisToScheduleInput(dueAt);
  vm.runInContext(
    "terminalSpecifiedTaskSourcePath = '/w/new'; terminalSpecifiedTaskFutureMode = true;"
      + "terminalSpecifiedTaskFutureEdit = { taskId: 't1', workingDir: '/w/old', dueAt: 0 };",
    context,
  );
  await context.submitTerminalSpecifiedFutureTask();

  const mutations = calls.filter((call) => call.method !== "GET");
  assert.equal(mutations.length, 2);
  assert.equal(mutations[0].method, "DELETE");
  assert.equal(mutations[0].url, "/api/terminal/scheduled-inputs/t1");
  assert.equal(mutations[1].method, "POST");
  assert.equal(mutations[1].url, "/api/terminal/scheduled-inputs");
  assert.equal(mutations[1].body.terminal_mode, "new");
  assert.equal(mutations[1].body.working_dir, "/w/new");
  assert.equal(mutations[1].body.send_enter, true);
  assert.equal(mutations[1].body.task_type, "future_task");
});

test("a fresh future task creates a scheduled launch instead of starting now", async () => {
  const calls = [];
  const { context, elements } = loadIntegrated({
    requestJson: async (url, options = {}) => {
      calls.push({ url, method: options.method || "GET", body: options.body ? JSON.parse(options.body) : null });
      if (url === "/api/terminal/scheduled-inputs") {
        return { tasks: [] };
      }
      return { ok: true };
    },
  });
  const dueAt = Math.floor((Date.now() + 3600_000) / 60_000) * 60_000;
  elements.get("terminal-specified-task-schedule").value =
    context.terminalFutureTaskMillisToScheduleInput(dueAt);
  vm.runInContext(
    "terminalSpecifiedTaskSourcePath = '/w/a'; terminalSpecifiedTaskFutureMode = true;"
      + "terminalSpecifiedTaskFutureEdit = null;",
    context,
  );
  await context.submitTerminalSpecifiedFutureTask();

  const mutations = calls.filter((call) => call.method !== "GET");
  assert.equal(mutations.length, 1);
  assert.equal(mutations[0].method, "POST");
  assert.equal(mutations[0].body.session_id, "");
  assert.equal(mutations[0].body.due_at, dueAt);
  assert.match(mutations[0].body.text, /^webclx run api 'preset-1' -- codex '修复登录'$/);
});
