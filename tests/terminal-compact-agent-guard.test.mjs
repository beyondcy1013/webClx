import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const source = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
const autoCompactSource = source("static/terminal-auto-continue.js");
const mobileKeysSource = source("static/terminal-mobile-keys.js");
const quickStartSource = source("static/terminal-command-quickstart.js");
const terminalSource = source("static/terminal.js");

function harness(agent = "", name = "fixture") {
  const timers = new Map();
  const inputs = [];
  const requests = [];
  const statuses = [];
  let timerId = 0;
  const session = { id: "target", activity_agent: agent, name, activity_state: "completed" };
  const sandbox = {
    console,
    state: {
      activeSessionId: "viewed",
      sessions: [session],
      autoCompactSessionIds: new Set([session.id]),
      autoCompactHandledSessions: new Set(),
      autoCompactScheduledTimers: new Map(),
      autoCompactPreviousActivityStates: new Map(),
      autoCompactStatusSessionIds: new Set(),
      autoCompactLastSent: new Map(),
      terminalUserInputAt: new Map(),
    },
    window: {
      setTimeout(callback) { timers.set(++timerId, callback); return timerId; },
      clearTimeout(id) { timers.delete(id); },
    },
    sessionAutoCompactToggleEl: {},
    MOBILE_SLASH_COMMAND_ENTER_DELAY_MS: 0,
    MOBILE_SLASH_COMMAND_CONFIRM_DELAY_MS: 0,
    MOBILE_TEXT_COMMAND_ENTER_DELAY_MS: 0,
    MOBILE_KEY_SEQUENCES: { enter: "\r" },
    sessionActivityState: (item) => item.activity_state,
    sessionActivityLabel: () => "待查看",
    autoContinueSessionLabel: (item) => item.name,
    ensureTerminalSessionCache: () => new Map(),
    normalizeTerminalQuickText: (value) => typeof value === "string" ? value.trim() : "",
    updateStatus: (...args) => statuses.push(args),
    storeTerminalAutoCompactStatusSessionIds() {},
    storeTerminalAutoCompactLastSent() {},
    storeTerminalAutoCompactSessionIds() {},
    focusTerminalAfterSoftKeyboardInput() {},
    sendTerminalInput: (data) => { inputs.push({ id: sandbox.state.activeSessionId, data }); },
    sendTerminalInputToSession: (data, id) => { inputs.push({ id, data }); return true; },
    async requestJson(url, options) {
      const body = JSON.parse(options.body);
      requests.push({ url, body });
      return { data: `${body.command_line}\n` };
    },
  };
  vm.createContext(sandbox);
  vm.runInContext("var mobileKeySendQueue = Promise.resolve();", sandbox);
  vm.runInContext(autoCompactSource, sandbox);
  vm.runInContext(quickStartSource, sandbox);
  vm.runInContext(mobileKeysSource, sandbox);
  vm.runInContext(terminalSource.slice(
    terminalSource.indexOf("function syncAutoCompactToggleForActiveSession()"),
    terminalSource.indexOf("async function persistTerminalAutoContinueOnError("),
  ), sandbox);
  return {
    sandbox, session, timers, inputs, requests, statuses,
    async fireNextTimer() {
      const [id, callback] = timers.entries().next().value;
      timers.delete(id);
      callback();
      await new Promise((resolve) => setImmediate(resolve));
    },
  };
}

for (const agent of ["CodeBuddy", "codebuddy", "AGY", "agy", "Antigravity", "Cline", "Codex/CodeBuddy"]) {
  test(`${agent} cannot schedule, auto-type or soft-key /compact`, async () => {
    const h = harness(agent);
    const { sandbox: s } = h;
    assert.equal(s.maybeAutoCompactCompletedSession(h.session), false);
    assert.equal(h.timers.size, 0);
    assert.equal(s.sendSlashCommand("/compact", { sessionId: "target" }), false);
    assert.equal(s.sendSlashCommand("/compact focus on tests", { sessionId: "target" }), false);
    assert.equal(await s.sendTerminalAutoTypedInput("/compact", { sessionId: "target" }), false);
    assert.deepEqual(h.inputs, []);
    assert.deepEqual(h.requests, []);
    s.state.activeSessionId = "target";
    s.syncAutoCompactToggleForActiveSession();
    assert.equal(s.sessionAutoCompactToggleEl.disabled, true);
    assert.equal(s.sessionAutoCompactToggleEl.checked, false);
    s.state.autoCompactSessionIds.clear();
    s.setAutoCompactForActiveSession(true);
    assert.equal(s.state.autoCompactSessionIds.size, 0);
    // A sent /compact no longer fabricates a compacting state on its own.
    s.syncAutoCompactCompletionStatuses();
    assert.equal(s.state.autoCompactStatusSessionIds.size, 0);
    // The guard is not a terminal-input filter or a general command ban.
    assert.equal(await s.sendTerminalAutoTypedInput("/status", { sessionId: "target" }), true);
    assert.equal(h.requests.length, 1);
  });
}

