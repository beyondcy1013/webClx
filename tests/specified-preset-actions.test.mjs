import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const source = readFileSync(
  new URL("../static/specified-preset-actions.js", import.meta.url),
  "utf8",
);
const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");

function createHarness(responses = []) {
  const requests = [];
  const launches = [];
  const queued = [...responses];
  const sandbox = {
    URLSearchParams,
    encodeURIComponent,
    requestJson: async (url, options = {}) => {
      requests.push({ url, options });
      const next = queued.shift();
      if (next instanceof Error) throw next;
      return next ?? {};
    },
    openFreshTerminalSession: (cwd, options) => {
      launches.push({ cwd, options });
      return "launched";
    },
    window: { setTimeout: (callback) => callback() },
  };
  vm.createContext(sandbox);
  vm.runInContext(source, sandbox);
  return { sandbox, requests, launches };
}

test("temporary fork delegates the real config transaction to webclx run", async () => {
  const harness = createHarness();
  const result = await harness.sandbox.executeSpecifiedPreset({
    action: "launch",
    presetId: "preset-grok",
    cwd: "/home/codes/tools/webClx",
    temporary: true,
    command: "codex fork 019f2350-db5f-7cf0-b476-1cf14855b05d",
    terminalName: "webClx_QA_fork",
    quickStart: false,
  });

  assert.deepEqual(harness.requests, []);
  assert.deepEqual(JSON.parse(JSON.stringify(harness.launches)), [
    {
      cwd: "/home/codes/tools/webClx",
      options: {
        runCommand: "webclx run api 'preset-grok' -- codex fork 019f2350-db5f-7cf0-b476-1cf14855b05d",
        quickStart: false,
        terminalName: "webClx_QA_fork",
        codexApiPresetId: "",
        autoCompact: true,
      },
    },
  ]);
  assert.equal(result.launchResult, "launched");
  assert.equal(result.applied.temporary, true);
});

test("new terminal launch forwards the suffix for backend-assigned naming", async () => {
  const harness = createHarness();
  await harness.sandbox.executeSpecifiedPreset({
    action: "launch",
    presetId: "preset-gpt",
    cwd: "/home/codes/tools/webClx",
    temporary: true,
    sessionAction: "new",
    terminalName: "webClx_1_终端如何与deepseek agent通讯",
    terminalNameSuffix: "终端如何与deepseek agent通讯",
  });

  assert.equal(harness.launches[0].options.terminalNameSuffix, "终端如何与deepseek agent通讯");
});

test("the shared call submits and polls a Codex task with optional parameters", async () => {
  const created = { id: "ct-1", status: "queued" };
  const running = { id: "ct-1", status: "running" };
  const succeeded = { id: "ct-1", status: "succeeded", result: "done" };
  const harness = createHarness([created, running, succeeded]);
  const progress = [];
  const record = await harness.sandbox.executeSpecifiedPreset({
    action: "task",
    mode: "terminal",
    presetName: "Grok",
    cwd: "webClx",
    task: "检查项目",
    timeoutSecs: 60,
    outputSchema: { type: "object" },
    pollIntervalMs: 1,
    onProgress: (current) => progress.push(current.status),
  });

  assert.deepEqual(JSON.parse(harness.requests[0].options.body), {
    mode: "terminal",
    preset: { name: "Grok" },
    cwd: "webClx",
    task: "检查项目",
    timeout_secs: 60,
    output_schema: { type: "object" },
  });
  assert.deepEqual(progress, ["queued", "running", "succeeded"]);
  assert.equal(record.result, "done");
});

test("apply accepts optional project and proxy preference parameters", async () => {
  const harness = createHarness([{ id: "preset-1" }]);
  await harness.sandbox.executeSpecifiedPreset({
    action: "apply",
    presetId: "preset-1",
    projectPath: "/home/codes/tools/webClx",
    respectSavedProxyPreference: false,
  });
  assert.equal(
    harness.requests[0].url,
    "/api/auth/codex-accounts/api/preset-1/apply?project_path=%2Fhome%2Fcodes%2FwebClx&respect_saved_proxy_preference=false",
  );
});

