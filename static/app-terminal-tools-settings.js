let terminalToolActionDraft = [];
let terminalToolPresetOptions = [];

function terminalToolDom() {
  return {
    body: document.getElementById("terminal-tool-entries-body"),
    dialog: document.getElementById("terminal-tool-editor-dialog"),
    form: document.getElementById("terminal-tool-editor-form"),
    title: document.getElementById("terminal-tool-editor-title"),
    id: document.getElementById("terminal-tool-editor-id"),
    kind: document.getElementById("terminal-tool-editor-kind"),
    label: document.getElementById("terminal-tool-editor-label"),
    root: document.getElementById("terminal-tool-editor-root"),
    parent: document.getElementById("terminal-tool-editor-parent"),
    sort: document.getElementById("terminal-tool-editor-sort"),
    actionsSection: document.getElementById("terminal-tool-actions-section"),
    actionsList: document.getElementById("terminal-tool-actions-list"),
    status: document.getElementById("terminal-tool-editor-status"),
  };
}

function terminalToolRootLabel(rootKey) {
  return TERMINAL_TOOL_ROOTS.find((entry) => entry.key === rootKey)?.label || rootKey;
}

function terminalToolActionLabel(kind) {
  return TERMINAL_TOOL_ACTION_TYPES.find((entry) => entry.key === kind)?.label || kind;
}

function terminalToolEntryPath(entry, entries = state.terminalToolEntries) {
  const labels = [entry.label];
  const seen = new Set([entry.id]);
  let parentId = entry.parent_id;
  while (parentId) {
    const parent = entries.find((candidate) => candidate.id === parentId);
    if (!parent || seen.has(parent.id)) {
      break;
    }
    seen.add(parent.id);
    labels.unshift(parent.label);
    parentId = parent.parent_id;
  }
  return labels.join(" / ");
}

function sortedTerminalToolChildren(entries, rootKey, parentId) {
  return entries
    .filter((entry) => entry.root_key === rootKey && (entry.parent_id || null) === (parentId || null))
    .sort((left, right) =>
      left.sort_order - right.sort_order
      || left.label.localeCompare(right.label, "zh-CN")
      || left.id.localeCompare(right.id)
    );
}

function flattenTerminalToolEntries(entries) {
  const flattened = [];
  const visit = (rootKey, parentId, depth) => {
    for (const entry of sortedTerminalToolChildren(entries, rootKey, parentId)) {
      flattened.push({ entry, depth });
      if (entry.kind === "folder") {
        visit(rootKey, entry.id, depth + 1);
      }
    }
  };
  for (const root of TERMINAL_TOOL_ROOTS) {
    visit(root.key, null, 0);
  }
  return flattened;
}

function terminalToolActionSummary(entry) {
  if (entry.kind === "folder") {
    const count = state.terminalToolEntries.filter((candidate) => candidate.parent_id === entry.id).length;
    return `${count} 个子条目`;
  }
  return entry.actions.map((action) => terminalToolActionLabel(action.kind)).join(" → ");
}

function makeTerminalToolTableCell(text, className = "") {
  const cell = document.createElement("td");
  cell.textContent = text;
  if (className) {
    cell.className = className;
  }
  return cell;
}

