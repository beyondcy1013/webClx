(function () {
  function createAuthManager(deps) {
    const {
      state,
      requestJson,
      updateStatus,
      firstFiniteNumber,
      updateEditorState,
      setAuthOauthStatus,
      refreshAuthPanels,
      renderConfigOverrideEditor,
      collectConfigOverrideEditorValues,
      normalizePresetConfigOverrides,
      collectPresetConfigKeys,
      renderAuthPresetTableHeader,
      renderPresetTable,
      movePresetById,
      movePresetOrderWithPersist,
      persistPresetOrder,
      makePresetRowClickable,
      buildPresetConfigValueMap,
      buildPresetConfigCells,
      createActionCell,
      createActionButton,
      createPresetDeleteButton,
      createCurrentIndicatorCell,
      setButtonBusy,
      normalizeTestResult,
      errorTestResult,
      prunePresetTestResults,
      presetTestPopup,
      textOrDash,
      formatDateLikeMonthDayTime,
      formatElapsedSince,
      formatQuotaWindow,
      formatMonthDayTime,
      formatDateLikeTime,
      formatCurrentApiSummary,
      formatCurrentAuthStatus,
      confirmEditedPresetOverwrite,
      refreshConfigOverrideDatalists,
      AUTH_FORM_DEFAULT_STATUS,
      elements,
    } = deps;
    const {
      editorEl,
      importAuthButton,
      saveButton,
      fileStatusEl,
      authPresetListEl,
      authRefreshAllQuotaButton,
      authTestAllPresetsButton,
      authCurrentFileEl,
      authConfigFileEl,
      authPresetFileEl,
      authCurrentAccountEl,
      authPresetInputEl,
      authPresetNameEl,
      authSavePresetButton,
      authSaveAsNewPresetButton,
      authApplyEditedPresetButton,
      authClearInputButton,
      authApplyInputButton,
      authImportFileButton,
      authFormStatusEl,
      authConfigOverrideControls,
      authOauthStartButton,
      authOauthOpenLinkEl,
      authOauthCopyCodeButton,
      authOauthSessionPanelEl,
      authOauthSessionSummaryEl,
      authOauthUserCodeEl,
      authImportDialogEl,
      authImportTextEl,
      authOauthStatusEl,
    } = elements;
    let authOauthPollTimer = null;

function stripJsonCodeFence(rawText) {
  const trimmed = rawText.trim();
  const fenced = trimmed.match(/^```(?:json)?\s*([\s\S]*?)\s*```$/i);
  return fenced ? fenced[1].trim() : trimmed;
}

function decodeBase64UrlJson(encoded) {
  if (typeof encoded !== "string" || !encoded.trim()) {
    return null;
  }

  const normalized = encoded.replace(/-/g, "+").replace(/_/g, "/");
  const padded = normalized.padEnd(normalized.length + ((4 - (normalized.length % 4)) % 4), "=");

  try {
    const binary = window.atob(padded);
    const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
    const text = new TextDecoder().decode(bytes);
    return JSON.parse(text);
  } catch {
    return null;
  }
}

function decodeJwtPayload(token) {
  if (typeof token !== "string") {
    return null;
  }

  const parts = token.split(".");
  if (parts.length < 2) {
    return null;
  }

  return decodeBase64UrlJson(parts[1]);
}

function mergeAuthSource(item, inherited = {}) {
  if (!item || typeof item !== "object" || Array.isArray(item)) {
    return null;
  }

  const auth = item.auth && typeof item.auth === "object" && !Array.isArray(item.auth)
    ? item.auth
    : item;
  const credentials = auth.credentials && typeof auth.credentials === "object" && !Array.isArray(auth.credentials)
    ? auth.credentials
    : {};
  const extra = auth.extra && typeof auth.extra === "object" && !Array.isArray(auth.extra)
    ? auth.extra
    : {};

  return {
    ...item,
    ...auth,
    ...credentials,
    account_id: firstNonEmptyString(
      credentials.account_id,
      credentials.chatgpt_account_id,
      auth.account_id,
      auth.chatgpt_account_id,
      item.account_id,
      item.chatgpt_account_id,
    ),
    last_refresh: firstNonEmptyString(
      extractAuthLastRefresh(auth),
      extractAuthLastRefresh(credentials),
      extractAuthLastRefresh(extra),
      inherited.exported_at,
    ),
  };
}

function parseAuthSources(rawText) {
  let parsed;
  try {
    parsed = JSON.parse(stripJsonCodeFence(rawText));
  } catch {
    throw new Error("内容不是有效的 JSON。");
  }

  const inherited = parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : {};
  const candidates = Array.isArray(parsed)
    ? parsed
    : Array.isArray(parsed?.accounts)
      ? parsed.accounts
      : [parsed];
  const sources = candidates
    .map((item) => mergeAuthSource(item, inherited))
    .filter((item) => item && typeof item === "object");
  if (sources.length === 0) {
    throw new Error("内容缺少可用的账号对象。");
  }

  return sources;
}

function parseAuthSource(rawText) {
  return parseAuthSources(rawText)[0];
}

function extractAuthLastRefresh(source) {
  return firstNonEmptyString(
    source?.last_refresh,
    source?.refresh_time,
    source?.["refresh time"],
    source?.refreshTime,
  );
}

function readExistingAuthApiKey() {
  try {
    const parsed = JSON.parse(editorEl.value);
    return Object.prototype.hasOwnProperty.call(parsed, "OPENAI_API_KEY") ? parsed.OPENAI_API_KEY : null;
  } catch {
    return null;
  }
}

function getAuthTokenSource(source) {
  if (source?.tokens && typeof source.tokens === "object") {
    return source.tokens;
  }
  if (source && typeof source === "object") {
    return source;
  }
  return null;
}

function normalizeAuthInput(rawText, fallbackApiKey = null) {
  const source = parseAuthSource(rawText);
  const tokens = getAuthTokenSource(source);
  if (!tokens || typeof tokens !== "object") {
    throw new Error("内容缺少 tokens 字段或 CPA 扁平 token 字段。");
  }

  const accessToken = firstNonEmptyString(tokens.access_token);
  const idToken = optionalTokenString(tokens.id_token, "id_token");
  const refreshToken = optionalTokenString(tokens.refresh_token, "refresh_token");
  const accessPayload = decodeJwtPayload(accessToken);
  const accessAuthClaim = accessPayload?.["https://api.openai.com/auth"];
  const accountId = firstNonEmptyString(
    tokens.account_id,
    tokens.chatgpt_account_id,
    source.account_id,
    source.chatgpt_account_id,
    accessAuthClaim?.chatgpt_account_id,
  );
  const apiKey = Object.prototype.hasOwnProperty.call(source, "OPENAI_API_KEY") ? source.OPENAI_API_KEY : fallbackApiKey;
  const lastRefresh = extractAuthLastRefresh(source) || new Date().toISOString();

  if (!accessToken || !accountId) {
    throw new Error("内容缺少 access_token / account_id（也支持 chatgpt_account_id）。");
  }

  const normalizedTokens = {
    access_token: accessToken,
    account_id: accountId,
  };
  if (idToken) {
    normalizedTokens.id_token = idToken;
  }
  if (refreshToken) {
    normalizedTokens.refresh_token = refreshToken;
  }

  return {
    OPENAI_API_KEY: typeof apiKey === "string" ? apiKey : null,
    last_refresh: lastRefresh,
    tokens: normalizedTokens,
  };
}

function optionalTokenString(value, fieldName) {
  if (value === undefined || value === null) {
    return "";
  }
  if (typeof value !== "string") {
    throw new Error(`${fieldName} 必须是字符串。`);
  }
  return value.trim();
}

function buildAuthPresetName(rawText) {
  try {
    const source = parseAuthSource(rawText);
    const namedParts = [source.name, source.email, source.account_name]
      .filter((value) => typeof value === "string" && value.trim())
      .map((value) => value.trim())
      .filter((value, index, values) => values.indexOf(value) === index);

    if (namedParts.length > 0) {
      return namedParts.join(" · ");
    }

    const accountId = source?.tokens?.account_id ?? source?.account_id;
    if (typeof accountId === "string" && accountId.trim()) {
      return `账号 ${accountId.trim().slice(-6)}`;
    }
  } catch {
    return "";
  }

  return "";
}

function firstNonEmptyString(...values) {
  for (const value of values) {
    if (typeof value === "string" && value.trim()) {
      return value.trim();
    }
  }
  return null;
}

function pickOrganizationTitle(organizations) {
  if (!Array.isArray(organizations)) {
    return null;
  }

  const titledOrg = organizations.find((item) => typeof item?.title === "string" && item.title.trim() && item.title !== "Personal");
  if (titledOrg) {
    return titledOrg.title.trim();
  }

  const fallbackOrg = organizations.find((item) => typeof item?.title === "string" && item.title.trim());
  return fallbackOrg ? fallbackOrg.title.trim() : null;
}

function normalizePlanTypeLabel(value) {
  if (typeof value !== "string" || !value.trim()) {
    return null;
  }
  return value.trim().toUpperCase();
}

function normalizeLoginMethodLabel(value) {
  if (typeof value !== "string" || !value.trim()) {
    return null;
  }

  const normalized = value.trim().toLowerCase();
  const labels = {
    password: "Password",
    "google-oauth2": "Google",
    google: "Google",
    github: "GitHub",
    apple: "Apple",
    microsoft: "Microsoft",
    "microsoft-account": "Microsoft",
    oauth: "OAuth",
  };

  if (labels[normalized]) {
    return labels[normalized];
  }

  return value.trim().charAt(0).toUpperCase() + value.trim().slice(1);
}

function extractAuthDetails(rawText) {
  const source = parseAuthSource(rawText);
  const tokens = getAuthTokenSource(source) ?? {};
  const idPayload = decodeJwtPayload(tokens.id_token);
  const accessPayload = decodeJwtPayload(tokens.access_token);
  const authClaim = idPayload?.["https://api.openai.com/auth"] ?? accessPayload?.["https://api.openai.com/auth"] ?? {};
  const profileClaim = accessPayload?.["https://api.openai.com/profile"] ?? {};
  const quota = source?.quota && typeof source.quota === "object" ? source.quota : {};
  const rateLimit = quota?.raw_data?.rate_limit ?? {};

  return {
    email: firstNonEmptyString(source.email, profileClaim.email, idPayload?.email),
    plan_type: normalizePlanTypeLabel(firstNonEmptyString(source.plan_type, authClaim.chatgpt_plan_type)),
    account_name: firstNonEmptyString(source.account_name, source.name, pickOrganizationTitle(authClaim.organizations)),
    login_method: normalizeLoginMethodLabel(
      firstNonEmptyString(source.auth_provider, idPayload?.auth_provider, source.auth_mode),
    ),
    hourly_percentage: firstFiniteNumber(
      quota.hourly_percentage,
      rateLimit?.primary_window?.used_percent,
    ),
    hourly_reset_time: firstFiniteNumber(
      quota.hourly_reset_time,
      rateLimit?.primary_window?.reset_at,
    ),
    weekly_percentage: firstFiniteNumber(
      quota.weekly_percentage,
      rateLimit?.secondary_window?.used_percent,
    ),
    weekly_reset_time: firstFiniteNumber(
      quota.weekly_reset_time,
      rateLimit?.secondary_window?.reset_at,
    ),
    last_refresh: extractAuthLastRefresh(source),
    created_at: firstFiniteNumber(source.created_at),
  };
}

function normalizeAuthInputs(rawText, fallbackApiKey = null) {
  return parseAuthSources(rawText).map((source) => {
    const sourceText = JSON.stringify(source);
    return {
      rawText: sourceText,
      auth: normalizeAuthInput(sourceText, fallbackApiKey),
      name: buildAuthPresetName(sourceText),
      details: extractAuthDetails(sourceText),
    };
  });
}

function applyAuthImportText(rawText) {
  const normalized = normalizeAuthInput(rawText, readExistingAuthApiKey());
  editorEl.value = `${JSON.stringify(normalized, null, 2)}\n`;
  state.dirty = true;
  updateEditorState();
  updateStatus(fileStatusEl, "已转换为当前 auth.json 格式，请点击保存文件。", "ok");
}

function isJsonAuthFile(sourceFile) {
  const fileName = typeof sourceFile?.name === "string" ? sourceFile.name.toLowerCase() : "";
  return fileName.endsWith(".json") || sourceFile?.type === "application/json";
}

async function readAuthFileAsText(sourceFile) {
  if (typeof sourceFile?.text === "function") {
    return sourceFile.text();
  }
  if (typeof sourceFile?.arrayBuffer === "function") {
    return new TextDecoder().decode(await sourceFile.arrayBuffer());
  }

  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result ?? ""));
    reader.onerror = () => reject(reader.error || new Error("读取 JSON 文件失败。"));
    reader.readAsText(sourceFile);
  });
}

