import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const extractionSource = readFileSync(
  new URL("../static/terminal-preset-extraction.js", import.meta.url),
  "utf8",
);
const terminalStyles = readFileSync(
  new URL("../static/styles-terminal.css", import.meta.url),
  "utf8",
);

test("terminal tools menu starts with batch preset update and ends with single extraction", () => {
  const toolsMenuStart = terminalHtml.indexOf('id="terminal-tools-menu"');
  const fullAccessToggle = terminalHtml.indexOf('id="terminal-codex-full-access-toggle"', toolsMenuStart);
  const embeddedToolsStart = terminalHtml.indexOf('id="terminal-tool-menu"', toolsMenuStart);
  const nextMenuStart = terminalHtml.indexOf('id="terminal-command-collections-menu"', toolsMenuStart);
  const singleAction = terminalHtml.indexOf('id="extract-session-preset"');
  const batchAction = terminalHtml.indexOf('id="extract-all-session-presets"');

  assert.ok(toolsMenuStart >= 0);
  assert.ok(batchAction > toolsMenuStart);
  assert.ok(fullAccessToggle > batchAction);
  assert.ok(embeddedToolsStart > toolsMenuStart);
  assert.ok(singleAction > embeddedToolsStart);
  assert.ok(nextMenuStart > singleAction);
  assert.match(terminalHtml, /id="extract-session-preset"[\s\S]*?>命令提取预设<\/button>/);
  assert.match(terminalHtml, /id="extract-all-session-presets"[\s\S]*?>更新所有终端预设<\/button>/);
  assert.equal(terminalHtml.match(/id="extract-session-preset"/g)?.length, 1);
  assert.equal(terminalHtml.match(/id="extract-all-session-presets"/g)?.length, 1);
  assert.match(terminalHtml, /terminal-preset-extraction\.js\?v=/);
});

test("terminal tools menu uses all available viewport height before scrolling", () => {
  assert.match(
    terminalStyles,
    /\.terminal-tools-menu-with-tools \{[\s\S]*?max-height: calc\(100vh - 16px\);[\s\S]*?max-height: calc\(100dvh - 16px\);[\s\S]*?overflow-y: auto;/,
  );
  assert.doesNotMatch(terminalStyles, /max-height: min\(76vh, 600px\)/);
  assert.match(terminalHtml, /styles-terminal\.css\?v=20260804b/);
});

test("single extraction updates the matching session from the status provider", async () => {
  const calls = [];
  const buttons = {
    "extract-session-preset": { addEventListener() {} },
    "extract-all-session-presets": { addEventListener() {} },
  };
  const sandbox = {
    document: { getElementById: (id) => buttons[id] || null },
    state: {
      sessions: [{ id: "s1", name: "terminal_1", codex_api_preset_name: "old", codex_api_base_url: "old-url" }],
    },
    activeSession() {
      return sandbox.state.sessions[0];
    },
    async requestJson(url, options) {
      calls.push([url, options]);
      return {
        session_id: "s1",
        preset_name: "sub2api_gpt-5.6_1M",
        base_url: "http://192.168.3.2:18381/v1",
      };
    },
    renderSessions() {
      calls.push(["render"]);
    },
    updateStatus() {},
  };
  vm.createContext(sandbox);
  vm.runInContext(extractionSource, sandbox);

  const extracted = await sandbox.extractTerminalPresetByCommand("s1");
  assert.equal(extracted.preset_name, "sub2api_gpt-5.6_1M");
  assert.deepEqual(JSON.parse(JSON.stringify(sandbox.state.sessions[0])), {
    id: "s1",
    name: "terminal_1",
    codex_api_preset_name: "sub2api_gpt-5.6_1M",
    codex_api_base_url: "http://192.168.3.2:18381/v1",
  });
  assert.deepEqual(JSON.parse(JSON.stringify(calls)), [
    ["/api/terminal/sessions/s1/extract-preset", { method: "POST" }],
    ["render"],
  ]);
});

test("batch extraction reuses the single-session status endpoint for every terminal", async () => {
  const buttons = {
    "extract-session-preset": { disabled: false, addEventListener() {} },
    "extract-all-session-presets": { disabled: false, addEventListener() {} },
  };
  const calls = [];
  const statuses = [];
  const sandbox = {
    document: { getElementById: (id) => buttons[id] || null },
    state: {
      sessions: [
        { id: "s1", name: "one", codex_api_preset_name: "old" },
        { id: "s2", name: "two", codex_api_preset_name: "old" },
      ],
    },
    activeSession() {
      return sandbox.state.sessions[0];
    },
    async requestJson(url, options) {
      calls.push([url, options]);
      if (url === "/api/terminal/sessions?all=true") {
        return { sessions: sandbox.state.sessions.map((session) => ({ ...session })) };
      }
      const sessionId = url.match(/sessions\/([^/]+)\/extract-preset$/)?.[1];
      return { session_id: sessionId, preset_name: `preset-${sessionId}`, base_url: `https://${sessionId}.test` };
    },
    renderSessions() {},
    updateStatus(message, tone) {
      statuses.push([message, tone]);
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(extractionSource, sandbox);

  await sandbox.extractAllTerminalPresets();

  assert.deepEqual(JSON.parse(JSON.stringify(calls)), [
    ["/api/terminal/sessions?all=true", null],
    ["/api/terminal/sessions/s1/extract-preset", { method: "POST" }],
    ["/api/terminal/sessions/s2/extract-preset", { method: "POST" }],
  ]);
  assert.deepEqual(
    JSON.parse(JSON.stringify(sandbox.state.sessions.map((session) => session.codex_api_preset_name))),
    ["preset-s1", "preset-s2"],
  );
  assert.deepEqual(JSON.parse(JSON.stringify(statuses.at(-1))), ["已更新全部 2 个终端预设。", "ok"]);
});