function renderTerminalToolEntriesTable() {
  const { body } = terminalToolDom();
  if (!body) {
    return;
  }
  body.replaceChildren();
  const entries = normalizeTerminalToolEntries(state.terminalToolEntries);
  state.terminalToolEntries = entries;
  if (entries.length === 0) {
    const row = document.createElement("tr");
    const cell = makeTerminalToolTableCell("尚未配置条目。", "settings-value muted-text");
    cell.colSpan = 7;
    row.append(cell);
    body.append(row);
    return;
  }

  for (const { entry, depth } of flattenTerminalToolEntries(entries)) {
    const row = document.createElement("tr");
    row.dataset.terminalToolId = entry.id;
    row.append(makeTerminalToolTableCell(entry.kind === "folder" ? "目录" : "工作流"));
    const labelCell = makeTerminalToolTableCell(entry.label, "terminal-tool-label-cell");
    labelCell.style.setProperty("--terminal-tool-depth", String(depth));
    row.append(labelCell);
    row.append(makeTerminalToolTableCell(terminalToolRootLabel(entry.root_key)));
    const parent = entries.find((candidate) => candidate.id === entry.parent_id);
    row.append(makeTerminalToolTableCell(parent ? terminalToolEntryPath(parent, entries) : "根目录"));
    row.append(makeTerminalToolTableCell(String(entry.sort_order), "mono-text"));
    row.append(makeTerminalToolTableCell(terminalToolActionSummary(entry), "terminal-tool-action-summary"));
    const actionsCell = document.createElement("td");
    actionsCell.className = "terminal-tool-row-actions";
    const editButton = document.createElement("button");
    editButton.type = "button";
    editButton.className = "mini-button";
    editButton.dataset.terminalToolEdit = entry.id;
    editButton.textContent = "编辑";
    const copyButton = document.createElement("button");
    copyButton.type = "button";
    copyButton.className = "mini-button";
    copyButton.dataset.terminalToolCopy = entry.id;
    copyButton.textContent = "复制";
    const deleteButton = document.createElement("button");
    deleteButton.type = "button";
    deleteButton.className = "mini-button danger";
    deleteButton.dataset.terminalToolDelete = entry.id;
    deleteButton.textContent = "删除";
    actionsCell.append(editButton, copyButton, deleteButton);
    row.append(actionsCell);
    body.append(row);
  }
}

function terminalToolDescendantIds(entryId) {
  const descendants = new Set();
  let changed = true;
  while (changed) {
    changed = false;
    for (const entry of state.terminalToolEntries) {
      if (entry.parent_id === entryId || descendants.has(entry.parent_id)) {
        if (!descendants.has(entry.id)) {
          descendants.add(entry.id);
          changed = true;
        }
      }
    }
  }
  return descendants;
}

function renderTerminalToolRootOptions(selectedRoot = "tools") {
  const { root } = terminalToolDom();
  if (!root) {
    return;
  }
  root.replaceChildren();
  for (const entry of TERMINAL_TOOL_ROOTS) {
    const option = document.createElement("option");
    option.value = entry.key;
    option.textContent = entry.label;
    option.selected = entry.key === selectedRoot;
    root.append(option);
  }
}

function renderTerminalToolParentOptions(selectedParent = null) {
  const dom = terminalToolDom();
  if (!dom.parent || !dom.root) {
    return;
  }
  const editingId = dom.id?.value || "";
  const excluded = editingId ? terminalToolDescendantIds(editingId) : new Set();
  if (editingId) {
    excluded.add(editingId);
  }
  dom.parent.replaceChildren();
  const rootOption = document.createElement("option");
  rootOption.value = "";
  rootOption.textContent = "根目录";
  dom.parent.append(rootOption);
  for (const { entry, depth } of flattenTerminalToolEntries(state.terminalToolEntries)) {
    if (entry.root_key !== dom.root.value || entry.kind !== "folder" || excluded.has(entry.id)) {
      continue;
    }
    const option = document.createElement("option");
    option.value = entry.id;
    option.textContent = `${"　".repeat(depth)}${entry.label}`;
    option.selected = entry.id === selectedParent;
    dom.parent.append(option);
  }
  dom.parent.value = selectedParent || "";
}

function setTerminalToolEditorStatus(message = "", tone = "muted") {
  const { status } = terminalToolDom();
  if (!status) {
    return;
  }
  status.hidden = !message;
  status.textContent = message;
  status.dataset.tone = tone;
}

