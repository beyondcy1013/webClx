// Unified scheduled task management for the settings panel.
// Loaded before app.js; functions run after app.js globals are initialized.

const UNIFIED_TASK_TYPE_LABELS = {
  paste: "粘贴消息",
  command: "执行命令",
  continue: "发送继续",
  "auto-continue": "自动继续",
  "preset-test": "预设 API 测试",
};

let unifiedTaskState = {
  pasteTasks: [],
  autoContinueTasks: [],
  presetTestTasks: [],
  terminals: [],
  workdirs: [],
  apiPresets: [],
  claudePresets: [],
  editTaskId: null,
  editTaskSource: null,
  requestToken: 0,
};

function setUnifiedCreateStatus(message, tone) {
  const el = document.getElementById("unified-task-create-status");
  if (!el) return;
  el.textContent = message || "";
  el.dataset.tone = tone || "muted";
}

function setUnifiedListStatus(message, tone) {
  const el = document.getElementById("unified-task-list-status");
  if (!el) return;
  el.textContent = message || "";
  el.dataset.tone = tone || "muted";
}

async function loadUnifiedTerminals() {
  try {
    const response = await requestJson("/api/terminal/sessions?all=true");
    const sessions = Array.isArray(response?.sessions) ? response.sessions : [];
    unifiedTaskState.terminals = sessions.map((s) => ({
      id: s.id,
      name: s.name || s.id,
      path: s.display_path || s.path || "",
    }));

    // Build unique working directory list from active terminal paths
    const dirSet = new Map();
    unifiedTaskState.terminals.forEach((t) => {
      if (t.path && !dirSet.has(t.path)) {
        dirSet.set(t.path, true);
      }
    });
    unifiedTaskState.workdirs = Array.from(dirSet.keys()).sort();

    populateUnifiedTerminalSelect();
    populateUnifiedWorkdirSelect();
  } catch (error) {
    setUnifiedCreateStatus(error?.message || "读取终端列表失败。", "warn");
  }
}

async function loadUnifiedPresetTargets() {
  const [apiResult, claudeResult] = await Promise.allSettled([
    requestJson("/api/auth/api-presets"),
    requestJson("/api/auth/claude-presets"),
  ]);
  unifiedTaskState.apiPresets = apiResult.status === "fulfilled"
    ? apiResult.value?.presets || []
    : [];
  unifiedTaskState.claudePresets = claudeResult.status === "fulfilled"
    ? claudeResult.value?.presets || []
    : [];
  populateUnifiedPresetSelect();
  if (apiResult.status === "rejected" || claudeResult.status === "rejected") {
    setUnifiedCreateStatus("部分预设列表读取失败，请刷新后重试。", "warn");
  }
}

function populateUnifiedPresetSelect(selectedId = "") {
  const select = document.getElementById("unified-task-preset-id");
  const kind = document.getElementById("unified-task-preset-kind")?.value || "api";
  if (!select) return;
  const presets = kind === "claude"
    ? unifiedTaskState.claudePresets
    : unifiedTaskState.apiPresets;
  if (!presets.length) {
    select.innerHTML = '<option value="">（无可选预设）</option>';
    return;
  }
  select.innerHTML = presets
    .map((preset) => `<option value="${escapeHtml(preset.id)}">${escapeHtml(preset.name || preset.id)}</option>`)
    .join("");
  if (selectedId && presets.some((preset) => preset.id === selectedId)) {
    select.value = selectedId;
  }
}

function populateUnifiedTerminalSelect() {
  const select = document.getElementById("unified-task-terminal");
  if (!select) return;
  if (!unifiedTaskState.terminals.length) {
    select.innerHTML = '<option value="">（无可用终端）</option>';
    return;
  }
  select.innerHTML = unifiedTaskState.terminals
    .map(
      (t) =>
        `<option value="${escapeHtml(t.id)}">${escapeHtml(t.name)} (${escapeHtml(t.path || "无路径")})</option>`,
    )
    .join("");
}

function populateUnifiedWorkdirSelect() {
  const input = document.getElementById("unified-task-workdir");
  const datalist = document.getElementById("unified-task-workdir-suggestions");
  if (!input) return;
  const currentPath = (typeof state !== "undefined" && state.currentPath) || "";
  // Populate the datalist with known working directories for quick selection
  const dirs = [...unifiedTaskState.workdirs];
  if (currentPath && !dirs.includes(currentPath)) {
    dirs.unshift(currentPath);
  }
  if (datalist) {
    datalist.innerHTML = dirs
      .map((dir) => `<option value="${escapeHtml(dir)}"></option>`)
      .join("");
  }
  // Only set the input value if empty (don't clobber user edits)
  if (!input.value && currentPath) {
    input.value = currentPath;
  }
}

function updateUnifiedTargetModeVisibility() {
  const modeSelect = document.getElementById("unified-task-target-mode");
  const terminalField = document.getElementById("unified-task-terminal-field");
  const workdirField = document.getElementById("unified-task-workdir-field");
  if (!modeSelect) return;
  const isNew = modeSelect.value === "new";
  if (terminalField) terminalField.classList.toggle("hidden", isNew);
  if (workdirField) workdirField.classList.toggle("hidden", !isNew);
}

function unifiedTaskIsPresetTest() {
  return document.getElementById("unified-task-type")?.value === "preset-test";
}

function updateUnifiedTaskTypeVisibility() {
  const presetTest = unifiedTaskIsPresetTest();
  document.querySelectorAll(".unified-terminal-task-field").forEach((field) => {
    field.classList.toggle("hidden", presetTest);
  });
  document.querySelectorAll(".unified-preset-test-field").forEach((field) => {
    field.classList.toggle("hidden", !presetTest);
  });
  if (presetTest) {
    updateUnifiedPresetScheduleVisibility();
    populateUnifiedPresetSelect(document.getElementById("unified-task-preset-id")?.value || "");
  } else {
    updateUnifiedTargetModeVisibility();
    updateUnifiedScheduleModeVisibility();
  }
}

