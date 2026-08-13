// Auto-continue task and scheduled paste task rendering for the settings page.
// Loaded before app.js; functions run after app.js globals are initialized.
function renderAutoContinueTasks(payload) {
  if (!autoContinueTaskListEl) {
    return;
  }
  const tasks = Array.isArray(payload?.auto_continue_tasks) ? payload.auto_continue_tasks : [];
  const expiredTasks = Array.isArray(payload?.expired_tasks) ? payload.expired_tasks : [];
  const crontabError = String(payload?.crontab_error || "").trim();

  if (crontabError) {
    setInlineStatus(autoContinueTaskStatusEl, crontabError, "warn");
  } else {
    setInlineStatus(
      autoContinueTaskStatusEl,
      tasks.length ? `已读取 ${tasks.length} 个自动继续定时任务。` : "当前没有已安排的自动继续定时任务。",
      tasks.length ? "ok" : "muted",
    );
  }

  if (!tasks.length) {
    autoContinueTaskListEl.innerHTML = `<tr><td colspan="8" class="meta-text">${
      crontabError ? "读取 crontab 失败，无法显示定时任务。" : "当前没有已安排的自动继续定时任务。"
    }</td></tr>`;
    renderAutoContinueHistory(expiredTasks);
    return;
  }

  autoContinueTaskListEl.innerHTML = tasks
    .map((task) => {
      const webclxTerminalName = task.webclx_terminal_name || task.session_name || "";
      const terminalName = webclxTerminalName || "未找到";
      const terminalNameTone = webclxTerminalName ? "" : ' data-tone="warn"';
      const scriptPath = task.script_path || "-";
      const scriptState = task.script_path ? (task.script_exists ? "存在" : "缺失") : "未知";
      const scriptTone = task.script_exists ? "ok" : "warn";
      const taskType = task.task_label || task.task_type || task.type || "限额重置";
      return `
        <tr>
          <td>${escapeHtml(taskType)}</td>
          <td${terminalNameTone}>${escapeHtml(terminalName)}</td>
          <td class="mono-text">${escapeHtml(task.session_id || "-")}</td>
          <td class="mono-text">${escapeHtml(task.tmux_session_name || "-")}</td>
          <td class="mono-text">${escapeHtml(task.schedule || "-")}</td>
          <td>
            <div class="mono-text compile-path-text">${escapeHtml(scriptPath)}</div>
            <div class="meta-text" data-tone="${scriptTone}">${scriptState}</div>
          </td>
          <td class="mono-text">${escapeHtml(task.marker || "-")}</td>
          <td class="mono-text compile-path-text">${escapeHtml(task.command || "-")}</td>
        </tr>
      `;
    })
    .join("");

  renderAutoContinueHistory(expiredTasks);
}

function formatExpiredAt(epoch) {
  const value = Number(epoch);
  if (!Number.isFinite(value) || value <= 0) {
    return "-";
  }
  try {
    return new Date(value * 1000).toLocaleString();
  } catch (_error) {
    return String(value);
  }
}

function renderAutoContinueHistory(expiredTasks) {
  if (!autoContinueHistoryListEl) {
    return;
  }
  const tasks = Array.isArray(expiredTasks) ? expiredTasks : [];
  if (!tasks.length) {
    autoContinueHistoryListEl.innerHTML =
      '<tr><td colspan="5" class="meta-text">暂无已归档的过期任务。</td></tr>';
    return;
  }
  autoContinueHistoryListEl.innerHTML = tasks
    .map((task) => {
      const webclxTerminalName = task.webclx_terminal_name || task.session_name || "";
      const terminalName = webclxTerminalName || "未找到";
      return `
        <tr>
          <td>${escapeHtml(terminalName)}</td>
          <td class="mono-text">${escapeHtml(task.session_id || "-")}</td>
          <td class="mono-text">${escapeHtml(task.tmux_session_name || "-")}</td>
          <td class="mono-text">${escapeHtml(task.schedule || "-")}</td>
          <td class="mono-text">${escapeHtml(formatExpiredAt(task.expired_at))}</td>
        </tr>
      `;
    })
    .join("");
}

async function loadAutoContinueTasks() {
  if (!autoContinueTaskListEl) {
    return;
  }
  const requestToken = state.autoContinueTaskRequestToken + 1;
  state.autoContinueTaskRequestToken = requestToken;
  setInlineStatus(autoContinueTaskStatusEl, "正在读取自动继续定时任务...", "muted");
  setButtonBusy(autoContinueTaskRefreshButtonEl, true, "刷新中...");
  try {
    const payload = await requestJson("/api/terminal/auto-continue-tasks");
    if (state.autoContinueTaskRequestToken !== requestToken) {
      return;
    }
    renderAutoContinueTasks(payload);
  } catch (error) {
    if (state.autoContinueTaskRequestToken !== requestToken) {
      return;
    }
    setInlineStatus(autoContinueTaskStatusEl, error.message, "warn");
    autoContinueTaskListEl.innerHTML = '<tr><td colspan="7" class="meta-text">读取定时任务失败。</td></tr>';
  } finally {
    if (state.autoContinueTaskRequestToken === requestToken) {
      setButtonBusy(autoContinueTaskRefreshButtonEl, false);
    }
  }
}

