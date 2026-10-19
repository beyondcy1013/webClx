let terminalToolCurrentRootKey = "tools";
let terminalToolCurrentParentId = null;
let terminalToolExecutionRunning = false;
let terminalToolMenuTriggerEl = null;
let terminalToolSearchQuery = "";

const TERMINAL_TOOL_SESSION_READY_TIMEOUT_MS = 15000;
const TERMINAL_TOOL_RESUME_TIMEOUT_MS = 30000;
const TERMINAL_TOOL_RESUME_SCAN_MAX_LINES = 240;
const LEGACY_WORKFLOW_SESSION_STORAGE_KEY = "webclx:workflow-terminal-sessions";
const TERMINAL_WORKFLOW_RECENT_STORAGE_KEY = "webclx:workflow-recent";
const TERMINAL_WORKFLOW_RUN_STORAGE_KEY = "webclx:workflow-runs";
const TERMINAL_WORKFLOW_RECENT_LIMIT = 6;
const TERMINAL_WORKFLOW_RUN_LIMIT = 20;
let terminalToolFailedExecution = null;

function terminalWorkflowOwnerKey(executionContext) {
  return String(executionContext.workflowEntryId || "").trim();
}

function terminalOwnedAgentSession(sessions, ownerKey) {
  const normalizedOwnerKey = String(ownerKey || "").trim();
  if (!normalizedOwnerKey || !Array.isArray(sessions)) {
    return null;
  }
  return sessions.find(
    (session) => session?.idle !== true
      && session?.origin === "agent"
      && session?.owner_key === normalizedOwnerKey,
  ) || null;
}

async function findReusableAgentSession(ownerKey) {
  const localSession = terminalOwnedAgentSession(state.sessions, ownerKey);
  if (localSession) {
    return localSession;
  }

  const response = await requestJson("/api/terminal/sessions?all=true");
  const session = terminalOwnedAgentSession(response?.sessions, ownerKey);
  if (session) {
    insertOrReplaceSession(session);
  }
  return session;
}

function terminalWorkflowStandbyPrompt(rawTask) {
  const skillInvocation = String(rawTask || "").match(/(?:^|\s)(\$[a-z0-9][\w-]*)/i)?.[1] || "";
  if (skillInvocation) {
    return `${skillInvocation}\n\n请仅加载上述技能及必要上下文，然后待命等待用户进一步指令。不要主动检查、修改或执行任何工作。`;
  }
  return "请仅加载当前项目的必要上下文，然后待命等待用户进一步指令。不要主动检查、修改或执行任何工作。";
}

function readLegacyWorkflowSessionIds() {
  try {
    const parsed = JSON.parse(
      window.localStorage.getItem(LEGACY_WORKFLOW_SESSION_STORAGE_KEY) || "[]",
    );
    return new Set(
      Array.isArray(parsed)
        ? parsed.map((value) => String(value || "").trim()).filter(Boolean)
        : [],
    );
  } catch {
    return new Set();
  }
}

function writeLegacyWorkflowSessionIds(ids) {
  try {
    window.localStorage.setItem(LEGACY_WORKFLOW_SESSION_STORAGE_KEY, JSON.stringify([...ids]));
  } catch {}
}

function legacyWorkflowSessionMetadata(session) {
  const terminalName = String(session?.name || "").trim();
  const entries = Array.isArray(state?.terminalToolEntries) ? state.terminalToolEntries : [];
  for (const entry of entries) {
    if (entry?.kind !== "action" || !Array.isArray(entry.actions)) {
      continue;
    }
    if (entry.actions.some(
      (action) => action?.kind === "codex_launch"
        && String(action.terminal_name || "").trim() === terminalName,
    )) {
      return { origin: "agent", ownerKey: String(entry.id || "").trim() };
    }
  }
  return { origin: "workflow", ownerKey: "" };
}

async function migrateLegacyWorkflowSessionOrigins() {
  const legacyIds = readLegacyWorkflowSessionIds();
  if (legacyIds.size === 0 || !Array.isArray(state?.sessions)) {
    return false;
  }

  const remainingIds = new Set(legacyIds);
  let migrated = false;
  for (const session of state.sessions) {
    if (!legacyIds.has(session?.id) || session?.origin !== "normal") {
      remainingIds.delete(session?.id);
      continue;
    }
    const metadata = legacyWorkflowSessionMetadata(session);
    try {
      const updated = await requestJson(
        `/api/terminal/sessions/${encodeURIComponent(session.id)}`,
        {
          method: "PATCH",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            origin: metadata.origin,
            owner_key: metadata.ownerKey,
          }),
        },
      );
      state.sessions = state.sessions.map((item) => item.id === updated.id ? updated : item);
      remainingIds.delete(session.id);
      migrated = true;
    } catch (error) {
      console.warn?.(`迁移旧工作流终端 ${session.id} 失败：${error.message || error}`);
    }
  }
  writeLegacyWorkflowSessionIds(remainingIds);
  return migrated;
}

function terminalToolRootLabel(rootKey) {
  return TERMINAL_TOOL_ROOTS.find((entry) => entry.key === rootKey)?.label || rootKey;
}

function terminalToolActionLabel(kind) {
  return TERMINAL_TOOL_ACTION_TYPES.find((entry) => entry.key === kind)?.label || kind;
}

function terminalToolEntryById(entryId) {
  return state.terminalToolEntries.find((entry) => entry.id === entryId) || null;
}

