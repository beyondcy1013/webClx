// webClx 终端命令/快捷键/环境变量规范化子系统：从 app.js 抽出，保持全局函数声明。
// 依赖运行时全局：state、terminalShortcutsListEl/terminalSlashCommandsInputEl 等，
// 以及 app.js 顶部从 WebClxTerminalSettings 解构出的 DEFAULT_*/normalize* 常量。
// 必须在 app.js 之前 <script defer> 加载，函数仅在初始化后被调用。

// 快捷键子 Tab 中两个“多功能按钮”（/斜杠、功能）的展开状态：默认全部折叠（不选中），
// 选中后才渲染该组下的子命令行；持久化到 localStorage 跨刷新保留。
const TERMINAL_SHORTCUT_EXPANDED_GROUPS_STORAGE_KEY = "webclx:terminal-shortcut-expanded-groups";
const TERMINAL_SHORTCUT_ALL_GROUP_KEYS = ["slash", "function"];

function parseTerminalRenamePresetsInput(value) {
  return normalizeTerminalRenamePresets(
    String(value || "")
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter((line) => line && !line.startsWith("#")),
    null,
  );
}

function formatTerminalRenamePresets(presets) {
  return normalizeTerminalRenamePresets(presets, null).join("\n");
}

function parseTerminalFunctionCommandsInput(value) {
  const commands = String(value || "")
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith("#"))
    .map((line) => {
      const [key = "", label = "", action = "", rawCommand = "", shortcut = ""] =
        line.split("|");
      const command = String(rawCommand)
        .trimStart()
        .replace(/[ \t]$/, "");
      return {
        key: key.trim(),
        label: label.trim(),
        action: action.trim(),
        command,
        shortcut: shortcut.trim(),
      };
    });
  return normalizeTerminalFunctionCommands(commands, null);
}

function formatTerminalFunctionCommands(commands) {
  return normalizeTerminalFunctionCommands(commands, null)
    .map((command) =>
      [command.key, command.label, command.action, command.command, command.shortcut].join(" | ")
    )
    .join("\n");
}

function terminalShortcutGroups() {
  return [
    {
      key: "slash",
      label: "快捷",
      commands: state.terminalSlashCommands,
      defaults: state.defaultTerminalSlashCommands,
    },
    {
      key: "function",
      label: "功能",
      commands: state.terminalFunctionCommands,
      defaults: state.defaultTerminalFunctionCommands,
    },
  ];
}

function syncTerminalCommandTextareasFromState() {
  if (terminalSlashCommandsInputEl) {
    terminalSlashCommandsInputEl.value = formatTerminalFunctionCommands(state.terminalSlashCommands);
  }
  if (terminalFunctionCommandsInputEl) {
    terminalFunctionCommandsInputEl.value = formatTerminalFunctionCommands(
      state.terminalFunctionCommands,
    );
  }
}

function syncTerminalCommandStateFromTextareas() {
  state.terminalSlashCommands = ensureBuiltInTerminalSlashCommands(
    parseTerminalFunctionCommandsInput(terminalSlashCommandsInputEl?.value || ""),
  );
  state.terminalFunctionCommands = filterMovedSlashCommands(
    parseTerminalFunctionCommandsInput(terminalFunctionCommandsInputEl?.value || ""),
  );
  renderTerminalShortcutSettings();
}

function updateTerminalShortcutCommand(groupKey, index, shortcut) {
  const normalizedShortcut = normalizeTerminalQuickText(shortcut, 80);
  const targetKey = groupKey === "function" ? "terminalFunctionCommands" : "terminalSlashCommands";
  const commands = Array.isArray(state[targetKey]) ? [...state[targetKey]] : [];
  if (!Number.isInteger(index) || index < 0 || index >= commands.length) {
    return;
  }
  commands[index] = { ...commands[index], shortcut: normalizedShortcut };
  state[targetKey] = commands;
  syncTerminalCommandTextareasFromState();
}

function commitTerminalShortcutInputs() {
  if (!terminalShortcutsListEl) {
    return;
  }
  terminalShortcutsListEl.querySelectorAll(".terminal-shortcut-input").forEach((input) => {
    const groupKey = input.dataset.commandGroup || "";
    const index = Number(input.dataset.commandIndex);
    updateTerminalShortcutCommand(groupKey, index, input.value);
  });
}

