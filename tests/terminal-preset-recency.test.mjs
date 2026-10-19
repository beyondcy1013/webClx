import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const source = readFileSync(new URL("../static/terminal-specified-task.js", import.meta.url), "utf8");

function harness(storage = new Map()) {
  const options = [];
  const preset = {
    value: "", dataset: {},
    get selectedOptions() { return options.filter((option) => option.value === this.value); },
    replaceChildren() { options.length = 0; this.value = ""; },
    append(option) {
      options.push(option);
      if (options.length === 1 || option.selected) this.value = option.value;
    },
  };
  let agent = "codex";
  let launch = async () => ({ launchResult: { id: "new-terminal" } });
  const context = vm.createContext({
    document: { createElement: () => ({ dataset: {} }) },
    localStorage: { getItem: (key) => storage.get(key) ?? null, setItem: (key, value) => storage.set(key, value) },
    state: { sessions: [], currentPath: "/workspace" },
    specifiedPresetModel: () => "",
    requestJson: async () => ({ accounts: [
      { id: "favorite", kind: "api", favorite: true },
      { id: "a", kind: "api", active: true },
      { id: "b", kind: "api" },
      { id: "c", kind: "oauth" },
    ] }),
    executeSpecifiedPreset: async (options) => launch(options),
    launchTerminalSpecifiedPreset() {},
  });
  vm.runInContext(source, context);
  Object.assign(context, {
    terminalSpecifiedTaskDom: () => ({ preset, task: { value: "" } }),
    terminalSpecifiedTaskAgent: () => agent,
    terminalSpecifiedTaskMode: () => "fixed",
    terminalSpecifiedTaskSessionAction: () => "new",
    terminalSpecifiedTaskTemporary: () => true,
    terminalSpecifiedTaskFinalTerminalName: () => "",
    terminalSpecifiedTaskSelectedGroup: () => "",
    terminalSpecifiedTaskSelectedGroupRole: () => "",
    terminalSpecifiedTaskCreateGroup: () => false,
    terminalSpecifiedTaskCreateGroupCount: () => 0,
    setTerminalSpecifiedTaskBusy() {},
    setTerminalSpecifiedTaskStatus() {},
    reportTerminalSpecifiedTaskPageStatus() {},
    closeTerminalSpecifiedTaskDialog() {},
  });
  return { context, storage, preset, options, setAgent(value) { agent = value; }, setLaunch(value) { launch = value; } };
}

test("successful launches move presets to the front, deduplicate, and survive a page reload", async () => {
  const h = harness();
  await h.context.loadTerminalSpecifiedTaskPresets();
  assert.deepEqual(h.options.map((option) => option.value), ["favorite", "a", "b", "c"]);
  for (const id of ["a", "b", "c", "a"]) {
    h.preset.value = id;
    await h.context.submitTerminalSpecifiedTask();
  }
  const refreshed = harness(h.storage);
  await refreshed.context.loadTerminalSpecifiedTaskPresets();
  assert.deepEqual(refreshed.options.map((option) => option.value), ["a", "c", "b", "favorite"]);
  assert.equal(refreshed.preset.value, "a");
  assert.equal(refreshed.preset.dataset.activeId, "a");
  assert.equal(JSON.parse([...h.storage.values()][0]).length, 3);
});

test("a choice alone, launch rejection, or deferred launch does not record usage", async () => {
  const h = harness();
  await h.context.loadTerminalSpecifiedTaskPresets();
  h.preset.value = "b";
  await h.context.loadTerminalSpecifiedTaskPresets();
  assert.equal(h.storage.size, 0);
  h.setLaunch(async () => { throw new Error("launch failed"); });
  await h.context.submitTerminalSpecifiedTask();
  assert.equal(h.storage.size, 0);
  h.setLaunch(async () => ({ applied: { deferred: true }, launchResult: null }));
  await h.context.submitTerminalSpecifiedTask();
  assert.equal(h.storage.size, 0);
});

test("Agent and OAuth/API identities are separate and deleted presets do not occupy a position", async () => {
  const h = harness();
  h.context.terminalSpecifiedTaskRememberPreset("claude", "b", "claude");
  h.context.terminalSpecifiedTaskRememberPreset("codex", "a", "api");
  h.context.terminalSpecifiedTaskRememberPreset("codex", "a", "oauth");
  h.context.terminalSpecifiedTaskRememberPreset("codex", "deleted", "api");
  const candidates = [{ id: "a", kind: "api" }, { id: "a", kind: "oauth" }, { id: "b", kind: "api" }];
  assert.deepEqual(Array.from(h.context.terminalSpecifiedTaskSortPresets(candidates, "codex")), [candidates[1], candidates[0], candidates[2]]);
  await h.context.loadTerminalSpecifiedTaskPresets();
  assert.equal(h.preset.value, "a");
  assert.equal(h.options[0].value, "a");
});

test("unavailable or malformed browser storage leaves launches usable", async () => {
  const h = harness(new Map([["webclx.new-terminal.recent-presets.codex", "{broken"]]));
  await h.context.loadTerminalSpecifiedTaskPresets();
  h.context.localStorage = { getItem() { throw new Error("blocked"); }, setItem() { throw new Error("blocked"); } };
  h.preset.value = "b";
  await h.context.submitTerminalSpecifiedTask();
  const sorted = h.context.terminalSpecifiedTaskSortPresets([{ id: "a", kind: "api" }, { id: "b", kind: "api" }], "codex");
  assert.deepEqual(Array.from(sorted, (item) => item.id), ["b", "a"]);
});