async function importAuthJsonFile(sourceFile) {
  if (!sourceFile) {
    return false;
  }
  if (!isJsonAuthFile(sourceFile)) {
    updateStatus(authFormStatusEl, "请选择 .json 文件。", "warn");
    return false;
  }
  if (
    authPresetInputEl.value.trim()
    && !window.confirm("当前账号 JSON 已有内容，导入文件会覆盖它。是否继续？")
  ) {
    return false;
  }

  updateStatus(authFormStatusEl, `正在读取 ${sourceFile.name || "JSON 文件"}…`, "info");
  if (authImportFileButton) {
    authImportFileButton.disabled = true;
  }

  try {
    const rawText = await readAuthFileAsText(sourceFile);
    normalizeAuthInputs(rawText);
    populateAuthFormFromRawText(rawText, {
      presetName: buildAuthPresetName(rawText),
      statusMessage: `已导入 ${sourceFile.name || "JSON 文件"}，可直接新增或应用到 auth.json。`,
      tone: "ok",
    });
    return true;
  } catch (error) {
    updateStatus(authFormStatusEl, error?.message || "导入 JSON 文件失败。", "warn");
    return false;
  } finally {
    if (authImportFileButton) {
      authImportFileButton.disabled = false;
    }
  }
}

function openAuthImportDialog(prefill = "") {
  authImportTextEl.value = prefill;
  if (typeof authImportDialogEl.showModal === "function") {
    if (!authImportDialogEl.open) {
      authImportDialogEl.showModal();
    }
  } else {
    authImportDialogEl.setAttribute("open", "");
  }
  window.requestAnimationFrame(() => {
    authImportTextEl.focus();
  });
}