function applyDefaultTerminalShortcuts(commands, defaults) {
  const defaultShortcuts = new Map(
    normalizeTerminalFunctionCommands(defaults, null).map((command) => [
      command.key,
      command.shortcut || "",
    ]),
  );
  return normalizeTerminalFunctionCommands(commands, null).map((command) => ({
    ...command,
    shortcut: defaultShortcuts.get(command.key) || "",
  }));
}

function resetTerminalShortcutsToDefaults() {
  state.terminalSlashCommands = applyDefaultTerminalShortcuts(
    state.terminalSlashCommands,
    state.defaultTerminalSlashCommands,
  );
  state.terminalFunctionCommands = applyDefaultTerminalShortcuts(
    state.terminalFunctionCommands,
    state.defaultTerminalFunctionCommands,
  );
  syncTerminalCommandTextareasFromState();
  renderTerminalShortcutSettings();
  updateStatus(settingsStatusEl, "快捷键已恢复默认；点击“保存设置”后才会持久化。", "muted");
}

function renderTerminalCommandCollectionsEditor() {
  if (!terminalCommandCollectionsEditorEl) {
    return;
  }

  terminalCommandCollectionsEditorEl.replaceChildren();
  const collections = Array.isArray(state.terminalCommandCollections)
    ? state.terminalCommandCollections
    : [];

  if (collections.length === 0) {
    const empty = document.createElement("p");
    empty.className = "terminal-command-collection-empty meta-text";
    empty.textContent = "暂无命令合集。点击右上角“添加合集”创建。";
    terminalCommandCollectionsEditorEl.appendChild(empty);
    return;
  }

  collections.forEach((collection, collectionIndex) => {
    const card = document.createElement("div");
    card.className = "terminal-command-collection-editor-card";

    const head = document.createElement("div");
    head.className = "terminal-command-collection-editor-head";

    const labelInput = document.createElement("input");
    labelInput.className = "text-input";
    labelInput.type = "text";
    labelInput.value = collection.label || "";
    labelInput.placeholder = "合集名称";
    labelInput.dataset.role = "collection-label";
    labelInput.dataset.collectionIndex = String(collectionIndex);
    labelInput.addEventListener("input", () => {
      const idx = Number(labelInput.dataset.collectionIndex);
      if (!state.terminalCommandCollections[idx]) {
        return;
      }
      state.terminalCommandCollections[idx].label = labelInput.value;
    });
    head.appendChild(labelInput);

    const deleteCollectionBtn = document.createElement("button");
    deleteCollectionBtn.type = "button";
    deleteCollectionBtn.className = "mini-button danger";
    deleteCollectionBtn.textContent = "删除合集";
    deleteCollectionBtn.dataset.role = "collection-delete";
    deleteCollectionBtn.dataset.collectionIndex = String(collectionIndex);
    deleteCollectionBtn.addEventListener("click", () => {
      const idx = Number(deleteCollectionBtn.dataset.collectionIndex);
      state.terminalCommandCollections.splice(idx, 1);
      renderTerminalCommandCollectionsEditor();
    });
    head.appendChild(deleteCollectionBtn);

    card.appendChild(head);

    const itemsWrap = document.createElement("div");
    itemsWrap.className = "terminal-command-collection-items";

    (collection.commands || []).forEach((item, itemIndex) => {
      const row = document.createElement("div");
      row.className = "terminal-command-collection-item-row";

      const itemLabelInput = document.createElement("input");
      itemLabelInput.className = "text-input";
      itemLabelInput.type = "text";
      itemLabelInput.value = item.label || "";
      itemLabelInput.placeholder = "标签";
      itemLabelInput.dataset.role = "item-label";
      itemLabelInput.dataset.collectionIndex = String(collectionIndex);
      itemLabelInput.dataset.itemIndex = String(itemIndex);
      itemLabelInput.addEventListener("input", () => {
        const ci = Number(itemLabelInput.dataset.collectionIndex);
        const ii = Number(itemLabelInput.dataset.itemIndex);
        const target = state.terminalCommandCollections[ci]?.commands?.[ii];
        if (target) {
          target.label = itemLabelInput.value;
        }
      });
      row.appendChild(itemLabelInput);

      const itemCommandInput = document.createElement("input");
      itemCommandInput.className = "text-input mono-text";
      itemCommandInput.type = "text";
      itemCommandInput.value = item.command || "";
      itemCommandInput.placeholder = "命令（如 codex --version）";
      itemCommandInput.dataset.role = "item-command";
      itemCommandInput.dataset.collectionIndex = String(collectionIndex);
      itemCommandInput.dataset.itemIndex = String(itemIndex);
      itemCommandInput.addEventListener("input", () => {
        const ci = Number(itemCommandInput.dataset.collectionIndex);
        const ii = Number(itemCommandInput.dataset.itemIndex);
        const target = state.terminalCommandCollections[ci]?.commands?.[ii];
        if (target) {
          target.command = itemCommandInput.value;
        }
      });
      row.appendChild(itemCommandInput);

      const actionSelect = document.createElement("select");
      actionSelect.className = "text-input";
      actionSelect.dataset.role = "item-action";
      actionSelect.dataset.collectionIndex = String(collectionIndex);
      actionSelect.dataset.itemIndex = String(itemIndex);
      [
        { value: "send_text", label: "发送+回车" },
        { value: "insert_text", label: "仅插入" },
      ].forEach((opt) => {
        const option = document.createElement("option");
        option.value = opt.value;
        option.textContent = opt.label;
        if ((item.action || "send_text") === opt.value) {
          option.selected = true;
        }
        actionSelect.appendChild(option);
      });
      actionSelect.addEventListener("change", () => {
        const ci = Number(actionSelect.dataset.collectionIndex);
        const ii = Number(actionSelect.dataset.itemIndex);
        const target = state.terminalCommandCollections[ci]?.commands?.[ii];
        if (target) {
          target.action = actionSelect.value;
        }
      });
      row.appendChild(actionSelect);

      const deleteItemBtn = document.createElement("button");
      deleteItemBtn.type = "button";
      deleteItemBtn.className = "mini-button danger";
      deleteItemBtn.textContent = "删除";
      deleteItemBtn.dataset.role = "item-delete";
      deleteItemBtn.dataset.collectionIndex = String(collectionIndex);
      deleteItemBtn.dataset.itemIndex = String(itemIndex);
      deleteItemBtn.addEventListener("click", () => {
        const ci = Number(deleteItemBtn.dataset.collectionIndex);
        const ii = Number(deleteItemBtn.dataset.itemIndex);
        const target = state.terminalCommandCollections[ci];
        if (target && Array.isArray(target.commands)) {
          target.commands.splice(ii, 1);
          renderTerminalCommandCollectionsEditor();
        }
      });
      row.appendChild(deleteItemBtn);

      itemsWrap.appendChild(row);
    });

    const addItemBtn = document.createElement("button");
    addItemBtn.type = "button";
    addItemBtn.className = "mini-button secondary";
    addItemBtn.textContent = "添加命令";
    addItemBtn.dataset.role = "item-add";
    addItemBtn.dataset.collectionIndex = String(collectionIndex);
    addItemBtn.addEventListener("click", () => {
      const ci = Number(addItemBtn.dataset.collectionIndex);
      const target = state.terminalCommandCollections[ci];
      if (!target) {
        return;
      }
      if (!Array.isArray(target.commands)) {
        target.commands = [];
      }
      if (target.commands.length >= MAX_TERMINAL_COMMAND_COLLECTION_ITEMS) {
        updateStatus(settingsStatusEl, `单个合集最多 ${MAX_TERMINAL_COMMAND_COLLECTION_ITEMS} 条命令。`, "warn");
        return;
      }
      target.commands.push({ label: "", action: "send_text", command: "" });
      renderTerminalCommandCollectionsEditor();
    });
    itemsWrap.appendChild(addItemBtn);

    card.appendChild(itemsWrap);
    terminalCommandCollectionsEditorEl.appendChild(card);
  });
}