test("a preset selector must contain exactly one lookup field", async () => {
  const harness = createHarness();
  await assert.rejects(
    harness.sandbox.executeSpecifiedPreset({
      action: "task",
      presetId: "preset-1",
      presetName: "Grok",
      task: "检查项目",
    }),
    /必须且只能/,
  );
});

test("temporary Codex resume uses webclx run and derives the terminal name", async () => {
  const harness = createHarness();
  await harness.sandbox.executeSpecifiedPreset({
    action: "launch",
    agent: "codex",
    presetId: "preset-grok",
    cwd: "/home/codes/tools/webClx",
    temporary: true,
    sessionAction: "resume",
    sessionId: "019f2350-db5f-7cf0-b476-1cf14855b05d",
    sourceTerminalName: "KTPro_3.1.6_5",
    task: "继续修复",
  });

  assert.deepEqual(JSON.parse(JSON.stringify(harness.launches[0])), {
    cwd: "/home/codes/tools/webClx",
    options: {
      runCommand: "webclx run api 'preset-grok' -- codex resume 019f2350-db5f-7cf0-b476-1cf14855b05d '继续修复'",
      quickStart: false,
      terminalName: "KTPro_3.1.6_5_resume",
      codexApiPresetId: "",
      autoCompact: true,
    },
  });
});

test("resume ignores model hints and relies on the applied config for Codex and Claude", () => {
  const harness = createHarness();
  const sessionId = "019f2350-db5f-7cf0-b476-1cf14855b05d";

  assert.equal(
    harness.sandbox.specifiedPresetLaunchCommand({
      agent: "codex",
      model: "gpt-5.4",
      sessionAction: "resume",
      sessionId,
    }),
    `codex resume ${sessionId}`,
  );
  assert.equal(
    harness.sandbox.specifiedPresetLaunchCommand({
      agent: "claude",
      model: "claude-opus-5[1m]",
      sessionAction: "resume",
      sessionId,
    }),
    `claude --resume ${sessionId}`,
  );
});

test("model hints never enter generated shell commands", () => {
  const harness = createHarness();
  assert.equal(
    harness.sandbox.specifiedPresetLaunchCommand({
      agent: "codex",
      model: "model'; echo unsafe",
      sessionAction: "resume",
      sessionId: "019f2350-db5f-7cf0-b476-1cf14855b05d",
    }),
    "codex resume 019f2350-db5f-7cf0-b476-1cf14855b05d",
  );
});

test("temporary Claude fork delegates the config transaction to webclx run", async () => {
  const harness = createHarness();
  await harness.sandbox.executeSpecifiedPreset({
    action: "launch",
    agent: "claude",
    presetId: "claude-preset",
    temporary: true,
    sessionAction: "fork",
    sessionId: "019f2350-db5f-7cf0-b476-1cf14855b05d",
    sourceTerminalName: "ClaudeWork",
  });

  assert.deepEqual(harness.requests, []);
  assert.equal(
    harness.launches[0].options.runCommand,
    "webclx run claude 'claude-preset' -- claude --resume 019f2350-db5f-7cf0-b476-1cf14855b05d --fork-session",
  );
  assert.equal(harness.launches[0].options.terminalName, "ClaudeWork_fork");
  assert.equal(harness.launches[0].options.codexApiPresetId, "");
});

test("temporary launch lets webclx run own apply, wait, and restore", async () => {
  const harness = createHarness();
  const result = await harness.sandbox.executeSpecifiedPreset({
    action: "launch",
    presetId: "preset-grok",
    temporary: true,
    command: "codex",
  });
  assert.equal(result.launchResult, "launched");
  assert.equal(harness.launches.length, 1);
  assert.equal(harness.launches[0].options.runCommand, "webclx run api 'preset-grok' -- codex");
  assert.deepEqual(harness.requests, []);
  assert.equal(result.applied.temporary, true);
});

test("explicit permanent launch applies shared config before direct Codex", async () => {
  const harness = createHarness();
  const result = await harness.sandbox.executeSpecifiedPreset({
    action: "launch",
    presetId: "preset-grok",
    command: "codex",
    temporary: false,
  });
  assert.equal(
    harness.requests[0].url,
    "/api/auth/codex-accounts/api/preset-grok/apply",
  );
  assert.equal(harness.requests[0].options.method, "PUT");
  assert.equal(
    harness.launches[0].options.runCommand,
    "codex",
  );
  assert.equal(result.launchResult, "launched");
});