function closeAuthImportDialog() {
  if (typeof authImportDialogEl.close === "function") {
    if (authImportDialogEl.open) {
      authImportDialogEl.close();
    }
  } else {
    authImportDialogEl.removeAttribute("open");
  }
  updateEditorState();
}
function updateAuthPresetRefreshingState(presetIds, refreshing) {
  const ids = Array.isArray(presetIds) ? presetIds : [presetIds];
  let changed = false;

  ids.filter(Boolean).forEach((presetId) => {
    if (refreshing) {
      if (!state.authRefreshingPresetIds.has(presetId)) {
        state.authRefreshingPresetIds.add(presetId);
        changed = true;
      }
      return;
    }

    if (state.authRefreshingPresetIds.delete(presetId)) {
      changed = true;
    }
  });

  if (changed) {
    renderAuthPresets(state.authPresets);
  }
}

function clearAuthPresetRefreshErrors(presetIds) {
  const ids = Array.isArray(presetIds) ? presetIds : [presetIds];
  ids.filter(Boolean).forEach((presetId) => {
    state.authRefreshErrorsByPresetId.delete(presetId);
  });
}

function setAuthPresetRefreshError(presetId, message) {
  if (!presetId) {
    return;
  }
  const normalized = typeof message === "string" && message.trim() ? message.trim() : "刷新失败";
  state.authRefreshErrorsByPresetId.set(presetId, normalized);
}

function pruneAuthPresetRefreshErrors(presets) {
  const validIds = new Set(presets.map((preset) => preset.id));
  for (const presetId of state.authRefreshErrorsByPresetId.keys()) {
    if (!validIds.has(presetId)) {
      state.authRefreshErrorsByPresetId.delete(presetId);
    }
  }
}

function authLastRefreshValue(preset) {
  return preset?.details?.last_refresh || preset?.auth?.last_refresh || "";
}

function authStatusSortRank(preset) {
  if (preset?.active) {
    return 3;
  }
  if (state.authRefreshingPresetIds.has(preset?.id)) {
    return 2;
  }
  if (state.authRefreshErrorsByPresetId.has(preset?.id)) {
    return 1;
  }
  return 0;
}

function authRefreshAgeSortValue(preset) {
  const timestamp = new Date(authLastRefreshValue(preset)).getTime();
  return Number.isFinite(timestamp) ? Date.now() - timestamp : null;
}

function buildAuthPresetSortColumns(configKeys) {
  return [
    { key: "status", type: "number", defaultDirection: "desc", getValue: authStatusSortRank },
    { key: "email", type: "text", getValue: (preset) => preset?.details?.email || "" },
    { key: "plan", type: "text", getValue: (preset) => preset?.details?.plan_type || "" },
    { key: "account_name", type: "text", getValue: (preset) => preset?.details?.account_name || "" },
    { key: "hourly", type: "number", defaultDirection: "desc", getValue: (preset) => firstFiniteNumber(preset?.details?.hourly_percentage) },
    { key: "weekly", type: "number", defaultDirection: "desc", getValue: (preset) => firstFiniteNumber(preset?.details?.weekly_percentage) },
    { key: "last_refresh", type: "date", defaultDirection: "desc", getValue: authLastRefreshValue },
    { key: "refresh_age", type: "number", defaultDirection: "desc", getValue: authRefreshAgeSortValue },
    { key: "saved_at", type: "date", defaultDirection: "desc", getValue: (preset) => preset?.saved_at || "" },
    { key: "switch_count", type: "number", defaultDirection: "desc", getValue: (preset) => preset?.switch_count || 0 },
    { key: "login", type: "text", getValue: (preset) => preset?.details?.login_method || "" },
    ...createPresetConfigSortColumns(configKeys, "config.toml: "),
  ];
}

async function moveAuthPresetOrder(presetId, direction) {
  await movePresetOrderWithPersist({
    presets: state.authPresets,
    presetId,
    direction,
    sortTableKey: "auth",
    reorderUrl: "/api/auth/presets/reorder",
    label: "auth",
    renderFn: renderAuthPresets,
    getStatus: () => state.authPresets,
    setStatus: (p) => { state.authPresets = p; },
    persistOrder: persistPresetOrder,
    updateStatus,
  });
}