function addTerminalCommandCollection() {
  if (!Array.isArray(state.terminalCommandCollections)) {
    state.terminalCommandCollections = [];
  }
  if (state.terminalCommandCollections.length >= MAX_TERMINAL_COMMAND_COLLECTIONS) {
    updateStatus(settingsStatusEl, `命令合集最多 ${MAX_TERMINAL_COMMAND_COLLECTIONS} 个。`, "warn");
    return;
  }
  const fallbackIndex = state.terminalCommandCollections.length + 1;
  state.terminalCommandCollections.push({
    key: `collection_${Date.now()}`,
    label: `新合集 ${fallbackIndex}`,
    commands: [{ label: "", action: "send_text", command: "" }],
  });
  renderTerminalCommandCollectionsEditor();
}

function resetTerminalCommandCollectionsToDefaults() {
  state.terminalCommandCollections = normalizeTerminalCommandCollections(
    cloneDefaultTerminalCommandCollections(),
  );
  renderTerminalCommandCollectionsEditor();
}

function loadTerminalShortcutExpandedGroups() {
  const result = {};
  TERMINAL_SHORTCUT_ALL_GROUP_KEYS.forEach((key) => {
    result[key] = false;
  });
  try {
    const raw = window.localStorage.getItem(TERMINAL_SHORTCUT_EXPANDED_GROUPS_STORAGE_KEY);
    if (!raw) {
      return result;
    }
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") {
      return result;
    }
    TERMINAL_SHORTCUT_ALL_GROUP_KEYS.forEach((key) => {
      if (typeof parsed[key] === "boolean") {
        result[key] = parsed[key];
      }
    });
  } catch {
    // 忽略解析失败，保持默认全部折叠。
  }
  return result;
}

