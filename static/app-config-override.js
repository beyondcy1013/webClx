// webClx 预设配置覆盖 / Codex 默认配置编辑子系统：从 app.js 抽出，保持全局函数声明。
// 依赖运行时全局：state、codexDefaultConfigListEl/authPresetListEl/apiPresetListEl/
// claudePresetListEl、escapeHtml 等。必须在 app.js 之前 <script defer> 加载。

function normalizeConfigOverrideValue(value) {
  return typeof value === "string" ? value : "";
}

function ensureDatalist(id) {
  let datalist = document.getElementById(id);
  if (!datalist) {
    datalist = document.createElement("datalist");
    datalist.id = id;
    document.body.appendChild(datalist);
  }
  return datalist;
}

function appendUniqueValue(list, seen, value) {
  const normalized = normalizeConfigOverrideValue(value).trim();
  if (!normalized || seen.has(normalized)) {
    return;
  }
  seen.add(normalized);
  list.push(normalized);
}

function setDatalistOptions(datalist, values) {
  if (!datalist) {
    return;
  }
  datalist.textContent = "";
  values.forEach((value) => {
    const option = document.createElement("option");
    option.value = value;
    datalist.appendChild(option);
  });
}

function normalizePresetConfigOverrides(source) {
  const items = [];
  const rawOverrides = Array.isArray(source?.config_overrides) ? source.config_overrides : [];
  rawOverrides.forEach((item) => {
    const key = normalizeConfigOverrideValue(item?.key);
    const value = normalizeConfigOverrideValue(item?.value);
    if (!key.trim() && !value.trim()) {
      return;
    }
    items.push({ key, value });
  });
  if (items.length > 0) {
    return items;
  }

  [
    {
      key: normalizeConfigOverrideValue(source?.config_key),
      value: normalizeConfigOverrideValue(source?.config_value),
    },
    {
      key: normalizeConfigOverrideValue(source?.secondary_config_key),
      value: normalizeConfigOverrideValue(source?.secondary_config_value),
    },
  ].forEach((item) => {
    if (!item.key.trim() && !item.value.trim()) {
      return;
    }
    items.push(item);
  });

  return items;
}

function cloneDefaultCodexDefaultConfigEntries() {
  return DEFAULT_CODEX_DEFAULT_CONFIG_ENTRIES.map((entry) => ({ ...entry }));
}

function cloneDefaultClaudeDefaultConfigEntries() {
  return DEFAULT_CLAUDE_DEFAULT_CONFIG_ENTRIES.map((entry) => ({ ...entry }));
}

function normalizeClaudeDefaultConfigEntries(entries) {
  const rawEntries = Array.isArray(entries) ? entries : [];
  const normalized = rawEntries
    .map((entry) => ({
      key: normalizeConfigOverrideValue(entry?.key).trim(),
      value: normalizeConfigOverrideValue(entry?.value).trim(),
    }))
    .filter((entry) => entry.key || entry.value);
  return normalized.length > 0 ? normalized : cloneDefaultClaudeDefaultConfigEntries();
}

function normalizeClaudeDefaultConfigEntriesFromSettings(settings) {
  return normalizeClaudeDefaultConfigEntries(settings?.claude_default_config_entries);
}

function normalizeCodexDefaultConfigEntries(entries) {
  const rawEntries = Array.isArray(entries) ? entries : [];
  const normalized = rawEntries
    .map((entry) => ({
      key: normalizeConfigOverrideValue(entry?.key).trim(),
      value: normalizeConfigOverrideValue(entry?.value).trim(),
    }))
    .filter((entry) => entry.key || entry.value);
  return normalized.length > 0 ? normalized : cloneDefaultCodexDefaultConfigEntries();
}