function renderAuthPresets(presets) {
  const configKeys = collectPresetConfigKeys(presets);
  const sortColumns = buildAuthPresetSortColumns(configKeys);
  const validIds = new Set(presets.map((preset) => preset.id));
  state.authPresetExportSelection.forEach((id) => {
    if (!validIds.has(id)) state.authPresetExportSelection.delete(id);
  });
  const selection = {
    label: "Codex_OAuth 账号",
    presets,
    selectedIds: state.authPresetExportSelection,
    onChange: () => renderAuthPresets(presets),
  };
  renderAuthPresetTableHeader(configKeys, {
    sortColumns,
    onSortChange: () => renderAuthPresets(presets),
    selection,
  });

  renderPresetTable({
    listEl: authPresetListEl,
    presets,
    emptyText: "还没有保存任何 auth 预设。",
    emptyColspan: 18 + configKeys.length,
    tableKey: "auth",
    sortColumns,
    order: {
      enabled: true,
      onMove: moveAuthPresetOrder,
    },
    selection,
    buildCells: (preset) => {
    const details = preset.details ?? {};
    const isRefreshing = state.authRefreshingPresetIds.has(preset.id);
    const refreshError = state.authRefreshErrorsByPresetId.get(preset.id);

    const currentCell = createCurrentIndicatorCell(preset.active, {
      testResult: state.authPresetTestResults.get(preset.id) || null,
      testKind: "auth",
      testing: state.authPresetsTesting.has(preset.id),
    });
    if (isRefreshing) {
      currentCell.replaceChildren();
      currentCell.classList.remove("has-test-result", "is-testing");
      currentCell.classList.add("is-refreshing");
      const indicator = document.createElement("span");
      indicator.className = "auth-refresh-indicator";
      indicator.textContent = preset.active ? "\u2192 刷新中" : "刷新中";
      currentCell.appendChild(indicator);
    } else if (refreshError) {
      currentCell.replaceChildren();
      currentCell.classList.remove("has-test-result", "is-testing");
      currentCell.classList.add("has-refresh-error");
      currentCell.title = refreshError;
      const indicator = document.createElement("span");
      indicator.className = "auth-refresh-error-indicator";
      indicator.textContent = preset.active ? "\u2192 错误" : "错误";
      currentCell.appendChild(indicator);
    }

    const emailCell = document.createElement("td");
    emailCell.className = "mono-text auth-email-cell";
    emailCell.textContent = textOrDash(details.email);

    const planCell = document.createElement("td");
    planCell.textContent = textOrDash(details.plan_type);

    const accountNameCell = document.createElement("td");
    accountNameCell.textContent = textOrDash(details.account_name);

    const loginCell = document.createElement("td");
    loginCell.textContent = textOrDash(details.login_method);

    const configValues = buildPresetConfigValueMap(preset);
    const configCells = buildPresetConfigCells(configKeys, configValues);

    const lastRefreshRaw = preset.details?.last_refresh || preset.auth?.last_refresh;
    const lastRefreshCell = document.createElement("td");
    lastRefreshCell.textContent = formatDateLikeMonthDayTime(lastRefreshRaw);

    const refreshAgo = formatElapsedSince(lastRefreshRaw);
    const refreshAgoCell = document.createElement("td");
    refreshAgoCell.className = "auth-relative-refresh-cell";
    refreshAgoCell.textContent = refreshAgo.label;
    if (refreshAgo.stale) {
      refreshAgoCell.classList.add("is-stale");
    }

    const hourlyCell = document.createElement("td");
    hourlyCell.textContent = formatQuotaWindow(details.hourly_percentage, details.hourly_reset_time);

    const weeklyCell = document.createElement("td");
    weeklyCell.textContent = formatQuotaWindow(details.weekly_percentage, details.weekly_reset_time);

    const timeCell = document.createElement("td");
    timeCell.textContent = formatMonthDayTime(preset.saved_at);

    const switchCountCell = document.createElement("td");
    switchCountCell.className = "mono-text";
    switchCountCell.textContent = String(preset.switch_count || 0);

    const applyCell = createActionCell(
      [createActionButton("切换", () => applyAuthPreset(preset.id), "mini-button accent")],
      "auth-action-cell",
      "actions preset-actions",
    );

    const refreshCell = createActionCell(
      [createActionButton("刷新", () => refreshAuthPresetQuota(preset.id, preset.name), "mini-button")],
      "auth-action-cell",
      "actions preset-actions",
    );

    const testCell = createActionCell(
      [createActionButton("测试", () => testAuthPreset(preset.id, preset.name), "mini-button")],
      "auth-action-cell",
      "actions preset-actions",
    );

    const editCell = createActionCell(
      [createActionButton("编辑", () => editAuthPreset(preset.id), "mini-button")],
      "auth-action-cell",
      "actions preset-actions",
    );

    const deleteCell = createActionCell(
      [createPresetDeleteButton(() => deleteAuthPreset(preset.id, preset.name))],
      "auth-action-cell",
      "actions preset-actions",
    );

    return [
      applyCell,
      refreshCell,
      testCell,
      editCell,
      currentCell,
      emailCell,
      planCell,
      accountNameCell,
      hourlyCell,
      weeklyCell,
      lastRefreshCell,
      refreshAgoCell,
      timeCell,
      switchCountCell,
      loginCell,
      ...configCells,
      deleteCell,
    ];
    },
    decorateRow: (row, preset) => makePresetRowClickable(row, preset, () => applyAuthPreset(preset.id)),
  });
}

async function loadAuthPresets() {
  if (state.authPresetsLoading) {
    return;
  }

  state.authPresetsLoading = true;
  authRefreshAllQuotaButton.disabled = true;
  authTestAllPresetsButton.disabled = true;

  try {
    const response = await requestJson("/api/auth/presets");
    state.authPresets = response.presets;
    state.authPresetsLoaded = true;
    pruneAuthPresetRefreshErrors(response.presets);
    prunePresetTestResults(state.authPresetTestResults, response.presets, state.authPresetsTesting);
    if (
      state.editingAuthPresetId &&
      !response.presets.some((preset) => preset.id === state.editingAuthPresetId)
    ) {
      resetAuthPresetForm("正在编辑的 auth 预设已不存在。", "warn");
    }
    authCurrentFileEl.textContent = response.auth_file;
    authConfigFileEl.textContent = response.config_file;
    authPresetFileEl.textContent = response.preset_file;
    authCurrentAccountEl.textContent = formatCurrentAuthStatus(response);
    renderAuthPresets(response.presets);
    refreshConfigOverrideDatalists();
    authRefreshAllQuotaButton.disabled = response.presets.length === 0;
    authTestAllPresetsButton.disabled = response.presets.length === 0;
  } catch (error) {
    state.authPresets = [];
    state.authPresetsLoaded = false;
    authPresetListEl.textContent = "";
    authCurrentFileEl.textContent = "读取失败";
    authConfigFileEl.textContent = "读取失败";
    authCurrentAccountEl.textContent = "读取失败";
    authPresetFileEl.textContent = "读取失败";
    refreshConfigOverrideDatalists();
    authRefreshAllQuotaButton.disabled = true;
    authTestAllPresetsButton.disabled = true;
  } finally {
    state.authPresetsLoading = false;
  }
}