test("the existing AGY-name fallback still blocks compact before process discovery", () => {
  for (const name of ["AGY", "agy worker", "agy-test"]) {
    const h = harness("", name);
    assert.equal(h.sandbox.isSessionCompactCapableAgent(h.session), false);
    assert.equal(h.sandbox.maybeAutoCompactCompletedSession(h.session), false);
  }
});

for (const agent of ["CodeBuddy", "AGY"]) {
  for (const phase of ["pending", "countdown"]) {
    test(`${agent} discovered during ${phase} cancels automatic compact`, async () => {
      const h = harness();
      assert.equal(h.sandbox.maybeAutoCompactCompletedSession(h.session), true);
      if (phase === "countdown") await h.fireNextTimer();
      h.session.activity_agent = agent;
      await h.fireNextTimer();
      assert.equal(h.timers.size, 0);
      assert.deepEqual(h.requests, []);
      assert.equal(h.sandbox.state.autoCompactLastSent.size, 0);
    });
  }
  test(`${agent} discovered after slash-command queuing blocks bytes`, async () => {
    const h = harness("Codex");
    assert.equal(h.sandbox.sendSlashCommand("/compact", { sessionId: "target" }), true);
    h.session.activity_agent = agent;
    await h.sandbox.mobileKeySendQueue;
    assert.deepEqual(h.inputs, []);
  });
}

for (const agent of ["Codex", "Claude"]) {
  test(`${agent} retains automatic compact support`, async () => {
    const h = harness(agent);
    assert.equal(h.sandbox.maybeAutoCompactCompletedSession(h.session), true);
    await h.fireNextTimer();
    await h.fireNextTimer();
    assert.equal(h.requests.length, 1);
    assert.equal(h.requests[0].body.command_line, "/compact");
    assert.equal(h.requests[0].body.session_id, "target");
    // Delivery is recorded; the compacting label itself comes from the
    // backend activity state, never from the send.
    assert.equal(h.sandbox.state.autoCompactLastSent.has("target"), true);
    assert.equal(h.sandbox.state.autoCompactStatusSessionIds.size, 0);
  });
}

test("the compacted marker follows the backend compacting state ending", async () => {
  const h = harness("Codex");
  assert.equal(h.sandbox.maybeAutoCompactCompletedSession(h.session), true);
  await h.fireNextTimer();
  await h.fireNextTimer();
  assert.equal(h.requests.length, 1);
  // The backend reports a live compaction progress line for this pane.
  h.session.activity_state = "compacting";
  h.sandbox.syncAutoCompactCompletionStatuses();
  assert.equal(h.sandbox.state.autoCompactStatusSessionIds.size, 0);
  // The progress line is gone: the compaction really ran and finished.
  h.session.activity_state = "completed";
  h.sandbox.syncAutoCompactCompletionStatuses();
  assert.equal(h.sandbox.state.autoCompactStatusSessionIds.has("target"), true);
});

test("a queued default compact command keeps its original session when focus changes", async () => {
  const h = harness("Codex");
  h.sandbox.state.activeSessionId = "target";
  h.sandbox.state.sessions.push({ id: "other", activity_agent: "CodeBuddy" });
  assert.equal(h.sandbox.sendSlashCommand("/compact"), true);
  h.sandbox.state.activeSessionId = "other";
  // Finish the confirmation delay without real terminal IO.
  await new Promise((resolve) => setImmediate(resolve));
  await h.fireNextTimer();
  await h.sandbox.mobileKeySendQueue;
  assert.deepEqual(h.inputs, [
    { id: "target", data: "/compact" },
    { id: "target", data: "\r" },
    { id: "target", data: "\r" },
  ]);
});

test("backend automated send boundaries share a non-configurable compact guard", () => {
  const manager = source("src/terminal/manager.rs");
  assert.match(manager, /fn send_terminal_slash_command\([^]*?ensure_session_compact_allowed\(session_id, command\)\?/);
  assert.match(manager, /pub fn send_session_input_silent\([^]*?ensure_session_compact_allowed\(session_id, &data\)\?/);
  const compact = manager.slice(manager.indexOf("fn send_session_auto_compact("), manager.indexOf("fn error_matches_previous_compact("));
  assert.match(compact, /terminal_compact_command_blocked_agent\(TERMINAL_COMPACT_COMMAND, &activity\)[^]*?NotEligible/);
  assert.doesNotMatch(compact, /SettingsManager|terminal_auto_compact_skip_codebuddy/);
  const html = source("static/index.html");
  assert.match(html, /id="terminal-auto-compact-skip-codebuddy-input"\s+type="checkbox"\s+checked\s+disabled/);
});