function terminalToolChildren(rootKey, parentId) {
  return state.terminalToolEntries
    .filter((entry) => entry.root_key === rootKey && (entry.parent_id || null) === (parentId || null))
    .sort((left, right) =>
      left.sort_order - right.sort_order
      || left.label.localeCompare(right.label, "zh-CN")
      || left.id.localeCompare(right.id)
    );
}

function terminalToolDirectoryTitle(parentId) {
  if (!parentId) {
    return terminalToolRootLabel(terminalToolCurrentRootKey);
  }
  const labels = [];
  const seen = new Set();
  let cursor = terminalToolEntryById(parentId);
  while (cursor && !seen.has(cursor.id)) {
    seen.add(cursor.id);
    labels.unshift(cursor.label);
    cursor = cursor.parent_id ? terminalToolEntryById(cursor.parent_id) : null;
  }
  return labels.join(" / ") || terminalToolRootLabel(terminalToolCurrentRootKey);
}

function setTerminalToolMenuStatus(message = "", tone = "muted") {
  if (!terminalToolMenuStatusEl) {
    return;
  }
  terminalToolMenuStatusEl.hidden = !message;
  terminalToolMenuStatusEl.textContent = message;
  terminalToolMenuStatusEl.dataset.tone = tone;
  window.requestAnimationFrame(positionTerminalToolMenu);
}

function terminalToolActionSummary(entry) {
  return entry.actions.map((action) => terminalToolActionLabel(action.kind)).join(" → ");
}

function readTerminalWorkflowRecentIds() {
  try {
    const parsed = JSON.parse(window.localStorage.getItem(TERMINAL_WORKFLOW_RECENT_STORAGE_KEY) || "[]");
    return Array.isArray(parsed) ? parsed.map((id) => String(id || "").trim()).filter(Boolean) : [];
  } catch {
    return [];
  }
}

function writeTerminalWorkflowRecentIds(ids) {
  try {
    window.localStorage.setItem(
      TERMINAL_WORKFLOW_RECENT_STORAGE_KEY,
      JSON.stringify([...new Set(ids)].slice(0, TERMINAL_WORKFLOW_RECENT_LIMIT)),
    );
  } catch {}
}

function terminalToolRecentEntries() {
  const byId = new Map(state.terminalToolEntries.map((entry) => [entry.id, entry]));
  return readTerminalWorkflowRecentIds()
    .map((id) => byId.get(id))
    .filter((entry) => entry?.kind === "action");
}

function rememberTerminalToolRecent(entryId) {
  const id = String(entryId || "").trim();
  if (!id) return;
  writeTerminalWorkflowRecentIds([id, ...readTerminalWorkflowRecentIds().filter((item) => item !== id)]);
}


function terminalWorkflowDefinitionStamp(entry, expandedSteps = null) {
  const serialized = JSON.stringify(expandedSteps === null ? {
    id: entry?.id,
    kind: entry?.kind,
    root_key: entry?.root_key,
    parent_id: entry?.parent_id,
    label: entry?.label,
    actions: Array.isArray(entry?.actions) ? entry.actions : [],
  } : {
    id: entry?.id,
    steps: Array.isArray(expandedSteps) ? expandedSteps : [],
  });
  let hash = 5381;
  for (let index = 0; index < serialized.length; index += 1) {
    hash = ((hash << 5) + hash + serialized.charCodeAt(index)) >>> 0;
  }
  return hash.toString(16);
}

function normalizeTerminalWorkflowRunRecords(raw) {
  const source = Array.isArray(raw) ? raw : [];
  const records = [];
  for (const item of source.slice(0, TERMINAL_WORKFLOW_RUN_LIMIT)) {
    const record = {
      id: String(item?.id || "").trim(),
      entryId: String(item?.entryId || "").trim(),
      entryLabel: String(item?.entryLabel || "").slice(0, 64),
      status: ["completed", "failed", "cancelled"].includes(item?.status) ? item.status : "failed",
      progress: String(item?.progress || "").slice(0, 32),
      startedAt: Number(item?.startedAt),
      finishedAt: Number(item?.finishedAt),
      definitionStamp: String(item?.definitionStamp || "").slice(0, 32),
    };
    if (
      !record.id
      || !record.entryId
      || !record.entryLabel
      || !Number.isFinite(record.startedAt)
      || !Number.isFinite(record.finishedAt)
      || !record.definitionStamp
    ) {
      continue;
    }
    records.push(record);
  }
  return records;
}

function readTerminalWorkflowRunRecords() {
  try {
    return normalizeTerminalWorkflowRunRecords(
      JSON.parse(window.localStorage.getItem(TERMINAL_WORKFLOW_RUN_STORAGE_KEY) || "[]"),
    );
  } catch {
    return [];
  }
}

function writeTerminalWorkflowRunRecords(records) {
  try {
    window.localStorage.setItem(
      TERMINAL_WORKFLOW_RUN_STORAGE_KEY,
      JSON.stringify(normalizeTerminalWorkflowRunRecords(records)),
    );
  } catch {}
}

function recordTerminalWorkflowRun(record) {
  writeTerminalWorkflowRunRecords([
    record,
    ...readTerminalWorkflowRunRecords().filter((item) => item.id !== record.id),
  ]);
}

function clearTerminalWorkflowRunRecords() {
  writeTerminalWorkflowRunRecords([]);
}

function terminalWorkflowStatusLabel(status) {
  return status === "completed" ? "完成" : status === "cancelled" ? "取消" : "失败";
}

function formatTerminalWorkflowRunTime(timestamp) {
  const date = new Date(timestamp);
  return Number.isFinite(date.getTime())
    ? date.toLocaleString("zh-CN", { hour12: false })
    : "";
}