function updateUnifiedScheduleModeVisibility() {
  const modeSelect = document.getElementById("unified-task-schedule-mode");
  const delayValue = document.getElementById("unified-task-delay-value");
  const delayUnit = document.getElementById("unified-task-delay-unit");
  const datetime = document.getElementById("unified-task-datetime");
  if (!modeSelect) return;
  const isDatetime = modeSelect.value === "datetime";
  if (delayValue) delayValue.classList.toggle("hidden", isDatetime);
  if (delayUnit) delayUnit.classList.toggle("hidden", isDatetime);
  if (datetime) datetime.classList.toggle("hidden", !isDatetime);
}

function updateUnifiedPresetScheduleVisibility() {
  if (!unifiedTaskIsPresetTest()) return;
  const scheduleType =
    document.getElementById("unified-task-preset-schedule-type")?.value || "daily";
  document
    .getElementById("unified-task-preset-time-field")
    ?.classList.toggle("hidden", scheduleType === "interval");
  document
    .getElementById("unified-task-preset-weekday-field")
    ?.classList.toggle("hidden", scheduleType !== "weekly");
  document
    .getElementById("unified-task-preset-interval-field")
    ?.classList.toggle("hidden", scheduleType !== "interval");
}

function resolveUnifiedTaskDueAt() {
  const modeSelect = document.getElementById("unified-task-schedule-mode");
  const now = Date.now();
  if (modeSelect?.value === "datetime") {
    const datetimeEl = document.getElementById("unified-task-datetime");
    if (!datetimeEl?.value) {
      return { error: "请选择一个发送时间。" };
    }
    const dueAtMs = Date.parse(`${datetimeEl.value}:00`);
    if (!Number.isFinite(dueAtMs)) {
      return { error: "时间格式无效。" };
    }
    if (dueAtMs <= now) {
      return { error: "指定时间必须晚于当前时间。" };
    }
    return { dueAtMs, label: new Date(dueAtMs).toLocaleString() };
  }
  const delayValueEl = document.getElementById("unified-task-delay-value");
  const delayUnitEl = document.getElementById("unified-task-delay-unit");
  const rawDelay = Number.parseFloat(delayValueEl?.value || "");
  if (!Number.isFinite(rawDelay) || rawDelay <= 0) {
    return { error: "请输入有效的延迟数值。" };
  }
  const unit = delayUnitEl?.value || "minutes";
  const multiplier = unit === "seconds" ? 1000 : unit === "hours" ? 3600000 : 60000;
  const dueAtMs = now + Math.round(rawDelay * multiplier);
  const label = `${rawDelay} ${unit === "seconds" ? "秒" : unit === "hours" ? "小时" : "分钟"}后`;
  return { dueAtMs, label };
}

function buildUnifiedPresetTestPayload() {
  const name = document.getElementById("unified-task-preset-name")?.value.trim() || "";
  const presetKind = document.getElementById("unified-task-preset-kind")?.value || "api";
  const presetId = document.getElementById("unified-task-preset-id")?.value || "";
  const scheduleType =
    document.getElementById("unified-task-preset-schedule-type")?.value || "daily";
  if (!name) return { error: "请输入任务名称。" };
  if (!presetId) return { error: "请选择要测试的预设。" };
  const payload = {
    name,
    preset_kind: presetKind,
    preset_id: presetId,
    schedule_type: scheduleType,
    enabled: document.getElementById("unified-task-preset-enabled")?.checked ?? true,
  };
  if (scheduleType === "daily" || scheduleType === "weekly") {
    payload.time = document.getElementById("unified-task-preset-time")?.value || "";
    if (!payload.time) return { error: "请选择执行时间。" };
  }
  if (scheduleType === "weekly") {
    const weekdayBoxes = document.querySelectorAll(
      "#unified-task-preset-weekdays input[type=checkbox]:checked",
    );
    payload.weekdays = Array.from(weekdayBoxes)
      .map((cb) => Number(cb.value))
      .filter((n) => Number.isFinite(n));
    if (payload.weekdays.length === 0) {
      return { error: "每周模式至少选择一个星期。" };
    }
  }
  if (scheduleType === "interval") {
    payload.interval_minutes = Number(
      document.getElementById("unified-task-preset-interval")?.value || 0,
    );
    if (!Number.isFinite(payload.interval_minutes) || payload.interval_minutes <= 0) {
      return { error: "间隔分钟数必须大于 0。" };
    }
  }
  return { payload };
}

async function createUnifiedPresetTestTask(createBtn) {
  const built = buildUnifiedPresetTestPayload();
  if (built.error) {
    setUnifiedCreateStatus(built.error, "warn");
    return;
  }
  setButtonBusy(createBtn, true, "创建中...");
  setUnifiedCreateStatus("正在创建预设 API 循环测试任务...", "info");
  try {
    await requestJson("/api/auth/preset-test-schedules", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(built.payload),
    });
    setUnifiedCreateStatus("已创建预设 API 循环测试任务。", "ok");
    clearUnifiedTaskForm();
    hideUnifiedTaskForm();
    await loadUnifiedTasks();
  } catch (error) {
    setUnifiedCreateStatus(error?.message || "创建预设 API 测试任务失败。", "warn");
  } finally {
    setButtonBusy(createBtn, false);
  }
}

