let terminalSpecifiedTaskId = "";
let terminalSpecifiedTaskTrigger = null;
let terminalSpecifiedTaskLastRecord = null;
let terminalSpecifiedTaskSourcePath = "";
let terminalSpecifiedTaskSourceName = "";
let terminalSpecifiedTaskSessionFieldPinned = false;
let terminalSpecifiedTaskAgentPinned = false;
let terminalSpecifiedTaskTerminalNameOverride = "";
let terminalSpecifiedTaskNamingAction = "";

function terminalSpecifiedTaskDom() {
  return {
    button: document.getElementById("terminal-specified-task-button"),
    dialog: document.getElementById("terminal-specified-task-dialog"),
    form: document.getElementById("terminal-specified-task-form"),
    title: document.getElementById("terminal-specified-task-title"),
    preset: document.getElementById("terminal-specified-task-preset"),
    task: document.getElementById("terminal-specified-task-text"),
    timeout: document.getElementById("terminal-specified-task-timeout"),
    timeoutField: document.getElementById("terminal-specified-task-timeout-field"),
    path: document.getElementById("terminal-specified-task-path"),
    fixedOptions: document.getElementById("terminal-specified-task-fixed-options"),
    temporary: document.getElementById("terminal-specified-task-temporary"),
    sessionIdField: document.getElementById("terminal-specified-task-session-id-field"),
    sessionId: document.getElementById("terminal-specified-task-session-id"),
    terminalName: document.getElementById("terminal-specified-task-terminal-name"),
    terminalNamePreview: document.getElementById("terminal-specified-task-terminal-name-preview"),
    taskLabel: document.getElementById("terminal-specified-task-text-label"),
    status: document.getElementById("terminal-specified-task-status"),
    result: document.getElementById("terminal-specified-task-result"),
    run: document.getElementById("terminal-specified-task-run"),
    cancelTask: document.getElementById("terminal-specified-task-cancel-task"),
    close: document.getElementById("terminal-specified-task-close"),
  };
}

function terminalSpecifiedTaskAgent() {
  return document.querySelector('input[name="terminal-specified-task-agent"]:checked')?.value === "claude"
    ? "claude"
    : "codex";
}

function terminalSpecifiedTaskMode() {
  return document.querySelector('input[name="terminal-specified-task-mode"]:checked')?.value || "fixed";
}

function terminalSpecifiedTaskTemporary() {
  return terminalSpecifiedTaskDom().temporary?.checked !== false;
}

function terminalSpecifiedTaskSessionAction() {
  return document.querySelector('input[name="terminal-specified-task-session-action"]:checked')?.value || "new";
}

function terminalCodexTaskStatusLabel(status) {
  return {
    queued: "排队中",
    applying_preset: "正在应用指定预设",
    starting: "正在启动",
    running: "执行中",
    collecting: "正在整理结果",
    succeeded: "已完成",
    failed: "失败",
    timed_out: "超时",
    cancelled: "已取消",
  }[status] || String(status || "未知");
}

function terminalSpecifiedTaskCurrentSession() {
  return state.sessions.find((item) => item.id === state.activeSessionId) || null;
}

function terminalSpecifiedTaskCurrentPath() {
  const session = terminalSpecifiedTaskCurrentSession();
  return session ? sessionPath(session) : state.currentPath;
}

