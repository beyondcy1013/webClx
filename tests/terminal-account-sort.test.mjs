import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const source = readFileSync(new URL("../static/terminal-specified-task.js", import.meta.url), "utf8");

function harness({ agent = "codebuddy", accounts, sortMode = "credits" }) {
  const options = [];
  const preset = {
    value: "",
    dataset: {},
    get selectedOptions() {
      return options.filter((option) => option.value === this.value);
    },
    replaceChildren() {
      options.length = 0;
      this.value = "";
    },
    append(option) {
      options.push(option);
      if (options.length === 1 || option.selected) this.value = option.value;
    },
  };
  const context = vm.createContext({
    document: { createElement: () => ({ dataset: {} }) },
    localStorage: { getItem: () => null, setItem: () => {} },
    state: { sessions: [], currentPath: "/workspace", terminalAccountSort: sortMode },
    specifiedPresetModel: () => "",
    requestJson: async () => accounts,
    executeSpecifiedPreset: async () => ({ launchResult: { id: "new-terminal" } }),
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
  return { context, preset, options };
}

function codebuddy(id, { active = false, credits = 0, lastSwitchAt = 0, switchCount = 0 } = {}) {
  return {
    id,
    nickname: id,
    active,
    switchCount,
    lastSwitchAt,
    credits: credits === null ? undefined : { totalRemaining: credits, updatedAt: 1 },
  };
}

const codebuddyAccounts = [
  codebuddy("low", { credits: 10 }),
  codebuddy("current", { active: true, credits: 5, lastSwitchAt: 1000 }),
  codebuddy("high", { credits: 900, lastSwitchAt: 3000, switchCount: 7 }),
  codebuddy("unknown", { credits: null, lastSwitchAt: 2000, switchCount: 2 }),
];

test("current CodeBuddy account stays first and the rest sort by remaining credits", async () => {
  const h = harness({ accounts: codebuddyAccounts });
  await h.context.loadTerminalSpecifiedTaskPresets();
  assert.deepEqual(h.options.map((option) => option.value), [
    "current",
    "high",
    "low",
    "unknown",
  ]);
  assert.equal(h.preset.value, "current");
  assert.match(h.options[1].textContent, /积分 900/);
  assert.match(h.options[0].textContent, /^当前 · current · 积分 5/);
  assert.match(h.options[3].textContent, /积分未知/);
});

test("sort mode from settings controls the non-current order", async () => {
  const bySwitch = harness({ accounts: codebuddyAccounts, sortMode: "recent_switch" });
  await bySwitch.context.loadTerminalSpecifiedTaskPresets();
  assert.deepEqual(bySwitch.options.map((option) => option.value), [
    "current",
    "high",
    "unknown",
    "low",
  ]);

  const byCount = harness({ accounts: codebuddyAccounts, sortMode: "switch_count" });
  await byCount.context.loadTerminalSpecifiedTaskPresets();
  assert.deepEqual(byCount.options.map((option) => option.value), [
    "current",
    "high",
    "unknown",
    "low",
  ]);

  const byName = harness({ accounts: codebuddyAccounts, sortMode: "name" });
  await byName.context.loadTerminalSpecifiedTaskPresets();
  assert.deepEqual(byName.options.map((option) => option.value), [
    "current",
    "high",
    "low",
    "unknown",
  ]);
});

test("unknown sort mode falls back to credits", async () => {
  const h = harness({ accounts: codebuddyAccounts, sortMode: "not-a-mode" });
  await h.context.loadTerminalSpecifiedTaskPresets();
  assert.equal(h.context.terminalSpecifiedTaskAccountSort(), "credits");
  assert.deepEqual(h.options.map((option) => option.value), [
    "current",
    "high",
    "low",
    "unknown",
  ]);
});

test("Codex accounts pin the current one and fall back to last switch time without credits", async () => {
  const accounts = {
    accounts: [
      { kind: "api", id: "a", name: "A", active: false, switch_count: 9, last_switch_at: 1000, favorite: false },
      { kind: "oauth", id: "b", name: "B", active: true, switch_count: 1, last_switch_at: 500, favorite: false },
      { kind: "api", id: "c", name: "C", active: false, switch_count: 0, last_switch_at: 9000, favorite: true },
    ],
  };
  const h = harness({ agent: "codex", accounts });
  await h.context.loadTerminalSpecifiedTaskPresets();
  assert.deepEqual(h.options.map((option) => option.value), ["b", "c", "a"]);
  assert.match(h.options[0].textContent, /^\[OAuth\] 当前 · B/);
  assert.match(h.options[2].textContent, /上次切换/);

  const byCount = harness({ agent: "codex", accounts, sortMode: "switch_count" });
  await byCount.context.loadTerminalSpecifiedTaskPresets();
  assert.deepEqual(byCount.options.map((option) => option.value), ["b", "a", "c"]);
});

test("AGY accounts sort by cached quota percent and keep the current one first", async () => {
  const account = (id, percent, { current = false, cachedAt = 10 } = {}) => ({
    id,
    name: id,
    current,
    quota_cached_at: cachedAt,
    quota_cache: {
      ok: true,
      quotaSummary: {
        groups: [
          {
            buckets: [
              { bucketId: "3p-weekly", window: "weekly", remainingFraction: percent },
              { bucketId: "gemini-weekly", remainingFraction: 1 },
            ],
          },
        ],
      },
    },
  });
  const accounts = [account("low", 0.1), account("current", 0.2, { current: true }), account("high", 0.9)];
  const h = harness({ agent: "agy", accounts });
  await h.context.loadTerminalSpecifiedTaskPresets();
  assert.deepEqual(h.options.map((option) => option.value), ["current", "high", "low"]);
  assert.match(h.options[1].textContent, /额度 90%/);
});

test("Devin accounts pin the current one and show the last switch distance", async () => {
  const accounts = [
    { id: "old", name: "old", active: false, last_switch_at: 1000 },
    { id: "current", name: "current", active: true, last_switch_at: 2000 },
  ];
  const h = harness({ agent: "devin", accounts, sortMode: "recent_switch" });
  await h.context.loadTerminalSpecifiedTaskPresets();
  assert.deepEqual(h.options.map((option) => option.value), ["current", "old"]);
  assert.equal(h.preset.value, "current");
  assert.match(h.options[1].textContent, /上次切换/);
});