function persistTerminalShortcutExpandedGroups(expandedMap) {
  try {
    window.localStorage.setItem(
      TERMINAL_SHORTCUT_EXPANDED_GROUPS_STORAGE_KEY,
      JSON.stringify(expandedMap || {}),
    );
  } catch {
    // 忽略写入失败，运行时仍能切换展开状态。
  }
}

function isTerminalShortcutGroupExpanded(groupKey) {
  const map = state.terminalShortcutExpandedGroups || {};
  return Boolean(map[groupKey]);
}

function setTerminalShortcutGroupExpanded(groupKey, expanded) {
  if (!TERMINAL_SHORTCUT_ALL_GROUP_KEYS.includes(groupKey)) {
    return;
  }
  if (!state.terminalShortcutExpandedGroups) {
    state.terminalShortcutExpandedGroups = loadTerminalShortcutExpandedGroups();
  }
  state.terminalShortcutExpandedGroups[groupKey] = Boolean(expanded);
  persistTerminalShortcutExpandedGroups(state.terminalShortcutExpandedGroups);
  renderTerminalShortcutSettings();
}

function toggleTerminalShortcutGroupExpanded(groupKey) {
  setTerminalShortcutGroupExpanded(groupKey, !isTerminalShortcutGroupExpanded(groupKey));
}

function renderTerminalShortcutGroupHeader(group, commandCount) {
  const row = document.createElement("tr");
  row.className = "terminal-shortcut-group-row";

  const selectCell = document.createElement("td");
  selectCell.className = "terminal-shortcut-group-select";
  const checkbox = document.createElement("input");
  checkbox.type = "checkbox";
  checkbox.className = "terminal-shortcut-group-toggle";
  checkbox.checked = isTerminalShortcutGroupExpanded(group.key);
  checkbox.dataset.groupKey = group.key;
  checkbox.setAttribute(
    "aria-label",
    `展开 ${group.label} 子命令快捷键`,
  );
  checkbox.addEventListener("change", () => {
    setTerminalShortcutGroupExpanded(group.key, checkbox.checked);
  });
  selectCell.appendChild(checkbox);
  row.appendChild(selectCell);

  const groupCell = document.createElement("td");
  groupCell.className = "terminal-shortcut-group-name";
  groupCell.textContent = group.label;
  row.appendChild(groupCell);

  const descCell = document.createElement("td");
  descCell.colSpan = 2;
  descCell.className = "terminal-shortcut-group-desc muted-text";
  descCell.textContent = `${commandCount} 条子命令；勾选后展开显示每个子命令的快捷键`;
  row.appendChild(descCell);

  const shortcutCell = document.createElement("td");
  shortcutCell.className = "terminal-shortcut-group-shortcut";
  if (commandCount === 0) {
    shortcutCell.textContent = "—";
    shortcutCell.classList.add("muted-text");
  } else {
    const expanded = isTerminalShortcutGroupExpanded(group.key);
    shortcutCell.textContent = expanded ? "已展开" : "未展开";
    shortcutCell.classList.add(expanded ? "is-expanded" : "is-collapsed");
  }
  row.appendChild(shortcutCell);

  return row;
}