async function loadTerminalToolPresetOptions() {
  try {
    const response = await requestJson("/api/auth/api-presets");
    terminalToolPresetOptions = (Array.isArray(response?.presets) ? response.presets : [])
      .map((preset) => ({
        id: String(preset?.id || "").trim(),
        label: String(preset?.name || preset?.label || preset?.id || "").trim(),
      }))
      .filter((preset) => preset.id);
  } catch {
    terminalToolPresetOptions = [];
  }
}

function createTerminalToolActionParameter(action, index) {
  if (action.kind === "create_terminal" || action.kind === "fork_session" || action.kind === "switch_api_preset_revert") {
    const value = document.createElement("span");
    value.className = "terminal-tool-action-parameter muted-text";
    value.textContent = action.kind === "fork_session" ? "自动提取 resume" : action.kind === "switch_api_preset_revert" ? "回切到上一次预设" : "当前目录";
    return value;
  }
  if (action.kind === "switch_api_preset") {
    const select = document.createElement("select");
    select.className = "text-input terminal-tool-action-parameter";
    const values = [...terminalToolPresetOptions];
    if (action.value && !values.some((preset) => preset.id === action.value)) {
      values.unshift({ id: action.value, label: action.value });
    }
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = values.length ? "选择预设" : "暂无预设";
    select.append(placeholder);
    for (const preset of values) {
      const option = document.createElement("option");
      option.value = preset.id;
      option.textContent = preset.label || preset.id;
      select.append(option);
    }
    select.value = action.value || "";
    select.addEventListener("change", () => {
      terminalToolActionDraft[index].value = select.value;
    });
    return select;
  }
  if (action.kind === "function_command") {
    return createTerminalToolFunctionCommandField(action, index);
  }
  if (action.kind === "run_workflow") {
    return createTerminalToolRunWorkflowField(action, index);
  }
  if (action.kind === "codex_launch") {
    return createTerminalToolCodexLaunchFields(action, index);
  }
  const isCodexTask = action.kind === "codex_exec" || action.kind === "codex_terminal";
  const input = document.createElement(isCodexTask ? "textarea" : "input");
  input.className = "text-input terminal-tool-action-parameter";
  input.autocomplete = "off";
  if (isCodexTask) {
    input.rows = 3;
    input.maxLength = 4096;
    input.value = action.value || "";
    input.placeholder = "任务内容";
    input.addEventListener("input", () => {
      terminalToolActionDraft[index].value = input.value;
    });
    return input;
  }
  if (action.kind === "wait") {
    input.type = "number";
    input.min = "0.1";
    input.max = "600";
    input.step = "0.1";
    input.value = String(action.seconds || 1);
    input.setAttribute("aria-label", "等待秒数");
    input.addEventListener("input", () => {
      terminalToolActionDraft[index].seconds = Number(input.value);
    });
  } else {
    input.type = "text";
    input.maxLength = action.kind === "send_command" ? 4096 : 128;
    input.value = action.value || "";
    input.placeholder = action.kind === "rename_terminal" ? "新的终端名称" : "命令";
    input.addEventListener("input", () => {
      terminalToolActionDraft[index].value = input.value;
    });
  }
  return input;
}

function createTerminalToolFunctionCommandField(action, index) {
  const wrapper = document.createElement("div");
  wrapper.className = "terminal-tool-function-command-field";
  const allCommands = [
    ...(Array.isArray(state.terminalFunctionCommands) ? state.terminalFunctionCommands : []),
    ...(Array.isArray(state.terminalSlashCommands) ? state.terminalSlashCommands : []),
  ];
  const label = document.createElement("span");
  label.className = "field-label";
  label.textContent = "功能命令";
  const select = document.createElement("select");
  select.className = "text-input";
  const placeholder = document.createElement("option");
  placeholder.value = "";
  placeholder.textContent = allCommands.length ? "选择功能命令" : "暂无功能命令";
  select.append(placeholder);
  for (const command of allCommands) {
    const option = document.createElement("option");
    option.value = command.key;
    option.textContent = command.label || command.key;
    select.append(option);
  }
  if (action.command_key && !allCommands.some((cmd) => cmd.key === action.command_key)) {
    const option = document.createElement("option");
    option.value = action.command_key;
    option.textContent = action.command_key;
    select.append(option);
  }
  select.value = action.command_key || "";
  select.addEventListener("change", () => {
    terminalToolActionDraft[index].value = select.value;
    terminalToolActionDraft[index].command_key = select.value;
  });
  wrapper.append(label, select);
  return wrapper;
}