async function createUnifiedTask() {
  // When editing an existing task, save changes instead of creating a new one.
  if (unifiedTaskState.editTaskId) {
    return updateUnifiedTask();
  }
  const typeSelect = document.getElementById("unified-task-type");
  const modeSelect = document.getElementById("unified-task-target-mode");
  const textEl = document.getElementById("unified-task-text");
  const sendEnterEl = document.getElementById("unified-task-send-enter");
  const createBtn = document.getElementById("unified-task-create-btn");

  const taskType = typeSelect?.value || "paste";
  const targetMode = modeSelect?.value || "existing";
  const text = textEl?.value || "";
  const sendEnter = sendEnterEl?.checked ?? true;

  if (taskType === "preset-test") {
    return createUnifiedPresetTestTask(createBtn);
  }

  if (!text.trim()) {
    setUnifiedCreateStatus("请输入要发送的消息内容。", "warn");
    textEl?.focus();
    return;
  }

  const resolved = resolveUnifiedTaskDueAt();
  if (resolved.error) {
    setUnifiedCreateStatus(resolved.error, "warn");
    return;
  }

  const body = {
    text,
    due_at: resolved.dueAtMs,
    label: resolved.label,
    send_enter: sendEnter,
    task_type: taskType,
    terminal_mode: targetMode,
  };

  if (targetMode === "new") {
    const workdirInput = document.getElementById("unified-task-workdir");
    body.working_dir = workdirInput?.value || "";
    if (!body.working_dir) {
      setUnifiedCreateStatus("新建终端模式下必须指定工作目录。", "warn");
      return;
    }
    body.session_id = "";
  } else {
    const terminalSelect = document.getElementById("unified-task-terminal");
    body.session_id = terminalSelect?.value || "";
    if (!body.session_id) {
      setUnifiedCreateStatus("请选择目标终端。", "warn");
      return;
    }
  }

  setButtonBusy(createBtn, true, "创建中...");
  setUnifiedCreateStatus("正在创建定时任务...", "info");
  try {
    await requestJson("/api/terminal/scheduled-inputs", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    setUnifiedCreateStatus(`已创建定时任务：${resolved.label}`, "ok");
    clearUnifiedTaskForm();
    hideUnifiedTaskForm();
    await loadUnifiedTasks();
  } catch (error) {
    setUnifiedCreateStatus(error?.message || "创建定时任务失败。", "warn");
  } finally {
    setButtonBusy(createBtn, false);
  }
}

async function loadUnifiedTasks() {
  const requestToken = ++unifiedTaskState.requestToken;
  setUnifiedListStatus("正在读取定时任务...", "muted");

  const [pasteResult, autoContinueResult, presetTestResult] = await Promise.allSettled([
    requestJson("/api/terminal/scheduled-inputs"),
    requestJson("/api/terminal/auto-continue-tasks"),
    requestJson("/api/auth/preset-test-schedules"),
  ]);

  if (requestToken !== unifiedTaskState.requestToken) return;

  unifiedTaskState.pasteTasks = pasteResult.status === "fulfilled"
    ? (pasteResult.value?.tasks || []).map(normalizeUnifiedPasteTask).filter(Boolean)
    : [];
  unifiedTaskState.autoContinueTasks = autoContinueResult.status === "fulfilled"
    ? (autoContinueResult.value?.auto_continue_tasks || []).map(normalizeUnifiedAutoContinueTask).filter(Boolean)
    : [];
  unifiedTaskState.presetTestTasks = presetTestResult.status === "fulfilled"
    ? (presetTestResult.value?.schedules || []).map(normalizeUnifiedPresetTestTask).filter(Boolean)
    : [];

  renderUnifiedTaskTable();

  const pasteError = pasteResult.status === "rejected" ? pasteResult.reason : null;
  const acError = autoContinueResult.status === "rejected" ? autoContinueResult.reason : null;
  const presetTestError = presetTestResult.status === "rejected" ? presetTestResult.reason : null;
  const crontabError = autoContinueResult.status === "fulfilled"
    ? autoContinueResult.value?.crontab_error
    : null;

  if (pasteError) {
    setUnifiedListStatus(pasteError.message || "读取粘贴任务失败。", "warn");
  } else if (acError) {
    setUnifiedListStatus(acError.message || "读取自动继续任务失败。", "warn");
  } else if (presetTestError) {
    setUnifiedListStatus(presetTestError.message || "读取预设 API 测试任务失败。", "warn");
  } else if (crontabError) {
    setUnifiedListStatus(`crontab 读取异常: ${crontabError}`, "warn");
  } else {
    const total =
      unifiedTaskState.pasteTasks.length +
      unifiedTaskState.autoContinueTasks.length +
      unifiedTaskState.presetTestTasks.length;
    setUnifiedListStatus(
      total
        ? `共 ${total} 个定时任务（终端 ${unifiedTaskState.pasteTasks.length}，自动继续 ${unifiedTaskState.autoContinueTasks.length}，预设测试 ${unifiedTaskState.presetTestTasks.length}）。`
        : "当前没有定时任务。",
      total ? "ok" : "muted",
    );
  }

  // Also render auto-continue history for the history card
  if (autoContinueResult.status === "fulfilled") {
    const expired = autoContinueResult.value?.expired_tasks || [];
    if (typeof renderAutoContinueHistory === "function") {
      renderAutoContinueHistory(expired);
    }
  }
}

function normalizeUnifiedPasteTask(task) {
  if (!task) return null;
  const taskId = String(task.taskId || task.id || "").trim();
  const dueAt = Number(task.dueAt ?? task.due_at ?? task.due_at_millis);
  if (!taskId || !Number.isFinite(dueAt)) return null;
  return {
    taskId,
    source: "server",
    type: task.task_type || "paste",
    sessionId: String(task.sessionId || task.session_id || ""),
    terminalName: String(task.terminalName || task.terminal_name || ""),
    dueAt,
    preview: String(task.preview || ""),
    text: String(task.text || task.preview || ""),
    sendEnter: task.send_enter ?? true,
    workingDir: String(task.working_dir || task.workingDir || ""),
    label: String(task.label || ""),
    editable: true,
  };
}

function normalizeUnifiedAutoContinueTask(task) {
  if (!task) return null;
  const sessionId = String(task.session_id || "").trim();
  const webclxName = task.webclx_terminal_name || task.session_name || "";
  const dueEpoch = Number(task.due_epoch || 0);
  const dueAt = dueEpoch > 0 ? dueEpoch * 1000 : 0;
  return {
    taskId: String(task.marker || sessionId),
    source: "crontab",
    type: "auto-continue",
    sessionId,
    terminalName: webclxName || "未找到",
    dueAt,
    preview: String(task.command || task.signature || ""),
    sendEnter: false,
    workingDir: "",
    label: String(task.task_label || task.schedule || ""),
    schedule: String(task.schedule || ""),
    editable: true,
  };
}

function normalizeUnifiedPresetTestTask(task) {
  if (!task?.id) return null;
  const result = task.last_result?.result;
  const resultSummary = !task.last_result
    ? "尚未执行"
    : `${task.last_result.ok ? "测试通过" : "测试失败"}${Number.isFinite(Number(result?.latency_ms)) ? ` · ${result.latency_ms}ms` : ""}${result?.status ? ` · HTTP ${result.status}` : ""}`;
  return {
    taskId: String(task.id),
    source: "preset-test",
    type: "preset-test",
    terminalName: String(task.preset_name || task.preset_id || "预设已删除"),
    dueAt: Number(task.next_fire_at_millis || 0),
    preview: resultSummary,
    label: String(task.schedule_desc || ""),
    schedule: String(task.schedule_desc || ""),
    editable: true,
    enabled: Boolean(task.enabled),
    name: String(task.name || ""),
    presetKind: String(task.preset_kind || "api"),
    presetId: String(task.preset_id || ""),
    scheduleType: String(task.schedule_type || "daily"),
    scheduleTime: String(task.time || ""),
    weekdays: Array.isArray(task.weekdays) ? task.weekdays.map(Number) : [],
    intervalMinutes: Number(task.interval_minutes || 0),
    lastResult: task.last_result || null,
  };
}

function unifiedTaskTypeBadge(type, source) {
  const label = UNIFIED_TASK_TYPE_LABELS[type] || type || "粘贴消息";
  const sourceAttr = source === "crontab" ? "crontab" : type;
  return `<span class="task-type-badge" data-type="${escapeHtml(type)}" data-source="${escapeHtml(sourceAttr)}">${escapeHtml(label)}</span>`;
}

function formatUnifiedTaskDue(dueAtMs) {
  if (!Number.isFinite(dueAtMs) || dueAtMs <= 0) return "-";
  try {
    return new Date(dueAtMs).toLocaleString();
  } catch (_e) {
    return String(dueAtMs);
  }
}

function formatUnifiedTaskRemaining(dueAtMs) {
  if (!Number.isFinite(dueAtMs) || dueAtMs <= 0) return "-";
  const remaining = dueAtMs - Date.now();
  if (remaining <= 0) return "即将发送";
  const totalSeconds = Math.ceil(remaining / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const parts = [];
  if (hours > 0) parts.push(`${hours} 时`);
  if (minutes > 0 || hours > 0) parts.push(`${minutes} 分`);
  parts.push(`${seconds} 秒`);
  return parts.join(" ");
}

function formatUnifiedTaskLocalInputValue(dueAtMs) {
  if (!Number.isFinite(dueAtMs) || dueAtMs <= 0) return "";
  const date = new Date(dueAtMs);
  if (Number.isNaN(date.getTime())) return "";
  const pad = (value) => String(value).padStart(2, "0");
  return [
    date.getFullYear(),
    pad(date.getMonth() + 1),
    pad(date.getDate()),
  ].join("-") + `T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function renderUnifiedTaskTable() {
  const tbody = document.getElementById("unified-task-list");
  if (!tbody) return;

  const allTasks = [
    ...unifiedTaskState.pasteTasks,
    ...unifiedTaskState.autoContinueTasks,
    ...unifiedTaskState.presetTestTasks,
  ].sort((a, b) => {
    // Sort: tasks with dueAt=0 (crontab without epoch) go last, then by dueAt ascending
    const aTime = a.dueAt > 0 ? a.dueAt : Number.MAX_SAFE_INTEGER;
    const bTime = b.dueAt > 0 ? b.dueAt : Number.MAX_SAFE_INTEGER;
    return aTime - bTime;
  });

  if (!allTasks.length) {
    tbody.innerHTML = '<tr><td colspan="8" class="meta-text">当前没有定时任务。</td></tr>';
    return;
  }

  tbody.innerHTML = allTasks
    .map((task) => {
      const sourceLabel = task.source === "crontab"
        ? "crontab"
        : task.source === "preset-test"
          ? "预设 API"
          : "服务端";
      const status = task.source === "preset-test"
        ? task.enabled
          ? task.lastResult
            ? task.lastResult.ok
              ? "启用 · 上次通过"
              : "启用 · 上次失败"
            : "启用 · 未测试"
          : "已停用"
        : formatUnifiedTaskRemaining(task.dueAt) === "即将发送"
          ? "待发"
          : "已排程";
      // Server-side tasks support full editing (type/target/schedule/text);
      // crontab auto-continue tasks can only adjust their fire time because
      // their content is a fixed continue command, so they fall back to the
      // time-only prompt editor.
      const editLabel = task.source === "crontab" ? "编辑时间" : "编辑";
      const actionCell = task.source === "preset-test"
        ? `
          <button class="button secondary unified-task-run" type="button" data-task-id="${escapeHtml(task.taskId)}">立即测试</button>
          <button class="button secondary unified-task-edit" type="button" data-source="${escapeHtml(task.source)}" data-task-id="${escapeHtml(task.taskId)}" data-due-at="${task.dueAt}">编辑</button>
          <button class="button secondary unified-task-toggle" type="button" data-task-id="${escapeHtml(task.taskId)}" data-enabled="${task.enabled}">${task.enabled ? "停用" : "启用"}</button>
          <button class="button secondary unified-task-cancel" type="button" data-source="${escapeHtml(task.source)}" data-task-id="${escapeHtml(task.taskId)}">删除</button>
        `
        : `
          <button class="button secondary unified-task-edit" type="button" data-source="${escapeHtml(task.source)}" data-task-id="${escapeHtml(task.taskId)}" data-due-at="${task.dueAt}">${escapeHtml(editLabel)}</button>
          <button class="button secondary unified-task-cancel" type="button" data-source="${escapeHtml(task.source)}" data-task-id="${escapeHtml(task.taskId)}">取消</button>
        `;
      const remainingCell = task.dueAt > 0
        ? `<span class="mono-text unified-task-remaining" data-due-at="${task.dueAt}">${escapeHtml(formatUnifiedTaskRemaining(task.dueAt))}</span>`
        : '<span class="meta-text">-</span>';
      const workdirInfo = task.workingDir ? ` <span class="meta-text">@${escapeHtml(task.workingDir)}</span>` : "";
      return `
        <tr data-task-id="${escapeHtml(task.taskId)}">
          <td>${unifiedTaskTypeBadge(task.type, task.source)}</td>
          <td class="meta-text">${escapeHtml(sourceLabel)}</td>
          <td>${escapeHtml(task.terminalName || "-")}${workdirInfo}</td>
          <td class="mono-text">${escapeHtml(task.source === "preset-test" ? `${task.schedule || "-"}${task.dueAt > 0 ? ` · ${formatUnifiedTaskDue(task.dueAt)}` : ""}` : task.dueAt > 0 ? formatUnifiedTaskDue(task.dueAt) : task.schedule || "-")}</td>
          <td>${remainingCell}</td>
          <td class="task-preview-cell mono-text compile-path-text">${escapeHtml(task.preview || "-")}</td>
          <td class="meta-text">${escapeHtml(status)}</td>
          <td>${actionCell}</td>
        </tr>
      `;
    })
    .join("");

  startUnifiedTaskRemainingTicker();
}

let unifiedTaskRemainingTimer = null;

function startUnifiedTaskRemainingTicker() {
  stopUnifiedTaskRemainingTimer();
  const tick = () => {
    const cells = document.querySelectorAll("#unified-task-list .unified-task-remaining");
    let anyDue = false;
    cells.forEach((cell) => {
      const dueAt = Number(cell.dataset.dueAt);
      if (Number.isFinite(dueAt) && dueAt > 0) {
        const text = formatUnifiedTaskRemaining(dueAt);
        cell.textContent = text;
        if (text === "即将发送") anyDue = true;
      }
    });
    if (anyDue) {
      loadUnifiedTasks();
    }
  };
  tick();
  unifiedTaskRemainingTimer = window.setInterval(tick, 1000);
}

function stopUnifiedTaskRemainingTimer() {
  if (unifiedTaskRemainingTimer) {
    window.clearInterval(unifiedTaskRemainingTimer);
    unifiedTaskRemainingTimer = null;
  }
}

async function cancelUnifiedTask(taskId) {
  return cancelUnifiedTaskBySource("server", taskId);
}

async function cancelUnifiedTaskBySource(source, taskId) {
  if (!taskId) return;
  if (!window.confirm(source === "preset-test" ? "确定删除此预设 API 测试任务？" : "确定取消此定时任务？")) return;
  const path = source === "crontab"
    ? `/api/terminal/auto-continue-tasks/${encodeURIComponent(taskId)}`
    : source === "preset-test"
      ? `/api/auth/preset-test-schedules/${encodeURIComponent(taskId)}`
      : `/api/terminal/scheduled-inputs/${encodeURIComponent(taskId)}`;
  try {
    await requestJson(path, {
      method: "DELETE",
    });
    setUnifiedListStatus("已取消定时任务。", "ok");
    await loadUnifiedTasks();
  } catch (error) {
    setUnifiedListStatus(error?.message || "取消定时任务失败。", "warn");
  }
}

// Route the edit action: server tasks enter full-field edit mode, crontab
// tasks fall back to a time-only prompt because their content is a fixed
// continue command that cannot be edited inline.
async function editUnifiedTask(source, taskId, currentDueAt) {
  if (!taskId) return;
  if (source === "server" || source === "preset-test") {
    enterUnifiedEditMode(taskId, source);
    return;
  }
  await editUnifiedTaskTimeCrontab(taskId, currentDueAt);
}

function findUnifiedTask(taskId) {
  if (!taskId) return null;
  return (
    unifiedTaskState.pasteTasks.find((t) => t.taskId === taskId) ||
    unifiedTaskState.autoContinueTasks.find((t) => t.taskId === taskId) ||
    unifiedTaskState.presetTestTasks.find((t) => t.taskId === taskId) ||
    null
  );
}

function fillUnifiedFormFromTask(task) {
  if (!task) return;
  const typeSelect = document.getElementById("unified-task-type");
  if (typeSelect) typeSelect.value = task.type || "paste";
  updateUnifiedTaskTypeVisibility();
  if (task.source === "preset-test") {
    const nameEl = document.getElementById("unified-task-preset-name");
    const kindEl = document.getElementById("unified-task-preset-kind");
    const scheduleTypeEl = document.getElementById("unified-task-preset-schedule-type");
    const timeEl = document.getElementById("unified-task-preset-time");
    const weekdayBoxes = document.querySelectorAll(
      "#unified-task-preset-weekdays input[type=checkbox]",
    );
    const intervalEl = document.getElementById("unified-task-preset-interval");
    const enabledEl = document.getElementById("unified-task-preset-enabled");
    if (nameEl) nameEl.value = task.name || "";
    if (kindEl) kindEl.value = task.presetKind || "api";
    populateUnifiedPresetSelect(task.presetId);
    if (scheduleTypeEl) scheduleTypeEl.value = task.scheduleType || "daily";
    if (timeEl) timeEl.value = task.scheduleTime || "09:00";
    const selectedWeekdays = Array.isArray(task.weekdays)
      ? task.weekdays.map(Number)
      : [];
    weekdayBoxes.forEach((cb) => {
      cb.checked = selectedWeekdays.includes(Number(cb.value));
    });
    if (intervalEl) intervalEl.value = String(task.intervalMinutes || 60);
    if (enabledEl) enabledEl.checked = task.enabled;
    updateUnifiedPresetScheduleVisibility();
    return;
  }
  // Server tasks always target an existing session (a "new" terminal task
  // already had its session created at creation time).
  const modeSelect = document.getElementById("unified-task-target-mode");
  if (modeSelect) modeSelect.value = "existing";
  updateUnifiedTargetModeVisibility();
  const terminalSelect = document.getElementById("unified-task-terminal");
  if (terminalSelect) terminalSelect.value = task.sessionId || "";
  // Default to the datetime picker so the existing exact time is visible.
  const scheduleMode = document.getElementById("unified-task-schedule-mode");
  if (scheduleMode) scheduleMode.value = "datetime";
  const datetimeEl = document.getElementById("unified-task-datetime");
  if (datetimeEl) datetimeEl.value = formatUnifiedTaskLocalInputValue(task.dueAt);
  updateUnifiedScheduleModeVisibility();
  const textEl = document.getElementById("unified-task-text");
  if (textEl) textEl.value = task.text || task.preview || "";
  const sendEnterEl = document.getElementById("unified-task-send-enter");
  if (sendEnterEl) sendEnterEl.checked = task.sendEnter ?? true;
}

function enterUnifiedEditMode(taskId, source = "server") {
  unifiedTaskState.editTaskId = taskId;
  unifiedTaskState.editTaskSource = source;
  fillUnifiedFormFromTask(findUnifiedTask(taskId));
  setUnifiedEditModeButtonLabels(true);
  setUnifiedCreateStatus("正在编辑定时任务，修改后点击「保存修改」。", "info");
  showUnifiedTaskForm();
  document
    .getElementById(source === "preset-test" ? "unified-task-preset-name" : "unified-task-text")
    ?.focus();
  document.getElementById("unified-task-create-btn")?.scrollIntoView({ behavior: "smooth", block: "center" });
}

function exitUnifiedEditMode() {
  unifiedTaskState.editTaskId = null;
  unifiedTaskState.editTaskSource = null;
  setUnifiedEditModeButtonLabels(false);
}

function showUnifiedTaskForm() {
  document.getElementById("unified-task-form-card")?.removeAttribute("hidden");
}

function hideUnifiedTaskForm() {
  document.getElementById("unified-task-form-card")?.setAttribute("hidden", "");
}

function setUnifiedEditModeButtonLabels(isEditing) {
  const createBtn = document.getElementById("unified-task-create-btn");
  const clearBtn = document.getElementById("unified-task-clear-btn");
  if (createBtn) createBtn.textContent = isEditing ? "保存修改" : "创建任务";
  if (clearBtn) clearBtn.textContent = isEditing ? "取消编辑" : "清空";
}

function clearUnifiedTaskForm() {
  const textEl = document.getElementById("unified-task-text");
  const sendEnterEl = document.getElementById("unified-task-send-enter");
  const presetNameEl = document.getElementById("unified-task-preset-name");
  const presetEnabledEl = document.getElementById("unified-task-preset-enabled");
  if (textEl) textEl.value = "";
  if (sendEnterEl) sendEnterEl.checked = true;
  if (presetNameEl) presetNameEl.value = "";
  document
    .querySelectorAll("#unified-task-preset-weekdays input[type=checkbox]")
    .forEach((cb) => {
      cb.checked = false;
    });
  if (presetEnabledEl) presetEnabledEl.checked = true;
  setUnifiedCreateStatus("", "muted");
}

async function updateUnifiedTask() {
  const taskId = unifiedTaskState.editTaskId;
  if (!taskId) return;
  const editingPresetTest = unifiedTaskState.editTaskSource === "preset-test";
  if (editingPresetTest !== unifiedTaskIsPresetTest()) {
    setUnifiedCreateStatus("编辑时不能转换任务类型；请删除原任务后重新创建。", "warn");
    return;
  }
  if (unifiedTaskState.editTaskSource === "preset-test") {
    return updateUnifiedPresetTestTask(taskId);
  }
  const textEl = document.getElementById("unified-task-text");
  const text = textEl?.value || "";
  if (!text.trim()) {
    setUnifiedCreateStatus("请输入要发送的消息内容。", "warn");
    textEl?.focus();
    return;
  }
  const resolved = resolveUnifiedTaskDueAt();
  if (resolved.error) {
    setUnifiedCreateStatus(resolved.error, "warn");
    return;
  }
  const typeSelect = document.getElementById("unified-task-type");
  const terminalSelect = document.getElementById("unified-task-terminal");
  const sendEnterEl = document.getElementById("unified-task-send-enter");
  const body = {
    due_at: resolved.dueAtMs,
    text,
    send_enter: sendEnterEl?.checked ?? true,
    task_type: typeSelect?.value || "paste",
    session_id: terminalSelect?.value || "",
  };
  if (!body.session_id) {
    setUnifiedCreateStatus("请选择目标终端。", "warn");
    return;
  }
  const saveBtn = document.getElementById("unified-task-create-btn");
  setButtonBusy(saveBtn, true, "保存中...");
  setUnifiedCreateStatus("正在保存定时任务...", "info");
  try {
    await requestJson(`/api/terminal/scheduled-inputs/${encodeURIComponent(taskId)}`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    setUnifiedCreateStatus(`已更新定时任务：${resolved.label}`, "ok");
    exitUnifiedEditMode();
    clearUnifiedTaskForm();
    hideUnifiedTaskForm();
    await loadUnifiedTasks();
  } catch (error) {
    setUnifiedCreateStatus(error?.message || "更新定时任务失败。", "warn");
  } finally {
    setButtonBusy(saveBtn, false);
    setUnifiedEditModeButtonLabels(Boolean(unifiedTaskState.editTaskId));
  }
}

async function updateUnifiedPresetTestTask(taskId) {
  const built = buildUnifiedPresetTestPayload();
  if (built.error) {
    setUnifiedCreateStatus(built.error, "warn");
    return;
  }
  const saveBtn = document.getElementById("unified-task-create-btn");
  setButtonBusy(saveBtn, true, "保存中...");
  setUnifiedCreateStatus("正在保存预设 API 测试任务...", "info");
  try {
    await requestJson(`/api/auth/preset-test-schedules/${encodeURIComponent(taskId)}`, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(built.payload),
    });
    setUnifiedCreateStatus("已更新预设 API 测试任务。", "ok");
    exitUnifiedEditMode();
    clearUnifiedTaskForm();
    hideUnifiedTaskForm();
    await loadUnifiedTasks();
  } catch (error) {
    setUnifiedCreateStatus(error?.message || "更新预设 API 测试任务失败。", "warn");
  } finally {
    setButtonBusy(saveBtn, false);
    setUnifiedEditModeButtonLabels(Boolean(unifiedTaskState.editTaskId));
  }
}

async function runUnifiedPresetTestTask(taskId) {
  if (!taskId) return;
  setUnifiedListStatus("正在触发预设 API 测试...", "info");
  try {
    const response = await requestJson(
      `/api/auth/preset-test-schedules/${encodeURIComponent(taskId)}/run`,
      { method: "POST" },
    );
    setUnifiedListStatus(response?.message || "测试已触发，请稍后刷新查看结果。", "ok");
    window.setTimeout(loadUnifiedTasks, 8000);
  } catch (error) {
    setUnifiedListStatus(error?.message || "触发预设 API 测试失败。", "warn");
  }
}

async function toggleUnifiedPresetTestTask(taskId, enabled) {
  if (!taskId) return;
  try {
    await requestJson(`/api/auth/preset-test-schedules/${encodeURIComponent(taskId)}`, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ enabled: !enabled }),
    });
    setUnifiedListStatus(`已${enabled ? "停用" : "启用"}预设 API 测试任务。`, "ok");
    await loadUnifiedTasks();
  } catch (error) {
    setUnifiedListStatus(error?.message || "切换预设 API 测试任务失败。", "warn");
  }
}

// Crontab auto-continue tasks only support time edits.
async function editUnifiedTaskTimeCrontab(taskId, currentDueAt) {
  if (!taskId) return;
  const currentValue = formatUnifiedTaskLocalInputValue(currentDueAt);
  const nextValue = window.prompt("请输入新的发送时间（YYYY-MM-DDTHH:mm）", currentValue);
  if (nextValue === null) return;
  const dueAt = Date.parse(`${nextValue.trim()}:00`);
  if (!Number.isFinite(dueAt) || dueAt <= Date.now()) {
    setUnifiedListStatus("新的发送时间必须晚于当前时间。", "warn");
    return;
  }
  try {
    await requestJson(`/api/terminal/auto-continue-tasks/${encodeURIComponent(taskId)}`, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ due_at: dueAt }),
    });
    setUnifiedListStatus("已更新定时任务时间。", "ok");
    await loadUnifiedTasks();
  } catch (error) {
    setUnifiedListStatus(error?.message || "更新定时任务失败。", "warn");
  }
}

// ---- Path browse dialog ----
let pathBrowseState = {
  currentPath: "",
  currentDisplayPath: "",
  selectedPath: "",
  selectedDisplayPath: "",
  onConfirm: null,
};

function openPathBrowseDialog(initialPath, onConfirm) {
  closePathBrowseDialog();
  pathBrowseState.onConfirm = onConfirm;
  pathBrowseState.selectedPath = initialPath || "";
  pathBrowseState.selectedDisplayPath = initialPath || "";
  pathBrowseState.currentPath = initialPath || "";
  pathBrowseState.currentDisplayPath = initialPath || "";

  const overlay = document.createElement("div");
  overlay.className = "path-browse-dialog-overlay";
  overlay.id = "path-browse-dialog-overlay";
  overlay.innerHTML = `
    <div class="path-browse-dialog" role="dialog" aria-label="选择工作目录">
      <div class="path-browse-dialog-header">
        <h3>选择目录</h3>
        <input class="text-input path-browse-dialog-path-input" id="path-browse-input" type="text" placeholder="相对路径，留空为根目录" autocomplete="off" spellcheck="false" />
      </div>
      <div class="path-browse-dialog-body" id="path-browse-body">
        <div class="meta-text" style="padding:8px 16px;">加载中...</div>
      </div>
      <div class="path-browse-dialog-footer">
        <span class="path-browse-selected-display" id="path-browse-selected-display"></span>
        <button class="button secondary" type="button" id="path-browse-cancel-btn">取消</button>
        <button class="button" type="button" id="path-browse-confirm-btn">确定</button>
      </div>
    </div>
  `;
  document.body.appendChild(overlay);

  overlay.addEventListener("click", (event) => {
    if (event.target === overlay) {
      closePathBrowseDialog();
    }
  });

  document.getElementById("path-browse-cancel-btn").addEventListener("click", closePathBrowseDialog);
  document.getElementById("path-browse-confirm-btn").addEventListener("click", () => {
    const selected =
      pathBrowseState.selectedDisplayPath ||
      pathBrowseState.currentDisplayPath ||
      pathBrowseState.selectedPath ||
      pathBrowseState.currentPath;
    if (pathBrowseState.onConfirm) {
      pathBrowseState.onConfirm(selected);
    }
    closePathBrowseDialog();
  });

  const pathInput = document.getElementById("path-browse-input");
  pathInput.value = pathBrowseState.currentPath;
  pathInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      pathBrowseState.currentPath = pathInput.value.trim();
      loadPathBrowseDirectory(pathBrowseState.currentPath);
    }
  });

  loadPathBrowseDirectory(pathBrowseState.currentPath);
}

function closePathBrowseDialog() {
  const overlay = document.getElementById("path-browse-dialog-overlay");
  if (overlay) {
    overlay.remove();
  }
  pathBrowseState.onConfirm = null;
}

async function loadPathBrowseDirectory(relPath) {
  pathBrowseState.currentPath = relPath || "";
  pathBrowseState.currentDisplayPath = relPath || "";
  const body = document.getElementById("path-browse-body");
  const pathInput = document.getElementById("path-browse-input");
  const selectedDisplay = document.getElementById("path-browse-selected-display");
  if (!body) return;

  if (pathInput) {
    pathInput.value = pathBrowseState.currentPath;
  }

  body.innerHTML = '<div class="meta-text" style="padding:8px 16px;">加载中...</div>';

  try {
    const params = new URLSearchParams();
    if (pathBrowseState.currentPath) {
      params.set("path", pathBrowseState.currentPath);
    }
    const url = `/api/entries${params.toString() ? "?" + params.toString() : ""}`;
    const directory = await requestJson(url);

    pathBrowseState.currentPath = directory.path || "";
    pathBrowseState.currentDisplayPath = directory.display_path || pathBrowseState.currentPath || "/";
    if (pathInput) {
      pathInput.value = pathBrowseState.currentPath;
    }
    pathBrowseState.selectedPath = pathBrowseState.currentPath;
    pathBrowseState.selectedDisplayPath = pathBrowseState.currentDisplayPath;
    if (selectedDisplay) {
      selectedDisplay.textContent = pathBrowseState.currentDisplayPath;
    }

    const entries = Array.isArray(directory.entries) ? directory.entries : [];
    const dirs = entries.filter((e) => e.kind === "dir");
    const hasParent = directory.parent_path != null && directory.path !== "";

    let html = "";
    if (hasParent) {
      html += `<div class="path-browse-dialog-entry" data-kind="parent" data-path="${escapeHtml(directory.parent_path)}">
        <span class="path-browse-dialog-entry-icon">&#x2191;</span><span>..</span>
      </div>`;
    }
    if (dirs.length === 0) {
      html += '<div class="meta-text" style="padding:8px 16px;">此目录下没有子目录。</div>';
    } else {
      html += dirs
        .map(
          (entry) => `<div class="path-browse-dialog-entry" data-kind="dir" data-path="${escapeHtml(entry.path)}">
            <span class="path-browse-dialog-entry-icon">/</span><span>${escapeHtml(entry.name)}</span>
          </div>`,
        )
        .join("");
    }
    body.innerHTML = html;

    body.querySelectorAll(".path-browse-dialog-entry").forEach((el) => {
      el.addEventListener("click", () => {
        const entryPath = el.dataset.path || "";
        if (el.dataset.kind === "parent") {
          loadPathBrowseDirectory(entryPath);
        } else {
          loadPathBrowseDirectory(entryPath);
        }
      });
    });
  } catch (error) {
    body.innerHTML = `<div class="meta-text" style="padding:8px 16px;" data-tone="warn">读取目录失败：${escapeHtml(error?.message || String(error))}</div>`;
  }
}

function initUnifiedTaskBindings() {
  document
    .getElementById("unified-task-type")
    ?.addEventListener("change", updateUnifiedTaskTypeVisibility);
  const targetModeEl = document.getElementById("unified-task-target-mode");
  targetModeEl?.addEventListener("change", updateUnifiedTargetModeVisibility);

  const scheduleModeEl = document.getElementById("unified-task-schedule-mode");
  scheduleModeEl?.addEventListener("change", updateUnifiedScheduleModeVisibility);
  document
    .getElementById("unified-task-preset-kind")
    ?.addEventListener("change", () => populateUnifiedPresetSelect());
  document
    .getElementById("unified-task-preset-schedule-type")
    ?.addEventListener("change", updateUnifiedPresetScheduleVisibility);

  document.getElementById("unified-task-create-btn")?.addEventListener("click", createUnifiedTask);
  document.getElementById("unified-task-new-btn")?.addEventListener("click", () => {
    // Start fresh: exit any active edit, reset the form, then reveal it.
    exitUnifiedEditMode();
    clearUnifiedTaskForm();
    updateUnifiedTaskTypeVisibility();
    showUnifiedTaskForm();
    setUnifiedCreateStatus("填写任务信息后点击「创建任务」。", "muted");
    document
      .getElementById(unifiedTaskIsPresetTest() ? "unified-task-preset-name" : "unified-task-text")
      ?.focus();
    document.getElementById("unified-task-form-card")?.scrollIntoView({ behavior: "smooth", block: "center" });
  });
  document.getElementById("unified-task-clear-btn")?.addEventListener("click", () => {
    // In edit mode the secondary button cancels editing; otherwise it clears.
    if (unifiedTaskState.editTaskId) {
      exitUnifiedEditMode();
      clearUnifiedTaskForm();
      hideUnifiedTaskForm();
      setUnifiedCreateStatus("已取消编辑。", "muted");
    } else {
      clearUnifiedTaskForm();
      hideUnifiedTaskForm();
      setUnifiedCreateStatus("已清空表单。", "muted");
    }
  });
  document.getElementById("unified-task-refresh")?.addEventListener("click", loadUnifiedTasks);
  document.getElementById("unified-task-workdir-browse")?.addEventListener("click", () => {
    const input = document.getElementById("unified-task-workdir");
    openPathBrowseDialog(input?.value || "", (selectedPath) => {
      if (input) {
        input.value = selectedPath;
      }
    });
  });

  const taskListEl = document.getElementById("unified-task-list");
  taskListEl?.addEventListener("click", (event) => {
    const runButton = event.target.closest?.(".unified-task-run");
    if (runButton) {
      runUnifiedPresetTestTask(runButton.dataset.taskId || "");
      return;
    }
    const toggleButton = event.target.closest?.(".unified-task-toggle");
    if (toggleButton) {
      toggleUnifiedPresetTestTask(
        toggleButton.dataset.taskId || "",
        toggleButton.dataset.enabled === "true",
      );
      return;
    }
    const editButton = event.target.closest?.(".unified-task-edit");
    if (editButton) {
      editUnifiedTask(
        editButton.dataset.source || "server",
        editButton.dataset.taskId || "",
        Number(editButton.dataset.dueAt || 0),
      );
      return;
    }
    const cancelButton = event.target.closest?.(".unified-task-cancel");
    if (!cancelButton) return;
    cancelUnifiedTaskBySource(cancelButton.dataset.source || "server", cancelButton.dataset.taskId || "");
  });

  updateUnifiedTargetModeVisibility();
  updateUnifiedScheduleModeVisibility();
  updateUnifiedTaskTypeVisibility();
}

// Initialize when DOM is ready
if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", () => {
    initUnifiedTaskBindings();
  });
} else {
  initUnifiedTaskBindings();
}