function renderTerminalShortcutSettings() {
  if (!terminalShortcutsListEl) {
    return;
  }

  if (!state.terminalShortcutExpandedGroups) {
    state.terminalShortcutExpandedGroups = loadTerminalShortcutExpandedGroups();
  }

  terminalShortcutsListEl.innerHTML = "";

  const groups = terminalShortcutGroups();
  let totalSubCommandCount = 0;
  let renderedSubCommandCount = 0;
  let selectedGroupCount = 0;

  groups.forEach((group) => {
    const commands = normalizeTerminalFunctionCommands(group.commands, null);
    totalSubCommandCount += commands.length;
    const expanded = isTerminalShortcutGroupExpanded(group.key);
    if (expanded) {
      selectedGroupCount += 1;
    }

    terminalShortcutsListEl.appendChild(renderTerminalShortcutGroupHeader(group, commands.length));

    if (!expanded) {
      return;
    }

    if (commands.length === 0) {
      const emptyRow = document.createElement("tr");
      emptyRow.className = "terminal-shortcut-empty-row";
      const emptyCell = document.createElement("td");
      emptyCell.colSpan = 5;
      emptyCell.className = "settings-value muted-text";
      emptyCell.textContent = "该组暂无子命令。";
      emptyRow.appendChild(emptyCell);
      terminalShortcutsListEl.appendChild(emptyRow);
      return;
    }

    commands.forEach((command, index) => {
      renderedSubCommandCount += 1;
      const row = document.createElement("tr");
      row.className = "terminal-shortcut-subcommand-row";

      const indentCell = document.createElement("td");
      indentCell.className = "terminal-shortcut-indent muted-text";
      indentCell.textContent = "└";
      row.appendChild(indentCell);

      const groupCell = document.createElement("td");
      groupCell.className = "terminal-shortcut-group-tag muted-text";
      groupCell.textContent = group.label;
      row.appendChild(groupCell);

      const labelCell = document.createElement("td");
      const labelText = document.createElement("div");
      labelText.className = "terminal-shortcut-label";
      labelText.textContent = command.label || command.key;
      const keyText = document.createElement("div");
      keyText.className = "terminal-shortcut-key mono-text";
      keyText.textContent = command.key;
      labelCell.append(labelText, keyText);
      row.appendChild(labelCell);

      const actionCell = document.createElement("td");
      const actionText = document.createElement("div");
      actionText.className = "mono-text terminal-shortcut-action";
      actionText.textContent = command.action || "send_text";
      const commandText = document.createElement("div");
      commandText.className = "mono-text terminal-shortcut-command";
      commandText.textContent = command.command || "";
      actionCell.append(actionText, commandText);
      row.appendChild(actionCell);

      const shortcutCell = document.createElement("td");
      const input = document.createElement("input");
      input.className = "text-input mono-text terminal-shortcut-input";
      input.type = "text";
      input.value = command.shortcut || "";
      input.placeholder = "Ctrl+Alt+R";
      input.autocomplete = "off";
      input.spellcheck = false;
      input.dataset.commandGroup = group.key;
      input.dataset.commandIndex = String(index);
      input.setAttribute("aria-label", `${group.label} ${command.label || command.key} 快捷键`);
      shortcutCell.appendChild(input);
      row.appendChild(shortcutCell);

      terminalShortcutsListEl.appendChild(row);
    });
  });

  if (totalSubCommandCount === 0) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 5;
    cell.className = "settings-value muted-text";
    cell.textContent = "没有可编辑的快捷键命令。";
    row.appendChild(cell);
    terminalShortcutsListEl.appendChild(row);
    return;
  }

  if (renderedSubCommandCount === 0) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 5;
    cell.className = "settings-value muted-text";
    cell.textContent = selectedGroupCount === 0
      ? "未选择任何多功能按钮；勾选上方复选框即可展开对应组的子命令快捷键。"
      : "已选择的组均无子命令。";
    row.appendChild(cell);
    terminalShortcutsListEl.appendChild(row);
  }
}