function normalizeCodexDefaultConfigEntriesFromSettings(settings) {
  if (Array.isArray(settings?.codex_default_config_entries)) {
    return normalizeCodexDefaultConfigEntries(settings.codex_default_config_entries);
  }
  return normalizeCodexDefaultConfigEntries([
    {
      key:
        typeof settings?.codex_config_key === "string" && settings.codex_config_key.trim()
          ? settings.codex_config_key
          : DEFAULT_CODEX_CONFIG_KEY,
      value:
        typeof settings?.codex_config_value === "string"
          ? settings.codex_config_value
          : typeof settings?.codex_model === "string"
            ? settings.codex_model
            : DEFAULT_CODEX_MODEL,
    },
    {
      key:
        typeof settings?.codex_secondary_config_key === "string" &&
        settings.codex_secondary_config_key.trim()
          ? settings.codex_secondary_config_key
          : DEFAULT_CODEX_SECONDARY_CONFIG_KEY,
      value:
        typeof settings?.codex_secondary_config_value === "string" &&
        settings.codex_secondary_config_value.trim()
          ? settings.codex_secondary_config_value
          : DEFAULT_CODEX_SECONDARY_CONFIG_VALUE,
    },
  ]);
}

function normalizeCodexApiAutoProxyMatchProviderIds(values) {
  const rawValues = Array.isArray(values) ? values : DEFAULT_CODEX_API_AUTO_PROXY_MATCH_PROVIDER_IDS;
  const result = [];
  rawValues.forEach((value) => {
    const id = String(value || "").trim().toLowerCase();
    if (CODEX_API_AUTO_PROXY_MATCH_PROVIDER_IDS.has(id) && !result.includes(id)) {
      result.push(id);
    }
  });
  return result;
}

function renderCodexApiAutoProxyMatchProviders(ids = state.codexApiAutoProxyMatchProviderIds) {
  const selected = new Set(normalizeCodexApiAutoProxyMatchProviderIds(ids));
  codexApiAutoProxyProviderInputEls.forEach((input) => {
    const providerId = input.dataset.codexApiAutoProxyProvider || "";
    const provider = CODEX_API_AUTO_PROXY_MATCH_PROVIDERS.find((item) => item.id === providerId);
    input.checked = selected.has(providerId);
    if (provider) {
      input.closest("label")?.setAttribute("title", provider.displayUrls.join("\n"));
    }
  });
}

function readCodexApiAutoProxyMatchProviderIds() {
  return codexApiAutoProxyProviderInputEls
    .filter((input) => input.checked)
    .map((input) => input.dataset.codexApiAutoProxyProvider || "")
    .filter((providerId) => CODEX_API_AUTO_PROXY_MATCH_PROVIDER_IDS.has(providerId));
}

function apiBaseUrlMatchesAutoProxyProvider(baseUrl, providerId) {
  const provider = CODEX_API_AUTO_PROXY_MATCH_PROVIDERS.find((item) => item.id === providerId);
  const normalizedBaseUrl = String(baseUrl || "").trim().toLowerCase();
  return Boolean(
    provider &&
      normalizedBaseUrl &&
      provider.urlPatterns.some((pattern) => normalizedBaseUrl.includes(pattern)),
  );
}

function apiBaseUrlMatchesSelectedAutoProxyProvider(baseUrl) {
  return state.codexApiAutoProxyMatchProviderIds.some((providerId) =>
    apiBaseUrlMatchesAutoProxyProvider(baseUrl, providerId),
  );
}

function syncLegacyCodexConfigStateFromEntries(entries = state.codexDefaultConfigEntries) {
  const normalized = normalizeCodexDefaultConfigEntries(entries);
  const first = normalized[0] || {};
  const second = normalized[1] || {};
  state.codexDefaultConfigEntries = normalized;
  state.codexConfigKey = first.key || DEFAULT_CODEX_CONFIG_KEY;
  state.codexConfigValue = first.value || DEFAULT_CODEX_MODEL;
  state.codexSecondaryConfigKey = second.key || DEFAULT_CODEX_SECONDARY_CONFIG_KEY;
  state.codexSecondaryConfigValue = second.value || DEFAULT_CODEX_SECONDARY_CONFIG_VALUE;
}

function getDefaultConfigOverridePairs() {
  return normalizeCodexDefaultConfigEntries(state.codexDefaultConfigEntries);
}

function normalizePresetConfigOverridePair(item, index = 0) {
  const fallback = getDefaultConfigOverridePairs()[index] || {};
  return {
    key: normalizeConfigOverrideValue(item?.key || fallback.key).trim(),
    value: normalizeConfigOverrideValue(item?.value || fallback.value).trim(),
  };
}

