import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const terminalRoutesSource = readFileSync(new URL("../src/routes/terminal.rs", import.meta.url), "utf8");
const managerSource = readFileSync(new URL("../src/terminal/manager.rs", import.meta.url), "utf8");
const source = readFileSync(new URL("../static/terminal-experience-save.js", import.meta.url), "utf8");

function createElement(tagName, initialChildren = []) {
  const element = {
    tagName: tagName.toUpperCase(),
    children: [...initialChildren],
    listeners: new Map(),
    disabled: false,
    textContent: "",
    dataset: {},
    hidden: false,
    open: false,
    returnValue: "",
    addEventListener(type, listener) {
      this.listeners.set(type, [...(this.listeners.get(type) || []), listener]);
    },
    removeEventListener(type, listener) {
      this.listeners.set(type, (this.listeners.get(type) || []).filter((item) => item !== listener));
    },
    dispatch(type) {
      for (const listener of this.listeners.get(type) || []) listener();
    },
    focus() {
      testState.focused = this;
    },
    show() {
      this.open = true;
      testState.shown = true;
    },
    showModal() {
      this.show();
    },
    close(returnValue = "") {
      this.open = false;
      this.returnValue = returnValue;
      this.dispatch("close");
    },
  };
  return element;
}

let testState;
function createSandbox({ scheduleError = null, scheduleResult = null } = {}) {
  testState = { focused: null, shown: false, requests: [] };
  const yes = createElement("button");
  const no = createElement("button");
  const cancel = createElement("button");
  const dialog = createElement("dialog");
  const elements = {
    "terminal-experience-save-dialog": dialog,
    "terminal-experience-save-title": createElement("h2"),
    "terminal-experience-save-countdown": createElement("p"),
    "terminal-experience-save-status": createElement("div"),
    "terminal-experience-save-yes": yes,
    "terminal-experience-save-no": no,
    "terminal-experience-save-cancel": cancel,
  };
  const sandbox = {
    console,
    encodeURIComponent,
    document: { getElementById: (id) => elements[id] || null },
    window: {
      timers: [],
      now: 0,
      listeners: new Map(),
      addEventListener(type, listener) {
        this.listeners.set(type, [...(this.listeners.get(type) || []), listener]);
      },
      removeEventListener(type, listener) {
        this.listeners.set(type, (this.listeners.get(type) || []).filter((item) => item !== listener));
      },
      setInterval(listener, interval) {
        const timer = { listener, interval, id: this.timers.length + 1 };
        this.timers.push(timer);
        return timer.id;
      },
      clearInterval(timerId) {
        const timer = this.timers.find((item) => item.id === timerId);
        if (timer) timer.cleared = true;
      },
    },
    async requestJson(url, options) {
      testState.requests.push({ url, options });
      if (scheduleError) throw scheduleError;
      if (scheduleResult) return scheduleResult;
      return { ok: true, task: { id: "task-1" } };
    },
  };
  sandbox.Date = { now: () => sandbox.window.now };
  vm.createContext(sandbox);
  vm.runInContext(source, sandbox);
  return { sandbox, dialog, yes, no, cancel, elements };
}

test("timeout defaults to No, closes directly, and does not schedule experience persistence", async () => {
  const env = createSandbox();
  const pending = env.sandbox.confirmTerminalExperienceSaveAndHide({ id: "s1", name: "one" });
  assert.equal(env.sandbox.window.timers.length, 1);
  assert.equal(testState.focused, env.no);
  env.sandbox.window.now = 10000;
  env.sandbox.window.timers[0].listener();
  const result = await pending;
  assert.equal(result.shouldClose, true);
  assert.equal(result.shouldHide, false);
  assert.equal(result.action, "close");
  assert.equal(testState.requests.length, 0);
  assert.equal(env.dialog.open, false);
  assert.ok(env.sandbox.window.timers[0].cleared);
});

