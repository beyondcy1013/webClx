// Workspace "指定" dialog: pick a Codex/Claude API preset and launch it with
// an isolated runtime. Mirrors the "利器"
// continuous-command flow and applyApiPresetAndLaunch.

let workspaceDesignatePresetTargetPath = "";
let workspaceDesignatePresetTargetName = "";
let workspaceDesignatePresetTrigger = null;
let workspaceDesignatePresetRequestToken = 0;

function workspaceDesignatePresetDialogElements() {
  return {
    dialog: document.getElementById("workspace-designate-preset-dialog"),
    form: document.getElementById("workspace-designate-preset-form"),
    mode: document.getElementById("workspace-designate-preset-mode"),
    list: document.getElementById("workspace-designate-preset-list"),
    status: document.getElementById("workspace-designate-preset-status"),
    submit: document.getElementById("workspace-designate-preset-submit"),
    cancel: document.getElementById("workspace-designate-preset-cancel"),
  };
}

function workspaceDesignatePresetSelectedAgent() {
  const checked = document.querySelector(
    'input[name="workspace-designate-preset-agent"]:checked',
  );
  return checked?.value === "claude" ? "claude" : "codex";
}

function updateWorkspaceDesignatePresetStatus(message, tone = "muted") {
  const { status } = workspaceDesignatePresetDialogElements();
  if (!status) {
    return;
  }
  status.textContent = message;
  status.dataset.tone = tone;
  status.hidden = !message;
}

// Model/base_url lines shown under each preset name.
function workspaceDesignatePresetMeta(preset, agent) {
  const model = specifiedPresetModel(preset, agent) || "未设置模型";
  const baseUrl = String(preset?.base_url || "").trim() || "未设置 Base URL";
  return `${model} · ${baseUrl}`;
}

function workspaceDesignatePresetOptions(agent) {
  return agent === "claude" ? state.claudePresets : state.apiPresets;
}

function workspaceDesignatePresetLoaded(agent) {
  return agent === "claude" ? state.claudePresetsLoaded : state.apiPresetsLoaded;
}

function workspaceDesignatePresetEndpoint(agent) {
  return specifiedPresetListEndpoint(agent);
}

function renderWorkspaceDesignatePresetOptions({ loading = false, error = "" } = {}) {
  const { list, submit } = workspaceDesignatePresetDialogElements();
  if (!list || !submit) {
    return;
  }

  const agent = workspaceDesignatePresetSelectedAgent();
  const presets = workspaceDesignatePresetOptions(agent);
  const availablePresets = Array.isArray(presets) ? presets.filter((preset) => preset?.id) : [];
  const selectedPresetId =
    list.querySelector('input[name="workspace-designate-preset"]:checked')?.value || "";

  list.replaceChildren();
  submit.disabled = loading || availablePresets.length === 0;

  if (loading && availablePresets.length === 0) {
    const loadingRow = document.createElement("div");
    loadingRow.className = "workspace-history-preset-empty";
    loadingRow.textContent = "正在读取预设…";
    list.appendChild(loadingRow);
    updateWorkspaceDesignatePresetStatus("", "info");
    return;
  }

  if (availablePresets.length === 0) {
    const emptyRow = document.createElement("div");
    emptyRow.className = "workspace-history-preset-empty";
    emptyRow.textContent = error || `还没有可用的 ${agent === "claude" ? "Claude" : "Codex"} API 预设。`;
    list.appendChild(emptyRow);
    updateWorkspaceDesignatePresetStatus(error, error ? "warn" : "muted");
    return;
  }

  const defaultPreset =
    availablePresets.find((preset) => preset.id === selectedPresetId) ||
    availablePresets.find((preset) => preset.active) ||
    availablePresets[0];

  availablePresets.forEach((preset) => {
    const option = document.createElement("label");
    option.className = "workspace-history-preset-option";

    const radio = document.createElement("input");
    radio.type = "radio";
    radio.name = "workspace-designate-preset";
    radio.value = preset.id;
    radio.checked = preset.id === defaultPreset.id;

    const content = document.createElement("span");
    content.className = "workspace-history-preset-option-content";
    const title = document.createElement("span");
    title.className = "workspace-history-preset-option-title";
    const name = document.createElement("strong");
    name.textContent = preset.name || preset.id;
    title.appendChild(name);
    if (preset.active) {
      const current = document.createElement("span");
      current.className = "workspace-history-preset-current";
      current.textContent = "当前";
      title.appendChild(current);
    }

    const meta = document.createElement("span");
    meta.className = "workspace-history-preset-option-meta mono-text";
    meta.textContent = workspaceDesignatePresetMeta(preset, agent);
    content.append(title, meta);
    option.append(radio, content);
    list.appendChild(option);
  });

  updateWorkspaceDesignatePresetStatus(
    error || `共 ${availablePresets.length} 个预设`,
    error ? "warn" : "muted",
  );
}