function createTerminalToolRunWorkflowField(action, index) {
  const wrapper = document.createElement("div");
  wrapper.className = "terminal-tool-run-workflow-field";
  const actionableEntries = (Array.isArray(state.terminalToolEntries) ? state.terminalToolEntries : [])
    .filter((entry) => entry.kind === "action");
  const label = document.createElement("span");
  label.className = "field-label";
  label.textContent = "嵌套工作流";
  const select = document.createElement("select");
  select.className = "text-input";
  const placeholder = document.createElement("option");
  placeholder.value = "";
  placeholder.textContent = actionableEntries.length ? "选择工作流" : "暂无可用工作流";
  select.append(placeholder);
  for (const entry of actionableEntries) {
    const option = document.createElement("option");
    option.value = entry.id;
    option.textContent = entry.label;
    select.append(option);
  }
  if (action.target_entry_id && !actionableEntries.some((entry) => entry.id === action.target_entry_id)) {
    const option = document.createElement("option");
    option.value = action.target_entry_id;
    option.textContent = action.target_entry_id;
    select.append(option);
  }
  select.value = action.target_entry_id || "";
  select.addEventListener("change", () => {
    terminalToolActionDraft[index].value = select.value;
    terminalToolActionDraft[index].target_entry_id = select.value;
  });
  wrapper.append(label, select);
  return wrapper;
}