const TERMINAL_WORKFLOW_PARAMETER_SOURCE = String.raw`\{\{\s*([\w\-\u00c0-\uffff]{1,64})\s*\}\}`;
const TERMINAL_WORKFLOW_PARAMETER_LIMIT = 20;

function collectTerminalWorkflowRuntimeParameters(plan) {
  const parameters = new Map();
  const pattern = new RegExp(TERMINAL_WORKFLOW_PARAMETER_SOURCE, "g");
  for (const step of Array.isArray(plan?.steps) ? plan.steps : []) {
    const value = String(step?.value || "");
    for (const match of value.matchAll(pattern)) {
      const name = String(match[1] || "").trim();
      if (!name || parameters.has(name)) continue;
      parameters.set(name, {
        name,
        long: ["codex_exec", "codex_terminal", "codex_launch"].includes(step.kind),
      });
    }
  }
  if (parameters.size > TERMINAL_WORKFLOW_PARAMETER_LIMIT) {
    throw new Error(`运行参数超过 ${TERMINAL_WORKFLOW_PARAMETER_LIMIT} 个。`);
  }
  return [...parameters.values()];
}

function materializeTerminalWorkflowAction(action, parameters) {
  const values = parameters || {};
  const value = String(action?.value || "");
  if (!Object.keys(values).length || !value) {
    return action;
  }
  const pattern = new RegExp(TERMINAL_WORKFLOW_PARAMETER_SOURCE, "g");
  const materialized = value.replace(pattern, (match, name) => (
    Object.prototype.hasOwnProperty.call(values, decodeURIComponent(name)) ? values[decodeURIComponent(name)] : match
  ));
  return materialized === value ? action : { ...action, value: materialized };
}

function openTerminalWorkflowParametersDialog(entry, parameters) {
  if (!Array.isArray(parameters) || parameters.length === 0) {
    return Promise.resolve({});
  }
  const dialog = document.getElementById("terminal-workflow-parameters-dialog");
  const form = document.getElementById("terminal-workflow-parameters-form");
  const title = document.getElementById("terminal-workflow-parameters-title");
  const body = document.getElementById("terminal-workflow-parameters-body");
  if (!dialog || !form || !title || !body || dialog.open) {
    return Promise.reject(new Error("运行参数对话框不可用。"));
  }
  form.reset();
  title.textContent = `运行参数：${entry.label}`;
  body.replaceChildren();
  let firstControl = null;
  for (const parameter of parameters) {
    const field = document.createElement("label");
    field.className = "terminal-specified-task-field terminal-workflow-parameter-field";
    const label = document.createElement("span");
    label.textContent = parameter.name;
    const control = document.createElement(parameter.long ? "textarea" : "input");
    control.className = "text-input mono-text";
    control.required = true;
    control.maxLength = 4096;
    control.setAttribute("data-terminal-workflow-parameter", parameter.name);
    if (parameter.long) control.rows = 3;
    else control.type = "text";
    control.autocomplete = "off";
    control.placeholder = `输入 ${parameter.name}`;
    field.append(label, control);
    body.append(field);
    firstControl ||= control;
  }
  const cleanupFunctions = new Set();
  return new Promise((resolve) => {
    let pendingResolve = resolve;
    const finish = (result) => {
      if (!pendingResolve) return;
      const resolveNow = pendingResolve;
      pendingResolve = null;
      dialog.close();
      resolveNow(result);
    };
    const handleSubmit = (event) => {
      event.preventDefault();
      const values = {};
      for (const parameter of parameters) {
        const control = body.querySelector(`[data-terminal-workflow-parameter="${CSS.escape(parameter.name)}"]`);
        const value = String(control?.value || "").trim();
        if (!value) {
          finish(null);
          return;
        }
        values[parameter.name] = value;
      }
      finish(values);
    };
    const handleCancel = () => finish(null);
    const handleClose = () => {
      if (!pendingResolve) return;
      const resolveNow = pendingResolve;
      pendingResolve = null;
      resolveNow(null);
    };
    const cleanup = () => {
      form.removeEventListener("submit", handleSubmit);
      document.getElementById("terminal-workflow-parameters-cancel")?.removeEventListener("click", handleCancel);
      dialog.removeEventListener("close", handleClose);
      dialog.removeEventListener("cancel", suppressCancelEvent);
    };
    const suppressCancelEvent = (event) => event.preventDefault();
    form.addEventListener("submit", handleSubmit, { capture: true });
    document.getElementById("terminal-workflow-parameters-cancel")?.addEventListener("click", handleCancel);
    dialog.addEventListener("close", handleClose);
    dialog.addEventListener("cancel", suppressCancelEvent);
    cleanupFunctions.add(cleanup);
    dialog.showModal();
    firstControl?.focus();
  }).finally(() => {
    for (const cleanup of cleanupFunctions) cleanup();
    cleanupFunctions.clear();
  });
}

// The preflight plan already expands references, so every entry point asks for each shared parameter once.
function prepareTerminalWorkflowRuntimeParameters(entry, plan) {
  const parameters = collectTerminalWorkflowRuntimeParameters(plan);
  return openTerminalWorkflowParametersDialog(entry, parameters);
}