function normalizeTerminalDefaultEnvKey(value) {
  const key = normalizeTerminalQuickText(value, 128);
  if (
    !/^[A-Za-z_][A-Za-z0-9_]*$/.test(key) ||
    RESERVED_TERMINAL_DEFAULT_ENV_KEYS.has(key)
  ) {
    return "";
  }
  return key;
}

function normalizeTerminalDefaultEnvValue(value) {
  return typeof value === "string"
    ? value
        .replace(/[\u0000-\u001f\u007f]/g, "")
        .trim()
        .slice(0, 4096)
    : "";
}

function normalizeTerminalDefaultEnvVars(vars) {
  const seen = new Set();
  const normalized = [];
  (Array.isArray(vars) ? vars : []).forEach((entry) => {
    if (normalized.length >= MAX_TERMINAL_DEFAULT_ENV_VARS) {
      return;
    }
    const key = normalizeTerminalDefaultEnvKey(entry?.key);
    if (!key || seen.has(key)) {
      return;
    }
    seen.add(key);
    normalized.push({
      key,
      value: normalizeTerminalDefaultEnvValue(entry?.value),
    });
  });
  return normalized;
}

function parseTerminalDefaultEnvInput(value) {
  const entries = [];
  String(value || "")
    .split(/\r?\n/)
    .forEach((line) => {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith("#")) {
        return;
      }
      const exportPrefix = "export ";
      const normalizedLine = trimmed.startsWith(exportPrefix)
        ? trimmed.slice(exportPrefix.length).trim()
        : trimmed;
      const equalsIndex = normalizedLine.indexOf("=");
      if (equalsIndex <= 0) {
        return;
      }
      entries.push({
        key: normalizedLine.slice(0, equalsIndex),
        value: normalizedLine.slice(equalsIndex + 1),
      });
    });
  return normalizeTerminalDefaultEnvVars(entries);
}

function formatTerminalDefaultEnvVars(vars) {
  return normalizeTerminalDefaultEnvVars(vars)
    .map((entry) => `${entry.key}=${entry.value}`)
    .join("\n");
}

function normalizeCompileEnvironment(vars) {
  const seen = new Set();
  const normalized = [];
  (Array.isArray(vars) ? vars : []).forEach((entry) => {
    if (normalized.length >= MAX_COMPILE_ENV_VARS) {
      return;
    }
    const key = normalizeCompileEnvKey(entry?.key);
    if (!key || seen.has(key)) {
      return;
    }
    seen.add(key);
    normalized.push({ key, value: normalizeCompileEnvValue(entry?.value) });
  });
  return normalized;
}

function normalizeCompileEnvKey(value) {
  const key = String(value || "").trim().slice(0, 128);
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(key) ? key : "";
}

function normalizeCompileEnvValue(value) {
  return typeof value === "string"
    ? value.replace(/[\u0000-\u001f\u007f]/g, "").slice(0, 4096)
    : "";
}

function parseCompileEnvironmentInput(value) {
  const entries = [];
  String(value || "")
    .split(/\r?\n/)
    .forEach((line) => {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith("#")) {
        return;
      }
      const exportPrefix = "export ";
      const normalizedLine = trimmed.startsWith(exportPrefix)
        ? trimmed.slice(exportPrefix.length).trim()
        : trimmed;
      const equalsIndex = normalizedLine.indexOf("=");
      if (equalsIndex <= 0) {
        return;
      }
      entries.push({
        key: normalizedLine.slice(0, equalsIndex),
        value: normalizedLine.slice(equalsIndex + 1),
      });
    });
  return normalizeCompileEnvironment(entries);
}

function formatCompileEnvironment(vars) {
  return normalizeCompileEnvironment(vars)
    .map((entry) => `${entry.key}=${entry.value}`)
    .join("\n");
}

function normalizeTerminalStartupEnvKey(value) {
  const key = normalizeTerminalQuickText(value, 128);
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(key) ? key : "";
}

function normalizeTerminalStartupEnvValue(value) {
  return typeof value === "string"
    ? value
        .replace(/[\u0000-\u001f\u007f]/g, "")
        .trim()
        .slice(0, 4096)
    : "";
}