function collectPresetConfigSuggestionPairs(presets) {
  const pairs = [];
  const sources = Array.isArray(presets) ? presets : [];
  sources.forEach((preset) => {
    normalizePresetConfigOverrides(preset).forEach((item, index) => {
      const pair = normalizePresetConfigOverridePair(item, index);
      if (pair.key || pair.value) {
        pairs.push(pair);
      }
    });
  });
  return pairs;
}

function buildConfigOverrideSuggestions() {
  const keys = [];
  const values = [];
  const keySeen = new Set();
  const valueSeen = new Set();
  const valuesByKey = new Map();

  const addPair = (pair) => {
    const key = normalizeConfigOverrideValue(pair?.key).trim();
    const value = normalizeConfigOverrideValue(pair?.value).trim();
    appendUniqueValue(keys, keySeen, key);
    appendUniqueValue(values, valueSeen, value);
    if (!key || !value) {
      return;
    }
    if (!valuesByKey.has(key)) {
      valuesByKey.set(key, []);
    }
    appendUniqueValue(valuesByKey.get(key), new Set(valuesByKey.get(key)), value);
  };

  collectPresetConfigSuggestionPairs(state.apiPresets).forEach(addPair);
  collectPresetConfigSuggestionPairs(state.authPresets).forEach(addPair);
  collectPresetConfigSuggestionPairs(state.claudePresets).forEach(addPair);
  getDefaultConfigOverridePairs().forEach(addPair);

  return { keys, values, valuesByKey };
}

function refreshConfigOverrideDatalists(focusedKey = "") {
  const suggestions = buildConfigOverrideSuggestions();
  setDatalistOptions(ensureDatalist(CONFIG_OVERRIDE_KEY_OPTIONS_ID), suggestions.keys);

  const key = normalizeConfigOverrideValue(focusedKey).trim();
  const keyValues = key ? suggestions.valuesByKey.get(key) || [] : [];
  const mergedValues = [];
  const seen = new Set();
  keyValues.forEach((value) => appendUniqueValue(mergedValues, seen, value));
  suggestions.values.forEach((value) => appendUniqueValue(mergedValues, seen, value));
  setDatalistOptions(ensureDatalist(CONFIG_OVERRIDE_VALUE_OPTIONS_ID), mergedValues);
}

function preferredConfigOverrideValueForKey(key) {
  const normalizedKey = normalizeConfigOverrideValue(key).trim();
  if (!normalizedKey) {
    return "";
  }
  const suggestions = buildConfigOverrideSuggestions();
  return suggestions.valuesByKey.get(normalizedKey)?.[0] || "";
}

function collectPresetConfigKeys(presets) {
  const keys = [];
  const seen = new Set();
  presets.forEach((preset) => {
    normalizePresetConfigOverrides(preset).forEach((item) => {
      const key = item.key.trim();
      if (!key || seen.has(key)) {
        return;
      }
      seen.add(key);
      keys.push(key);
    });
  });
  return keys;
}

function buildPresetConfigValueMap(preset) {
  const values = new Map();
  normalizePresetConfigOverrides(preset).forEach((item) => {
    const key = item.key.trim();
    if (!key) {
      return;
    }
    values.set(key, item.value.trim());
  });
  return values;
}

function createPresetSelectAllHeader(selection = {}) {
  const headerCell = document.createElement("th");
  headerCell.className = "preset-selection-cell";
  headerCell.scope = "col";
  const checkbox = document.createElement("input");
  checkbox.type = "checkbox";
  checkbox.setAttribute("aria-label", `全选 ${selection.label || "账号"}`);
  const ids = (selection.presets || []).map((preset) => preset.id).filter(Boolean);
  const selectedCount = ids.filter((id) => selection.selectedIds?.has(id)).length;
  checkbox.checked = ids.length > 0 && selectedCount === ids.length;
  checkbox.indeterminate = selectedCount > 0 && selectedCount < ids.length;
  checkbox.disabled = ids.length === 0;
  checkbox.addEventListener("change", () => {
    ids.forEach((id) => {
      if (checkbox.checked) {
        selection.selectedIds?.add(id);
      } else {
        selection.selectedIds?.delete(id);
      }
    });
    selection.onChange?.();
  });
  headerCell.appendChild(checkbox);
  return headerCell;
}