// Search labels and folder paths only: command bodies may contain sensitive values.
function terminalToolSearchResults(query) {
  const terms = String(query || "").trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length) return [];
  return state.terminalToolEntries
    .filter((entry) => entry.kind === "action")
    .map((entry) => ({
      entry,
      path: [terminalToolRootLabel(entry.root_key),
        entry.parent_id ? terminalToolDirectoryTitle(entry.parent_id) : ""].filter(Boolean).join(" / "),
    }))
    .filter(({ entry, path }) => terms.every((term) => `${path} / ${entry.label}`.toLocaleLowerCase().includes(term)))
    .sort((a, b) => a.path.localeCompare(b.path, "zh-CN")
      || a.entry.sort_order - b.entry.sort_order
      || a.entry.label.localeCompare(b.entry.label, "zh-CN")
      || a.entry.id.localeCompare(b.entry.id));
}

function handleTerminalToolSearch(event) {
  terminalToolSearchQuery = event.target.value;
  renderTerminalToolMenu();
}

function renderTerminalToolMenu() {
  if (!terminalToolMenuBodyEl || !terminalToolMenuTitleEl) {
    return;
  }
  const searching = Boolean(terminalToolSearchQuery.trim());
  terminalToolMenuTitleEl.textContent = searching ? "搜索全部工作流" : terminalToolDirectoryTitle(terminalToolCurrentParentId);
  terminalToolMenuTitleEl.title = terminalToolMenuTitleEl.textContent;
  if (terminalToolMenuBackEl) {
    terminalToolMenuBackEl.disabled = searching || !terminalToolCurrentParentId || terminalToolExecutionRunning;
  }
  terminalToolMenuBodyEl.replaceChildren();
  const results = searching ? terminalToolSearchResults(terminalToolSearchQuery) : null;
  const children = results ? results.map((result) => result.entry) : terminalToolChildren(terminalToolCurrentRootKey, terminalToolCurrentParentId);
  if (children.length === 0) {
    const empty = document.createElement("p");
    empty.className = "meta-text terminal-tool-menu-empty";
    empty.textContent = searching ? "没有匹配的工作流，请更换名称或目录关键词。" : "此目录没有条目。";
    terminalToolMenuBodyEl.append(empty);
    window.requestAnimationFrame(positionTerminalToolMenu);
    return;
  }
  const recent = !searching && !terminalToolCurrentParentId ? terminalToolRecentEntries() : [];
  const appendEntry = (entry, detailOverride = "") => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `terminal-tool-menu-item terminal-tool-menu-item-${entry.kind}`;
    button.setAttribute("role", "menuitem");
    button.dataset.terminalToolEntry = entry.id;
    button.disabled = terminalToolExecutionRunning;
    const label = document.createElement("span");
    label.className = "terminal-tool-menu-item-label";
    label.textContent = entry.label;
    const detail = document.createElement("span");
    detail.className = "terminal-tool-menu-item-detail";
    detail.textContent = detailOverride || (searching
      ? results.find((result) => result.entry.id === entry.id).path
      : entry.kind === "folder" ? "进入 ›" : terminalToolActionSummary(entry));
    if (searching || detailOverride) {
      detail.style.display = "block";
      detail.title = detail.textContent;
    }
    button.append(label, detail);
    terminalToolMenuBodyEl.append(button);
  };
  const runRecords = !searching && !terminalToolCurrentParentId ? readTerminalWorkflowRunRecords().slice(0, 5) : [];
  if (runRecords.length) {
    const heading = document.createElement("div");
    heading.className = "terminal-tool-menu-section-title";
    heading.textContent = "执行记录";
    const clear = document.createElement("button");
    clear.type = "button";
    clear.className = "terminal-tool-menu-history-clear";
    clear.textContent = "清除";
    clear.dataset.terminalWorkflowRunAction = "clear";
    clear.disabled = terminalToolExecutionRunning;
    heading.append(clear);
    terminalToolMenuBodyEl.append(heading);
    const currentEntries = new Map(state.terminalToolEntries.map((entry) => [entry.id, entry]));
    for (const record of runRecords) {
      const item = document.createElement("div");
      item.className = `terminal-tool-menu-history terminal-tool-menu-history-${record.status}`;
      const info = document.createElement("div");
      info.className = "terminal-tool-menu-history-info";
      const label = document.createElement("span");
      label.className = "terminal-tool-menu-history-label";
      label.textContent = record.entryLabel;
      const detail = document.createElement("span");
      detail.className = "terminal-tool-menu-history-detail";
      detail.textContent = [
        terminalWorkflowStatusLabel(record.status),
        record.progress,
        formatTerminalWorkflowRunTime(record.finishedAt),
      ].filter(Boolean).join(" · ");
      info.append(label, detail);
      const entry = currentEntries.get(record.entryId);
      const actions = document.createElement("span");
      actions.className = "terminal-tool-menu-history-actions";
      const canRetry = record.status === "failed"
        && terminalToolFailedExecution?.runId === record.id
        && entry
        && terminalWorkflowDefinitionStamp(
          entry,
          WebClxTerminalSettings.inspectTerminalWorkflow(entry, state.terminalToolEntries).steps,
        ) === terminalToolFailedExecution.definitionStamp;
      if (canRetry) {
        const retry = document.createElement("button");
        retry.type = "button";
        retry.className = "mini-button";
        retry.textContent = "重试失败步骤";
        retry.dataset.terminalWorkflowRunAction = "retry";
        retry.dataset.terminalWorkflowRunId = record.id;
        retry.disabled = terminalToolExecutionRunning;
        actions.append(retry);
      }
      if (entry?.kind === "action") {
        const restart = document.createElement("button");
        restart.type = "button";
        restart.className = "mini-button";
        restart.textContent = "重新开始";
        restart.dataset.terminalWorkflowRunAction = "restart";
        restart.dataset.terminalWorkflowRunId = record.id;
        restart.disabled = terminalToolExecutionRunning;
        actions.append(restart);
      }
      item.append(info, actions);
      terminalToolMenuBodyEl.append(item);
    }
  }
  if (recent.length) {
    const heading = document.createElement("div");
    heading.className = "terminal-tool-menu-section-title";
    heading.textContent = "最近使用";
    terminalToolMenuBodyEl.append(heading);
    recent.forEach((entry) => appendEntry(entry, "最近执行"));
    if (children.length) {
      const heading = document.createElement("div");
      heading.className = "terminal-tool-menu-section-title";
      heading.textContent = "全部工作流";
      terminalToolMenuBodyEl.append(heading);
    }
  }
  for (const entry of children) appendEntry(entry);
  window.requestAnimationFrame(positionTerminalToolMenu);
}

