import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const mobileKeysSource = readFileSync(
  new URL("../static/terminal-mobile-keys.js", import.meta.url),
  "utf8",
);
const discussionSource = readFileSync(
  new URL("../static/terminal-multi-terminal-discussion.js", import.meta.url),
  "utf8",
);

test("project management exposes the multi-terminal discussion workflow", () => {
  assert.match(
    terminalHtml,
    /<option value="open_multi_terminal_discussion">群组讨论<\/option>/,
  );
  assert.match(
    terminalHtml,
    /data-project-action="open_multi_terminal_discussion">群组讨论<\/button>/,
  );
  assert.match(terminalHtml, /id="terminal-multi-terminal-discussion-dialog"/);
  assert.match(
    terminalHtml,
    /terminal-multi-terminal-discussion\.js\?v=/,
  );
  assert.match(
    mobileKeysSource,
    /action === "open_multi_terminal_discussion"[\s\S]*?openTerminalMultiTerminalDiscussionDialog\(\)/,
  );
});

function loadDiscussion(overrides = {}) {
  const listeners = new Map();
  const context = {
    console,
    crypto: { randomUUID: () => "discussion-id" },
    document: {
      getElementById: () => null,
    },
    state: { activeSessionId: "s1", currentPath: "project", sessions: [] },
    requestJson: async () => ({ submitted: true }),
    ...overrides,
  };
  context.window = context;
  vm.createContext(context);
  vm.runInContext(discussionSource, context);
  return { context, listeners };
}

test("discussion candidates stay inside the current project and require an Agent", () => {
  const { context } = loadDiscussion();
  const sessions = [
    { id: "s1", name: "lead", path: "project", activity_agent: "codex" },
    { id: "s2", name: "review", path: "project", activity_agent: "claude" },
    { id: "s3", name: "shell", path: "project", activity_agent: "" },
    { id: "s4", name: "other", path: "other", activity_agent: "codex" },
  ];
  const candidates = context.terminalMultiTerminalDiscussionCandidates(sessions, "project");
  assert.deepEqual(
    Array.from(candidates, (item) => item.id),
    ["s1", "s2"],
  );
});

test("discussion contract assigns roles and makes convergence evidence-driven and time-bounded", () => {
  const { context } = loadDiscussion();
  const participants = [
    { id: "s1", name: "lead" },
    { id: "s2", name: "review" },
  ];
  const common = {
    topic: "是否拆分当前模块？",
    participants,
    coordinator: participants[0],
    startedAt: "2026-08-29T10:00:00+08:00",
    deadline: "2026-08-29T10:15:00+08:00",
    timeboxMinutes: 15,
  };
  const coordinatorPrompt = context.terminalMultiTerminalDiscussionPrompt({
    ...common,
    recipient: participants[0],
  });
  const participantPrompt = context.terminalMultiTerminalDiscussionPrompt({
    ...common,
    recipient: participants[1],
  });

  for (const prompt of [coordinatorPrompt, participantPrompt]) {
    assert.match(prompt, /第一性原理/);
    assert.match(prompt, /用时间说话/);
    assert.match(prompt, /硬截止时间/);
    assert.match(prompt, /只读/);
    assert.match(prompt, /不修改文件、不构建、不部署/);
    assert.match(prompt, /不能用多数票代替证据/);
    assert.match(prompt, /事实与证据|可复现证据/);
    assert.match(prompt, /最终结论标准/);
    assert.match(prompt, /未决分歧/);
    assert.match(prompt, /终端消息/);
  }
  assert.match(coordinatorPrompt, /你是协调终端/);
  assert.match(coordinatorPrompt, /形成最终结论/);
  assert.match(participantPrompt, /你是参与终端/);
  assert.match(participantPrompt, /回复协调终端 lead/);
});

test("broadcast counts only explicitly verified submissions as delivered", async () => {
  const responses = new Map([
    ["s1", { ok: true, submitted: true }],
    ["s2", { ok: true, submitted: false }],
  ]);
  const calls = [];
  const { context } = loadDiscussion({
    requestJson: async (_url, options) => {
      const payload = JSON.parse(options.body);
      calls.push(payload);
      return responses.get(payload.target);
    },
  });
  const participants = [
    { id: "s1", name: "lead" },
    { id: "s2", name: "review" },
  ];
  const results = await context.broadcastTerminalDiscussion({
    topic: "讨论主题",
    participants,
    coordinator: participants[0],
    startedAt: "START",
    deadline: "DEADLINE",
    timeboxMinutes: 15,
  });

  assert.equal(results[0].submitted, true);
  assert.equal(results[1].submitted, false);
  assert.equal(calls.length, 2);
  assert.ok(calls.every((payload) => payload.verify_submission === true));
  assert.ok(calls.every((payload) => payload.submit === true));
  assert.notEqual(calls[0].delivery_id, calls[1].delivery_id);
});