function renderPresetTableHeader({
  listEl,
  baseLabels = [],
  configKeys = [],
  configTitlePrefix = "",
  trailingLabels = [],
  tableKey = "",
  sortColumns = [],
  onSortChange = null,
  selection = null,
} = {}) {
  const headerRow = listEl?.closest("table")?.querySelector("thead tr");
  if (!headerRow) {
    return;
  }

  const sortState = typeof getPresetTableSortState === "function"
    ? getPresetTableSortState(tableKey)
    : null;
  const sortColumnByKey = new Map(
    (Array.isArray(sortColumns) ? sortColumns : [])
      .filter((column) => column?.key)
      .map((column) => [column.key, column]),
  );

  const normalizeHeader = (item, isConfig = false) => {
    if (item && typeof item === "object") {
      return {
        label: item.label || "",
        sortKey: item.sortKey || (isConfig ? `config:${item.label || ""}` : ""),
        title: item.title || "",
        className: item.className || "",
      };
    }
    const label = String(item ?? "");
    return {
      label,
      sortKey: isConfig ? `config:${label}` : "",
      title: "",
      className: "",
    };
  };

  const appendHeader = (item, isConfig = false) => {
    const { label, sortKey, title, className } = normalizeHeader(item, isConfig);
    const headerCell = document.createElement("th");
    if (className) headerCell.classList.add(...className.split(/\s+/).filter(Boolean));
    const sortColumn = sortKey ? sortColumnByKey.get(sortKey) : null;
    const active = Boolean(sortColumn && sortState?.key === sortKey);
    if (sortColumn && tableKey && typeof togglePresetTableSort === "function") {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "preset-sort-button";
      button.setAttribute("aria-label", `按 ${label} 排序`);
      button.addEventListener("click", () => {
        togglePresetTableSort(tableKey, sortKey, sortColumn.defaultDirection || "asc");
        if (typeof onSortChange === "function") {
          onSortChange();
        }
      });

      const labelSpan = document.createElement("span");
      labelSpan.textContent = label;
      const icon = document.createElement("span");
      icon.className = "preset-sort-icon";
      icon.setAttribute("aria-hidden", "true");
      icon.textContent = active
        ? (sortState.direction === "desc" ? "↓" : "↑")
        : "↕";
      button.append(labelSpan, icon);
      headerCell.appendChild(button);
      headerCell.classList.add("sortable-table-header");
      headerCell.setAttribute("aria-sort", active
        ? (sortState.direction === "desc" ? "descending" : "ascending")
        : "none");
    } else {
      headerCell.textContent = label;
    }
    if (isConfig) {
      headerCell.classList.add("mono-text");
      if (configTitlePrefix) {
        headerCell.title = `${configTitlePrefix}${label}`;
      }
    } else if (label === "状态指示") {
      headerCell.classList.add("col-status-indicator");
    }
    if (title) {
      headerCell.title = title;
    }
    headerRow.appendChild(headerCell);
  };

  headerRow.replaceChildren();
  if (selection) {
    headerRow.appendChild(createPresetSelectAllHeader(selection));
  }
  baseLabels.forEach((label) => appendHeader(label));
  configKeys.forEach((label) => appendHeader(label, true));
  trailingLabels.forEach((label) => appendHeader(label));
}

function renderAuthPresetTableHeader(configKeys, options = {}) {
  renderPresetTableHeader({
    listEl: authPresetListEl,
    tableKey: "auth",
    sortColumns: options.sortColumns,
    onSortChange: options.onSortChange,
    selection: options.selection,
    baseLabels: [
      { label: "序号" },
      { label: "排序" },
      { label: "切换" },
      { label: "刷新" },
      { label: "测试" },
      { label: "编辑" },
      { label: "状态指示", sortKey: "status" },
      { label: "邮箱", sortKey: "email" },
      { label: "套餐", sortKey: "plan" },
      { label: "Team/账号名", sortKey: "account_name" },
      { label: "Hourly（已用）", sortKey: "hourly" },
      { label: "Weekly（已用）", sortKey: "weekly" },
      { label: "上次刷新", sortKey: "last_refresh" },
      { label: "刷新距今", sortKey: "refresh_age" },
      { label: "保存时间", sortKey: "saved_at" },
      { label: "使用", sortKey: "switch_count" },
      { label: "登录", sortKey: "login" },
    ],
    configKeys,
    configTitlePrefix: "config.toml: ",
    trailingLabels: [{ label: "删除" }],
  });
}

