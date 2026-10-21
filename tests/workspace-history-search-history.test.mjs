import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const html = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const script = readFileSync(new URL("../static/app-workspace-history.js", import.meta.url), "utf8");

function functionSource(name) {
  const start = script.indexOf(`function ${name}(`);
  assert.notEqual(start, -1, `missing function ${name}`);
  const bodyStart = script.indexOf("{", start);
  let depth = 0;
  for (let index = bodyStart; index < script.length; index += 1) {
    if (script[index] === "{") depth += 1;
    if (script[index] === "}") depth -= 1;
    if (depth === 0) return script.slice(start, index + 1);
  }
  assert.fail(`unterminated function ${name}`);
}

function createHarness(initial = []) {
  const values = new Map([["webclx:workspace-history-search-history", JSON.stringify(initial)]]);
  const options = [];
  const datalist = {
    textContent: "",
    appendChild(option) {
      options.push(option.value);
    },
  };
  const context = vm.createContext({
    MAX_WORKSPACE_HISTORY_SEARCH_HISTORY: 12,
    WORKSPACE_HISTORY_SEARCH_HISTORY_STORAGE_KEY: "webclx:workspace-history-search-history",
    window: { localStorage: {
      getItem(key) { return values.get(key) || null; },
      setItem(key, value) { values.set(key, value); },
    } },
    document: {
      createElement() { return {}; },
    },
    workspaceHistorySearchHistoryEl: datalist,
  });
  vm.runInContext(
    `${functionSource("readWorkspaceHistorySearchHistory")}\n${functionSource("renderWorkspaceHistorySearchHistory")}\n${functionSource("rememberWorkspaceHistorySearch")}`,
    context,
  );
  return { context, values, options };
}

test("workspace history search keeps recent unique terms and renders datalist options", () => {
  assert.match(html, /id="workspace-history-search-input"[\s\S]*list="workspace-history-search-history"/);
  assert.match(html, /<datalist id="workspace-history-search-history"><\/datalist>/);
  assert.doesNotMatch(html, /id="workspace-history-search-input"[^>]*type="select"/);

  const harness = createHarness(["older", "latest", "older"]);
  harness.context.renderWorkspaceHistorySearchHistory();
  assert.deepEqual(harness.options, ["older", "latest"]);

  harness.context.rememberWorkspaceHistorySearch(" latest ");
  assert.deepEqual(
    JSON.parse(harness.values.get("webclx:workspace-history-search-history")),
    ["latest", "older"],
  );
});

test("workspace history search ignores blank terms", () => {
  const harness = createHarness(["existing"]);
  harness.context.rememberWorkspaceHistorySearch("  ");
  assert.deepEqual(
    JSON.parse(harness.values.get("webclx:workspace-history-search-history")),
    ["existing"],
  );
});