test("accepting schedules one short delayed agent message and failure still hides", async () => {
  const env = createSandbox();
  const pending = env.sandbox.confirmTerminalExperienceSaveAndHide({ id: "s2", name: "two" });
  env.no.listeners.get("click").at(-1)();
  const noResult = await pending;
  assert.equal(noResult.shouldClose, true);
  assert.equal(noResult.shouldHide, false);
  assert.equal(noResult.action, "close");

  const accepted = createSandbox();
  const acceptedPending = accepted.sandbox.confirmTerminalExperienceSaveAndHide({
    id: "s3",
    name: "three",
    path: "tools/webClx",
  });
  accepted.yes.listeners.get("click").at(-1)();
  const acceptedResult = await acceptedPending;
  assert.equal(acceptedResult.shouldHide, true);
  assert.equal(acceptedResult.shouldClose, false);
  assert.equal(acceptedResult.action, "save_and_hide");
  assert.equal((await acceptedResult.experienceResult).experienceScheduled, true);
  assert.equal(testState.requests.length, 1);
  const request = testState.requests[0];
  assert.equal(request.url, "/api/terminal/scheduled-inputs");
  assert.equal(request.options.method, "POST");
  const body = JSON.parse(request.options.body);
  assert.equal(body.session_id, "s3");
  assert.equal(body.due_at, 3000);
  assert.equal(body.task_type, "agent_message");
  assert.equal(body.send_enter, true);
  assert.equal(body.repeat_interval_millis, 0);
  assert.match(body.text, /\.agents\/skills\/<项目名>-ops\/SKILL\.md/);
  assert.match(body.text, /运维技能名为：项目名-ops/);
  assert.match(body.text, /提供关键字供反查追溯/);
  assert.match(body.text, /只需指路/);
  assert.match(body.text, /保持追加到运维技能或主文档的简洁性/);
  assert.match(body.text, /skill-bidirectional-sync/);
  assert.match(body.text, /webclx terminal close "s3"/);
  assert.match(body.text, /\/api\/terminal\/sessions\/s3\/close/);
  assert.doesNotMatch(body.text, /无项目名时创建/);

  const failed = createSandbox({ scheduleError: new Error("offline") });
  const failedPending = failed.sandbox.confirmTerminalExperienceSaveAndHide({ id: "s4" });
  failed.yes.listeners.get("click").at(-1)();
  const failedResult = await failedPending;
  assert.equal(failedResult.shouldHide, true);
  assert.equal(failedResult.shouldClose, false);
  const failure = await failedResult.experienceResult;
  assert.equal(failure.experienceScheduled, false);
  assert.equal(failure.error, "offline");
});

test("reopening clears old handlers and accepting hides before a stalled schedule completes", async () => {
  const env = createSandbox({ scheduleResult: new Promise(() => {}) });
  const first = env.sandbox.confirmTerminalExperienceSaveAndHide({ id: "old" });
  env.dialog.close();
  assert.equal((await first).shouldClose, true);
  assert.equal(env.yes.listeners.get("click").length, 0);
  const second = env.sandbox.confirmTerminalExperienceSaveAndHide({ id: "new" });
  const duplicate = await env.sandbox.confirmTerminalExperienceSaveAndHide({ id: "other" });
  assert.equal(duplicate.shouldHide, false);
  assert.equal(duplicate.shouldClose, false);
  env.yes.dispatch("click");
  assert.equal((await second).shouldHide, true);
  assert.equal((await second).shouldClose, false);
  assert.equal(env.dialog.open, false);
  assert.equal(testState.requests.length, 1);
  assert.equal(JSON.parse(testState.requests[0].options.body).session_id, "new");
});

