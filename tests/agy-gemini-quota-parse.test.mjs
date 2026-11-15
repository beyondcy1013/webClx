import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const agyManagerSource = readFileSync(new URL("../static/app-agy-manager.js", import.meta.url), "utf8");
const specifiedTaskSource = readFileSync(new URL("../static/terminal-specified-task.js", import.meta.url), "utf8");

// Mock DOM environment for app-agy-manager.js
class MockElement {
  constructor(tag) {
    this.tagName = tag.toUpperCase();
    this.children = [];
    this.classList = new Set();
    this.className = "";
    this.attributes = {};
    this.innerHTML = "";
    this.dataset = {};
  }
  set textContent(v) {
    this.innerHTML = v == null ? "" : String(v);
  }
  get textContent() {
    return this.innerHTML;
  }
  appendChild(child) {
    this.children.push(child);
    return child;
  }
  setAttribute(k, v) { this.attributes[k] = String(v); }
  getAttribute(k) { return this.attributes[k]; }
  insertAdjacentHTML(position, html) {
    this.innerHTML += html;
  }
}

const mockDocument = {
  getElementById: () => null,
  createElement: (tag) => new MockElement(tag),
};

const context = {
  document: mockDocument,
  localStorage: { getItem: () => null, setItem: () => {} },
  window: { localStorage: { getItem: () => null, setItem: () => {} } },
  fetch: async () => ({ ok: true, json: async () => ({}) }),
  console,
  setTimeout,
  clearTimeout,
  Date,
  Math,
  Number,
  String,
  Array,
  Object,
  JSON,
};

vm.createContext(context);
vm.runInContext(agyManagerSource, context);
vm.runInContext(specifiedTaskSource, context);

const agyManager = context.window.agyManager;
assert.ok(agyManager, "agyManager must be attached to window");

// Sample Google response where Gemini is only in models
const sampleModelsOnly = {
  ok: true,
  models: {
    "gemini-2.5-pro": {
      displayName: "Gemini 2.5 Pro",
      quotaInfo: {
        remainingFraction: 0.908,
        resetTime: "2026-11-12T18:00:00Z",
      },
    },
    "claude-opus-4-6-thinking": {
      displayName: "Claude Opus 4.6 (Thinking)",
      quotaInfo: {
        resetTime: "2026-11-18T23:42:21Z",
      },
    },
  },
  quotaSummary: {
    groups: [
      {
        displayName: "Claude and GPT models",
        description: "Models within this group: Claude Opus, Claude Sonnet, GPT-OSS",
        buckets: [
          {
            bucketId: "3p-weekly",
            window: "weekly",
            remainingFraction: 0.0,
            resetTime: "2026-11-18T23:42:21Z",
          },
          {
            bucketId: "3p-5h",
            window: "5h",
            remainingFraction: 0.5,
            resetTime: "2026-11-12T19:00:00Z",
          },
        ],
      },
    ],
  },
};

// Test renderRow with item using models fallback
const row = agyManager.renderRow({
  id: "agy-test",
  name: "Test Account",
  email: "test@example.com",
  valid: true,
  current: false,
  quota_cache: sampleModelsOnly,
  quota_cached_at: 1700000000,
});

assert.ok(row, "renderRow should return element");
assert.match(row.innerHTML, /Gemini/, "HTML must contain Gemini label");
assert.match(row.innerHTML, /Claude/, "HTML must contain Claude label");
assert.match(row.innerHTML, /90\.8%/, "HTML must display Gemini remaining percentage");
assert.match(row.innerHTML, /100\.0%/, "HTML must display Gemini weekly percentage");
assert.doesNotMatch(row.innerHTML, /Gemini[\s\S]*?没有数据/, "Gemini row must not say 没有数据 when models has quotaInfo");
assert.doesNotMatch(row.innerHTML, /Argon/, "Non-pro account without Argon must not render Argon row");