function renderApiPresetTableHeader(configKeys, options = {}) {
  renderPresetTableHeader({
    listEl: apiPresetListEl,
    tableKey: "api",
    sortColumns: options.sortColumns,
    onSortChange: options.onSortChange,
    selection: options.selection,
    baseLabels: [
      { label: "序号" },
      { label: "操作", className: "api-preset-operation-cell" },
      { label: "状态指示", sortKey: "status", className: "api-preset-status-cell" },
      { label: "名字", sortKey: "name", className: "api-preset-name-cell" },
      { label: "Base URL", sortKey: "base_url", className: "api-preset-base-url-cell" },
      { label: "Wire", sortKey: "wire_api" },
      { label: "协议转换", sortKey: "responses_proxy" },
      { label: "本机入口", sortKey: "local_proxy" },
      { label: "管理URL", sortKey: "management_url" },
      { label: "凭据", sortKey: "api_key" },
      { label: "启动环境", sortKey: "terminal_env" },
      { label: "启动脚本", sortKey: "terminal_script" },
      { label: "使用", sortKey: "switch_count" },
    ],
    configKeys,
    configTitlePrefix: "config.toml: ",
    trailingLabels: [{ label: "保存时间", sortKey: "saved_at" }],
  });
}

function renderClaudePresetTableHeader(configKeys, options = {}) {
  renderPresetTableHeader({
    listEl: claudePresetListEl,
    tableKey: "claude",
    sortColumns: options.sortColumns,
    onSortChange: options.onSortChange,
    selection: options.selection,
    baseLabels: [
      { label: "序号" },
      { label: "排序" },
      { label: "切换" },
      { label: "测试" },
      { label: "OpenCode" },
      { label: "编辑" },
      { label: "删除" },
      { label: "状态指示", sortKey: "status" },
      { label: "名字", sortKey: "name" },
      { label: "Base URL", sortKey: "base_url" },
      { label: "管理URL", sortKey: "management_url" },
      { label: "Token", sortKey: "token" },
      { label: "模型", sortKey: "models" },
      { label: "协议转换", sortKey: "access_mode" },
      { label: "使用", sortKey: "switch_count" },
    ],
    configKeys,
    configTitlePrefix: "settings.json env: ",
    trailingLabels: [{ label: "保存时间", sortKey: "saved_at" }],
  });
}

function collectConfigOverrideEditorValues(controls) {
  if (!controls?.listEl) {
    return [];
  }

  return Array.from(controls.listEl.querySelectorAll(".config-override-row"))
    .map((row) => {
      const keyInput = row.querySelector(".config-override-key-input");
      const valueInput = row.querySelector(".config-override-value-input");
      const key = keyInput?.value?.trim() || "";
      const value = valueInput?.value?.trim() || "";
      return {
        key: key || null,
        value: value || null,
      };
    })
    .filter((item) => item.key || item.value);
}

function syncConfigOverrideEditorState(controls, { keepOpen = true } = {}) {
  if (!controls?.listEl || !controls?.summaryEl || !controls?.detailsEl) {
    return;
  }

  const rows = Array.from(controls.listEl.querySelectorAll(".config-override-row"));
  rows.forEach((row, index) => {
    row.setAttribute("aria-label", `覆盖项 ${index + 1}`);
  });

  if (rows.length === 0) {
    const emptyEl = document.createElement("p");
    emptyEl.className = "meta-text config-override-empty";
    emptyEl.textContent = controls.emptyMessage || "当前没有额外覆盖项。";
    controls.listEl.replaceChildren(emptyEl);
    controls.summaryEl.textContent = "未设置";
    if (!keepOpen) {
      controls.detailsEl.open = false;
    }
    return;
  }

  controls.summaryEl.textContent = `已添加 ${rows.length} 项`;
  if (keepOpen) {
    controls.detailsEl.open = true;
  }
}

