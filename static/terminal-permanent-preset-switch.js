let terminalPermanentPresetTrigger = null;
let terminalPermanentPresetRequestToken = 0;

function terminalPermanentPresetDom() {
  return {
    dialog: document.getElementById("terminal-permanent-preset-dialog"),
    form: document.getElementById("terminal-permanent-preset-form"),
    agent: document.getElementById("terminal-permanent-preset-agent"),
    preset: document.getElementById("terminal-permanent-preset-select"),
    path: document.getElementById("terminal-permanent-preset-path"),
    status: document.getElementById("terminal-permanent-preset-status"),
    submit: document.getElementById("terminal-permanent-preset-submit"),
    close: document.getElementById("terminal-permanent-preset-close"),
  };
}

function terminalPermanentPresetAgent() {
  return specifiedPresetAgent(terminalPermanentPresetDom().agent?.value);
}

function terminalPermanentPresetProjectPath() {
  const session = state.sessions.find((item) => item.id === state.activeSessionId);
  return String(session ? sessionPath(session) : state.currentPath || "");
}

function setTerminalPermanentPresetStatus(message = "", tone = "muted") {
  const { status } = terminalPermanentPresetDom();
  if (!status) return;
  status.hidden = !message;
  status.textContent = message;
  status.dataset.tone = tone;
}

function setTerminalPermanentPresetBusy(busy) {
  const { agent, preset, submit, close } = terminalPermanentPresetDom();
  if (agent) agent.disabled = busy;
  if (preset) preset.disabled = busy;
  if (submit) submit.disabled = busy;
  if (close) close.disabled = busy;
}

async function loadTerminalPermanentPresetOptions() {
  const requestToken = ++terminalPermanentPresetRequestToken;
  const agent = terminalPermanentPresetAgent();
  const { preset, submit } = terminalPermanentPresetDom();
  if (!preset) return;
  preset.disabled = true;
  if (submit) submit.disabled = true;
  setTerminalPermanentPresetStatus("正在读取预设…", "info");
  try {
    const response = await requestJson(specifiedPresetListEndpoint(agent));
    if (requestToken !== terminalPermanentPresetRequestToken) return;
    const presets = Array.isArray(response?.presets)
      ? response.presets.filter((item) => item?.id)
      : [];
    preset.replaceChildren();
    for (const item of presets) {
      const option = document.createElement("option");
      const model = specifiedPresetModel(item, agent);
      option.value = item.id;
      option.textContent = `${item.name || item.id}${model ? ` · ${model}` : ""}`;
      option.selected = Boolean(item.active);
      preset.append(option);
    }
    if (!preset.value && presets[0]) preset.value = presets[0].id;
    if (presets.length === 0) {
      const option = document.createElement("option");
      option.value = "";
      option.textContent = "没有可用预设";
      preset.append(option);
    }
    preset.disabled = false;
    if (submit) submit.disabled = presets.length === 0;
    setTerminalPermanentPresetStatus(
      presets.length > 0 ? `共 ${presets.length} 个预设` : "没有可用预设",
      presets.length > 0 ? "muted" : "warn",
    );
  } catch (error) {
    if (requestToken !== terminalPermanentPresetRequestToken) return;
    setTerminalPermanentPresetStatus(`读取预设失败：${error.message}`, "warn");
  }
}

async function openTerminalPermanentPresetSwitchDialog(trigger = null) {
  const dom = terminalPermanentPresetDom();
  if (!dom.dialog) {
    updateStatus("永久切换预设对话框不可用。", "warn");
    return;
  }
  terminalPermanentPresetTrigger = trigger;
  if (dom.path) {
    dom.path.textContent = terminalDisplayPath(terminalPermanentPresetProjectPath());
  }
  setTerminalPermanentPresetStatus();
  if (!dom.dialog.open) dom.dialog.showModal();
  await loadTerminalPermanentPresetOptions();
  dom.preset?.focus();
}

function closeTerminalPermanentPresetSwitchDialog() {
  const { dialog } = terminalPermanentPresetDom();
  terminalPermanentPresetRequestToken += 1;
  if (dialog?.open) dialog.close();
  terminalPermanentPresetTrigger?.focus?.({ preventScroll: true });
  terminalPermanentPresetTrigger = null;
}

async function submitTerminalPermanentPresetSwitch() {
  const dom = terminalPermanentPresetDom();
  if (!dom.preset?.value) {
    setTerminalPermanentPresetStatus("请选择一个可用预设。", "warn");
    return;
  }
  const agent = terminalPermanentPresetAgent();
  setTerminalPermanentPresetBusy(true);
  setTerminalPermanentPresetStatus("正在永久切换预设…", "info");
  try {
    const applied = await executeSpecifiedPreset({
      action: "apply",
      agent,
      presetId: dom.preset.value,
      projectPath: terminalPermanentPresetProjectPath(),
    });
    if (applied?.deferred) {
      setTerminalPermanentPresetStatus(
        `已登记切换到 ${applied.name || applied.preset_id || "指定预设"}。当前临时切换结束并恢复原配置后，才会永久切换到该预设。`,
        "info",
      );
      return;
    }
    updateStatus(
      `已永久切换到 ${applied?.name || applied?.preset_id || "指定预设"}。`,
      "ok",
    );
    closeTerminalPermanentPresetSwitchDialog();
  } catch (error) {
    setTerminalPermanentPresetStatus(`永久切换失败：${error.message}`, "warn");
  } finally {
    setTerminalPermanentPresetBusy(false);
  }
}

function bindTerminalPermanentPresetSwitchDialog() {
  const dom = terminalPermanentPresetDom();
  if (!dom.dialog || !dom.form || dom.dialog.dataset.bound === "true") return;
  dom.dialog.dataset.bound = "true";
  dom.agent?.addEventListener("change", loadTerminalPermanentPresetOptions);
  dom.form.addEventListener("submit", (event) => {
    event.preventDefault();
    submitTerminalPermanentPresetSwitch();
  });
  dom.close?.addEventListener("click", closeTerminalPermanentPresetSwitchDialog);
  dom.dialog.addEventListener("cancel", (event) => {
    event.preventDefault();
    closeTerminalPermanentPresetSwitchDialog();
  });
}

bindTerminalPermanentPresetSwitchDialog();