// Test account with Argon quota
const sampleWithArgon = {
  ok: true,
  models: {
    "gemini-2.5-pro": {
      displayName: "Gemini 2.5 Pro",
      quotaInfo: {
        remainingFraction: 0.854,
        resetTime: "2026-11-12T12:30:00Z",
      },
    },
    "gemini-4-argon": {
      displayName: "Gemini 4 Argon",
      quotaInfo: {
        remainingFraction: 0.4054,
        resetTime: "2026-11-19T07:24:00Z",
      },
    },
  },
  quotaSummary: {
    groups: [
      {
        displayName: "Gemini Models",
        description: "Models within this group: Gemini Flash, Gemini Pro",
        buckets: [
          {
            bucketId: "gemini-weekly",
            window: "weekly",
            remainingFraction: 0.9509,
            resetTime: "2026-11-19T07:30:00Z",
          },
          {
            bucketId: "gemini-5h",
            window: "5h",
            remainingFraction: 0.8540,
            resetTime: "2026-11-12T12:30:00Z",
          },
        ],
      },
      {
        displayName: "Claude and GPT models",
        description: "Models within this group: Claude Opus, Claude Sonnet, GPT-OSS",
        buckets: [
          {
            bucketId: "3p-weekly",
            window: "weekly",
            remainingFraction: 0.9905,
            resetTime: "2026-11-19T07:35:00Z",
          },
          {
            bucketId: "3p-5h",
            window: "5h",
            remainingFraction: 0.9810,
            resetTime: "2026-11-12T12:35:00Z",
          },
        ],
      },
      {
        displayName: "Gemini 4 Argon",
        description: "For paid Pro plans, Argon quota is separate. Higher Argon quota (included as part of the larger Gemini quota bucket) is available on Ultra",
        buckets: [
          {
            bucketId: "argon-weekly",
            displayName: "Gemini 4 Argon Weekly Limit",
            window: "weekly",
            remainingFraction: 0.4054,
            resetTime: "2026-11-19T07:24:00Z",
          },
        ],
      },
    ],
  },
};

const proRow = agyManager.renderRow({
  id: "agy-pro",
  name: "Pro Account",
  email: "beyondcy@gmail.com",
  valid: true,
  current: true,
  quota_cache: sampleWithArgon,
  quota_cached_at: 1700000000,
});

assert.ok(proRow, "proRow must exist");
assert.match(proRow.innerHTML, /Claude/, "Pro account HTML must contain Claude label");
assert.match(proRow.innerHTML, /Gemini/, "Pro account HTML must contain Gemini label");
assert.match(proRow.innerHTML, /Argon/, "Pro account HTML must contain Argon label");
assert.match(proRow.innerHTML, /95\.1%/, "Pro account HTML must display Gemini weekly percentage 95.1%");
assert.match(proRow.innerHTML, /85\.4%/, "Pro account HTML must display Gemini 5h percentage 85.4%");
assert.match(proRow.innerHTML, /40\.5%/, "Pro account HTML must display Argon weekly percentage 40.5%");

// Test account with Argon quota in pure CLI snake_case format
const sampleWithArgonCli = {
  ok: true,
  quotaSummary: {
    groups: [
      {
        name: "Gemini Models",
        description: "Models within this group: Gemini Flash, Gemini Pro",
        buckets: [
          {
            id: "gemini-weekly",
            name: "Weekly Limit Remaining",
            window: "weekly",
            remaining_fraction: 0.9509,
            reset_time: "2026-11-19T07:30:00Z",
          },
          {
            id: "gemini-5h",
            name: "Five Hour Limit Remaining",
            window: "5h",
            remaining_fraction: 0.8540,
            reset_time: "2026-11-12T12:30:00Z",
          },
        ],
      },
      {
        name: "Claude and GPT models",
        description: "Models within this group: Claude Opus, Claude Sonnet, GPT-OSS",
        buckets: [
          {
            id: "3p-weekly",
            name: "Weekly Limit Remaining",
            window: "weekly",
            remaining_fraction: 0.9905,
            reset_time: "2026-11-19T07:35:00Z",
          },
          {
            id: "3p-5h",
            name: "Five Hour Limit Remaining",
            window: "5h",
            remaining_fraction: 0.9810,
            reset_time: "2026-11-12T12:35:00Z",
          },
        ],
      },
      {
        name: "Gemini 4 Argon",
        description: "For paid Pro plans, Argon quota is separate.",
        buckets: [
          {
            id: "gemini-argon-wtu-weekly",
            name: "Gemini 4 Argon Weekly Limit",
            window: "weekly",
            remaining_fraction: 0.066,
            reset_time: "2026-11-19T07:16:18Z",
          },
        ],
      },
    ],
  },
};

