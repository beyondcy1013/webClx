// 未来任务：定时启动 Agent（工作目录 + 大模型 + 提示词）。
// 复用 scheduled-inputs 后端：保存时在目标工作目录建好终端，到点由后端
// 定时输入 runner 原样发送原生 Agent 启动命令。预设配置应在任务创建前写入。
let terminalFutureTasksItems = [];

function terminalFutureTaskDom() {
  return {
    dialog: document.getElementById("terminal-future-tasks-dialog"),
    form: document.getElementById("terminal-future-tasks-form"),
    status: document.getElementById("terminal-future-tasks-status"),
    tableWrap: document.getElementById("terminal-future-tasks-table-wrap"),
    tbody: document.getElementById("terminal-future-tasks-tbody"),
    newButton: document.getElementById("terminal-future-tasks-new"),
    refreshButton: document.getElementById("terminal-future-tasks-refresh"),
    closeButton: document.getElementById("terminal-future-tasks-close"),
    schedule: document.getElementById("terminal-specified-task-schedule"),
  };
}

function terminalFutureTaskSetStatus(message, tone = "muted") {
  const { status } = terminalFutureTaskDom();
  if (!status) {
    return;
  }
  status.textContent = String(message || "");
  status.dataset.tone = tone;
  status.hidden = !String(message || "");
}

// 单引号/双引号感知的 shell argv 拆分，与指定启动命令的 shell 转义互为逆操作。
function terminalFutureTaskShellSplit(command) {
  const text = String(command || "");
  const args = [];
  let current = "";
  let hasToken = false;
  let index = 0;
  while (index < text.length) {
    const char = text[index];
    if (char === "'" || char === '"') {
      const end = text.indexOf(char, index + 1);
      if (end === -1) {
        current += text.slice(index + 1);
        hasToken = true;
        break;
      }
      current += text.slice(index + 1, end);
      hasToken = true;
      index = end + 1;
      continue;
    }
    if (char === " " || char === "\t" || char === "\n") {
      if (hasToken || current) {
        args.push(current);
        current = "";
        hasToken = false;
      }
      index += 1;
      continue;
    }
    current += char;
    hasToken = true;
    index += 1;
  }
  if (hasToken || current) {
    args.push(current);
  }
  return args;
}

function buildTerminalFutureTaskRunCommand(agent, presetKind, presetId, prompt) {
  if (typeof specifiedPresetLaunchCommand !== "function") {
    throw new Error("启动命令构建器未加载。");
  }
  return specifiedPresetLaunchCommand({
    agent,
    presetId,
    sessionAction: "new",
    futureTask: true,
    task: String(prompt || "").trim(),
  });
}

function parseTerminalFutureTaskCommand(text) {
  const argv = terminalFutureTaskShellSplit(String(text || "").trim());
  if (argv.length < 1) {
    return null;
  }
  let commandArguments = argv;
  let presetId = "";
    if (argv[0] === "webclx" && argv[1] === "run") {
    if (argv.length < 7 || argv[4] !== "--") return null;
    const presetPrefix = String(argv[2] || "");
    if (!["api", "claude"].includes(presetPrefix)) return null;
    presetId = String(argv[3] || "");
    commandArguments = argv.slice(5);
  }
  const agent = String(commandArguments[0] || "").trim().toLowerCase();
  if (!["codex", "claude"].includes(agent)) {
    return null;
  }
  return {
    agent,
    presetKind: agent === "claude" ? "claude" : "api",
    presetId,
    prompt: commandArguments.slice(1).join(" "),
  };
}