function ensureAuthPresetsLoaded() {
  if (state.authPresetsLoaded || state.authPresetsLoading) {
    return;
  }

  loadAuthPresets();
}

async function saveAuthPresetFromRawText(
  rawText,
  presetName,
  statusElement,
  buttons = [],
  presetId = "",
  configOverrides = [],
  forceNewPreset = false,
) {
  const trimmedText = rawText.trim();
  if (!trimmedText) {
    updateStatus(statusElement, "先粘贴一段账号 JSON。", "warn");
    return null;
  }

  const isEditing = Boolean(presetId) && !forceNewPreset;
  updateStatus(statusElement, isEditing ? "正在更新 auth 预设…" : "正在保存 auth 预设…", "info");
  buttons.forEach((button) => {
    button.disabled = true;
  });

  try {
    const auth = normalizeAuthInput(trimmedText);
    const autoName = buildAuthPresetName(trimmedText);
    const payload = {
      name: presetName.trim() || autoName,
      details: extractAuthDetails(trimmedText),
      config_overrides: configOverrides,
      auth,
    };
    const response = await requestJson(
      isEditing ? `/api/auth/presets/${encodeURIComponent(presetId)}` : "/api/auth/presets",
      {
        method: isEditing ? "PUT" : "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify(payload),
      },
    );
    updateStatus(statusElement, `${isEditing ? "已更新" : "已保存"}预设：${response.preset.name}`, "ok");
    await loadAuthPresets();
    return {
      ...response,
      saved_count: 1,
    };
  } catch (error) {
    updateStatus(statusElement, error.message, "warn");
    return null;
  } finally {
    buttons.forEach((button) => {
      button.disabled = false;
    });
  }
}

function buildAuthApplyPayload(rawText, presetName = "") {
  const trimmedText = rawText.trim();
  if (!trimmedText) {
    throw new Error("先粘贴一段账号 JSON。");
  }

  return {
    rawText: trimmedText,
    name: presetName.trim() || buildAuthPresetName(trimmedText),
    details: extractAuthDetails(trimmedText),
    auth: normalizeAuthInput(trimmedText),
  };
}

async function saveAuthPreset() {
  return saveAuthPresetWithMode(false);
}

async function saveAuthPresetAsNew() {
  return saveAuthPresetWithMode(true);
}

async function saveAuthPresetWithMode(forceNewPreset = false) {
  const editingPresetId = state.editingAuthPresetId;
  const editingPreset = state.authPresets.find((item) => item.id === editingPresetId);
  if (!authPresetInputEl.value.trim()) {
    updateStatus(authFormStatusEl, "先粘贴一段账号 JSON。", "warn");
    return;
  }
  const isEditing = Boolean(editingPresetId) && !forceNewPreset;
  if (isEditing && !confirmEditedPresetOverwrite({
    presetKind: "auth 预设",
    presetName: editingPreset?.name || authPresetNameEl.value.trim(),
  })) {
    return;
  }
  const response = await saveAuthPresetFromRawText(
    authPresetInputEl.value,
    authPresetNameEl.value,
    authFormStatusEl,
    [authSavePresetButton, authSaveAsNewPresetButton, authApplyEditedPresetButton, authClearInputButton],
    forceNewPreset ? "" : editingPresetId,
    collectConfigOverrideEditorValues(authConfigOverrideControls),
    forceNewPreset,
  );

  if (response) {
    const message = response.saved_count > 1
      ? `已新增 ${response.saved_count} 个预设。`
      : `${isEditing ? "已更新" : "已新增"}预设：${response.preset.name}`;
    resetAuthPresetForm(message, "ok");
  }
}

async function applyAuthFromInput() {
  let payload;
  try {
    payload = buildAuthApplyPayload(authPresetInputEl.value, authPresetNameEl.value);
  } catch (error) {
    updateStatus(authFormStatusEl, error.message, "warn");
    return;
  }

  updateStatus(authFormStatusEl, "正在写入 auth.json…", "info");
  authApplyInputButton.disabled = true;
  authSavePresetButton.disabled = true;

  try {
    const response = await requestJson("/api/auth/current", {
      method: "PUT",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        auth: payload.auth,
      }),
    });
    updateStatus(authFormStatusEl, `已写入 auth.json：${response.account_id}`, "ok");
    await refreshAuthPanels();
  } catch (error) {
    updateStatus(authFormStatusEl, error.message, "warn");
  } finally {
    authApplyInputButton.disabled = false;
    authSavePresetButton.disabled = false;
  }
}

function handleAuthApplyInputAction() {
  if (!state.editingAuthPresetId) {
    applyAuthFromInput();
    return;
  }

  const preset = state.authPresets.find((item) => item.id === state.editingAuthPresetId);
  if (!preset) {
    updateStatus(authFormStatusEl, "找不到要删除的 auth 预设。", "warn");
    return;
  }

  deleteAuthPreset(preset.id, preset.name, authFormStatusEl);
}

async function applyAuthPreset(presetId, statusElement = authFormStatusEl) {
  updateStatus(statusElement, "正在覆盖 auth.json 并校正 config.toml…", "info");

  try {
    // Pass the current project directory so a project-local .codex/config.toml
    // that overrides the global config gets synced too.
    const projectPath = state.currentDirectory?.display_path || state.workspaceDir || "";
    const params = new URLSearchParams();
    if (projectPath) {
      params.set("project_path", projectPath);
    }
    const query = params.toString() ? `?${params.toString()}` : "";
    const response = await requestJson(
      `/api/auth/presets/${encodeURIComponent(presetId)}/apply${query}`,
      {
        method: "PUT",
      },
    );
    if (response.deferred) {
      updateStatus(
        statusElement,
        `已登记预设切换：${response.name}。当前指定 Agent 退出并恢复原配置后，将实际写入该预设。`,
        "info",
      );
    } else {
      const note = response.local_config_file
        ? `（已同步项目本地配置：${response.local_config_file}）`
        : "";
      updateStatus(statusElement, `已应用预设：${response.name}${note}`, "ok");
    }
    await refreshAuthPanels();
  } catch (error) {
    updateStatus(statusElement, error.message, "warn");
  }
}