function createTerminalToolCodexLaunchFields(action, index) {
  const wrapper = document.createElement("div");
  wrapper.className = "terminal-tool-codex-launch-fields";

  const presetMatchField = document.createElement("label");
  presetMatchField.className = "field terminal-tool-launch-field";
  const presetMatchLabel = document.createElement("span");
  presetMatchLabel.className = "field-label";
  presetMatchLabel.textContent = "预设匹配";
  const presetMatchSelect = document.createElement("select");
  presetMatchSelect.className = "text-input";
  for (const [value, text] of [["id", "ID"], ["exact_name", "精确名称"], ["unique_contains", "唯一包含"]]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = text;
    presetMatchSelect.append(option);
  }
  presetMatchSelect.value = action.preset_match || "unique_contains";
  presetMatchSelect.addEventListener("change", () => {
    terminalToolActionDraft[index].preset_match = presetMatchSelect.value;
  });
  presetMatchField.append(presetMatchLabel, presetMatchSelect);

  const presetSelectorField = document.createElement("label");
  presetSelectorField.className = "field terminal-tool-launch-field";
  const presetSelectorLabel = document.createElement("span");
  presetSelectorLabel.className = "field-label";
  presetSelectorLabel.textContent = "预设";
  const presetSelectorInput = document.createElement("input");
  presetSelectorInput.className = "text-input";
  presetSelectorInput.type = "text";
  presetSelectorInput.autocomplete = "off";
  presetSelectorInput.maxLength = 128;
  presetSelectorInput.value = action.preset_selector || "";
  presetSelectorInput.placeholder = "预设 ID 或名称";
  const presetValues = [...terminalToolPresetOptions];
  if (action.preset_selector && !presetValues.some((preset) => preset.label === action.preset_selector || preset.id === action.preset_selector)) {
    presetValues.unshift({ id: action.preset_selector, label: action.preset_selector });
  }
  if (presetValues.length) {
    const list = document.createElement("datalist");
    list.id = "terminal-tool-launch-presets";
    for (const preset of presetValues) {
      const option = document.createElement("option");
      option.value = preset.label || preset.id;
      list.append(option);
    }
    presetSelectorInput.setAttribute("list", list.id);
    wrapper.append(list);
  }
  presetSelectorInput.addEventListener("input", () => {
    terminalToolActionDraft[index].preset_selector = presetSelectorInput.value;
  });
  presetSelectorField.append(presetSelectorLabel, presetSelectorInput);

  const cwdField = document.createElement("label");
  cwdField.className = "field terminal-tool-launch-field";
  const cwdLabel = document.createElement("span");
  cwdLabel.className = "field-label";
  cwdLabel.textContent = "工作目录";
  const cwdInput = document.createElement("input");
  cwdInput.className = "text-input";
  cwdInput.type = "text";
  cwdInput.autocomplete = "off";
  cwdInput.maxLength = 1024;
  cwdInput.value = action.cwd || "";
  cwdInput.placeholder = "/home/system";
  cwdInput.addEventListener("input", () => {
    terminalToolActionDraft[index].cwd = cwdInput.value;
  });
  cwdField.append(cwdLabel, cwdInput);

  const projectPathField = document.createElement("label");
  projectPathField.className = "field terminal-tool-launch-field";
  const projectPathLabel = document.createElement("span");
  projectPathLabel.className = "field-label";
  projectPathLabel.textContent = "项目路径";
  const projectPathInput = document.createElement("input");
  projectPathInput.className = "text-input";
  projectPathInput.type = "text";
  projectPathInput.autocomplete = "off";
  projectPathInput.maxLength = 1024;
  projectPathInput.value = action.project_path || "";
  projectPathInput.placeholder = "/home/system";
  projectPathInput.addEventListener("input", () => {
    terminalToolActionDraft[index].project_path = projectPathInput.value;
  });
  projectPathField.append(projectPathLabel, projectPathInput);

  const terminalNameField = document.createElement("label");
  terminalNameField.className = "field terminal-tool-launch-field";
  const terminalNameLabel = document.createElement("span");
  terminalNameLabel.className = "field-label";
  terminalNameLabel.textContent = "终端名称";
  const terminalNameInput = document.createElement("input");
  terminalNameInput.className = "text-input";
  terminalNameInput.type = "text";
  terminalNameInput.autocomplete = "off";
  terminalNameInput.maxLength = 64;
  terminalNameInput.value = action.terminal_name || "";
  terminalNameInput.placeholder = "终端名称";
  terminalNameInput.addEventListener("input", () => {
    terminalToolActionDraft[index].terminal_name = terminalNameInput.value;
  });
  terminalNameField.append(terminalNameLabel, terminalNameInput);

  const taskField = document.createElement("label");
  taskField.className = "field terminal-tool-launch-field terminal-tool-launch-task";
  const taskLabel = document.createElement("span");
  taskLabel.className = "field-label";
  taskLabel.textContent = "初始任务";
  const taskTextarea = document.createElement("textarea");
  taskTextarea.className = "text-input";
  taskTextarea.rows = 3;
  taskTextarea.maxLength = 4096;
  taskTextarea.value = action.value || "";
  taskTextarea.placeholder = "$skill-name 任务描述";
  taskTextarea.addEventListener("input", () => {
    terminalToolActionDraft[index].value = taskTextarea.value;
  });
  taskField.append(taskLabel, taskTextarea);

  wrapper.append(presetMatchField, presetSelectorField, cwdField, projectPathField, terminalNameField, taskField);
  return wrapper;
}

