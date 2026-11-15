import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalJs = readEntryScriptBundle("terminal.html");
const terminalCommonJs = readFileSync(new URL("../static/terminal-common.js", import.meta.url), "utf8");

test("terminal.html renders the model family toggle in the tools menu", () => {
  assert.match(
    terminalHtml,
    /id="session-agent-toggle"[\s\S]*?>\s*智能体[\s\S]*?id="session-model-family-toggle"[\s\S]*?>\s*大模型类型[\s\S]*?id="session-context-toggle"/,
    "terminal tools menu should include '大模型类型' toggle between '智能体' and '上下文'",
  );
  assert.match(
    terminalHtml,
    /<input id="session-model-family-toggle" type="checkbox" role="menuitemcheckbox" \/>/,
    "session-model-family-toggle must be a checkbox with role menuitemcheckbox",
  );
});

test("terminal.js binds storage, state, and toggle listener for session model family", () => {
  assert.match(
    terminalJs,
    /const TERMINAL_SESSION_MODEL_FAMILY_STORAGE_KEY = "webclx:terminal-session-model-family";/,
    "terminal.js must define the storage key constant",
  );
  assert.match(
    terminalJs,
    /const sessionModelFamilyToggleEl = document\.getElementById\("session-model-family-toggle"\)/,
    "terminal.js must bind sessionModelFamilyToggleEl",
  );
  assert.match(
    terminalJs,
    /showSessionModelFamily: readStoredTerminalSessionModelFamily\(\)/,
    "terminal state must initialize showSessionModelFamily",
  );
  assert.match(
    terminalJs,
    /sessionModelFamilyToggleEl\.addEventListener\("change"[\s\S]*state\.showSessionModelFamily = sessionModelFamilyToggleEl\.checked[\s\S]*storeTerminalSessionModelFamily\(state\.showSessionModelFamily\)[\s\S]*renderSessions\(\)/,
    "toggling model family checkbox should update state, storage, and re-render sessions",
  );
});

test("sessionModelFamilyLabel extracts model family accurately", () => {
  const sandbox = {};
  vm.createContext(sandbox);
  vm.runInContext(terminalCommonJs, sandbox);

  // 1. Explicit model_family from backend session probe
  assert.equal(sandbox.sessionModelFamilyLabel({ model_family: "deepseek" }), "deepseek");
  assert.equal(sandbox.sessionModelFamilyLabel({ modelFamily: "glm" }), "glm");
  assert.equal(sandbox.sessionModelFamilyLabel({ model_family: "GPT-6-Astra" }), "GPT-6-Astra");
  assert.equal(sandbox.sessionModelFamilyLabel({ model_family: "Gemini 3.8 Flash" }), "Gemini 3.8 Flash");
  assert.equal(sandbox.sessionModelFamilyLabel({ model_family: "deepseek-v4.1-flash" }), "deepseek-v4.1-flash");
  assert.equal(sandbox.sessionModelFamilyLabel({ model_family: "Kimi-K3" }), "Kimi-K3");

  // 2. Fallback to codex_status model
  assert.equal(sandbox.sessionModelFamilyLabel({ codex_status: { model: "DeepSeek-V3" } }), "deepseek");
  assert.equal(sandbox.sessionModelFamilyLabel({ codex_status: { model: "GLM-5.3" } }), "glm");
  assert.equal(sandbox.sessionModelFamilyLabel({ codex_status: { model: "Claude-3-7-Sonnet" } }), "claude");
  assert.equal(sandbox.sessionModelFamilyLabel({ codex_status: { model: "gpt-5.6-sol" } }), "gpt");
  assert.equal(sandbox.sessionModelFamilyLabel({ codex_status: { model: "gemini-2.5-pro" } }), "gemini");
  assert.equal(sandbox.sessionModelFamilyLabel({ codex_status: { model: "qwen-2.5-coder" } }), "qwen");
  assert.equal(sandbox.sessionModelFamilyLabel({ codex_status: { model: "kimi-k3" } }), "kimi");

  // 3. Fallback to session name
  assert.equal(sandbox.sessionModelFamilyLabel({ name: "测试-deepseek-工作区" }), "deepseek");
  assert.equal(sandbox.sessionModelFamilyLabel({ name: "前端-glm-开发" }), "glm");
  assert.equal(sandbox.sessionModelFamilyLabel({ name: "调研-sonnet" }), "claude");
  assert.equal(sandbox.sessionModelFamilyLabel({ name: "管理-openai-终端" }), "gpt");

  // 4. Unknown session returns empty string
  assert.equal(sandbox.sessionModelFamilyLabel({ id: "s1", name: "普通终端" }), "");
  assert.equal(sandbox.sessionModelFamilyLabel(null), "");
});

test("sessionOptionLabel includes model family when showSessionModelFamily is enabled", () => {
  const sandbox = {
    state: {
      showSessionDetails: false,
      showSessionAgent: false,
      showSessionModelFamily: true,
      showSessionContext: false,
    },
    sessionActivityAgentPrefix: () => "",
    sessionActivityPrefix: () => "",
    sessionActivityAgentSuffix: () => "",
    sessionActivityAgentLabel: () => "",
  };
  vm.createContext(sandbox);
  vm.runInContext(terminalCommonJs, sandbox);

  const sessionWithoutModel = { id: "s1", name: "终端1" };
  assert.equal(
    sandbox.sessionOptionLabel(sessionWithoutModel),
    "终端1 | 未记录",
    "should show 未记录 when model family is unknown",
  );

  const sessionWithModel = { id: "s1", name: "终端1", model_family: "deepseek" };
  assert.equal(
    sandbox.sessionOptionLabel(sessionWithModel),
    "终端1 | deepseek",
    "should show model family from session",
  );

  // When toggle is disabled, model family is hidden
  sandbox.state.showSessionModelFamily = false;
  assert.equal(
    sandbox.sessionOptionLabel(sessionWithModel),
    "终端1",
    "should hide model family when showSessionModelFamily is false",
  );

  // When combined with api details, agent details, and context
  sandbox.state.showSessionDetails = true;
  sandbox.state.showSessionAgent = true;
  sandbox.state.showSessionModelFamily = true;
  sandbox.state.showSessionContext = true;
  sandbox.state.sessionContextCache = new Map();
  sandbox.state.sessionContextCache.set("s1", {
    used_tokens: 59096,
    total_tokens: 258400,
  });
  sandbox.sessionActivityAgentLabel = () => "Codex";

  const fullSession = {
    id: "s1",
    name: "终端1",
    codex_api_preset_name: "gpt-6.1-sol",
    model_family: "deepseek",
  };
  assert.equal(
    sandbox.sessionOptionLabel(fullSession),
    "终端1 | gpt-6.1-sol | Codex | deepseek | 59.1K / 258.4K",
    "should combine api, agent, model family, and context details seamlessly",
  );
});

test("localStorage persistence helpers for session model family", () => {
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
    `const TERMINAL_SESSION_MODEL_FAMILY_STORAGE_KEY = "webclx:terminal-session-model-family";
     ${terminalJs.match(/function readStoredTerminalSessionModelFamily\(\) \{[\s\S]*?\n\}/)?.[0]}
     ${terminalJs.match(/function storeTerminalSessionModelFamily\(enabled\) \{[\s\S]*?\n\}/)?.[0]}`,
    sandbox,
  );

  assert.equal(sandbox.readStoredTerminalSessionModelFamily(), false);
  sandbox.storeTerminalSessionModelFamily(true);
  assert.equal(sandbox.readStoredTerminalSessionModelFamily(), true);
  assert.equal(storage.get("webclx:terminal-session-model-family"), "true");
  sandbox.storeTerminalSessionModelFamily(false);
  assert.equal(sandbox.readStoredTerminalSessionModelFamily(), false);
  assert.equal(storage.get("webclx:terminal-session-model-family"), "false");
});