function createConfigOverrideTable() {
  const table = document.createElement("table");
  table.className = "config-override-table";

  const header = document.createElement("thead");
  const headerRow = document.createElement("tr");
  ["键名", "键值", "作用域", ""].forEach((label) => {
    const cell = document.createElement("th");
    cell.scope = "col";
    cell.textContent = label;
    headerRow.appendChild(cell);
  });
  header.appendChild(headerRow);

  const body = document.createElement("tbody");
  table.append(header, body);
  return table;
}

function ensureConfigOverrideTable(controls) {
  let table = controls.listEl.querySelector(".config-override-table");
  if (!table) {
    table = createConfigOverrideTable();
    controls.listEl.replaceChildren(table);
  }
  return table;
}

function createConfigOverrideRow(controls, override = {}) {
  const row = document.createElement("tr");
  row.className = "config-override-row";

  const keyCell = document.createElement("td");
  keyCell.className = "config-override-key-cell";

  const keyLabel = document.createElement("span");
  keyLabel.className = "sr-only";
  keyLabel.textContent = "键名";

  const keyInput = document.createElement("input");
  keyInput.className = "text-input mono-text config-override-key-input";
  keyInput.type = "text";
  keyInput.placeholder = "model 或 features.goals";
  keyInput.autocomplete = "off";
  keyInput.spellcheck = false;
  keyInput.setAttribute("aria-label", "键名");
  keyInput.setAttribute("list", CONFIG_OVERRIDE_KEY_OPTIONS_ID);
  keyInput.value = normalizeConfigOverrideValue(override.key);

  const valueCell = document.createElement("td");
  valueCell.className = "config-override-value-cell";

  const valueLabel = document.createElement("span");
  valueLabel.className = "sr-only";
  valueLabel.textContent = "键值";

  const valueInput = document.createElement("input");
  valueInput.className = "text-input mono-text config-override-value-input";
  valueInput.type = "text";
  valueInput.placeholder = "留空用默认值";
  valueInput.autocomplete = "off";
  valueInput.spellcheck = false;
  valueInput.setAttribute("aria-label", "键值");
  valueInput.setAttribute("list", CONFIG_OVERRIDE_VALUE_OPTIONS_ID);
  valueInput.value = normalizeConfigOverrideValue(override.value);

  const syncValueSuggestionForKey = () => {
    refreshConfigOverrideDatalists(keyInput.value);
    if (!valueInput.value.trim()) {
      valueInput.value = preferredConfigOverrideValueForKey(keyInput.value);
    }
  };

  keyInput.addEventListener("focus", () => {
    refreshConfigOverrideDatalists(keyInput.value);
  });
  keyInput.addEventListener("input", syncValueSuggestionForKey);
  keyInput.addEventListener("change", syncValueSuggestionForKey);
  valueInput.addEventListener("focus", () => {
    refreshConfigOverrideDatalists(keyInput.value);
  });

  const scopeCell = document.createElement("td");
  scopeCell.className = "config-override-scope-cell codex-default-config-scope";
  const renderScope = () => {
    const scope = codexConfigScopeForKey(keyInput.value);
    scopeCell.textContent = scope.label;
    scopeCell.dataset.scope = scope.kind;
    scopeCell.title = scope.title;
  };
  keyInput.addEventListener("input", renderScope);
  renderScope();

  const actionCell = document.createElement("td");
  actionCell.className = "config-override-action-cell";

  const removeButton = document.createElement("button");
  removeButton.type = "button";
  removeButton.className = "mini-button";
  removeButton.textContent = "删除";
  removeButton.addEventListener("click", () => {
    row.remove();
    syncConfigOverrideEditorState(controls);
  });

  keyCell.append(keyLabel, keyInput);
  valueCell.append(valueLabel, valueInput);
  actionCell.appendChild(removeButton);

  row.append(keyCell, valueCell, scopeCell, actionCell);
  return row;
}