function terminalSpecifiedTaskNameWithoutAutoIndices(name) {
  return String(name || "")
    .trim()
    .replace(/[_#]\d+(?=$|[\s_])/g, "")
    .replace(/_{2,}/g, "_")
    .replace(/\s{2,}/g, " ")
    .replace(/^[_\s]+|[_\s]+$/g, "");
}

function terminalSpecifiedTaskUniqueFinalName(name) {
  const normalized = String(name || "").trim();
  if (!normalized) {
    return "";
  }
  const usedNames = new Set(
    state.sessions.map((session) => String(session?.name || "").trim()),
  );
  if (!usedNames.has(normalized)) {
    return normalized;
  }
  let ordinal = 2;
  while (usedNames.has(`${normalized}-${ordinal}`)) {
    ordinal += 1;
  }
  return `${normalized}-${ordinal}`;
}

function terminalSpecifiedTaskFinalTerminalName() {
  if (terminalSpecifiedTaskTerminalNameOverride) {
    return terminalSpecifiedTaskUniqueFinalName(terminalSpecifiedTaskTerminalNameOverride);
  }
  const { terminalName } = terminalSpecifiedTaskDom();
  const sourceTerminalName = String(
    terminalName ? terminalName.value : terminalSpecifiedTaskSourceName,
  ).trim();
  if (!sourceTerminalName) {
    return "";
  }
  const namingBase = terminalSpecifiedTaskNameWithoutAutoIndices(sourceTerminalName);
  if (!namingBase) {
    return "";
  }
  const namingAction = terminalSpecifiedTaskNamingAction || terminalSpecifiedTaskSessionAction();
  if (namingAction === "new") {
    return terminalSpecifiedTaskUniqueFinalName(`${namingBase}_new`);
  }
  const finalName = specifiedPresetTerminalName({
    sourceTerminalName: namingBase,
    sessionAction: namingAction,
  });
  return terminalSpecifiedTaskUniqueFinalName(finalName);
}

function renderTerminalSpecifiedTaskTerminalNamePreview() {
  const { terminalNamePreview } = terminalSpecifiedTaskDom();
  if (!terminalNamePreview) {
    return;
  }
  terminalNamePreview.textContent = terminalSpecifiedTaskFinalTerminalName() || "自动分配";
}

function setTerminalSpecifiedTaskStatus(message = "", tone = "muted") {
  const { status } = terminalSpecifiedTaskDom();
  if (!status) {
    return;
  }
  status.hidden = !message;
  status.textContent = message;
  status.dataset.tone = tone;
}

function syncTerminalSpecifiedTaskForm() {
  const dom = terminalSpecifiedTaskDom();
  const agent = terminalSpecifiedTaskAgent();
  let mode = terminalSpecifiedTaskMode();
  if (agent === "claude" && mode !== "fixed") {
    const fixed = dom.form?.querySelector('input[name="terminal-specified-task-mode"][value="fixed"]');
    if (fixed) {
      fixed.checked = true;
      mode = "fixed";
    }
  }

  dom.form?.querySelectorAll('input[name="terminal-specified-task-mode"]').forEach((input) => {
    input.disabled = Boolean(terminalSpecifiedTaskId) || (agent === "claude" && input.value !== "fixed");
  });
  const fixed = mode === "fixed";
  const sessionAction = terminalSpecifiedTaskSessionAction();
  dom.form?.querySelectorAll('input[name="terminal-specified-task-agent"]').forEach((input) => {
    input.disabled = Boolean(terminalSpecifiedTaskId) || terminalSpecifiedTaskAgentPinned;
  });
  if (dom.fixedOptions) {
    dom.fixedOptions.hidden = !fixed;
  }
  if (dom.sessionIdField) {
    dom.sessionIdField.hidden = !fixed
      || (sessionAction === "new" && !terminalSpecifiedTaskSessionFieldPinned);
  }
  if (dom.sessionId) {
    dom.sessionId.required = fixed && sessionAction !== "new";
  }
  if (dom.timeoutField) {
    dom.timeoutField.hidden = fixed;
  }
  if (dom.task) {
    dom.task.required = !fixed;
    dom.task.placeholder = fixed ? "可选：启动后立即交给 Agent 的任务" : "输入任务要求";
  }
  if (dom.taskLabel) {
    dom.taskLabel.textContent = fixed ? "初始任务（可选）" : "任务";
  }
  if (dom.run) {
    dom.run.textContent = fixed ? "启动" : "执行";
  }
  renderTerminalSpecifiedTaskTerminalNamePreview();
}

function setTerminalSpecifiedTaskBusy(busy) {
  const dom = terminalSpecifiedTaskDom();
  dom.form?.querySelectorAll("input, select, textarea").forEach((input) => {
    input.disabled = busy;
  });
  if (!busy) {
    syncTerminalSpecifiedTaskForm();
  }
  if (dom.run) {
    dom.run.disabled = busy;
  }
  if (dom.cancelTask) {
    dom.cancelTask.hidden = !terminalSpecifiedTaskId;
    dom.cancelTask.disabled = !busy || !terminalSpecifiedTaskId;
  }
}

function terminalSpecifiedTaskResultText(record) {
  if (String(record?.result || "").trim()) {
    return record.result.trim();
  }
  if (String(record?.error || "").trim()) {
    return record.error.trim();
  }
  return String(record?.transcript_tail || "").trim();
}

function renderTerminalSpecifiedTaskRecord(record) {
  if (!record) {
    return;
  }
  terminalSpecifiedTaskLastRecord = record;
  const { status, result } = terminalSpecifiedTaskDom();
  const statusLabel = terminalCodexTaskStatusLabel(record.status);
  const presetName = record.preset?.name || record.preset?.id || "未知预设";
  const expectedModel = record.preset?.model || "未设置模型";
  const actualModel = record.actual_model || "等待 Codex 报告";
  const terminalState = record.mode === "terminal" && WEBCLX_CODEX_TASK_FINAL_STATUSES.has(record.status)
    ? `；临时终端${record.terminal_closed ? "已关闭" : "关闭失败"}`
    : "";
  if (status) {
    status.hidden = false;
    status.dataset.tone = record.status === "succeeded"
      ? "ok"
      : WEBCLX_CODEX_TASK_FINAL_STATUSES.has(record.status)
        ? "warn"
        : "info";
    status.textContent = `${statusLabel}；${presetName}；预设模型 ${expectedModel}；实际模型 ${actualModel}${terminalState}`;
  }
  if (result) {
    const text = terminalSpecifiedTaskResultText(record);
    result.hidden = !text;
    result.textContent = text;
  }
}

async function loadTerminalSpecifiedTaskPresets() {
  const { preset } = terminalSpecifiedTaskDom();
  if (!preset) {
    return;
  }
  const agent = terminalSpecifiedTaskAgent();
  const previous = preset.dataset.agent === agent ? preset.value : "";
  preset.disabled = true;
  try {
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
      option.selected = item.id === previous || (!previous && item.active);
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
    preset.dataset.agent = agent;
    preset.disabled = Boolean(terminalSpecifiedTaskId);
  } catch (error) {
    setTerminalSpecifiedTaskStatus(`读取预设失败：${error.message}`, "warn");
    preset.disabled = false;
  }
}

function showTerminalCodexTaskResult(record, { source = "specified" } = {}) {
  const { dialog } = terminalSpecifiedTaskDom();
  renderTerminalSpecifiedTaskRecord(record);
  if (dialog && !dialog.open) {
    dialog.dataset.source = source;
    dialog.showModal();
  }
}

async function openTerminalSpecifiedTaskDialog(trigger = null, options = {}) {
  const dom = terminalSpecifiedTaskDom();
  if (!dom.dialog) {
    return;
  }
  const sourceSession = terminalSpecifiedTaskCurrentSession();
  terminalSpecifiedTaskSourcePath = options.sourcePath === undefined
    ? terminalSpecifiedTaskCurrentPath()
    : String(options.sourcePath || "");
  terminalSpecifiedTaskSourceName = options.sourceTerminalName === undefined
    ? String(sourceSession?.name || "").trim()
    : String(options.sourceTerminalName || "").trim();
  terminalSpecifiedTaskTerminalNameOverride = String(options.terminalName || "").trim();
  terminalSpecifiedTaskNamingAction = options.namingAction
    ? specifiedPresetSessionAction(options.namingAction)
    : "";
  terminalSpecifiedTaskSessionFieldPinned = options.showSessionField === true;
  terminalSpecifiedTaskAgentPinned = options.lockAgent === true;
  terminalSpecifiedTaskTrigger = trigger;
  if (options.agent) {
    const agent = specifiedPresetAgent(options.agent);
    dom.form?.querySelectorAll('input[name="terminal-specified-task-agent"]').forEach((input) => {
      input.checked = input.value === agent;
    });
  }
  if (options.mode) {
    const mode = dom.form?.querySelector(
      `input[name="terminal-specified-task-mode"][value="${options.mode}"]`,
    );
    if (mode) {
      mode.checked = true;
    }
  }
  if (options.sessionAction) {
    const action = dom.form?.querySelector(
      `input[name="terminal-specified-task-session-action"][value="${options.sessionAction}"]`,
    );
    if (action) {
      action.checked = true;
    }
  }
  if (dom.sessionId && options.sessionId !== undefined) {
    dom.sessionId.value = String(options.sessionId || "").trim();
  }
  if (dom.task && options.resetTask === true) {
    dom.task.value = "";
  }
  if (dom.title) {
    dom.title.textContent = options.title ? String(options.title) : "指定预设临时执行";
  }
  if (dom.path) {
    dom.path.textContent = terminalDisplayPath(terminalSpecifiedTaskSourcePath);
  }
  if (dom.terminalName) {
    dom.terminalName.value = terminalSpecifiedTaskSourceName;
  }
  if (!terminalSpecifiedTaskId && !terminalSpecifiedTaskLastRecord) {
    setTerminalSpecifiedTaskStatus();
    if (dom.result) {
      dom.result.hidden = true;
      dom.result.textContent = "";
    }
  }
  syncTerminalSpecifiedTaskForm();
  if (!dom.dialog.open) {
    dom.dialog.showModal();
  }
  await loadTerminalSpecifiedTaskPresets();
  dom.preset?.focus();
}

function closeTerminalSpecifiedTaskDialog() {
  const { dialog, sessionId } = terminalSpecifiedTaskDom();
  if (dialog?.open) {
    dialog.close();
  }
  terminalSpecifiedTaskTrigger?.focus?.({ preventScroll: true });
  terminalSpecifiedTaskTrigger = null;
  if (terminalSpecifiedTaskSessionFieldPinned && sessionId) {
    sessionId.value = "";
  }
  terminalSpecifiedTaskSessionFieldPinned = false;
  terminalSpecifiedTaskAgentPinned = false;
  terminalSpecifiedTaskTerminalNameOverride = "";
  terminalSpecifiedTaskNamingAction = "";
}

async function cleanupFailedTerminalSpecifiedPresetLaunch(created, previousSessionId, cwd) {
  const createdSessionId = String(created?.id || "").trim();
  if (!createdSessionId) {
    return;
  }
  const wasActiveSession = state.activeSessionId === createdSessionId;
  const deleted = await requestJson(
    `/api/terminal/sessions/${encodeURIComponent(createdSessionId)}`,
    {
      method: "DELETE",
      headers: {
        "X-WebClx-Confirm-Session": createdSessionId,
        "X-WebClx-Delete-Source": "specified-preset-launch",
      },
    },
  );
  state.pendingCreatedSessionIds.delete(createdSessionId);
  announceSessionMutation("deleted", deleted);
  if (wasActiveSession) {
    closeSocket({ suppressEvents: true });
    clearActiveSession();
    updateStatus("指定预设启动失败，已恢复原终端。", "warn");
  }
  disposeTerminalSessionContext(createdSessionId);
  forgetSessionPreference(cwd || state.currentPath, createdSessionId);
  await loadSessions({
    preferredSessionId: previousSessionId,
    forcePreferredSession: true,
  });
}

async function launchTerminalSpecifiedPreset(cwd, options = {}) {
  const previousSessionId = state.activeSessionId;
  let created = null;
  try {
    created = await createSession({
      autoSelect: true,
      suppressLoadingStatus: true,
      pushHistoryOnSelect: true,
      enableQuickStart: false,
      throwOnError: true,
      path: cwd,
      origin: options.origin || "agent",
      ownerKey: options.ownerKey || "",
      codexApiPresetId: options.codexApiPresetId || "",
    });
    if (!created) {
      throw new Error("固定终端创建失败。");
    }
    if (options.terminalName) {
      try {
        created = await renameTerminalForTool(created.id, options.terminalName, cwd);
      } catch (renameError) {
        // Name conflict: keep the auto-generated name rather than failing the whole launch
        console.warn?.(`终端名称 ${options.terminalName} 已存在，保留默认名称 ${created.name}`);
      }
    }
    await waitForTerminalToolSessionReady(created.id);
    const sent = await sendTerminalAutoTypedInput(options.runCommand, {
      sessionId: created.id,
      throwOnError: true,
    });
    if (!sent) {
      throw new Error("Agent 启动命令为空，未发送。");
    }
    return created;
  } catch (launchError) {
    if (!created?.id) {
      throw launchError;
    }
    try {
      await cleanupFailedTerminalSpecifiedPresetLaunch(created, previousSessionId, cwd);
    } catch (cleanupError) {
      throw new Error(
        `${launchError.message || launchError}；自动清理失败：${cleanupError.message || cleanupError}`,
      );
    }
    throw launchError;
  }
}

async function submitTerminalSpecifiedTask() {
  const dom = terminalSpecifiedTaskDom();
  const agent = terminalSpecifiedTaskAgent();
  const mode = terminalSpecifiedTaskMode();
  const task = String(dom.task?.value || "").trim();
  if (!dom.preset?.value) {
    setTerminalSpecifiedTaskStatus(`请选择 ${agent === "claude" ? "Claude" : "Codex"} API 预设。`, "warn");
    return;
  }
  if (mode !== "fixed" && !task) {
    setTerminalSpecifiedTaskStatus("请输入交给 Codex 的任务。", "warn");
    return;
  }
  if (agent === "claude" && mode !== "fixed") {
    setTerminalSpecifiedTaskStatus("Claude API 预设当前仅支持固定终端。", "warn");
    return;
  }

  const timeoutSecs = Number(dom.timeout?.value || 1800);
  if (mode !== "fixed" && (!Number.isFinite(timeoutSecs) || timeoutSecs < 1 || timeoutSecs > 7200)) {
    setTerminalSpecifiedTaskStatus("超时时间必须在 1 到 7200 秒之间。", "warn");
    return;
  }
  setTerminalSpecifiedTaskBusy(true);
  if (dom.result) {
    dom.result.hidden = true;
    dom.result.textContent = "";
  }
  try {
    if (mode === "fixed") {
      const sessionAction = terminalSpecifiedTaskSessionAction();
      const result = await executeSpecifiedPreset({
        action: "launch",
        agent,
        presetId: dom.preset.value,
        cwd: terminalSpecifiedTaskSourcePath || terminalSpecifiedTaskCurrentPath(),
        temporary: terminalSpecifiedTaskTemporary(),
        sessionAction,
        sessionId: dom.sessionId?.value,
        terminalName: terminalSpecifiedTaskFinalTerminalName(),
        sourceTerminalName: dom.terminalName?.value || terminalSpecifiedTaskSourceName,
        task,
        origin: "agent",
        ownerKey: "manual-specified-preset",
        quickStart: false,
        launchTerminal: launchTerminalSpecifiedPreset,
      });
      const launched = result.launchResult;
      const deferred = Boolean(result.applied?.deferred);
      setTerminalSpecifiedTaskStatus(
        deferred
          ? `已创建固定终端 ${launched?.name || launched?.id || ""}，正在等待当前预设任务结束后启动 Agent。`
          : `已启动固定终端 ${launched?.name || launched?.id || ""}。`,
        "ok",
      );
      updateStatus(
        deferred
          ? `已按指定预设创建 ${agent === "claude" ? "Claude" : "Codex"} 固定终端；预设已排队，当前任务结束后自动启动。`
          : `已按临时预设启动 ${agent === "claude" ? "Claude" : "Codex"} 固定终端；Agent 启动后将恢复原预设。`,
        "ok",
      );
      closeTerminalSpecifiedTaskDialog();
      return;
    }

    const record = await executeSpecifiedPreset({
      action: "task",
      agent,
      mode,
      presetId: dom.preset.value,
      cwd: terminalSpecifiedTaskSourcePath || terminalSpecifiedTaskCurrentPath(),
      task,
      timeoutSecs,
      onCreated(created) {
        terminalSpecifiedTaskId = created.id;
      },
      onProgress: renderTerminalSpecifiedTaskRecord,
    });
    updateStatus(
      record.status === "succeeded"
        ? `指定任务已完成：${record.preset?.name || record.preset?.id || "Codex"}`
        : `指定任务${terminalCodexTaskStatusLabel(record.status)}：${record.error || "请查看结果"}`,
      record.status === "succeeded" ? "ok" : "warn",
    );
  } catch (error) {
    setTerminalSpecifiedTaskStatus(`执行失败：${error.message}`, "warn");
  } finally {
    terminalSpecifiedTaskId = "";
    setTerminalSpecifiedTaskBusy(false);
  }
}

async function cancelTerminalSpecifiedTask() {
  if (!terminalSpecifiedTaskId) {
    return;
  }
  const { cancelTask } = terminalSpecifiedTaskDom();
  if (cancelTask) {
    cancelTask.disabled = true;
  }
  try {
    const record = await requestJson(
      `/api/codex/tasks/${encodeURIComponent(terminalSpecifiedTaskId)}`,
      { method: "DELETE" },
    );
    renderTerminalSpecifiedTaskRecord(record);
    setTerminalSpecifiedTaskStatus("已请求取消，正在关闭任务进程和临时终端。", "info");
  } catch (error) {
    setTerminalSpecifiedTaskStatus(`取消失败：${error.message}`, "warn");
    if (cancelTask) {
      cancelTask.disabled = false;
    }
  }
}

function bindTerminalSpecifiedTaskDialog() {
  const dom = terminalSpecifiedTaskDom();
  if (!dom.dialog || !dom.form || dom.dialog.dataset.bound === "true") {
    return;
  }
  dom.dialog.dataset.bound = "true";
  dom.form.addEventListener("submit", (event) => {
    event.preventDefault();
    submitTerminalSpecifiedTask();
  });
  dom.form.querySelectorAll('input[name="terminal-specified-task-agent"]').forEach((input) => {
    input.addEventListener("change", async () => {
      syncTerminalSpecifiedTaskForm();
      await loadTerminalSpecifiedTaskPresets();
    });
  });
  dom.form.querySelectorAll(
    'input[name="terminal-specified-task-mode"], input[name="terminal-specified-task-session-action"]',
  ).forEach((input) => input.addEventListener("change", syncTerminalSpecifiedTaskForm));
  dom.terminalName?.addEventListener("input", renderTerminalSpecifiedTaskTerminalNamePreview);
  dom.close?.addEventListener("click", closeTerminalSpecifiedTaskDialog);
  dom.cancelTask?.addEventListener("click", cancelTerminalSpecifiedTask);
  dom.dialog.addEventListener("cancel", (event) => {
    event.preventDefault();
    closeTerminalSpecifiedTaskDialog();
  });
  dom.dialog.addEventListener("click", (event) => {
    if (event.target === dom.dialog) {
      closeTerminalSpecifiedTaskDialog();
    }
  });
  syncTerminalSpecifiedTaskForm();
}