function terminalFutureTaskMillisToScheduleInput(millis) {
  const value = Number(millis);
  if (!Number.isFinite(value) || value <= 0) {
    return "";
  }
  const date = new Date(value);
  const pad = (part) => String(part).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function terminalFutureTaskScheduleInputToMillis(value) {
  const raw = String(value || "").trim();
  if (!raw) {
    return 0;
  }
  const millis = new Date(raw).getTime();
  return Number.isFinite(millis) ? millis : 0;
}

function terminalFutureTaskNormalizeDir(value) {
  return String(value || "").trim().replace(/\/+$/, "") || "/";
}

function terminalFutureTaskFormatDue(millis) {
  const value = Number(millis);
  if (!Number.isFinite(value) || value <= 0) {
    return "未知";
  }
  const date = new Date(value);
  const pad = (part) => String(part).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

async function loadTerminalFutureTaskPresetLabels() {
  const labels = new Map();
  const endpoints = [
    { url: "/api/auth/codex-accounts", items: (response) => response?.accounts },
    { url: "/api/auth/claude-presets", items: (response) => response?.presets },
  ];
  await Promise.all(endpoints.map(async ({ url, items }) => {
    try {
      const response = await requestJson(url);
      (Array.isArray(items(response)) ? items(response) : []).forEach((item) => {
        if (!item?.id) {
          return;
        }
        const model = typeof specifiedPresetModel === "function" ? specifiedPresetModel(item) : "";
        labels.set(String(item.id), `${item.name || item.id}${model ? ` · ${model}` : ""}`);
      });
    } catch {
      // 预设读取失败时回退显示预设 ID，不阻塞列表。
    }
  }));
  return labels;
}

function terminalFutureTaskRender(labels) {
  const { tbody, tableWrap } = terminalFutureTaskDom();
  if (!tbody) {
    return;
  }
  tbody.replaceChildren();
  if (!terminalFutureTasksItems.length) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 5;
    cell.className = "terminal-future-tasks-empty";
    cell.textContent = "暂无未来任务。点击“新建未来任务”创建一个定时启动的 Agent 任务。";
    row.append(cell);
    tbody.append(row);
    return;
  }
  for (const task of terminalFutureTasksItems) {
    const parsed = parseTerminalFutureTaskCommand(task.text) || {};
    const presetLabel = labels.get(String(parsed.presetId || ""))
      || (parsed.presetId ? `预设 ${parsed.presetId}` : "未知");
    const agentLabel = parsed.agent === "claude" ? "Claude" : "Codex";
    const row = document.createElement("tr");
    row.className = "terminal-future-task-row";
    row.dataset.taskId = task.id;
    row.title = "点击编辑该任务";

    const dueCell = document.createElement("td");
    dueCell.textContent = terminalFutureTaskFormatDue(task.due_at_millis);
    const presetCell = document.createElement("td");
    presetCell.textContent = `${agentLabel} · ${presetLabel}`;
    const workdirCell = document.createElement("td");
    workdirCell.className = "mono-text";
    workdirCell.textContent = task.working_dir || "/";
    workdirCell.title = task.working_dir || "/";
    const promptCell = document.createElement("td");
    const promptText = String(parsed.prompt || task.preview || "").trim();
    promptCell.textContent = promptText || "（无提示词）";
    promptCell.title = promptText;
    if (promptText.length > 60) {
      promptCell.textContent = `${promptText.slice(0, 60)}…`;
    }

    const actionsCell = document.createElement("td");
    const editButton = document.createElement("button");
    editButton.type = "button";
    editButton.className = "button secondary terminal-future-task-action";
    editButton.textContent = "编辑";
    editButton.dataset.futureTaskAction = "edit";
    const deleteButton = document.createElement("button");
    deleteButton.type = "button";
    deleteButton.className = "button secondary terminal-future-task-action terminal-future-task-delete";
    deleteButton.textContent = "删除";
    deleteButton.dataset.futureTaskAction = "delete";
    actionsCell.append(editButton, deleteButton);

    row.append(dueCell, presetCell, workdirCell, promptCell, actionsCell);
    tbody.append(row);
  }
  if (tableWrap) {
    tableWrap.hidden = false;
  }
}

async function refreshTerminalFutureTasks() {
  try {
    const response = await requestJson("/api/terminal/scheduled-inputs");
    terminalFutureTasksItems = (Array.isArray(response?.tasks) ? response.tasks : [])
      .filter((task) => String(task?.task_type || "") === "future_task")
      .filter((task) => Number(task?.due_at_millis) > Date.now())
      .sort((left, right) => Number(left.due_at_millis) - Number(right.due_at_millis));
    const labels = await loadTerminalFutureTaskPresetLabels();
    terminalFutureTaskRender(labels);
    terminalFutureTaskSetStatus(
      terminalFutureTasksItems.length ? `共 ${terminalFutureTasksItems.length} 个未来任务。` : "",
      "muted",
    );
  } catch (error) {
    terminalFutureTaskSetStatus(`读取未来任务失败：${error.message}`, "warn");
  }
}

function openTerminalFutureTasksDialog(trigger = null) {
  const dom = terminalFutureTaskDom();
  if (!dom.dialog) {
    updateStatus("未来任务界面未加载。", "warn");
    return;
  }
  terminalFutureTaskSetStatus("");
  if (!dom.dialog.open) {
    dom.dialog.showModal();
  }
  refreshTerminalFutureTasks();
  if (trigger instanceof HTMLElement) {
    dom.dialog.dataset.triggerId = trigger.id || "";
  }
}

function closeTerminalFutureTasksDialog() {
  const dom = terminalFutureTaskDom();
  if (dom.dialog?.open) {
    dom.dialog.close();
  }
}

function openTerminalFutureTaskEditor(task, trigger = null) {
  const parsed = parseTerminalFutureTaskCommand(task?.text);
  if (!parsed) {
    updateStatus("该任务缺少启动命令信息，无法编辑。", "warn");
    return;
  }
  closeTerminalFutureTasksDialog();
  openTerminalSpecifiedTaskDialog(trigger, {
    agent: parsed.agent,
    lockAgent: false,
    mode: "fixed",
    resetPreset: true,
    resetTask: true,
    sessionAction: "new",
    temporary: true,
    sourcePath: String(task.working_dir || ""),
    sourceTerminalName: String(task.terminal_name || ""),
    title: "编辑未来任务",
    updatePageStatus: updateStatus,
    futureTask: true,
    futureTaskEdit: {
      taskId: String(task.id || ""),
      label: String(task.label || ""),
      dueAt: Number(task.due_at_millis) || 0,
      agent: parsed.agent,
      presetKind: parsed.presetKind,
      presetId: parsed.presetId,
      prompt: parsed.prompt,
      workingDir: String(task.working_dir || ""),
    },
  });
}

async function deleteTerminalFutureTask(taskId) {
  const id = String(taskId || "").trim();
  if (!id) {
    return;
  }
  if (!window.confirm("确定删除该未来任务？到点后将不再启动。")) {
    return;
  }
  try {
    await requestJson(`/api/terminal/scheduled-inputs/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
    updateStatus("已删除未来任务。", "ok");
    await refreshTerminalFutureTasks();
  } catch (error) {
    terminalFutureTaskSetStatus(`删除失败：${error.message}`, "warn");
  }
}

function newTerminalFutureTask(trigger = null) {
  closeTerminalFutureTasksDialog();
  openTerminalSpecifiedTaskDialog(trigger, {
    agent: "codex",
    mode: "fixed",
    resetPreset: true,
    resetTask: true,
    sessionAction: "new",
    temporary: true,
    title: "新建未来任务",
    updatePageStatus: updateStatus,
    futureTask: true,
  });
}

// “新开”对话框定时模式的提交入口：不立即启动，保存为未来任务。
async function submitTerminalSpecifiedFutureTask() {
  const dom = terminalSpecifiedTaskDom();
  const edit = terminalSpecifiedTaskFutureEdit;
  const agent = terminalSpecifiedTaskAgent();
  if (agent !== "codex" && agent !== "claude") {
    setTerminalSpecifiedTaskStatus("未来任务仅支持 Codex / Claude 预设。", "warn");
    return;
  }
  const presetId = String(dom.preset?.value || "").trim();
  if (!presetId) {
    setTerminalSpecifiedTaskStatus(`请选择 ${agent === "claude" ? "Claude API" : "Codex"} 账号。`, "warn");
    return;
  }
  const dueAt = terminalFutureTaskScheduleInputToMillis(dom.schedule?.value);
  if (!dueAt || dueAt <= Date.now()) {
    setTerminalSpecifiedTaskStatus("请选择晚于当前时间的执行时间。", "warn");
    return;
  }
  const workingDir = terminalSpecifiedTaskSourcePath || terminalSpecifiedTaskCurrentPath();
  const prompt = String(dom.task?.value || "").trim();
  const presetKind = terminalSpecifiedTaskSelectedPresetKind();
  let runCommand = "";
  try {
    runCommand = buildTerminalFutureTaskRunCommand(agent, presetKind, presetId, prompt);
  } catch (error) {
    setTerminalSpecifiedTaskStatus(error.message || "构建启动命令失败。", "warn");
    return;
  }
  const presetOption = dom.preset?.selectedOptions?.[0];
  const presetName = String(presetOption?.textContent || presetId).trim();
  const label = `未来任务 · ${agent === "claude" ? "Claude" : "Codex"} · ${terminalFutureTaskFormatDue(dueAt)}`;

  setTerminalSpecifiedTaskBusy(true);
  try {
    // 编辑优先 PATCH（不动已建终端）；只有更换工作目录才删除重建，
    // 因为工作目录在保存时落成新终端，PATCH 无法迁移目录。
    const dirChanged = Boolean(edit?.taskId)
      && terminalFutureTaskNormalizeDir(edit.workingDir) !== terminalFutureTaskNormalizeDir(workingDir);
    if (edit?.taskId && !dirChanged) {
      await requestJson(`/api/terminal/scheduled-inputs/${encodeURIComponent(edit.taskId)}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          text: runCommand,
          due_at: dueAt,
          label,
          task_type: "future_task",
        }),
      });
    } else {
      if (edit?.taskId) {
        await requestJson(`/api/terminal/scheduled-inputs/${encodeURIComponent(edit.taskId)}`, {
          method: "DELETE",
        });
      }
      await requestJson("/api/terminal/scheduled-inputs", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          session_id: "",
          terminal_mode: "new",
          working_dir: workingDir,
          text: runCommand,
          due_at: dueAt,
          label,
          send_enter: true,
          task_type: "future_task",
        }),
      });
    }
    setTerminalSpecifiedTaskStatus(`未来任务已保存，将于 ${terminalFutureTaskFormatDue(dueAt)} 启动。`, "ok");
    reportTerminalSpecifiedTaskPageStatus(
      `未来任务已保存：${terminalFutureTaskFormatDue(dueAt)} 在 ${workingDir} 启动 ${agent === "claude" ? "Claude" : "Codex"}。`,
      "ok",
    );
    closeTerminalSpecifiedTaskDialog();
    openTerminalFutureTasksDialog();
  } catch (error) {
    setTerminalSpecifiedTaskStatus(`保存未来任务失败：${error.message}`, "warn");
  } finally {
    setTerminalSpecifiedTaskBusy(false);
  }
}