test("cancel button aborts the hide entirely without scheduling", async () => {
  const env = createSandbox();
  const pending = env.sandbox.confirmTerminalExperienceSaveAndHide({ id: "sc", name: "cancel-session" });
  // Click the cancel button
  env.cancel.listeners.get("click").at(-1)();
  const result = await pending;
  assert.equal(result.shouldHide, false);
  assert.equal(testState.requests.length, 0);
  assert.equal(env.dialog.open, false);
  assert.equal(env.dialog.returnValue, "cancel");
  // Listeners should be cleaned up — no lingering yes/no/cancel handlers
  assert.equal((env.yes.listeners.get("click") || []).length, 0);
  assert.equal((env.no.listeners.get("click") || []).length, 0);
  assert.equal((env.cancel.listeners.get("click") || []).length, 0);
});

test("Escape key cancels and aborts the hide entirely", async () => {
  const env = createSandbox();
  const pending = env.sandbox.confirmTerminalExperienceSaveAndHide({ id: "se", name: "esc-session" });
  // Fire Escape key via the window listener
  const keydownListeners = env.sandbox.window.listeners.get("keydown") || [];
  assert.ok(keydownListeners.length > 0, "should have registered keydown listener");
  keydownListeners.at(-1)({ key: "Escape", preventDefault() {} });
  const result = await pending;
  assert.equal(result.shouldHide, false);
  assert.equal(testState.requests.length, 0);
  // Timer should be cleared
  assert.ok(env.sandbox.window.timers[0].cleared);
});

test("single-session hide remains separate from grouped idle semantics", () => {
  assert.match(terminalRoutesSource, /sessions\/\{session_id\}\/hide.*terminal::hide_session/);
  assert.match(managerSource, /pub fn set_session_hidden\(/);
  assert.match(managerSource, /hiding_one_group_member_only_hides_that_session/);
});


test("save-only prompt preserves persistence requirements and excludes self-close instructions", () => {
  const { sandbox } = createSandbox();
  const closing = sandbox.terminalExperienceSavePrompt({ id: "target" });
  const saving = sandbox.terminalExperienceSavePrompt({ id: "target" }, { closeAfterSave: false });
  assert.equal(saving.split("完成后请汇报")[0], closing.split("确认完成后")[0]);
  assert.match(saving, /不要关闭或退出本终端/);
  assert.doesNotMatch(saving, /webclx terminal close|curl|自毁|\/close/);
});

test("save-only action freezes its target, suppresses duplicate clicks and preserves terminal state", async () => {
  const { sandbox } = createSandbox();
  const originalState = { activeSessionId: "s1", sessions: [{ id: "s1", name: "one" }, { id: "s2" }] };
  sandbox.state = originalState;
  sandbox.isTerminalConnected = () => true;
  const statuses = [];
  sandbox.setTerminalToolsStatus = (message) => statuses.push(message);
  sandbox.updateStatus = () => {};
  let closedMenu = 0;
  sandbox.closeTerminalToolsMenu = () => { closedMenu += 1; };
  let finish;
  const sends = [];
  sandbox.sendTerminalAutoTypedInput = (text, options) => {
    sends.push({ text, options });
    return new Promise((resolve) => { finish = resolve; });
  };
  const button = { disabled: false };
  const pending = sandbox.sendTerminalExperienceSave({ currentTarget: button });
  assert.equal(button.disabled, true);
  originalState.activeSessionId = "s2";
  await sandbox.sendTerminalExperienceSave({ currentTarget: button });
  assert.equal(sends.length, 1);
  assert.equal(sends[0].options.sessionId, "s1");
  finish(true);
  await pending;
  assert.equal(button.disabled, false);
  assert.equal(originalState.activeSessionId, "s2");
  assert.equal(originalState.sessions.length, 2);
  assert.equal(testState.requests.length, 0);
  assert.equal(closedMenu, 1);

  sandbox.sendTerminalAutoTypedInput = async () => { throw new Error("offline"); };
  await sandbox.sendTerminalExperienceSave({ currentTarget: button });
  assert.equal(button.disabled, false);
  assert.equal(statuses.at(-1), "offline");
  assert.equal(closedMenu, 1);
});