async function applyEditingAuthPreset() {
  if (!state.editingAuthPresetId) {
    updateStatus(authFormStatusEl, "先点击一条 auth 预设的编辑。", "warn");
    return;
  }

  await applyAuthPreset(state.editingAuthPresetId, authFormStatusEl);
}

async function refreshAuthPresetQuota(presetId, presetName, statusElement = authFormStatusEl) {
  updateStatus(statusElement, `正在刷新 ${presetName} 的额度…`, "info");
  clearAuthPresetRefreshErrors(presetId);
  updateAuthPresetRefreshingState(presetId, true);

  try {
    const response = await requestJson(
      `/api/auth/presets/${encodeURIComponent(presetId)}/refresh-quota`,
      {
        method: "PUT",
      },
    );
    updateStatus(statusElement, `已刷新额度：${response.preset.name}`, "ok");
    clearAuthPresetRefreshErrors(presetId);
    await refreshAuthPanels();
  } catch (error) {
    const hint = error.message.includes("403")
      ? " — Token 已失效，请重新导入该账号的 auth.json"
      : error.message.includes("401")
        ? " — 认证失败，请检查账号凭据是否有效"
        : error.message.includes("请求") && error.message.includes("失败")
          ? " — 网络异常，请检查网络连接后重试"
          : "";
    setAuthPresetRefreshError(presetId, `${error.message}${hint}`);
    updateStatus(statusElement, `${presetName} 刷新失败：${error.message}${hint}`, "warn");
  } finally {
    updateAuthPresetRefreshingState(presetId, false);
  }
}

function formatRefreshAllAuthQuotaMessage(response) {
  const total = Number(response?.total) || 0;
  const successCount = Number(response?.success_count) || 0;
  const failureCount = Number(response?.failure_count) || 0;
  const failures = Array.isArray(response?.failures) ? response.failures : [];

  if (total === 0) {
    return "没有可刷新的账号预设。";
  }

  if (failureCount === 0) {
    return `已刷新全部 ${successCount} 个账号额度。`;
  }

  const failureDetails = failures
    .slice(0, 3)
    .map((item) => {
      const name = item?.name || item?.preset_id || "未知";
      const error = item?.error || "未知错误";
      return `${name}（${error}）`;
    })
    .join("；");
  const moreSuffix = failureCount > 3 ? `等共 ${failureCount} 个` : "";
  return `已刷新 ${successCount}/${total} 个账号，失败 ${failureCount} 个：${failureDetails}${moreSuffix}。`;
}

async function refreshAllAuthPresetQuotas() {
  if (!state.authPresets.length) {
    updateStatus(authFormStatusEl, "没有可刷新的账号预设。", "warn");
    return;
  }

  const presetIds = state.authPresets.map((preset) => preset.id);
  updateStatus(authFormStatusEl, "正在刷新全部账号额度…", "info");
  authRefreshAllQuotaButton.disabled = true;
  clearAuthPresetRefreshErrors(presetIds);
  updateAuthPresetRefreshingState(presetIds, true);

  try {
    const response = await requestJson("/api/auth/presets/refresh-all-quotas", {
      method: "PUT",
    });
    updateStatus(
      authFormStatusEl,
      formatRefreshAllAuthQuotaMessage(response),
      response.failure_count > 0 ? "warn" : "ok",
    );
    (response.failures || []).forEach((failure) => {
      setAuthPresetRefreshError(failure.preset_id, failure.error);
    });
    await refreshAuthPanels();
  } catch (error) {
    const hint = error.message.includes("Failed to fetch") || error.message.includes("NetworkError")
      ? " — 服务不可达，请检查后端是否正常运行"
      : "";
    presetIds.forEach((presetId) => setAuthPresetRefreshError(presetId, `${error.message}${hint}`));
    updateStatus(authFormStatusEl, `刷新全部额度失败：${error.message}${hint}`, "warn");
  } finally {
    updateAuthPresetRefreshingState(presetIds, false);
    authRefreshAllQuotaButton.disabled = state.authPresets.length === 0;
  }
}

async function testAuthPreset(presetId, presetName) {
  state.authPresetsTesting.add(presetId);
  presetTestPopup.hide();
  renderAuthPresets(state.authPresets);
  try {
    const response = await requestJson(`/api/auth/presets/${encodeURIComponent(presetId)}/test`, {
      method: "POST",
    });
    const result = normalizeTestResult(response.result, {
      presetId,
      fallbackName: presetName,
    });
    state.authPresetTestResults.set(presetId, result);
  } catch (error) {
    state.authPresetTestResults.set(
      presetId,
      errorTestResult(error, { presetId, fallbackName: presetName }),
    );
  } finally {
    state.authPresetsTesting.delete(presetId);
    presetTestPopup.hide();
    renderAuthPresets(state.authPresets);
  }
}

async function testAllAuthPresets() {
  if (!state.authPresets.length) {
    updateStatus(authFormStatusEl, "没有可测试的 OAuth 预设。", "warn");
    return;
  }

  state.authPresets.forEach((preset) => state.authPresetsTesting.add(preset.id));
  setButtonBusy(authTestAllPresetsButton, true, "测试中…");
  presetTestPopup.hide();
  renderAuthPresets(state.authPresets);
  try {
    const response = await requestJson("/api/auth/presets/test-all", {
      method: "POST",
    });
    const results = Array.isArray(response.results) ? response.results : [];
    results.forEach((raw) => {
      const normalized = normalizeTestResult(raw, { presetId: raw.preset_id });
      state.authPresetTestResults.set(normalized.preset_id, normalized);
    });
  } catch (error) {
    state.authPresets.forEach((preset) => {
      state.authPresetTestResults.set(
        preset.id,
        errorTestResult(error, { presetId: preset.id, fallbackName: preset.name }),
      );
    });
  } finally {
    state.authPresetsTesting.clear();
    setButtonBusy(authTestAllPresetsButton, false);
    presetTestPopup.hide();
    renderAuthPresets(state.authPresets);
    authTestAllPresetsButton.disabled = state.authPresets.length === 0;
  }
}

