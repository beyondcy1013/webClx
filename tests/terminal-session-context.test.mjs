import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalJs = readEntryScriptBundle("terminal.html");
const terminalCommonJs = readFileSync(new URL("../static/terminal-common.js", import.meta.url), "utf8");

test("terminal.html renders the context size toggle in the tools menu", () => {
  assert.match(
    terminalHtml,
    /id="session-agent-toggle"[\s\S]*?>\s*智能体[\s\S]*?id="session-context-toggle"[\s\S]*?>\s*上下文[\s\S]*?id="session-auto-continue-toggle"/,
    "terminal tools menu should include '上下文' toggle directly after '智能体'",
  );
  assert.match(
    terminalHtml,
    /<input id="session-context-toggle" type="checkbox" role="menuitemcheckbox" \/>/,
    "session-context-toggle must be a checkbox with role menuitemcheckbox",
  );
});

test("terminal.js binds storage, state, and toggle listener for session context", () => {
  assert.match(
    terminalJs,
    /const TERMINAL_SESSION_CONTEXT_STORAGE_KEY = "webclx:terminal-session-context";/,
    "terminal.js must define the storage key constant",
  );
  assert.match(
    terminalJs,
    /const sessionContextToggleEl = document\.getElementById\("session-context-toggle"\);/,
    "terminal.js must bind sessionContextToggleEl",
  );
  assert.match(
    terminalJs,
    /showSessionContext: readStoredTerminalSessionContext\(\)/,
    "terminal state must initialize showSessionContext",
  );
  assert.match(
    terminalJs,
    /sessionContextCache: new Map\(\)/,
    "terminal state must initialize sessionContextCache",
  );
  assert.match(
    terminalJs,
    /sessionContextToggleEl\.addEventListener\("change"[\s\S]*state\.showSessionContext = sessionContextToggleEl\.checked[\s\S]*storeTerminalSessionContext\(state\.showSessionContext\)[\s\S]*renderSessions\(\)/,
    "toggling context checkbox should update state, storage, and re-render sessions",
  );
});

test("formatTerminalTokenCount formats tokens properly", () => {
  const sandbox = {};
  vm.createContext(sandbox);
  vm.runInContext(terminalCommonJs, sandbox);

  assert.equal(sandbox.formatTerminalTokenCount(0), "0");
  assert.equal(sandbox.formatTerminalTokenCount(500), "500");
  assert.equal(sandbox.formatTerminalTokenCount(1000), "1K");
  assert.equal(sandbox.formatTerminalTokenCount(59096), "59.1K");
  assert.equal(sandbox.formatTerminalTokenCount(258400), "258.4K");
  assert.equal(sandbox.formatTerminalTokenCount(950000), "950K");
  assert.equal(sandbox.formatTerminalTokenCount(1000000), "1M");
  assert.equal(sandbox.formatTerminalTokenCount(1250000), "1.25M");
  assert.equal(sandbox.formatTerminalTokenCount(-5), "");
  assert.equal(sandbox.formatTerminalTokenCount(null), "");
  assert.equal(sandbox.formatTerminalTokenCount(undefined), "");
});

test("sessionOptionLabel includes context size when showSessionContext is enabled", () => {
  const sandbox = {
    state: {
      showSessionDetails: false,
      showSessionAgent: false,
      showSessionContext: true,
      sessionContextCache: new Map(),
    },
    sessionActivityAgentPrefix: () => "",
    sessionActivityPrefix: () => "",
    sessionActivityAgentSuffix: () => "",
    sessionActivityAgentLabel: () => "",
  };
  vm.createContext(sandbox);
  vm.runInContext(terminalCommonJs, sandbox);

  const sessionWithoutContext = { id: "s1", name: "终端1" };
  assert.equal(
    sandbox.sessionOptionLabel(sessionWithoutContext),
    "终端1 | 未记录",
    "should show 未记录 when no context is cached",
  );

  sandbox.state.sessionContextCache.set("s1", {
    used_tokens: 59096,
    total_tokens: 258400,
  });
  assert.equal(
    sandbox.sessionOptionLabel(sessionWithoutContext),
    "终端1 | 59.1K / 258.4K",
    "should show used / total tokens from cache",
  );

  // When context toggle is disabled, suffix is hidden
  sandbox.state.showSessionContext = false;
  assert.equal(
    sandbox.sessionOptionLabel(sessionWithoutContext),
    "终端1",
    "should hide context detail when showSessionContext is false",
  );

  // When combined with api details and agent details
  sandbox.state.showSessionDetails = true;
  sandbox.state.showSessionAgent = true;
  sandbox.state.showSessionContext = true;
  const fullSession = {
    id: "s1",
    name: "终端1",
    codex_api_preset_name: "gpt-6.1-sol",
  };
  sandbox.sessionActivityAgentLabel = () => "Codex";
  assert.equal(
    sandbox.sessionOptionLabel(fullSession),
    "终端1 | gpt-6.1-sol | Codex | 59.1K / 258.4K",
    "should combine api, agent, and context details seamlessly",
  );

  // When only used_tokens is available without total_tokens
  sandbox.state.sessionContextCache.set("s2", { used_tokens: 3500 });
  assert.equal(
    sandbox.sessionOptionLabel({ id: "s2", name: "终端2", codex_api_preset_name: "gpt-6.1-sol" }),
    "终端2 | gpt-6.1-sol | Codex | 3.5K",
    "should format only used tokens when total_tokens is omitted",
  );
});

test("localStorage persistence helpers for session context", () => {
  const storage = new Map();
  const sandbox = {
    window: {
      localStorage: {
        getItem: (k) => (storage.has(k) ? storage.get(k) : null),
        setItem: (k, v) => storage.set(k, String(v)),
      },
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(
    `const TERMINAL_SESSION_CONTEXT_STORAGE_KEY = "webclx:terminal-session-context";
     ${terminalJs.match(/function readStoredTerminalSessionContext\(\) \{[\s\S]*?\n\}/)?.[0]}
     ${terminalJs.match(/function storeTerminalSessionContext\(enabled\) \{[\s\S]*?\n\}/)?.[0]}
    `,
    sandbox,
  );

  assert.equal(sandbox.readStoredTerminalSessionContext(), false, "default is false");
  sandbox.storeTerminalSessionContext(true);
  assert.equal(sandbox.readStoredTerminalSessionContext(), true, "reads true after setting true");
  sandbox.storeTerminalSessionContext(false);
  assert.equal(sandbox.readStoredTerminalSessionContext(), false, "reads false after setting false");
});

