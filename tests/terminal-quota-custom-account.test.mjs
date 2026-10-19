// 套餐查询「指定账号」条目回归测试：
// 快速切换下拉除按平台自动过滤的预设外，还应单独提供一项用户指定的
// Codex_API 预设（不受平台域名过滤限制），选中即用该账号的
// api_key/base_url 查询；保存配置时把预设 id 持久化为 custom_preset_id。
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const quotaSource = readFileSync(
  new URL("../static/terminal-quota.js", import.meta.url),
  "utf8",
);

const PRESETS = [
  {
    id: "p-zhipu",
    name: "智谱主力",
    api_key: "zhipu-key",
    masked_api_key: "zhipu***",
    base_url: "https://open.bigmodel.cn/api/coding/paas/v4",
  },
  {
    id: "p-relay",
    name: "中继账号",
    api_key: "relay-key",
    masked_api_key: "relay***",
    // 非识别平台：平台自动过滤不会收它，但「指定账号」必须可用。
    base_url: "https://relay.example.com/v1",
  },
];

const makeKeySelect = () => ({ innerHTML: "", value: undefined, options: [] });

function loadQuotaModule(overrides = {}) {
  const keySelect = overrides.keySelect || makeKeySelect();
  const context = vm.createContext({
    URLSearchParams,
    console,
    requestJson: overrides.requestJson || (async () => { throw new Error("unexpected fetch"); }),
    window: { clearTimeout, setTimeout },
    terminalQuotaBodyEl: { innerHTML: "" },
    terminalQuotaKeyStatusEl: { textContent: "" },
    terminalQuotaKeySelectEl: keySelect,
    ...overrides.extraContext,
  });
  vm.runInContext(quotaSource, context);
  return { context, keySelect };
}

test("quota preset caches keep the full list for custom account resolution", async () => {
  const { context } = loadQuotaModule({
    requestJson: async (url) => {
      assert.equal(url, "/api/auth/api-presets");
      return { presets: PRESETS };
    },
  });
  const filtered = await vm.runInContext("fetchQuotaApiPresets()", context);
  // 平台过滤仍只收匹配预设；完整列表保留给「指定账号」解析。
  assert.equal(filtered.length, 1);
  assert.equal(filtered[0].id, "p-zhipu");
  assert.equal(vm.runInContext("quotaAllApiPresetsCache.length", context), 2);
  assert.equal(
    vm.runInContext("resolveQuotaCustomPreset('p-relay').name", context),
    "中继账号",
  );
  // 未指定 / 已删除的 id 解析为 null。
  assert.equal(vm.runInContext("resolveQuotaCustomPreset('')", context), null);
  assert.equal(vm.runInContext("resolveQuotaCustomPreset('p-gone')", context), null);
});

test("key dropdown gains a custom-account entry beyond platform presets", async () => {
  const { context, keySelect } = loadQuotaModule({
    requestJson: async () => ({ presets: PRESETS }),
  });
  await vm.runInContext("fetchQuotaApiPresets()", context);
  vm.runInContext(
    "quotaConfigCache = { custom_preset_id: 'p-relay' };" +
      "populateQuotaKeyDropdown(quotaApiPresetsCache, 'CUSTOM', 'p-relay')",
    context,
  );
  // 指定账号条目在已保存配置之后、平台预设之前，且被 CUSTOM 默认选中。
  assert.match(keySelect.innerHTML, /<option value="">已保存配置<\/option>/);
  const customAt = keySelect.innerHTML.indexOf('value="custom"');
  const presetAt = keySelect.innerHTML.indexOf('value="0"');
  assert.ok(customAt > 0 && customAt < presetAt, "custom entry should precede platform presets");
  assert.match(keySelect.innerHTML, /指定账号: 中继账号/);
  assert.equal(keySelect.value, "custom");
  // 选中「指定账号」时查询参数来自指定预设本身。
  // vm 跨 realm 对象原型不同，逐字段断言而不是 deepEqual。
  const params = vm.runInContext("selectedQuotaOverrideParams()", context);
  assert.equal(params.apiKey, "relay-key");
  assert.equal(params.baseUrl, "https://relay.example.com/v1");
});

test("custom-account entry disappears when the preset no longer resolves", async () => {
  const { context, keySelect } = loadQuotaModule({
    requestJson: async () => ({ presets: PRESETS }),
  });
  await vm.runInContext("fetchQuotaApiPresets()", context);
  vm.runInContext(
    "quotaConfigCache = { custom_preset_id: 'p-gone' };" +
      "populateQuotaKeyDropdown(quotaApiPresetsCache, 'CUSTOM', 'p-gone')",
    context,
  );
  assert.doesNotMatch(keySelect.innerHTML, /value="custom"/);
  assert.equal(keySelect.value, "");
});