function bindTerminalFutureTasksDialog() {
  const dom = terminalFutureTaskDom();
  if (!dom.dialog || !dom.form || dom.dialog.dataset.bound === "true") {
    return;
  }
  dom.dialog.dataset.bound = "true";
  dom.newButton?.addEventListener("click", (event) => {
    event.preventDefault();
    newTerminalFutureTask(event.currentTarget);
  });
  dom.refreshButton?.addEventListener("click", (event) => {
    event.preventDefault();
    refreshTerminalFutureTasks();
  });
  dom.closeButton?.addEventListener("click", (event) => {
    event.preventDefault();
    closeTerminalFutureTasksDialog();
  });
  dom.tbody?.addEventListener("click", (event) => {
    if (!(event.target instanceof Element)) {
      return;
    }
    const actionButton = event.target.closest("button[data-future-task-action]");
    const row = event.target.closest("tr[data-task-id]");
    if (!row) {
      return;
    }
    const taskId = row.dataset.taskId;
    const task = terminalFutureTasksItems.find((item) => item.id === taskId);
    if (!task) {
      return;
    }
    if (actionButton?.dataset.futureTaskAction === "delete") {
      event.preventDefault();
      deleteTerminalFutureTask(taskId);
      return;
    }
    event.preventDefault();
    openTerminalFutureTaskEditor(task, actionButton instanceof HTMLElement ? actionButton : null);
  });
}
