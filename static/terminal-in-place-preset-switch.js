const TERMINAL_IN_PLACE_PRESET_EXIT_TIMEOUT_MS = 12000;

let terminalInPlacePresetTarget = null;
let terminalInPlacePresetTrigger = null;
let terminalInPlacePresetOpening = false;

function terminalInPlacePresetDom() {
  return {
    dialog: document.getElementById("terminal-in-place-preset-dialog"),
    form: document.getElementById("terminal-in-place-preset-form"),
    agent: document.getElementById("terminal-in-place-preset-agent"),
    preset: document.getElementById("terminal-in-place-preset-select"),
    session: document.getElementById("terminal-in-place-preset-session"),
    path: document.getElementById("terminal-in-place-preset-path"),
    status: document.getElementById("terminal-in-place-preset-status"),
    submit: document.getElementById("terminal-in-place-preset-submit"),
    close: document.getElementById("terminal-in-place-preset-close"),
    title: document.getElementById("terminal-in-place-preset-title"),
  };
}

function setTerminalInPlacePresetStatus(message = "", tone = "muted") {
  const { status } = terminalInPlacePresetDom();
  if (!status) {
    return;
  }
  status.hidden = !message;
  status.textContent = message;
  status.dataset.tone = tone;
}

function setTerminalInPlacePresetBusy(busy) {
  const { preset, submit, close } = terminalInPlacePresetDom();
  if (preset) preset.disabled = busy;
  if (submit) submit.disabled = busy;
  if (close) close.disabled = busy;
}

function terminalInPlacePresetCursorLine(sourceContext) {
  const buffer = sourceContext?.term?.buffer?.active;
  if (!buffer || typeof buffer.getLine !== "function") {
    return "";
  }
  const row = Math.max(0, Number(buffer.baseY || 0) + Number(buffer.cursorY || 0));
  return buffer.getLine(row)?.translateToString(true) || "";
}

function terminalInPlacePresetShellReady(sourceContext, initialLine) {
  const currentLine = terminalInPlacePresetCursorLine(sourceContext);
  return currentLine !== initialLine
    && Boolean(WebClxTerminalCursorGuard?.isLikelyShellPrompt?.(currentLine));
}

function waitForTerminalInPlacePresetShell(
  sourceContext,
  initialLine,
  timeoutMs = TERMINAL_IN_PLACE_PRESET_EXIT_TIMEOUT_MS,
) {
  return new Promise((resolve, reject) => {
    let settled = false;
    let renderDisposable = null;
    const finish = (error = null) => {
      if (settled) return;
      settled = true;
      renderDisposable?.dispose?.();
      window.clearTimeout(timeoutId);
      if (error) reject(error);
      else resolve();
    };
    const inspect = () => {
      if (terminalInPlacePresetShellReady(sourceContext, initialLine)) {
        finish();
      }
    };
    const timeoutId = window.setTimeout(() => {
      finish(new Error("退出当前 Agent 超时，未切换预设。"));
    }, timeoutMs);
    renderDisposable = sourceContext?.term?.onRender?.(inspect) || null;
    inspect();
  });
}

async function executeTerminalInPlacePresetSwitch(target, presetSelection) {
  const selection = presetSelection && typeof presetSelection === "object"
    ? presetSelection
    : { id: presetSelection };
  const normalizedPresetId = String(selection.id || "").trim();
  const sessionAction = target?.sessionAction === "new" ? "new" : "resume";
  if (!target?.sessionId || (sessionAction === "resume" && !target?.resumeId) || !normalizedPresetId) {
    throw new Error("终端、Session 或预设无效。");
  }

  if (!target.agentExited) {
    const initialLine = terminalInPlacePresetCursorLine(target.sourceContext);
    target.onStage?.("exiting");
    const exitSent = sendSlashCommand("/exit", { sessionId: target.sessionId });
    if (!exitSent) {
      throw new Error("无法向当前 Agent 发送 /exit。");
    }
    await mobileKeySendQueue;
    await waitForTerminalInPlacePresetShell(target.sourceContext, initialLine);
    target.agentExited = true;
  }

  target.onStage?.(sessionAction === "new" ? "starting" : "resuming");
  const agentCommand = specifiedPresetLaunchCommand({
    agent: target.agent,
    sessionAction,
    sessionId: sessionAction === "resume" ? target.resumeId : "",
  });
  const lease = await acquireSpecifiedPresetLease({
    agent: target.agent,
    presetId: normalizedPresetId,
    projectPath: target.cwd,
    owner: "terminal-in-place-preset",
  });
  try {
    const resumed = await sendTerminalAutoTypedInput(agentCommand, {
      sessionId: target.sessionId,
      throwOnError: true,
    });
    if (!resumed) {
      throw new Error("resume 命令为空，未发送。");
    }
    await waitForSpecifiedPresetAgentStart(target.sessionId, lease.lease_id);
    await releaseSpecifiedPresetLease(lease.lease_id);
    return agentCommand;
  } catch (error) {
    try {
      await releaseSpecifiedPresetLease(lease.lease_id);
    } catch {}
    throw error;
  }
}

async function loadTerminalInPlacePresetOptions(agent) {
  const { preset } = terminalInPlacePresetDom();
  if (!preset) return;
  preset.disabled = true;
  const response = await requestJson(specifiedPresetListEndpoint(agent));
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
  if (!preset.value && presets[0]) {
    preset.value = presets[0].id;
  }
  if (presets.length === 0) {
    const option = document.createElement("option");
    option.value = "";
    option.textContent = "没有可用预设";
    preset.append(option);
  }
  preset.disabled = false;
}