test("a deferred permanent launch never starts an agent with stale config", async () => {
  const harness = createHarness([{
    deferred: true,
    name: "Pending",
    preset_id: "preset-grok",
  }]);
  const result = await harness.sandbox.executeSpecifiedPreset({
    action: "launch",
    presetId: "preset-grok",
    command: "codex",
    temporary: false,
  });

  assert.equal(harness.requests.length, 1);
  assert.deepEqual(harness.launches, []);
  assert.equal(result.launchResult, null);
  assert.equal(result.applied.deferred, true);
});

test("workspace loads the current shared preset launcher", () => {
  assert.match(indexHtml, /specified-preset-actions\.js\?v=20260928-cline-opencode/);
});

test("resume and fork reject malformed session IDs", async () => {
  const harness = createHarness();
  assert.throws(
    () => harness.sandbox.specifiedPresetLaunchCommand({
      agent: "codex",
      sessionAction: "resume",
      sessionId: "then",
    }),
    /有效的 session ID/,
  );
  await assert.rejects(
    harness.sandbox.executeSpecifiedPreset({
      action: "launch",
      presetId: "preset-1",
      sessionAction: "fork",
      sessionId: "bad; reboot",
    }),
    /有效的 session ID/,
  );
  assert.deepEqual(harness.requests, [], "invalid launch input must not apply a preset");
});

test("launch shell-quotes the optional initial task", () => {
  const harness = createHarness();
  assert.equal(
    harness.sandbox.specifiedPresetLaunchCommand({
      agent: "claude",
      task: "don't expand $HOME; echo bad",
    }),
    "claude 'don'\"'\"'t expand $HOME; echo bad'",
  );
});

// --- resolveSpecifiedPreset contract ---

test("resolveSpecifiedPreset by id matches exactly", () => {
  const harness = createHarness();
  const presets = [
    { id: "api-1", name: "Grok" },
    { id: "api-2", name: "MiniMax3" },
  ];
  const result = harness.sandbox.resolveSpecifiedPreset(presets, {
    selector: "api-1",
    match: "id",
  });
  assert.equal(result.id, "api-1");
});

test("resolveSpecifiedPreset by exact_name is case-insensitive", () => {
  const harness = createHarness();
  const presets = [
    { id: "api-1", name: "MiniMax3" },
    { id: "api-2", name: "minimax2" },
  ];
  const result = harness.sandbox.resolveSpecifiedPreset(presets, {
    selector: "minimax3",
    match: "exact_name",
  });
  assert.equal(result.id, "api-1");
});

test("resolveSpecifiedPreset by unique_contains prefers exact name", () => {
  const harness = createHarness();
  const presets = [
    { id: "api-1", name: "miniMax" },
    { id: "api-2", name: "miniMax2" },
    { id: "api-3", name: "Other" },
  ];
  const result = harness.sandbox.resolveSpecifiedPreset(presets, {
    selector: "miniMax",
    match: "unique_contains",
  });
  assert.equal(result.id, "api-1");
});

test("resolveSpecifiedPreset by unique_contains falls back to unique substring", () => {
  const harness = createHarness();
  const presets = [
    { id: "api-1", name: "MiniMax3" },
    { id: "api-2", name: "Grok" },
  ];
  const result = harness.sandbox.resolveSpecifiedPreset(presets, {
    selector: "miniMax",
    match: "unique_contains",
  });
  assert.equal(result.id, "api-1");
});

test("resolveSpecifiedPreset by unique_contains rejects ambiguity", () => {
  const harness = createHarness();
  const presets = [
    { id: "api-1", name: "MiniMax3" },
    { id: "api-2", name: "MiniMax2" },
  ];
  assert.throws(
    () => harness.sandbox.resolveSpecifiedPreset(presets, {
      selector: "minimax",
      match: "unique_contains",
    }),
    /多个/,
  );
});

test("resolveSpecifiedPreset throws on no match", () => {
  const harness = createHarness();
  assert.throws(
    () => harness.sandbox.resolveSpecifiedPreset([], {
      selector: "miniMax",
      match: "unique_contains",
    }),
    /没有找到/,
  );
});