function renderTerminalToolActionDraft() {
  const { actionsList } = terminalToolDom();
  if (!actionsList) {
    return;
  }
  actionsList.replaceChildren();
  terminalToolActionDraft.forEach((action, index) => {
    const row = document.createElement("div");
    row.className = "terminal-tool-action-row";
    const order = document.createElement("span");
    order.className = "terminal-tool-action-order mono-text";
    order.textContent = String(index + 1);
    const kindSelect = document.createElement("select");
    kindSelect.className = "text-input terminal-tool-action-kind";
    for (const type of TERMINAL_TOOL_ACTION_TYPES) {
      const option = document.createElement("option");
      option.value = type.key;
      option.textContent = type.label;
      kindSelect.append(option);
    }
    kindSelect.value = action.kind;
    kindSelect.addEventListener("change", () => {
      if (kindSelect.value === "codex_launch") {
        terminalToolActionDraft[index] = {
          kind: "codex_launch",
          value: "",
          seconds: 0,
          preset_selector: "",
          preset_match: "unique_contains",
          cwd: state.currentPath || "",
          project_path: state.currentPath || "",
          terminal_name: "",
          session_action: "new",
        };
      } else if (kindSelect.value === "function_command") {
        terminalToolActionDraft[index] = {
          kind: "function_command",
          value: "",
          seconds: 0,
          command_key: "",
        };
      } else if (kindSelect.value === "run_workflow") {
        terminalToolActionDraft[index] = {
          kind: "run_workflow",
          value: "",
          seconds: 0,
          target_entry_id: "",
        };
      } else {
        terminalToolActionDraft[index] = {
          kind: kindSelect.value,
          value: "",
          seconds: kindSelect.value === "wait" ? 1 : 0,
        };
      }
      renderTerminalToolActionDraft();
    });
    const parameter = createTerminalToolActionParameter(action, index);
    const controls = document.createElement("span");
    controls.className = "terminal-tool-action-controls";
    for (const [label, title, delta] of [["↑", "上移", -1], ["↓", "下移", 1]]) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "mini-button terminal-tool-action-move";
      button.textContent = label;
      button.title = title;
      button.setAttribute("aria-label", title);
      button.disabled = index + delta < 0 || index + delta >= terminalToolActionDraft.length;
      button.addEventListener("click", () => {
        const target = index + delta;
        [terminalToolActionDraft[index], terminalToolActionDraft[target]] =
          [terminalToolActionDraft[target], terminalToolActionDraft[index]];
        renderTerminalToolActionDraft();
      });
      controls.append(button);
    }
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "mini-button danger";
    remove.textContent = "×";
    remove.title = "删除动作";
    remove.setAttribute("aria-label", "删除动作");
    remove.addEventListener("click", () => {
      terminalToolActionDraft.splice(index, 1);
      renderTerminalToolActionDraft();
    });
    controls.append(remove);
    row.append(order, kindSelect, parameter, controls);
    actionsList.append(row);
  });
  if (terminalToolActionDraft.length === 0) {
    const empty = document.createElement("p");
    empty.className = "muted-text terminal-tool-actions-empty";
    empty.textContent = "尚未添加动作。";
    actionsList.append(empty);
  }
}

async function openTerminalToolEditor(kind, entry = null) {
  const dom = terminalToolDom();
  if (!dom.dialog || !dom.form) {
    return;
  }
  const entryKind = entry?.kind || kind;
  dom.form.reset();
  dom.id.value = entry?.id || "";
  dom.kind.value = entryKind;
  dom.title.textContent = entry
    ? `编辑${entryKind === "folder" ? "目录" : "工作流"}`
    : `新建${entryKind === "folder" ? "目录" : "工作流"}`;
  dom.label.value = entry?.label || "";
  dom.sort.value = String(entry?.sort_order ?? 100);
  renderTerminalToolRootOptions(entry?.root_key || "tools");
  renderTerminalToolParentOptions(entry?.parent_id || null);
  terminalToolActionDraft = entryKind === "action"
    ? (entry?.actions?.map((action) => ({ ...action })) || [{ kind: "create_terminal", value: "", seconds: 0 }])
    : [];
  dom.actionsSection.hidden = entryKind !== "action";
  renderTerminalToolActionDraft();
  setTerminalToolEditorStatus();
  dom.dialog.showModal();
  dom.label.focus();
  await loadTerminalToolPresetOptions();
  if (dom.dialog.open && entryKind === "action") {
    renderTerminalToolActionDraft();
  }
}