test("saved GLM key config shows as a labeled entry even without any preset", async () => {
  const { context, keySelect } = loadQuotaModule({
    requestJson: async () => ({ presets: [] }),
  });
  await vm.runInContext("fetchQuotaApiPresets()", context);
  vm.runInContext(
    "quotaConfigCache = { base_url: 'https://open.bigmodel.cn'," +
      " api_key_masked: '741b***6hWC', custom_preset_id: null };",
    context,
  );
  // 默认显示=智谱但没有任何智谱预设：应实名展示并选中已保存配置。
  vm.runInContext("populateQuotaKeyDropdown(quotaApiPresetsCache, 'ZHIPU', null)", context);
  assert.match(
    keySelect.innerHTML,
    /已保存配置  \[智谱 GLM Coding Plan\]  741b\*\*\*6hWC/,
    "saved GLM key config should be labeled with platform and masked key",
  );
  assert.equal(keySelect.value, "");
  // 空值查询即用已保存配置：不带 override 参数。
  const params = vm.runInContext("selectedQuotaOverrideParams()", context);
  assert.equal(Object.keys(params).length, 0);
});

test("saved-entry fallback only applies when the platform matches", async () => {
  const minimaxPreset = {
    id: "p-minimax",
    name: "MiniMax3",
    api_key: "mm-key",
    masked_api_key: "mm***",
    base_url: "https://api.minimaxi.com/v1",
  };
  const { context, keySelect } = loadQuotaModule({
    requestJson: async () => ({ presets: [minimaxPreset] }),
  });
  await vm.runInContext("fetchQuotaApiPresets()", context);
  vm.runInContext(
    `quotaApiPresetsCache = ${JSON.stringify([minimaxPreset])};` +
      "quotaConfigCache = { base_url: 'https://open.bigmodel.cn', api_key_masked: '741b***6hWC' };",
    context,
  );
  // MiniMax 预设存在时默认显示 MiniMax 仍选中预设。
  vm.runInContext("populateQuotaKeyDropdown(quotaApiPresetsCache, 'MINIMAX', null)", context);
  assert.equal(keySelect.value, "0");
  // 默认显示智谱但预设里没有智谱：回退选中已保存的智谱 GLM key 配置。
  vm.runInContext("populateQuotaKeyDropdown(quotaApiPresetsCache, 'ZHIPU', null)", context);
  assert.equal(keySelect.value, "");
});

test("saving quota config persists the designated preset id and reselects it", async () => {
  const calls = [];
  const keySelect = makeKeySelect();
  const { context } = loadQuotaModule({
    keySelect,
    requestJson: async (url, opts) => {
      calls.push({ url, opts });
      if (url === "/api/auth/api-presets") return { presets: PRESETS };
      if (url === "/api/quota/config" && opts?.method === "PUT") {
        return {
          ok: true,
          api_key_masked: "zhipu***",
          base_url: "https://open.bigmodel.cn",
          default_provider: "CUSTOM",
          custom_preset_id: JSON.parse(opts.body).custom_preset_id,
        };
      }
      if (url.startsWith("/api/quota/query")) return { platform: "ZHIPU" };
      throw new Error(`unexpected url ${url}`);
    },
    extraContext: {
      terminalQuotaSaveConfigBtnEl: { disabled: false },
      terminalQuotaApiKeyInputEl: { value: "zhipu-key" },
      terminalQuotaBaseUrlInputEl: { value: "https://open.bigmodel.cn" },
      terminalQuotaDefaultProviderEl: { value: "CUSTOM", options: [{ value: "CUSTOM" }] },
      terminalQuotaCustomPresetSelectEl: { value: "p-relay", options: [{ value: "p-relay" }] },
      terminalQuotaConfigStatusEl: { textContent: "" },
    },
  });
  await vm.runInContext("fetchQuotaApiPresets()", context);
  await vm.runInContext("saveTerminalQuotaConfig()", context);

  const put = calls.find((c) => c.url === "/api/quota/config");
  assert.equal(put.opts.method, "PUT");
  assert.equal(JSON.parse(put.opts.body).custom_preset_id, "p-relay");
  // 保存后缓存同步、下拉复位到「指定账号」并用该账号重查。
  assert.equal(vm.runInContext("quotaConfigCache.custom_preset_id", context), "p-relay");
  assert.equal(keySelect.value, "custom");
  const query = calls.find((c) => c.url.startsWith("/api/quota/query"));
  assert.equal(query.url, "/api/quota/query?api_key=relay-key&base_url=https%3A%2F%2Frelay.example.com%2Fv1");
});