async function openTerminalInPlacePresetSwitchDialog(trigger = null, options = {}) {
  if (terminalInPlacePresetOpening) {
    updateStatus("正在读取当前 Session…", "info");
    return;
  }
  const sourceSessionId = String(state.activeSessionId || "").trim();
  const sourceSession = state.sessions.find((session) => session.id === sourceSessionId);
  if (!sourceSessionId || !sourceSession) {
    updateStatus("当前没有可切换预设的活动终端。", "warn");
    return;
  }

  terminalInPlacePresetOpening = true;
  updateStatus("正在读取当前 Session…", "info");
  try {
    await waitForTerminalToolSessionReady(sourceSessionId);
    const sourceContext = ensureTerminalSessionCache().get(sourceSessionId) || null;
    if (!sourceContext?.term) {
      throw new Error("当前终端尚未就绪。");
    }
    const detected = await detectAgentResumeIdComplete(sourceSessionId, sourceContext);
    if (!detected?.resumeId) {
      throw new Error("无法提取当前 Session，未退出当前会话。");
    }

    const agent = specifiedPresetAgent(detected.program);
    const sessionAction = options.sessionAction === "new" ? "new" : "resume";
    terminalInPlacePresetTarget = {
      agent,
      cwd: String(sessionPath(sourceSession) ?? state.currentPath),
      resumeId: detected.resumeId,
      sessionId: sourceSessionId,
      sourceContext,
      agentExited: false,
      sessionAction,
    };
    terminalInPlacePresetTrigger = trigger;
    const dom = terminalInPlacePresetDom();
    if (!dom.dialog) {
      throw new Error("终端内切换预设对话框不可用。");
    }
    if (dom.title) {
      dom.title.textContent = sessionAction === "new"
        ? "终端内临时切换预设（新会话）"
        : "原地切换预设+恢复";
    }
    if (dom.agent) dom.agent.value = agent === "claude" ? "Claude" : "Codex";
    if (dom.session) {
      dom.session.value = sessionAction === "resume" ? detected.resumeId : "新会话（不恢复）";
    }
    if (dom.submit) {
      dom.submit.textContent = sessionAction === "new" ? "切换并新建" : "切换并恢复";
    }
    if (dom.path) dom.path.textContent = terminalDisplayPath(terminalInPlacePresetTarget.cwd);
    setTerminalInPlacePresetStatus();
    dom.dialog.showModal();
    try {
      await loadTerminalInPlacePresetOptions(agent);
      dom.preset?.focus();
    } catch (error) {
      setTerminalInPlacePresetStatus(`读取预设失败：${error.message}`, "warn");
    }
    updateStatus(
      sessionAction === "new"
        ? "已识别当前 Agent，请选择新会话使用的预设。"
        : `已保存 Session ${shortResumeId(detected.resumeId)}，请选择预设。`,
      "ok",
    );
  } catch (error) {
    terminalInPlacePresetTarget = null;
    updateStatus(error?.message || "读取当前 Session 失败。", "warn");
  } finally {
    terminalInPlacePresetOpening = false;
  }
}

function closeTerminalInPlacePresetSwitchDialog() {
  const { dialog } = terminalInPlacePresetDom();
  if (dialog?.open) dialog.close();
  terminalInPlacePresetTrigger?.focus?.({ preventScroll: true });
  terminalInPlacePresetTrigger = null;
  terminalInPlacePresetTarget = null;
}

async function submitTerminalInPlacePresetSwitch() {
  const { preset } = terminalInPlacePresetDom();
  const target = terminalInPlacePresetTarget;
  if (!target || !preset?.value) {
    setTerminalInPlacePresetStatus("请选择一个可用预设。", "warn");
    return;
  }
  setTerminalInPlacePresetBusy(true);
  target.onStage = (stage) => {
    const messages = {
      exiting: "正在退出当前 Agent…",
      resuming: "正在恢复已保存的 Session…",
      starting: "正在启动新会话…",
    };
    setTerminalInPlacePresetStatus(messages[stage] || "正在切换…", "info");
  };
  try {
    await executeTerminalInPlacePresetSwitch(target, preset.value);
    updateStatus(
      target.sessionAction === "new"
        ? "已在原终端使用所选预设启动新会话。"
        : `已在原终端恢复 Session ${shortResumeId(target.resumeId)}。`,
      "ok",
    );
    closeTerminalInPlacePresetSwitchDialog();
  } catch (error) {
    setTerminalInPlacePresetStatus(error?.message || "终端内切换预设失败。", "warn");
  } finally {
    setTerminalInPlacePresetBusy(false);
  }
}

function bindTerminalInPlacePresetSwitchDialog() {
  const dom = terminalInPlacePresetDom();
  if (!dom.dialog || !dom.form || dom.dialog.dataset.bound === "true") return;
  dom.dialog.dataset.bound = "true";
  dom.form.addEventListener("submit", (event) => {
    event.preventDefault();
    submitTerminalInPlacePresetSwitch();
  });
  dom.close?.addEventListener("click", closeTerminalInPlacePresetSwitchDialog);
  dom.dialog.addEventListener("cancel", (event) => {
    event.preventDefault();
    closeTerminalInPlacePresetSwitchDialog();
  });
}

bindTerminalInPlacePresetSwitchDialog();