function normalizeTerminalStartupEnvVars(vars) {
  const seen = new Set();
  const normalized = [];
  (Array.isArray(vars) ? vars : []).forEach((entry) => {
    if (normalized.length >= MAX_TERMINAL_DEFAULT_ENV_VARS) {
      return;
    }
    const key = normalizeTerminalStartupEnvKey(entry?.key);
    if (!key || seen.has(key)) {
      return;
    }
    seen.add(key);
    normalized.push({
      key,
      value: normalizeTerminalStartupEnvValue(entry?.value),
    });
  });
  return normalized;
}

function parseTerminalStartupEnvInput(value) {
  const entries = [];
  String(value || "")
    .split(/\r?\n/)
    .forEach((line) => {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith("#")) {
        return;
      }
      const exportPrefix = "export ";
      const normalizedLine = trimmed.startsWith(exportPrefix)
        ? trimmed.slice(exportPrefix.length).trim()
        : trimmed;
      const equalsIndex = normalizedLine.indexOf("=");
      if (equalsIndex <= 0) {
        return;
      }
      entries.push({
        key: normalizedLine.slice(0, equalsIndex),
        value: normalizedLine.slice(equalsIndex + 1),
      });
    });
  return normalizeTerminalStartupEnvVars(entries);
}

function formatTerminalStartupEnvVars(vars) {
  return normalizeTerminalStartupEnvVars(vars)
    .map((entry) => `${entry.key}=${entry.value}`)
    .join("\n");
}

function presetMatchesDomesticModelBaseUrl(preset) {
  const haystack = [
    preset?.name,
    preset?.provider_name,
    preset?.base_url,
    preset?.management_url,
    ...(Array.isArray(preset?.config_overrides)
      ? preset.config_overrides.map((item) => `${item?.key || ""} ${item?.value || ""}`)
      : []),
  ]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
  return DOMESTIC_MODEL_BASE_URL_KEYWORDS.some((keyword) => haystack.includes(keyword));
}

function formatBaseUrlPresetOptionLabel(preset) {
  return [preset?.provider_name, preset?.name]
    .map((value) => String(value || "").trim())
    .filter(Boolean)
    .filter((value, index, values) => values.indexOf(value) === index)
    .join(" / ");
}

function collectBaseUrlPresetOptions(presets = []) {
  const options = new Map();
  (Array.isArray(presets) ? presets : []).forEach((preset) => {
    const baseUrl = String(preset?.base_url || "").trim();
    if (!baseUrl || !presetMatchesDomesticModelBaseUrl(preset)) {
      return;
    }
    const existing = options.get(baseUrl) || new Set();
    const label = formatBaseUrlPresetOptionLabel(preset);
    if (label) {
      existing.add(label);
    }
    options.set(baseUrl, existing);
  });
  return Array.from(options.entries()).map(([value, labels]) => ({
    value,
    label: Array.from(labels).slice(0, 2).join(" / "),
  }));
}

function renderBaseUrlPresetOptions(datalistEl, options) {
  if (!datalistEl) {
    return;
  }
  datalistEl.replaceChildren(
    ...options.map((item) => {
      const option = document.createElement("option");
      option.value = item.value;
      if (item.label) {
        option.label = item.label;
        option.textContent = item.label;
      }
      return option;
    }),
  );
}

function refreshBaseUrlPresetOptions() {
  renderBaseUrlPresetOptions(
    apiBaseUrlPresetsEl,
    collectBaseUrlPresetOptions(state.apiPresets),
  );
  renderBaseUrlPresetOptions(
    claudeBaseUrlPresetsEl,
    collectBaseUrlPresetOptions(state.claudePresets),
  );
}

function terminalQuickCommandDisplay(command) {
  const label = normalizeTerminalQuickText(command?.label, 24);
  const commandLine =
    normalizeTerminalQuickText(command?.command, 1000) ||
    [normalizeTerminalQuickText(command?.program, 160), normalizeTerminalQuickText(command?.args, 500)]
      .filter(Boolean)
      .join(" ");
  return label || commandLine || normalizeTerminalQuickText(command?.key, 8) || "快捷命令";
}