function renderTerminalToolRootButtons() {
  if (terminalToolMenuEl && terminalToolsMenuEl && !terminalToolsMenuEl.hidden) {
    renderTerminalToolMenu();
  }
}

function positionTerminalToolMenu() {
  if (!terminalToolMenuEl || terminalToolMenuEl.hidden) {
    return;
  }
  positionTerminalToolsMenu();
}

function setTerminalToolMenuExpanded(expanded, { restoreFocus = false } = {}) {
  if (!terminalToolMenuEl) {
    return;
  }
  terminalToolMenuEl.hidden = !expanded;
  if (expanded) {
    positionTerminalToolMenu();
    window.requestAnimationFrame(positionTerminalToolMenu);
    return;
  }
  if (restoreFocus && terminalToolMenuTriggerEl) {
    terminalToolMenuTriggerEl.focus({ preventScroll: true });
  }
}

function openTerminalToolMenu(rootKey, triggerEl) {
  if (!terminalToolMenuEl) {
    return;
  }
  terminalToolMenuTriggerEl = triggerEl || terminalToolsButtonEl || null;
  terminalToolSearchQuery = "";
  const searchInput = document.getElementById("terminal-tool-search");
  if (searchInput) searchInput.value = "";
  terminalToolCurrentRootKey = rootKey;
  terminalToolCurrentParentId = null;
  setTerminalToolMenuStatus();
  renderTerminalToolMenu();
  setTerminalToolMenuExpanded(true);
}

function closeTerminalToolMenu(options = {}) {
  setTerminalToolMenuExpanded(false, options);
  if (terminalToolsMenuEl && !terminalToolsMenuEl.hidden) {
    closeTerminalToolsMenu(options);
  }
}

function waitForTerminalToolDelay(milliseconds) {
  return new Promise((resolve) => window.setTimeout(resolve, milliseconds));
}

async function waitForTerminalToolSessionReady(
  sessionId,
  timeoutMs = TERMINAL_TOOL_SESSION_READY_TIMEOUT_MS,
) {
  const targetSessionId = String(sessionId || "").trim();
  if (!targetSessionId) {
    throw new Error("缺少工作流目标终端。");
  }
  const startedAt = Date.now();
  while (true) {
    const context = ensureTerminalSessionCache().get(targetSessionId);
    if (context?.connectionError) {
      throw new Error(`目标终端连接失败：${context.connectionError}`);
    }
    if (context?.disposed) {
      throw new Error("目标终端已释放，请重新选择终端。");
    }
    if (
      context
      && !context.disconnected
      && terminalContextSocketOpen(context)
      && terminalInitialReplaySettled(context)
    ) {
      return;
    }
    if (Date.now() - startedAt >= timeoutMs) {
      const phase = !context ? "等待终端上下文"
        : !terminalContextSocketOpen(context) || context.disconnected ? "等待连接"
        : context.initialReplayPending ? "等待首屏回放开始"
        : "等待首屏回放写入完成";
      throw new Error(`等待目标终端完成连接和首屏同步超时（${phase}，${Date.now() - startedAt}ms）。`);
    }
    await waitForTerminalToolDelay(100);
  }
}

function terminalToolResumeCommand(context) {
  return extractLatestResumeCommand(readTerminalBufferTailTextFrom(context?.term));
}

function waitForTerminalToolResumeCommand(
  context,
  baselineCommand = "",
  timeoutMs = TERMINAL_TOOL_RESUME_TIMEOUT_MS,
  options = {},
) {
  if (!context?.term || typeof context.term.onRender !== "function") {
    return Promise.reject(new Error("无法监听原终端的 fork 输出。"));
  }
  const baseline = String(baselineCommand || "").trim();
  const allowBaseline = Boolean(options.allowBaseline);
  const initialBufferText = String(options.initialBufferText || "");
  const maxLines = Math.max(1, Number(options.maxLines) || TERMINAL_TOOL_RESUME_SCAN_MAX_LINES);
  return new Promise((resolve, reject) => {
    let settled = false;
    let renderSubscription = null;
    let timeoutId = null;
    const finish = (callback, value) => {
      if (settled) {
        return;
      }
      settled = true;
      if (timeoutId !== null) {
        window.clearTimeout(timeoutId);
      }
      renderSubscription?.dispose?.();
      callback(value);
    };
    const inspect = () => {
      const bufferText = readTerminalBufferTailTextFrom(context?.term, maxLines);
      if (bufferText === initialBufferText) {
        return;
      }
      const command = extractLatestResumeCommand(bufferText);
      if (command && (command !== baseline || allowBaseline)) {
        finish(resolve, command);
      }
    };
    renderSubscription = context.term.onRender(inspect);
    timeoutId = window.setTimeout(() => {
      finish(reject, new Error("等待 /fork 输出 resume 命令超时。"));
    }, timeoutMs);
    inspect();
  });
}

