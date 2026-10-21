import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const advisorSource = readFileSync(
  new URL("../static/terminal-multi-model-advisor.js", import.meta.url),
  "utf8",
);

function loadAdvisor(overrides = {}) {
  const context = {
    TERMINAL_MULTI_MODEL_MAX_PRESETS: 3,
    TERMINAL_MULTI_MODEL_SYNTHESIS_INPUT_CHARS: 96 * 1024,
    TERMINAL_MULTI_MODEL_INLINE_MESSAGE_CHARS: 1200,
    document: { getElementById: () => null },
    requestJson: async () => ({}),
    state: { activeSessionId: "s1", currentPath: "project", workspaceDir: "/workspace" },
    ...overrides,
  };
  vm.createContext(context);
  vm.runInContext(advisorSource, context);
  return context;
}

test("advisor entry is group scoped and labeled for group advice", () => {
  assert.match(terminalHtml, /<option value="open_multi_model_advisor">群组建议<\/option>/);
  assert.match(terminalHtml, /data-project-action="open_multi_model_advisor">群组建议<\/button>/);
  assert.match(terminalHtml, /id="terminal-multi-model-advisor-title">群组建议<\/h2>/);
  assert.match(terminalHtml, /terminal-multi-model-advisor\.js\?v=/);
});

test("advisor reports use the existing workspace file API with a stable report path", async () => {
  const calls = [];
  const context = loadAdvisor({
    crypto: { randomUUID: () => "01234567-89ab-cdef" },
    executeSpecifiedPreset: async () => ({
      status: "succeeded",
      result: "结论",
      actual_model: "model-a",
    }),
    requestJson: async (url, options = {}) => {
      calls.push({ url, options });
      if (url === "/api/file") {
        return { ok: true, saved_bytes: 1 };
      }
      return {};
    },
  });
  const outcome = await context.runTerminalMultiModelAdvice({
    task: "评估拆分",
    presets: [{ id: "p1", name: "Model A" }],
    targets: [],
  });
  const reportCall = calls.find((item) => item.url === "/api/file");
  assert.ok(reportCall);
  assert.equal(reportCall.options.method, "PUT");
  const payload = JSON.parse(reportCall.options.body);
  assert.match(payload.path, /^\.webclx\/advisor-reports\/.*\.md$/);
  assert.match(payload.content, /# 评估拆分/);
  assert.match(payload.content, /Model A/);
  assert.equal(outcome.reportPath, payload.path);
});