function renderConfigOverrideEditor(controls, overrides = [], { open = false } = {}) {
  if (!controls?.listEl || !controls?.detailsEl) {
    return;
  }

  const normalized = Array.isArray(overrides) ? overrides : [];
  controls.listEl.replaceChildren();
  if (normalized.length > 0) {
    const table = ensureConfigOverrideTable(controls);
    const body = table.querySelector("tbody");
    normalized.forEach((override) => {
      body.appendChild(createConfigOverrideRow(controls, override));
    });
  }
  controls.detailsEl.open = open || normalized.length > 0;
  syncConfigOverrideEditorState(controls, { keepOpen: controls.detailsEl.open });
}

function addConfigOverrideRow(controls, override = {}) {
  if (!controls?.listEl || !controls?.detailsEl) {
    return;
  }

  refreshConfigOverrideDatalists(override.key || "");
  const emptyEl = controls.listEl.querySelector(".config-override-empty");
  if (emptyEl) {
    emptyEl.remove();
  }
  const table = ensureConfigOverrideTable(controls);
  const row = createConfigOverrideRow(controls, override);
  table.querySelector("tbody").appendChild(row);
  controls.detailsEl.open = true;
  syncConfigOverrideEditorState(controls);
  focusTextInputToEnd(row.querySelector(".config-override-key-input"));
}

function createCodexDefaultConfigRow(entry = {}) {
  const row = document.createElement("tr");
  row.className = "codex-default-config-row";

  const keyCell = document.createElement("td");
  const keyInput = document.createElement("input");
  keyInput.className = "text-input mono-text codex-default-config-key-input";
  keyInput.type = "text";
  keyInput.placeholder = "model 或 features.goals";
  keyInput.autocomplete = "off";
  keyInput.spellcheck = false;
  keyInput.setAttribute("aria-label", "config 键名");
  keyInput.setAttribute("list", CONFIG_OVERRIDE_KEY_OPTIONS_ID);
  keyInput.value = normalizeConfigOverrideValue(entry.key);

  const valueCell = document.createElement("td");
  const valueInput = document.createElement("input");
  valueInput.className = "text-input mono-text codex-default-config-value-input";
  valueInput.type = "text";
  valueInput.placeholder = "留空则不写入";
  valueInput.autocomplete = "off";
  valueInput.spellcheck = false;
  valueInput.setAttribute("aria-label", "config 默认键值");
  valueInput.setAttribute("list", CONFIG_OVERRIDE_VALUE_OPTIONS_ID);
  valueInput.value = normalizeConfigOverrideValue(entry.value);

  keyInput.addEventListener("focus", () => refreshConfigOverrideDatalists(keyInput.value));
  keyInput.addEventListener("input", () => refreshConfigOverrideDatalists(keyInput.value));
  valueInput.addEventListener("focus", () => refreshConfigOverrideDatalists(keyInput.value));

  const scopeCell = document.createElement("td");
  scopeCell.className = "codex-default-config-scope";
  const renderScope = () => {
    const scope = codexConfigScopeForKey(keyInput.value);
    scopeCell.textContent = scope.label;
    scopeCell.dataset.scope = scope.kind;
    scopeCell.title = scope.title;
  };
  keyInput.addEventListener("input", renderScope);
  renderScope();

  const actionCell = document.createElement("td");
  const removeButton = document.createElement("button");
  removeButton.type = "button";
  removeButton.className = "mini-button";
  removeButton.dataset.action = "delete-codex-default-config";
  removeButton.textContent = "删除";
  actionCell.appendChild(removeButton);

  keyCell.appendChild(keyInput);
  valueCell.appendChild(valueInput);
  row.append(keyCell, valueCell, scopeCell, actionCell);
  return row;
}

function codexConfigScopeForKey(key) {
  const normalized = normalizeConfigOverrideValue(key).trim().toLowerCase();
  const providerOwned =
    normalized === "model_provider" ||
    normalized === "provider" ||
    normalized === "wire_api" ||
    normalized === "model_providers" ||
    normalized.startsWith("model_providers.");
  if (providerOwned) {
    return {
      kind: "provider",
      label: "预设 Provider",
      title: "由 Codex_API 预设管理",
    };
  }
  if (normalized.includes(".")) {
    return {
      kind: "table",
      label: "配置表",
      title: "写入点号路径对应的 config.toml 表",
    };
  }
  return {
    kind: "root",
    label: "顶级键",
    title: "写入 config.toml 顶级",
  };
}