function closeTerminalToolEditor() {
  const { dialog } = terminalToolDom();
  if (dialog?.open) {
    dialog.close();
  }
}

function generateTerminalToolId() {
  const random = globalThis.crypto?.randomUUID?.().replaceAll("-", "")
    || `${Date.now()}${Math.random().toString(16).slice(2)}`;
  return `tool_${random}`.slice(0, 64);
}

function applyTerminalToolEditor() {
  const dom = terminalToolDom();
  const editingId = dom.id.value;
  const entry = {
    id: editingId || generateTerminalToolId(),
    root_key: dom.root.value,
    parent_id: dom.parent.value || null,
    kind: dom.kind.value,
    label: dom.label.value,
    sort_order: Number(dom.sort.value),
    actions: dom.kind.value === "action" ? terminalToolActionDraft.map((action) => ({ ...action })) : [],
  };
  const candidate = editingId
    ? state.terminalToolEntries.map((item) => item.id === editingId ? entry : item)
    : [...state.terminalToolEntries, entry];
  const normalized = normalizeTerminalToolEntries(candidate);
  if (normalized.length !== candidate.length) {
    setTerminalToolEditorStatus("请检查名称、目录、排序和动作参数。", "warn");
    return false;
  }
  state.terminalToolEntries = normalized;
  renderTerminalToolEntriesTable();
  updateStatus(settingsStatusEl, "利器条目已更新，点击“保存设置”后生效。", "info");
  closeTerminalToolEditor();
  return true;
}

function copyTerminalToolEntry(entryId) {
  const entry = state.terminalToolEntries.find((candidate) => candidate.id === entryId);
  if (!entry) {
    return;
  }
  const copy = {
    id: generateTerminalToolId(),
    root_key: entry.root_key,
    parent_id: entry.parent_id,
    kind: entry.kind,
    label: `${entry.label} 副本`.slice(0, 64),
    sort_order: Math.min(10000, entry.sort_order + 1),
    actions: entry.actions.map((action) => ({ ...action })),
  };
  const normalized = normalizeTerminalToolEntries([...state.terminalToolEntries, copy]);
  if (normalized.length !== state.terminalToolEntries.length + 1) {
    setTerminalToolEditorStatus("复制条目时校验失败，请检查配置。", "warn");
    return;
  }
  state.terminalToolEntries = normalized;
  renderTerminalToolEntriesTable();
  updateStatus(settingsStatusEl, "工作流条目已复制，点击“保存设置”后生效。", "info");
}

function deleteTerminalToolEntry(entryId) {
  const entry = state.terminalToolEntries.find((candidate) => candidate.id === entryId);
  if (!entry) {
    return;
  }
  const descendants = terminalToolDescendantIds(entryId);
  const message = descendants.size
    ? `删除“${entry.label}”会同时删除 ${descendants.size} 个子条目，确定继续吗？`
    : `确定删除“${entry.label}”吗？`;
  if (!window.confirm(message)) {
    return;
  }
  descendants.add(entryId);
  state.terminalToolEntries = state.terminalToolEntries.filter((candidate) => !descendants.has(candidate.id));
  renderTerminalToolEntriesTable();
  updateStatus(settingsStatusEl, "利器条目已删除，点击“保存设置”后生效。", "info");
}