// ---- Server-side paste scheduled sends ----
function formatPasteScheduledRemaining(dueAtMs) {
  if (!Number.isFinite(dueAtMs)) {
    return "-";
  }
  const remaining = dueAtMs - Date.now();
  if (remaining <= 0) {
    return "即将发送";
  }
  const totalSeconds = Math.ceil(remaining / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const parts = [];
  if (hours > 0) {
    parts.push(`${hours} 时`);
  }
  if (minutes > 0 || hours > 0) {
    parts.push(`${minutes} 分`);
  }
  parts.push(`${seconds} 秒`);
  return parts.join(" ");
}

function formatPasteScheduledDue(dueAtMs) {
  if (!Number.isFinite(dueAtMs)) {
    return "-";
  }
  try {
    return new Date(dueAtMs).toLocaleString();
  } catch (_error) {
    return String(dueAtMs);
  }
}

function normalizePasteScheduledSnapshot(task) {
  if (!task) {
    return null;
  }
  const taskId = String(task.taskId || task.id || "").trim();
  const dueAt = Number(task.dueAt ?? task.due_at ?? task.due_at_millis);
  if (!taskId || !Number.isFinite(dueAt)) {
    return null;
  }
  return {
    taskId,
    sessionId: String(task.sessionId || task.session_id || ""),
    terminalName: String(task.terminalName || task.terminal_name || "目标终端"),
    dueAt,
    label: String(task.label || ""),
    preview: String(task.preview || ""),
  };
}

async function readPasteScheduledSnapshots() {
  const payload = await requestJson("/api/terminal/scheduled-inputs");
  return (Array.isArray(payload?.tasks) ? payload.tasks : [])
    .map(normalizePasteScheduledSnapshot)
    .filter(Boolean)
    .sort((a, b) => a.dueAt - b.dueAt);
}

async function renderPasteScheduledTasks() {
  if (!pasteScheduledTaskListEl) {
    return;
  }
  let snapshots = [];
  try {
    snapshots = await readPasteScheduledSnapshots();
  } catch (error) {
    state.pasteScheduledTaskLastSnapshots = [];
    state.pasteScheduledTaskLastSnapshot = null;
    setInlineStatus(pasteScheduledTaskStatusEl, error?.message || "读取服务端粘贴定时任务失败。", "warn");
    pasteScheduledTaskListEl.innerHTML =
      '<tr><td colspan="5" class="meta-text">读取服务端粘贴定时任务失败。</td></tr>';
    stopPasteScheduledTaskTicker();
    return;
  }
  state.pasteScheduledTaskLastSnapshots = snapshots;
  state.pasteScheduledTaskLastSnapshot = snapshots[0] || null;

  if (snapshots.length === 0) {
    setInlineStatus(pasteScheduledTaskStatusEl, "当前没有服务端待发的粘贴定时任务。", "muted");
    pasteScheduledTaskListEl.innerHTML =
      '<tr><td colspan="5" class="meta-text">当前没有服务端待发的粘贴定时任务。</td></tr>';
    stopPasteScheduledTaskTicker();
    return;
  }

  setInlineStatus(
    pasteScheduledTaskStatusEl,
    `服务端当前有 ${snapshots.length} 个待发粘贴定时任务。`,
    "info",
  );
  pasteScheduledTaskListEl.innerHTML = snapshots
    .map((snapshot) => {
      const taskId = escapeHtml(snapshot.taskId);
      const preview = escapeHtml(snapshot.preview || "");
      const terminalName = escapeHtml(snapshot.terminalName || "当前终端");
      return `
        <tr data-paste-task-id="${taskId}">
          <td>${terminalName}</td>
          <td class="mono-text">${escapeHtml(formatPasteScheduledDue(snapshot.dueAt))}</td>
          <td class="mono-text paste-scheduled-remaining" data-paste-task-id="${taskId}">-</td>
          <td class="mono-text compile-path-text">${preview}</td>
          <td>
            <button class="button secondary paste-scheduled-cancel" type="button" data-paste-task-id="${taskId}">取消</button>
          </td>
        </tr>
      `;
    })
    .join("");
  startPasteScheduledTaskTicker();
}

function startPasteScheduledTaskTicker() {
  stopPasteScheduledTaskTicker();
  const tick = () => {
    const snapshots = Array.isArray(state.pasteScheduledTaskLastSnapshots)
      ? state.pasteScheduledTaskLastSnapshots
      : [];
    if (snapshots.length === 0) {
      return;
    }
    if (snapshots.some((snapshot) => Number(snapshot.dueAt) <= Date.now())) {
      renderPasteScheduledTasks();
      return;
    }
    const cells = Array.from(
      pasteScheduledTaskListEl?.querySelectorAll(".paste-scheduled-remaining") || [],
    );
    snapshots.forEach((snapshot) => {
      const cell = cells.find((candidate) => candidate.dataset.pasteTaskId === snapshot.taskId);
      if (cell) {
        cell.textContent = formatPasteScheduledRemaining(snapshot.dueAt);
      }
    });
  };
  tick();
  state.pasteScheduledTaskTickTimer = window.setInterval(tick, 1000);
}

function stopPasteScheduledTaskTicker() {
  if (state.pasteScheduledTaskTickTimer) {
    window.clearInterval(state.pasteScheduledTaskTickTimer);
    state.pasteScheduledTaskTickTimer = null;
  }
}

function loadPasteScheduledTasks() {
  renderPasteScheduledTasks();
}

function refreshPasteScheduledTasksIfVisible() {
  const panel = document.getElementById("settings-panel-auto-continue-tasks");
  if (!panel || panel.hidden) {
    return;
  }
  renderPasteScheduledTasks();
}

async function cancelPasteScheduledTask(taskId) {
  if (!taskId) {
    return;
  }
  try {
    await requestJson(`/api/terminal/scheduled-inputs/${encodeURIComponent(taskId)}`, {
      method: "DELETE",
    });
  } catch (error) {
    setInlineStatus(pasteScheduledTaskStatusEl, error?.message || "取消服务端定时任务失败。", "warn");
    return;
  }
  setInlineStatus(pasteScheduledTaskStatusEl, "已取消服务端定时任务。", "muted");
  renderPasteScheduledTasks();
}