function renderCodexDefaultConfigEntries(entries = state.codexDefaultConfigEntries) {
  if (!codexDefaultConfigListEl) {
    return;
  }
  const normalized = normalizeCodexDefaultConfigEntries(entries);
  codexDefaultConfigListEl.replaceChildren(
    ...normalized.map((entry) => createCodexDefaultConfigRow(entry)),
  );
}

function readCodexDefaultConfigEntriesFromTable() {
  if (!codexDefaultConfigListEl) {
    return cloneDefaultCodexDefaultConfigEntries();
  }
  return normalizeCodexDefaultConfigEntries(
    Array.from(codexDefaultConfigListEl.querySelectorAll(".codex-default-config-row")).map(
      (row) => ({
        key: row.querySelector(".codex-default-config-key-input")?.value || "",
        value: row.querySelector(".codex-default-config-value-input")?.value || "",
      }),
    ),
  );
}

function addCodexDefaultConfigRow(entry = {}) {
  if (!codexDefaultConfigListEl) {
    return;
  }
  const row = createCodexDefaultConfigRow(entry);
  codexDefaultConfigListEl.appendChild(row);
  focusTextInputToEnd(row.querySelector(".codex-default-config-key-input"));
}

function createClaudeDefaultConfigRow(entry = {}) {
  const row = document.createElement("tr");
  row.className = "claude-default-config-row";

  const keyCell = document.createElement("td");
  const keyInput = document.createElement("input");
  keyInput.className = "text-input mono-text claude-default-config-key-input";
  keyInput.type = "text";
  keyInput.placeholder = "ANTHROPIC_DEFAULT_SONNET_MODEL";
  keyInput.autocomplete = "off";
  keyInput.spellcheck = false;
  keyInput.setAttribute("aria-label", "Claude env 环境变量");
  keyInput.value = normalizeConfigOverrideValue(entry.key);

  const valueCell = document.createElement("td");
  const valueInput = document.createElement("input");
  valueInput.className = "text-input mono-text claude-default-config-value-input";
  valueInput.type = "text";
  valueInput.placeholder = "默认值";
  valueInput.autocomplete = "off";
  valueInput.spellcheck = false;
  valueInput.setAttribute("aria-label", "Claude env 默认值");
  valueInput.value = normalizeConfigOverrideValue(entry.value);

  const actionCell = document.createElement("td");
  const removeButton = document.createElement("button");
  removeButton.type = "button";
  removeButton.className = "mini-button";
  removeButton.dataset.action = "delete-claude-default-config";
  removeButton.textContent = "删除";
  actionCell.appendChild(removeButton);

  keyCell.appendChild(keyInput);
  valueCell.appendChild(valueInput);
  row.append(keyCell, valueCell, actionCell);
  return row;
}

function renderClaudeDefaultConfigEntries(entries = state.claudeDefaultConfigEntries) {
  if (!claudeDefaultConfigListEl) {
    return;
  }
  const normalized = normalizeClaudeDefaultConfigEntries(entries);
  claudeDefaultConfigListEl.replaceChildren(
    ...normalized.map((entry) => createClaudeDefaultConfigRow(entry)),
  );
}

function readClaudeDefaultConfigEntriesFromTable() {
  if (!claudeDefaultConfigListEl) {
    return cloneDefaultClaudeDefaultConfigEntries();
  }
  return normalizeClaudeDefaultConfigEntries(
    Array.from(claudeDefaultConfigListEl.querySelectorAll(".claude-default-config-row")).map(
      (row) => ({
        key: row.querySelector(".claude-default-config-key-input")?.value || "",
        value: row.querySelector(".claude-default-config-value-input")?.value || "",
      }),
    ),
  );
}

function addClaudeDefaultConfigRow(entry = {}) {
  if (!claudeDefaultConfigListEl) {
    return;
  }
  const row = createClaudeDefaultConfigRow(entry);
  claudeDefaultConfigListEl.appendChild(row);
  focusTextInputToEnd(row.querySelector(".claude-default-config-key-input"));
}