const cliRow = agyManager.renderRow({
  id: "agy-cli",
  name: "CLI Account",
  email: "cli@gmail.com",
  valid: true,
  current: true,
  quota_cache: sampleWithArgonCli,
  quota_cached_at: 1700000000,
});

assert.ok(cliRow, "cliRow must exist");
assert.match(cliRow.innerHTML, /Claude/, "CLI row must contain Claude label");
assert.match(cliRow.innerHTML, /Gemini/, "CLI row must contain Gemini label");
assert.match(cliRow.innerHTML, /Argon/, "CLI row must contain Argon label");
assert.match(cliRow.innerHTML, /6\.6%/, "CLI row must display Argon 6.6%");

// Test terminalSpecifiedTaskQuotaPercent
const quotaPercent = vm.runInContext("terminalSpecifiedTaskQuotaPercent", context)(sampleModelsOnly);
assert.equal(quotaPercent, 0, "When 3p-weekly is 0, minimum fraction is 0%");

const sampleGeminiOnly = {
  ok: true,
  models: {
    "gemini-2.5-pro": {
      quotaInfo: {
        remainingFraction: 0.85,
        resetTime: "2026-11-12T18:00:00Z",
      },
    },
  },
};
const geminiOnlyPercent = vm.runInContext("terminalSpecifiedTaskQuotaPercent", context)(sampleGeminiOnly);
assert.equal(geminiOnlyPercent, 85, "Gemini-only fallback should return 85%");

// Test terminalSpecifiedTaskAgyQuotaLabel multi-quota formatting
const agyQuotaLabel = vm.runInContext("terminalSpecifiedTaskAgyQuotaLabel", context);
const modelsOnlyLabel = agyQuotaLabel(sampleModelsOnly);
assert.match(modelsOnlyLabel, /Claude/, "modelsOnlyLabel must display Claude");
assert.match(modelsOnlyLabel, /Gemini/, "modelsOnlyLabel must display Gemini");

const argonLabel = agyQuotaLabel(sampleWithArgon);
assert.match(argonLabel, /Claude 周99\.1% 5h98\.1%/, "argonLabel must display Claude weekly and 5h");
assert.match(argonLabel, /Gemini 周95\.1% 5h85\.4%/, "argonLabel must display Gemini weekly and 5h");
assert.match(argonLabel, /Argon 周40\.5%/, "argonLabel must display Argon weekly");

const argonCliLabel = agyQuotaLabel(sampleWithArgonCli);
assert.match(argonCliLabel, /Claude 周99\.1% 5h98\.1%/, "argonCliLabel must display Claude weekly and 5h");
assert.match(argonCliLabel, /Gemini 周95\.1% 5h85\.4%/, "argonCliLabel must display Gemini weekly and 5h");
assert.match(argonCliLabel, /Argon 周6\.6%/, "argonCliLabel must display Argon weekly 6.6%");

// Test terminalSpecifiedTaskAccountPreset sets full multi-quota label for AGY
const accountPresetFn = vm.runInContext("terminalSpecifiedTaskAccountPreset", context);
const accountOptionTextFn = vm.runInContext("terminalSpecifiedTaskAccountOptionText", context);

const agyPreset = accountPresetFn({
  id: "agy-test-multi",
  name: "test-user",
  current: true,
  quota_cache: sampleWithArgonCli,
  quota_cached_at: 1700000000,
}, "agy");

assert.ok(agyPreset.creditsLabel, "creditsLabel must be populated");
assert.match(agyPreset.creditsLabel, /Claude/, "creditsLabel must contain Claude");
assert.match(agyPreset.creditsLabel, /Gemini/, "creditsLabel must contain Gemini");
assert.match(agyPreset.creditsLabel, /Argon/, "creditsLabel must contain Argon");

const optionText = accountOptionTextFn(agyPreset, "agy", Date.now());
assert.match(optionText, /Claude/, "optionText must contain Claude quota");
assert.match(optionText, /Gemini/, "optionText must contain Gemini quota");
assert.match(optionText, /Argon/, "optionText must contain Argon quota");

console.log("AGY Gemini quota parsing unit tests passed successfully!");
