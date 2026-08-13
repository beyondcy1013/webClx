// 预设状态格式化与 API/Claude/Auth 预设 CRUD 操作模块。
// 由 app.js 拆出，在 app.js 之前以 <script defer> 加载，
// 通过共享全局作用域向 app.js 提供下列函数，无需修改调用方。
// 依赖的全局（apiManager、claudeManager、authManager 等）均为
// app.js 顶层声明，加载顺序保证可用。

function formatCurrentApiSummary(currentApi, modeLabel) {
  return [
    modeLabel,
    currentApi.preset_name ? `预设 ${currentApi.preset_name}` : null,
    currentApi.provider_name ? `名字 ${currentApi.provider_name}` : null,
    currentApi.base_url ? `Base ${currentApi.base_url}` : null,
    currentApi.management_url ? `管理 ${currentApi.management_url}` : null,
    currentApi.masked_api_key ? `Key ${currentApi.masked_api_key}` : null,
    currentApi.wire_api ? `Wire ${currentApi.wire_api}` : null,
  ].filter(Boolean).join(" · ");
}

function formatCurrentApiHeadline(response) {
  if (response.current_api) {
    return [
      response.current_mode === "auth" ? "当前仍是登录 Auth 模式" : "当前处于 API 模式",
      response.current_api.preset_name ? `预设 ${response.current_api.preset_name}` : null,
    ].filter(Boolean).join(" · ");
  }

  if (response.current_mode === "auth" && response.current_auth) {
    return "当前为登录 Auth 模式";
  }

  return response.current_config_error || response.current_auth_error || "当前尚未配置 API 模式。";
}

function formatCurrentAuthStatus(response) {
  if (response.current_mode === "api") {
    const currentApi = response.current_api;
    if (currentApi) {
      return formatCurrentApiSummary(currentApi, "当前处于 API 模式");
    }
  }

  if (response.current_auth) {
    return [
      response.current_auth.email || `账号 ${response.current_auth.short_id}`,
      response.current_auth.plan_type,
      formatDateLikeTime(response.current_auth.last_refresh),
    ].filter(Boolean).join(" · ");
  }

  return response.current_auth_error || response.current_config_error || "当前 auth.json 尚不存在。";
}

function formatCurrentApiStatus(response) {
  if (response.current_api) {
    return formatCurrentApiSummary(
      response.current_api,
      response.current_mode === "auth" ? "当前仍是登录 Auth 模式" : "当前处于 API 模式",
    );
  }

  if (response.current_mode === "auth" && response.current_auth) {
    return [
      "当前为登录 Auth 模式",
      response.current_auth.email || `账号 ${response.current_auth.short_id}`,
      response.current_auth.plan_type,
    ].filter(Boolean).join(" · ");
  }

  return response.current_config_error || response.current_auth_error || "当前尚未配置 API 模式。";
}

function renderApiPresets(presets) {
  return apiManager.renderApiPresets(presets);
}

function renderClaudePresets(presets) {
  return claudeManager.renderClaudePresets(presets);
}

function loadAuthPresets() {
  return authManager.loadAuthPresets();
}

function ensureAuthPresetsLoaded() {
  return authManager.ensureAuthPresetsLoaded();
}

function normalizeUpstreamProxySettings(value) {
  return {
    codex_api_proxy_enabled: Boolean(value?.codex_api_proxy_enabled),
    claude_proxy_enabled: Boolean(value?.claude_proxy_enabled),
    active_api_proxy_preset_id: value?.active_api_proxy_preset_id || null,
    active_claude_proxy_preset_id: value?.active_claude_proxy_preset_id || null,
  };
}

function renderUpstreamProxyToggles() {
  if (claudeUpstreamProxyToggleEl) {
    claudeUpstreamProxyToggleEl.checked = Boolean(state.upstreamProxy.claude_proxy_enabled);
  }
}

function loadApiPresets() {
  return apiManager.loadApiPresets();
}

function ensureApiPresetsLoaded() {
  return apiManager.ensureApiPresetsLoaded();
}

async function loadClaudePresets() {
  return claudeManager.loadClaudePresets();
}

function ensureClaudePresetsLoaded() {
  return claudeManager.ensureClaudePresetsLoaded();
}

async function refreshAuthPanels() {
  await Promise.allSettled([loadAuthPresets(), loadApiPresets(), loadClaudePresets()]);
}

function confirmEditedPresetOverwrite({ presetKind = "预设", presetName = "" } = {}) {
  const nameSuffix = presetName ? `「${presetName}」` : "当前条目";
  return window.confirm(
    `确定保存回原 ${presetKind} ${nameSuffix} 吗？\n\n这会覆盖原条目；如果要另存为新条目，请点击“新增”。`,
  );
}

function editAuthPreset(presetId) {
  return authManager.editAuthPreset(presetId);
}