async function openWorkspaceDesignatePresetDialog(path, trigger = null) {
  const { dialog } = workspaceDesignatePresetDialogElements();
  if (!dialog) {
    return;
  }

  const resolvedPath = String(path || state.currentPath || "").trim();
  workspaceDesignatePresetTargetPath = resolvedPath;
  workspaceDesignatePresetTargetName = "";
  workspaceDesignatePresetTrigger = trigger;
  workspaceDesignatePresetRequestToken += 1;

  renderWorkspaceDesignatePresetOptions({
    loading: !workspaceDesignatePresetLoaded(workspaceDesignatePresetSelectedAgent()),
  });
  if (!dialog.open) {
    dialog.showModal();
  }

  await refreshWorkspaceDesignatePresetOptions();
  document
    .querySelector('input[name="workspace-designate-preset"]:checked')
    ?.focus();
}

async function refreshWorkspaceDesignatePresetOptions() {
  const requestToken = ++workspaceDesignatePresetRequestToken;
  const agent = workspaceDesignatePresetSelectedAgent();
  try {
    const response = await requestJson(workspaceDesignatePresetEndpoint(agent));
    if (requestToken !== workspaceDesignatePresetRequestToken) {
      return;
    }
    const presets = Array.isArray(response?.presets) ? response.presets : [];
    if (agent === "claude") {
      state.claudePresets = presets;
      state.claudePresetsLoaded = true;
    } else {
      state.apiPresets = presets;
      state.apiPresetsLoaded = true;
    }
    renderWorkspaceDesignatePresetOptions();
  } catch (error) {
    if (requestToken !== workspaceDesignatePresetRequestToken) {
      return;
    }
    renderWorkspaceDesignatePresetOptions({ error: `读取预设失败：${error.message}` });
  }
}

function closeWorkspaceDesignatePresetDialog({ restoreFocus = true } = {}) {
  const { dialog } = workspaceDesignatePresetDialogElements();
  if (dialog?.open) {
    dialog.close();
  }
  const trigger = workspaceDesignatePresetTrigger;
  workspaceDesignatePresetRequestToken += 1;
  workspaceDesignatePresetTargetPath = "";
  workspaceDesignatePresetTargetName = "";
  workspaceDesignatePresetTrigger = null;
  if (restoreFocus) {
    trigger?.focus?.();
  }
}

async function launchWorkspaceDesignatePreset() {
  const { dialog, list, submit } = workspaceDesignatePresetDialogElements();
  const agent = workspaceDesignatePresetSelectedAgent();
  const selectedPresetId =
    list?.querySelector('input[name="workspace-designate-preset"]:checked')?.value || "";
  const targetPath = workspaceDesignatePresetTargetPath;
  if (!selectedPresetId) {
    updateWorkspaceDesignatePresetStatus("请选择一个可用预设。", "warn");
    return;
  }

  submit.disabled = true;
  updateWorkspaceDesignatePresetStatus("正在准备临时预设…", "info");
  try {
    await executeSpecifiedPreset({
      action: "launch",
      agent,
      presetId: selectedPresetId,
      cwd: targetPath,
      command: agent,
      quickStart: false,
    });
  } catch (error) {
    updateWorkspaceDesignatePresetStatus(`准备临时预设失败：${error.message}`, "warn");
    if (dialog?.open) {
      submit.disabled = false;
    }
    return;
  }

  closeWorkspaceDesignatePresetDialog({ restoreFocus: false });
}

function bindWorkspaceDesignatePresetDialog() {
  const { dialog, form, cancel, mode } = workspaceDesignatePresetDialogElements();
  if (!dialog || !form || !cancel || dialog.dataset.bound === "true") {
    return;
  }
  dialog.dataset.bound = "true";
  dialog.addEventListener("cancel", (event) => {
    event.preventDefault();
    closeWorkspaceDesignatePresetDialog();
  });
  dialog.addEventListener("click", (event) => {
    if (event.target === dialog) {
      closeWorkspaceDesignatePresetDialog();
    }
  });
  cancel.addEventListener("click", () => closeWorkspaceDesignatePresetDialog());
  mode?.addEventListener("change", () => {
    workspaceDesignatePresetRequestToken += 1;
    renderWorkspaceDesignatePresetOptions({
      loading: !workspaceDesignatePresetLoaded(workspaceDesignatePresetSelectedAgent()),
    });
    refreshWorkspaceDesignatePresetOptions();
  });
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    await launchWorkspaceDesignatePreset();
  });
}