async function deleteAuthPreset(presetId, presetName, statusElement = authFormStatusEl) {
  if (!window.confirm(`确定删除预设"${presetName}"吗？`)) {
    return;
  }

  updateStatus(statusElement, "正在删除 auth 预设…", "info");

  try {
    await requestJson(`/api/auth/presets/${encodeURIComponent(presetId)}`, {
      method: "DELETE",
    });
    if (state.editingAuthPresetId === presetId) {
      resetAuthPresetForm("正在编辑的 auth 预设已删除。", "warn");
    }
    updateStatus(statusElement, `已删除预设：${presetName}`, "ok");
    await loadAuthPresets();
  } catch (error) {
    updateStatus(statusElement, error.message, "warn");
  }
}
function buildEditableAuthText(auth, details = {}, fallbackAccountId = "", fallbackLastRefresh = "") {
  const quota = {};

  if (Number.isFinite(details.hourly_percentage)) {
    quota.hourly_percentage = details.hourly_percentage;
  }
  if (Number.isFinite(details.hourly_reset_time)) {
    quota.hourly_reset_time = details.hourly_reset_time;
  }
  if (Number.isFinite(details.weekly_percentage)) {
    quota.weekly_percentage = details.weekly_percentage;
  }
  if (Number.isFinite(details.weekly_reset_time)) {
    quota.weekly_reset_time = details.weekly_reset_time;
  }

  const editableTokens = {
    access_token: auth?.tokens?.access_token ?? "",
    account_id: auth?.tokens?.account_id ?? fallbackAccountId,
  };
  const idToken = firstNonEmptyString(auth?.tokens?.id_token);
  const refreshToken = firstNonEmptyString(auth?.tokens?.refresh_token);
  if (idToken) {
    editableTokens.id_token = idToken;
  }
  if (refreshToken) {
    editableTokens.refresh_token = refreshToken;
  }

  const editable = {
    email: details.email ?? undefined,
    plan_type: details.plan_type ?? undefined,
    account_name: details.account_name ?? undefined,
    auth_provider: details.login_method ?? undefined,
    last_refresh: auth?.last_refresh ?? fallbackLastRefresh,
    OPENAI_API_KEY: Object.prototype.hasOwnProperty.call(auth ?? {}, "OPENAI_API_KEY")
      ? auth.OPENAI_API_KEY
      : undefined,
    tokens: editableTokens,
  };

  if (Object.keys(quota).length > 0) {
    editable.quota = quota;
  }

  return `${JSON.stringify(editable, null, 2)}\n`;
}

function buildEditablePresetText(preset) {
  return buildEditableAuthText(
    preset.auth,
    preset.details ?? {},
    preset.account_id,
    preset.last_refresh,
  );
}

function populateAuthFormFromRawText(
  rawText,
  {
    presetName = "",
    statusMessage = AUTH_FORM_DEFAULT_STATUS,
    tone = "muted",
  } = {},
) {
  authPresetInputEl.value = rawText;
  authPresetNameEl.value = presetName || buildAuthPresetName(rawText);
  renderConfigOverrideEditor(authConfigOverrideControls, [], { open: false });
  setAuthPresetEditingState("");
  updateStatus(authFormStatusEl, statusMessage, tone);
  authPresetInputEl.focus();
  authPresetInputEl.scrollIntoView({ behavior: "smooth", block: "nearest" });
}

function setAuthPresetEditingState(presetId = "") {
  state.editingAuthPresetId = presetId;
  authSavePresetButton.textContent = presetId ? "保存修改" : "新增";
  authSaveAsNewPresetButton.disabled = !presetId;
  authSaveAsNewPresetButton.hidden = !presetId;
  authSaveAsNewPresetButton.textContent = "另存为新预设";
  authApplyEditedPresetButton.disabled = !presetId;
  authClearInputButton.textContent = presetId ? "取消编辑" : "清空";
  authApplyInputButton.textContent = presetId ? "删除此预设" : "应用到 auth.json";
}

function resetAuthPresetForm(message = "输入内容已清空。", tone = "muted") {
  authPresetInputEl.value = "";
  authPresetNameEl.value = "";
  renderConfigOverrideEditor(authConfigOverrideControls, [], { open: false });
  setAuthPresetEditingState("");
  updateStatus(authFormStatusEl, message || AUTH_FORM_DEFAULT_STATUS, tone);
}

function editAuthPreset(presetId) {
  const preset = state.authPresets.find((item) => item.id === presetId);
  if (!preset) {
    updateStatus(authFormStatusEl, "找不到要编辑的 auth 预设。", "warn");
    return;
  }

  const rawText = buildEditablePresetText(preset);
  authPresetInputEl.value = rawText;
  authPresetNameEl.value = preset.name || buildAuthPresetName(rawText);
  const configOverrides = normalizePresetConfigOverrides(preset);
  renderConfigOverrideEditor(authConfigOverrideControls, configOverrides, {
    open: configOverrides.length > 0,
  });
  setAuthPresetEditingState(preset.id);
  updateStatus(authFormStatusEl, `正在编辑 auth 预设：${preset.name}`, "info");
  authPresetInputEl.focus();
  authPresetInputEl.scrollIntoView({ behavior: "smooth", block: "nearest" });
}

function clearAuthOauthPolling() {
  if (authOauthPollTimer) {
    window.clearTimeout(authOauthPollTimer);
    authOauthPollTimer = null;
  }
}

function setAuthOauthBusy(isBusy) {
  if (!authOauthStartButton) {
    return;
  }
  if (!authOauthStartButton.dataset.defaultLabel) {
    authOauthStartButton.dataset.defaultLabel = authOauthStartButton.textContent;
  }
  authOauthStartButton.disabled = isBusy;
  authOauthStartButton.textContent = isBusy ? "等待登录…" : authOauthStartButton.dataset.defaultLabel;
}