function exportTerminalToolEntries() {
  const data = {
    version: 1,
    terminal_tool_entries: normalizeTerminalToolEntries(state.terminalToolEntries),
  };
  const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `webclx-workflows-${new Date().toISOString().slice(0, 10)}.json`;
  document.body.append(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}

function importTerminalToolEntries() {
  const input = document.getElementById("terminal-tool-import-file");
  if (!input) {
    return;
  }
  input.value = "";
  input.click();
}

function handleTerminalToolImportFile(event) {
  const file = event.target.files?.[0];
  if (!file) {
    return;
  }
  const reader = new FileReader();
  reader.onload = () => {
    try {
      const data = JSON.parse(String(reader.result || ""));
      if (!data || data.version !== 1 || !Array.isArray(data.terminal_tool_entries)) {
        updateStatus(settingsStatusEl, "导入失败：文件格式无效或版本不受支持。", "warn");
        return;
      }
      const normalized = normalizeTerminalToolEntries(data.terminal_tool_entries);
      if (normalized.length === 0 && data.terminal_tool_entries.length > 0) {
        updateStatus(settingsStatusEl, "导入失败：条目校验未通过，当前配置未改变。", "warn");
        return;
      }
      const count = normalized.length;
      if (!window.confirm(`导入 ${count} 个工作流条目，将替换当前列表，确定继续吗？`)) {
        return;
      }
      state.terminalToolEntries = normalized;
      renderTerminalToolEntriesTable();
      updateStatus(settingsStatusEl, `已导入 ${count} 个工作流条目，点击"保存设置"后生效。`, "info");
    } catch (error) {
      updateStatus(settingsStatusEl, `导入失败：${error?.message || error}`, "warn");
    }
  };
  reader.readAsText(file);
}

function bindTerminalToolSettings() {
  const dom = terminalToolDom();
  document.getElementById("terminal-tool-add-folder")?.addEventListener("click", () => {
    openTerminalToolEditor("folder");
  });
  document.getElementById("terminal-tool-add-action")?.addEventListener("click", () => {
    openTerminalToolEditor("action");
  });
  document.getElementById("terminal-tool-export")?.addEventListener("click", () => {
    exportTerminalToolEntries();
  });
  document.getElementById("terminal-tool-import")?.addEventListener("click", () => {
    importTerminalToolEntries();
  });
  document.getElementById("terminal-tool-import-file")?.addEventListener("change", (event) => {
    handleTerminalToolImportFile(event);
  });
  dom.body?.addEventListener("click", (event) => {
    const editButton = event.target.closest("[data-terminal-tool-edit]");
    if (editButton) {
      const entry = state.terminalToolEntries.find((candidate) => candidate.id === editButton.dataset.terminalToolEdit);
      if (entry) {
        openTerminalToolEditor(entry.kind, entry);
      }
      return;
    }
    const copyButton = event.target.closest("[data-terminal-tool-copy]");
    if (copyButton) {
      copyTerminalToolEntry(copyButton.dataset.terminalToolCopy);
      return;
    }
    const deleteButton = event.target.closest("[data-terminal-tool-delete]");
    if (deleteButton) {
      deleteTerminalToolEntry(deleteButton.dataset.terminalToolDelete);
    }
  });
  dom.root?.addEventListener("change", () => renderTerminalToolParentOptions(null));
  document.getElementById("terminal-tool-action-add")?.addEventListener("click", () => {
    if (terminalToolActionDraft.length >= 20) {
      setTerminalToolEditorStatus("每个功能最多包含 20 个动作。", "warn");
      return;
    }
    terminalToolActionDraft.push({ kind: "create_terminal", value: "", seconds: 0 });
    renderTerminalToolActionDraft();
  });
  dom.form?.addEventListener("submit", (event) => {
    event.preventDefault();
    applyTerminalToolEditor();
  });
  document.getElementById("terminal-tool-editor-close")?.addEventListener("click", closeTerminalToolEditor);
  document.getElementById("terminal-tool-editor-cancel")?.addEventListener("click", closeTerminalToolEditor);
  dom.dialog?.addEventListener("cancel", (event) => {
    event.preventDefault();
    closeTerminalToolEditor();
  });
}