function resetAuthPresetForm(message = "输入内容已清空。", tone = "muted") {
  return authManager.resetAuthPresetForm(message, tone);
}

function syncApiApplyProxyRecommendation() {
  return apiManager.syncApiApplyProxyRecommendation();
}

function warnApiApplyProxyRecommendationIfNeeded() {
  return apiManager.warnApiApplyProxyRecommendationIfNeeded();
}

function syncApiManagementUrlField({
  useBaseUrl,
  syncValue,
  focusInput,
} = {}) {
  return apiManager.syncApiManagementUrlField({ useBaseUrl, syncValue, focusInput });
}

function resetApiPresetForm(message = "输入内容已清空。", tone = "muted") {
  return apiManager.resetApiPresetForm(message, tone);
}

function startNewApiPreset() {
  return apiManager.startNewApiPreset();
}

function editApiPreset(presetId) {
  return apiManager.editApiPreset(presetId);
}

function syncClaudeModelGroupState(preferredMode = "") {
  return claudeManager.syncClaudeModelGroupState(preferredMode);
}

function resetClaudePresetForm(message = "输入内容已清空。", tone = "muted") {
  return claudeManager.resetClaudePresetForm(message, tone);
}

function editClaudePreset(presetId) {
  return claudeManager.editClaudePreset(presetId);
}

async function saveApiPreset() {
  return apiManager.saveApiPreset();
}

async function saveApiPresetAsNew() {
  return apiManager.saveApiPresetAsNew();
}

async function applyApiPreset(
  presetId,
  statusElement = apiManagerStatusEl,
  { respectSavedProxyPreference = true } = {},
) {
  return apiManager.applyApiPreset(presetId, statusElement, { respectSavedProxyPreference });
}

async function applyEditingApiPreset() {
  return apiManager.applyEditingApiPreset();
}

async function applyApiPresetAndLaunch(presetId) {
  return apiManager.applyApiPresetAndLaunch(presetId);
}

async function updateUpstreamProxySettings(payload) {
  const response = await requestJson("/api/auth/upstream-proxy-settings", {
    method: "PUT",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(payload),
  });
  state.upstreamProxy = normalizeUpstreamProxySettings(response.upstream_proxy);
  renderUpstreamProxyToggles();
  return state.upstreamProxy;
}

function normalizeTestResult(raw, { presetId = null, fallbackName = "" } = {}) {
  const status = Number.isFinite(Number(raw?.status)) ? Number(raw.status) : null;
  const latency = Number.isFinite(Number(raw?.latency_ms)) ? Number(raw.latency_ms) : null;
  return {
    name: raw?.name || fallbackName || raw?.preset_id || presetId || "未知预设",
    preset_id: raw?.preset_id || presetId || null,
    ok: Boolean(raw?.ok),
    status,
    latency_ms: latency,
    message: raw?.message || (raw?.ok ? "测试通过" : "测试失败"),
    tested_at: Number.isFinite(Number(raw?.tested_at)) ? Number(raw.tested_at) : Date.now(),
  };
}

function errorTestResult(error, ctx) {
  const message = error?.message || (typeof error === "string" ? error : "测试失败");
  return normalizeTestResult({ ok: false, status: null, latency_ms: null, message }, ctx);
}

function prunePresetTestResults(map, presets, testingSet = null) {
  const liveIds = new Set(
    (Array.isArray(presets) ? presets : [])
      .map((preset) => preset?.id)
      .filter(Boolean),
  );
  if (map instanceof Map) {
    for (const id of map.keys()) {
      if (!liveIds.has(id)) map.delete(id);
    }
  }
  if (testingSet instanceof Set) {
    for (const id of testingSet) {
      if (!liveIds.has(id)) testingSet.delete(id);
    }
  }
}

async function testApiPreset(presetId, presetName) {
  return apiManager.testApiPreset(presetId, presetName);
}

async function testAllApiPresets() {
  return apiManager.testAllApiPresets();
}

async function deleteApiPreset(presetId, presetName) {
  return apiManager.deleteApiPreset(presetId, presetName);
}

async function saveClaudePreset() {
  return claudeManager.saveClaudePreset();
}

async function saveClaudePresetAsNew() {
  return claudeManager.saveClaudePresetAsNew();
}

async function applyClaudePreset(presetId, statusElement) {
  return claudeManager.applyClaudePreset(presetId, statusElement);
}

async function applyEditingClaudePreset() {
  return claudeManager.applyEditingClaudePreset();
}

async function applyClaudePresetToOpencode(presetId, statusElement) {
  return claudeManager.applyClaudePresetToOpencode(presetId, statusElement);
}

async function testClaudePreset(presetId, presetName) {
  return claudeManager.testClaudePreset(presetId, presetName);
}

async function testAllClaudePresets() {
  return claudeManager.testAllClaudePresets();
}

async function deleteClaudePreset(presetId, presetName) {
  return claudeManager.deleteClaudePreset(presetId, presetName);
}