async function renameTerminalForTool(sessionId, nextName, path = state.currentPath) {
  const targetSessionId = String(sessionId || "").trim();
  const session = state.sessions.find((item) => item.id === targetSessionId);
  if (!session || isIdleSession(session.id)) {
    throw new Error("找不到需要更名的目标终端。");
  }
  const renamed = await requestJson(`/api/terminal/sessions/${encodeURIComponent(session.id)}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ path, name: nextName }),
  });
  announceSessionMutation("renamed", renamed);
  state.sessions = sortSessionsByRecentActivity(
    state.sessions.map((item) => item.id === renamed.id ? renamed : item),
  );
  renderSessions();
  return renamed;
}

async function forkTerminalSessionForTool(executionContext) {
  const sourceSessionId = String(
    executionContext.sourceSessionId || executionContext.sessionId || "",
  ).trim();
  const sourceSession = state.sessions.find((item) => item.id === sourceSessionId);
  if (!sourceSession || isIdleSession(sourceSession.id)) {
    throw new Error("当前没有可 fork 的活动终端。");
  }
  const sourceSessionName = String(
    executionContext.sourceSessionName || sourceSession.name || "",
  ).trim();
  const sourcePath = String(
    executionContext.sourcePath ?? sessionPath(sourceSession) ?? state.currentPath,
  );
  if (!sourceSessionName) {
    throw new Error("无法读取原终端名称。");
  }

  await waitForTerminalToolSessionReady(sourceSessionId);
  const sourceContext = ensureTerminalSessionCache().get(sourceSessionId);
  const sourceResumeInfo = sourceContext
    ? extractLatestResumeInfo(readTerminalBufferTailTextFrom(sourceContext?.term))
    : { id: "", program: "codex" };
  const sourceUuid = String(sourceResumeInfo?.id || "").trim();
  if (!sourceUuid) {
    throw new Error("无法从当前终端提取会话 id，请确认 codex/claude 已显示 Session。");
  }

  // 直接复用 codex/claude 的 fork 子命令在新终端里 fork 原会话。
  // 不能在原终端里跑 in-session /fork：那会把原终端也切到 fork 会话，导致两个终端
  // 最终落在同一个会话上。独立的 `codex fork <uuid>` 会在新终端里创建一个全新的、
  // UUID 不同的 fork 会话，原终端保持不变。
  const forkCommand = sourceResumeInfo.program === "claude"
    ? `claude --resume ${sourceUuid} --fork-session`
    : `codex fork ${sourceUuid}`;

  const created = await createSession({
    autoSelect: true,
    suppressLoadingStatus: true,
    pushHistoryOnSelect: true,
    throwOnError: true,
    path: sourcePath,
    origin: "workflow",
    ownerKey: terminalWorkflowOwnerKey(executionContext),
  });
  if (!created) {
    throw new Error("fork 新终端创建失败。");
  }
  executionContext.sessionId = created.id;
  await waitForTerminalToolSessionReady(created.id);
  const forkSent = await sendTerminalAutoTypedInput(forkCommand, {
    sessionId: created.id,
    throwOnError: true,
  });
  if (!forkSent) {
    throw new Error("fork 命令内容为空，未发送。");
  }
  await renameTerminalForTool(created.id, `${sourceSessionName}_fork`, sourcePath);
}

async function executeTerminalToolAction(action, executionContext = {}) {
  switch (action.kind) {
    case "create_terminal": {
      const created = await createSession({
        autoSelect: true,
        suppressLoadingStatus: true,
        pushHistoryOnSelect: true,
        throwOnError: true,
        path: executionContext.sourcePath ?? state.currentPath,
        origin: "workflow",
        ownerKey: terminalWorkflowOwnerKey(executionContext),
      });
      if (!created) {
        throw new Error("新建终端失败。");
      }
      executionContext.sessionId = created.id;
      await waitForTerminalToolSessionReady(created.id);
      return;
    }
    case "fork_session":
      await forkTerminalSessionForTool(executionContext);
      return;
    case "rename_terminal":
      await renameTerminalForTool(
        executionContext.sessionId || state.activeSessionId,
        action.value,
      );
      return;
    case "switch_api_preset": {
      const activePreset = (Array.isArray(state.apiPresets) ? state.apiPresets : [])
        .find((preset) => preset?.active);
      const currentPresetId = String(
        executionContext.presetId || activePreset?.id || "",
      ).trim();
      if (currentPresetId) executionContext.previousPresetId = currentPresetId;
      executionContext.presetId = action.value;
      return;
    }
    case "switch_api_preset_revert": {
      const previousPresetId = String(executionContext.previousPresetId || "").trim();
      if (!previousPresetId) {
        throw new Error("没有记录上一次的预设，无法回切。");
      }
      executionContext.presetId = previousPresetId;
      return;
    }
    case "codex_exec":
    case "codex_terminal": {
      const presetId = String(executionContext.presetId || "").trim();
      if (!presetId) {
        throw new Error("请在 Codex 任务前添加“指定预设”动作。");
      }
      const record = await executeSpecifiedPreset({
        action: "task",
        mode: action.kind === "codex_terminal" ? "terminal" : "exec",
        presetId,
        cwd: executionContext.sourcePath ?? state.currentPath,
        task: action.value,
        onProgress(current) {
          setTerminalToolMenuStatus(
            `${terminalToolActionLabel(action.kind)}：${terminalCodexTaskStatusLabel(current.status)}`,
            "info",
          );
        },
      });
      showTerminalCodexTaskResult(record, { source: "tool" });
      if (record.status !== "succeeded") {
        throw new Error(record.error || `Codex 任务状态：${terminalCodexTaskStatusLabel(record.status)}`);
      }
      return;
    }
    case "codex_launch": {
      const presetResponse = await requestJson(specifiedPresetListEndpoint("codex"));
      const presetList = Array.isArray(presetResponse?.presets) ? presetResponse.presets : [];
      const preset = resolveSpecifiedPreset(presetList, {
        selector: action.preset_selector,
        match: action.preset_match,
      });
      if (!preset?.id) {
        throw new Error(`解析预设 ${action.preset_selector} 失败：未返回有效 ID。`);
      }
      const terminalName = String(action.terminal_name || "").trim();
      const ownerKey = terminalWorkflowOwnerKey(executionContext);
      // A display name is not an ownership key: normal terminals may use the
      // same name and must never be taken over by a workflow launch.
      const existing = await findReusableAgentSession(ownerKey);
      if (existing?.id) {
        await selectSession(existing.id, { connect: true, pushHistory: true });
        executionContext.sessionId = existing.id;
        return;
      }
      const loadInstruction = terminalWorkflowStandbyPrompt(action.value);
      const launchResult = await executeSpecifiedPreset({
        action: "launch",
        agent: "codex",
        presetId: preset.id,
        cwd: action.cwd,
        projectPath: action.project_path,
        temporary: true,
        sessionAction: action.session_action,
        task: loadInstruction,
        terminalName,
        origin: "agent",
        ownerKey,
        quickStart: false,
        launchTerminal: launchTerminalSpecifiedPreset,
      });
      const launched = launchResult?.launchResult;
      if (launched?.id) {
        executionContext.sessionId = launched.id;
      }
      return;
    }
    case "function_command": {
      const command = [
        ...state.terminalFunctionCommands,
        ...state.terminalSlashCommands,
      ].find((item) => item.key === action.command_key);
      if (!command) {
        throw new Error(`找不到功能命令 ${action.command_key}。`);
      }
      const targetSessionId = executionContext.sessionId || state.activeSessionId;
      const sent = await runTerminalFunctionCommand(command, { sessionId: targetSessionId });
      if (!sent) {
        throw new Error(`功能命令 ${command.key || command.label} 执行失败。`);
      }
      return;
    }
    case "run_workflow": {
      const targetEntry = state.terminalToolEntries.find(
        (item) => item.id === action.target_entry_id,
      );
      if (!targetEntry) {
        throw new Error(`找不到嵌套工作流 ${action.target_entry_id}。`);
      }
      if (targetEntry.kind !== "action") {
        throw new Error(`嵌套工作流 ${targetEntry.label} 不是可执行的功能。`);
      }
      if (executionContext.workflowStack?.includes(targetEntry.id)) {
        throw new Error(`工作流嵌套循环：${targetEntry.label} 已在调用链中。`);
      }
      const previousWorkflowStack = executionContext.workflowStack;
      executionContext.workflowStack = [
        ...(executionContext.workflowStack || []),
        targetEntry.id,
      ];
      const previousWorkflowEntryId = executionContext.workflowEntryId;
      executionContext.workflowEntryId = targetEntry.id;
      const priorStatus = terminalToolMenuStatusEl?.textContent || "";
      try {
        for (let nestedIndex = 0; nestedIndex < targetEntry.actions.length; nestedIndex += 1) {
          const nestedAction = materializeTerminalWorkflowAction(
            targetEntry.actions[nestedIndex],
            executionContext.runtimeParameters,
          );
          const nestedProgress = `${targetEntry.label}（嵌套）：${nestedIndex + 1}/${targetEntry.actions.length} ${terminalToolActionLabel(nestedAction.kind)}`;
          setTerminalToolMenuStatus(nestedProgress, "info");
          await executeTerminalToolAction(nestedAction, executionContext);
        }
      } finally {
        executionContext.workflowEntryId = previousWorkflowEntryId;
        executionContext.workflowStack = previousWorkflowStack;
      }
      if (priorStatus) {
        setTerminalToolMenuStatus(priorStatus, "info");
      }
      return;
    }
    case "wait":
      await waitForTerminalToolDelay(Math.round(action.seconds * 1000));
      return;
    case "send_command": {
      const sessionId = executionContext.sessionId || state.activeSessionId;
      await waitForTerminalToolSessionReady(sessionId);
      const sent = await sendTerminalAutoTypedInput(action.value, {
        sessionId,
        throwOnError: true,
      });
      if (!sent) {
        throw new Error("工作流命令内容为空，未发送。");
      }
      return;
    }
    default:
      throw new Error(`不支持的工作流动作：${action.kind}`);
  }
}

async function executeTerminalToolEntry(entry, resumeState = null) {
  if (terminalToolExecutionRunning || entry.kind !== "action") {
    return;
  }
  closeTerminalToolMenu();
  terminalToolExecutionRunning = true;
  const sourceSession = state.sessions.find((item) => item.id === state.activeSessionId);
  let definitionStamp = terminalWorkflowDefinitionStamp(entry);
  let runId = "";
  let startIndex = 0;
  let actions = [];
  let executionContext = null;
  let runtimeParameters = null;
  let completion = "";
  let startedAt = 0;
  try {
    const plan = WebClxTerminalSettings.inspectTerminalWorkflow(entry, state.terminalToolEntries);
    completion = plan.completion;
    definitionStamp = terminalWorkflowDefinitionStamp(entry, plan.steps);
    if (resumeState) {
      if (resumeState.entryId !== entry.id || resumeState.definitionStamp !== definitionStamp) {
        throw new Error("工作流定义已变化，不能从失败步骤继续；请重新开始整条流程。");
      }
      runId = resumeState.runId;
      startIndex = resumeState.nextIndex;
      actions = resumeState.actions;
      executionContext = resumeState.executionContext;
      runtimeParameters = resumeState.runtimeParameters;
      startedAt = resumeState.startedAt;
    } else {
      runId = `run_${Date.now()}_${Math.random().toString(16).slice(2, 8)}`;
      startedAt = Date.now();
      runtimeParameters = await prepareTerminalWorkflowRuntimeParameters(entry, plan);
      if (!runtimeParameters) {
        recordTerminalWorkflowRun({
          id: runId,
          entryId: entry.id,
          entryLabel: entry.label,
          status: "cancelled",
          progress: "0/1",
          startedAt,
          finishedAt: Date.now(),
          definitionStamp,
        });
        throw new Error("已取消工作流执行。");
      }
      actions = plan.steps.map((action) => materializeTerminalWorkflowAction(action, runtimeParameters));
      executionContext = {
        sessionId: sourceSession?.id || "",
        sourceSessionId: sourceSession?.id || "",
        sourceSessionName: sourceSession?.name || "",
        sourcePath: sourceSession ? sessionPath(sourceSession) : state.currentPath,
        presetId: "",
        deferPresetApply: plan.steps.some(
          (action) => action.kind === "codex_exec" || action.kind === "codex_terminal",
        ),
        workflowStack: [entry.id],
        workflowEntryId: entry.id,
        runtimeParameters,
      };
    }
    for (let index = startIndex; index < actions.length; index += 1) {
      const action = actions[index];
      executionContext.workflowEntryId = action.workflow_entry_id || entry.id;
      const progress = `${entry.label}：${index + 1}/${actions.length} ${terminalToolActionLabel(action.kind)}`;
      setTerminalToolMenuStatus(progress, "info");
      updateStatus(`工作流“${progress}”`, "info");
      try {
        await executeTerminalToolAction(action, executionContext);
      } catch (error) {
        terminalToolFailedExecution = {
          runId,
          entryId: entry.id,
          definitionStamp,
          nextIndex: index,
          actions,
          executionContext,
          runtimeParameters,
          startedAt,
        };
        recordTerminalWorkflowRun({
          id: runId,
          entryId: entry.id,
          entryLabel: entry.label,
          status: "failed",
          progress: `${index + 1}/${actions.length}`,
          startedAt,
          finishedAt: Date.now(),
          definitionStamp,
        });
        throw error;
      }
    }
    terminalToolFailedExecution = null;
    rememberTerminalToolRecent(entry.id);
    recordTerminalWorkflowRun({
      id: runId,
      entryId: entry.id,
      entryLabel: entry.label,
      status: "completed",
      progress: `${actions.length}/${actions.length}`,
      startedAt,
      finishedAt: Date.now(),
      definitionStamp,
    });
    setTerminalToolMenuStatus(`${entry.label}：${completion}`, "ok");
    updateStatus(`工作流“${entry.label}”：${completion}`, "ok");
  } catch (error) {
    const message = error?.message || "执行失败。";
    setTerminalToolMenuStatus(`${entry.label}：${message}`, "warn");
    updateStatus(`工作流“${entry.label}”执行失败：${message}`, "warn");
  } finally {
    terminalToolExecutionRunning = false;
    if (terminalToolMenuEl && !terminalToolMenuEl.hidden) {
      renderTerminalToolMenu();
    }
  }
}

function handleTerminalToolMenuClick(event) {
  const target = event.target instanceof Element ? event.target : null;
  const runButton = target?.closest("[data-terminal-workflow-run-action]");
  if (runButton && !terminalToolExecutionRunning) {
    const action = runButton.dataset.terminalWorkflowRunAction;
    if (action === "clear") {
      clearTerminalWorkflowRunRecords();
      terminalToolFailedExecution = null;
      renderTerminalToolMenu();
      return;
    }
    const record = readTerminalWorkflowRunRecords()
      .find((item) => item.id === runButton.dataset.terminalWorkflowRunId);
    const entry = record ? terminalToolEntryById(record.entryId) : null;
    if (!record || entry?.kind !== "action") {
      return;
    }
    if (action === "retry") {
      if (terminalToolFailedExecution?.runId !== record.id) {
        return;
      }
      executeTerminalToolEntry(entry, terminalToolFailedExecution);
      return;
    }
    if (action === "restart") {
      executeTerminalToolEntry(entry);
      return;
    }
  }
  const button = target?.closest("[data-terminal-tool-entry]");
  if (!button || terminalToolExecutionRunning) {
    return;
  }
  const entry = terminalToolEntryById(button.dataset.terminalToolEntry);
  if (!entry) {
    return;
  }
  if (entry.kind === "folder") {
    terminalToolCurrentParentId = entry.id;
    setTerminalToolMenuStatus();
    renderTerminalToolMenu();
    return;
  }
  executeTerminalToolEntry(entry);
}

function navigateTerminalToolMenuBack() {
  if (!terminalToolCurrentParentId || terminalToolExecutionRunning) {
    return;
  }
  const current = terminalToolEntryById(terminalToolCurrentParentId);
  terminalToolCurrentParentId = current?.parent_id || null;
  setTerminalToolMenuStatus();
  renderTerminalToolMenu();
}