function renderAuthOauthSession(session = null) {
  if (!authOauthSessionPanelEl || !authOauthSessionSummaryEl || !authOauthUserCodeEl) {
    return;
  }

  if (!session) {
    authOauthSessionPanelEl.hidden = true;
    authOauthSessionSummaryEl.textContent = "等待发起…";
    authOauthUserCodeEl.textContent = "-";
    if (authOauthOpenLinkEl) {
      authOauthOpenLinkEl.hidden = true;
      authOauthOpenLinkEl.href = "https://auth.openai.com/codex/device";
    }
    if (authOauthCopyCodeButton) {
      authOauthCopyCodeButton.hidden = true;
      authOauthCopyCodeButton.disabled = true;
    }
    return;
  }

  authOauthSessionPanelEl.hidden = false;
  authOauthUserCodeEl.textContent = session.user_code || "-";
  if (authOauthOpenLinkEl) {
    authOauthOpenLinkEl.hidden = false;
    authOauthOpenLinkEl.href = session.authorize_url || session.verification_url || "https://auth.openai.com/codex/device";
  }
  if (authOauthCopyCodeButton) {
    authOauthCopyCodeButton.hidden = !session.user_code;
    authOauthCopyCodeButton.disabled = !session.user_code;
  }

  const statusLabels = {
    pending: "等待你在官网完成登录",
    completed: "官方登录成功，token 已拿到",
    error: "官方登录失败",
    expired: "官方登录已超时",
  };
  authOauthSessionSummaryEl.textContent = [
    statusLabels[session.status] || "会话状态未知",
    session.user_code ? `验证码 ${session.user_code}` : null,
  ].filter(Boolean).join(" · ");
}

function shouldContinueAuthOauthStart() {
  if (!state.editingAuthPresetId && !authPresetInputEl.value.trim()) {
    return true;
  }

  return window.confirm("当前编辑区已有内容，官方登录成功后会覆盖编辑区，并退出当前编辑状态。是否继续？");
}

function copyAuthOauthUserCode() {
  const code = authOauthUserCodeEl?.textContent?.trim();
  if (!code || code === "-") {
    return;
  }

  if (navigator.clipboard?.writeText) {
    navigator.clipboard.writeText(code).then(
      () => {
        const previousLabel = authOauthCopyCodeButton.textContent;
        authOauthCopyCodeButton.textContent = "已复制";
        window.setTimeout(() => {
          authOauthCopyCodeButton.textContent = previousLabel;
        }, 1200);
      },
      () => {
        window.prompt("复制验证码：", code);
      },
    );
    return;
  }

  window.prompt("复制验证码：", code);
}

function scheduleAuthOauthPoll(delayMs) {
  clearAuthOauthPolling();
  authOauthPollTimer = window.setTimeout(() => {
    pollAuthOauthSession();
  }, Math.max(1500, delayMs || 3000));
}

async function pollAuthOauthSession() {
  const sessionId = state.authOauthSessionId;
  if (!sessionId) {
    clearAuthOauthPolling();
    setAuthOauthBusy(false);
    return;
  }

  try {
    const session = await requestJson(`/api/auth/oauth/codex/sessions/${encodeURIComponent(sessionId)}`);
    renderAuthOauthSession(session);

    if (session.status === "pending") {
      setAuthOauthStatus(
        `等待你在官网完成登录，验证码 ${session.user_code}。完成后会自动回填到编辑器。`,
        "info",
      );
      scheduleAuthOauthPoll((Number(session.poll_interval_seconds) || 3) * 1000);
      return;
    }

    clearAuthOauthPolling();
    setAuthOauthBusy(false);

    if (session.status === "completed" && session.auth) {
      const rawText = buildEditableAuthText(
        session.auth,
        session.details ?? {},
        session.auth?.tokens?.account_id ?? "",
        session.auth?.last_refresh ?? "",
      );
      populateAuthFormFromRawText(rawText, {
        presetName: session.suggested_name || buildAuthPresetName(rawText),
        statusMessage: "已从 Codex 官网登录获取 token，当前内容尚未保存；可直接保存为预设或应用到 auth.json。",
        tone: "ok",
      });
      setAuthOauthStatus("官方登录成功，token 已回填到编辑器。", "ok");
      state.authOauthSessionId = "";
      return;
    }

    const failureMessage = session.error || (session.status === "expired"
      ? "等待官网登录超时，请重新发起登录。"
      : "官方登录失败，请重试。");
    setAuthOauthStatus(failureMessage, "warn");
    state.authOauthSessionId = "";
  } catch (error) {
    clearAuthOauthPolling();
    setAuthOauthBusy(false);
    setAuthOauthStatus(`查询官方登录状态失败：${error.message}`, "warn");
    state.authOauthSessionId = "";
  }
}

async function startAuthOauthSession() {
  if (!shouldContinueAuthOauthStart()) {
    return;
  }

  clearAuthOauthPolling();
  state.authOauthSessionId = "";
  renderAuthOauthSession(null);
  setAuthOauthBusy(true);
  setAuthOauthStatus("正在向 Codex 官网申请登录验证码…", "info");

  try {
    const session = await requestJson("/api/auth/oauth/codex/start", {
      method: "POST",
    });
    state.authOauthSessionId = session.session_id;
    renderAuthOauthSession(session);
    setAuthOauthStatus(
      `已发起官网登录，请在新窗口输入验证码 ${session.user_code} 完成授权。`,
      "info",
    );

    const popup = window.open(
      session.authorize_url || session.verification_url || "https://auth.openai.com/codex/device",
      "_blank",
      "noopener,noreferrer",
    );
    if (!popup) {
      setAuthOauthStatus(
        `已发起官网登录，但浏览器拦截了新窗口。请点击“打开官网”并输入验证码 ${session.user_code}。`,
        "warn",
      );
    }
    scheduleAuthOauthPoll((Number(session.poll_interval_seconds) || 3) * 1000);
  } catch (error) {
    setAuthOauthBusy(false);
    setAuthOauthStatus(error.message, "warn");
    renderAuthOauthSession(null);
  }
}

    return {
      applyAuthFromInput,
      applyAuthImportText,
      applyAuthPreset,
      applyEditingAuthPreset,
      buildAuthPresetName,
      buildEditableAuthText,
      buildEditablePresetText,
      clearAuthOauthPolling,
      copyAuthOauthUserCode,
      deleteAuthPreset,
      editAuthPreset,
      ensureAuthPresetsLoaded,
      extractAuthDetails,
      handleAuthApplyInputAction,
      importAuthJsonFile,
      loadAuthPresets,
      normalizeAuthInput,
      normalizeAuthInputs,
      openAuthImportDialog,
      closeAuthImportDialog,
      populateAuthFormFromRawText,
      pruneAuthPresetRefreshErrors,
      refreshAllAuthPresetQuotas,
      refreshAuthPresetQuota,
      renderAuthOauthSession,
      renderAuthPresets,
      resetAuthPresetForm,
      saveAuthPreset,
      saveAuthPresetAsNew,
      startAuthOauthSession,
      testAllAuthPresets,
    };
  }

  globalThis.WebClxAuthManager = Object.freeze({ create: createAuthManager });
})();
