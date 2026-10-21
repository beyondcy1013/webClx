let terminalSpecifiedTaskId = "";
let terminalSpecifiedTaskTrigger = null;
let terminalSpecifiedTaskLastRecord = null;
let terminalSpecifiedTaskSourcePath = "";
let terminalSpecifiedTaskSourceName = "";
let terminalSpecifiedTaskSessionFieldPinned = false;
let terminalSpecifiedTaskAgentPinned = false;
let terminalSpecifiedTaskTerminalNameOverride = "";
let terminalSpecifiedTaskNamingAction = "";
let terminalSpecifiedTaskLaunchTerminal = null;
let terminalSpecifiedTaskPageStatus = null;
let terminalSpecifiedTaskBrowsePath = "";
let terminalSpecifiedTaskBrowseDisplayPath = "";
let terminalSpecifiedTaskImportedTitle = "";
let terminalSpecifiedTaskQuestionSessionId = "";
const terminalSpecifiedTaskRecentPresets = new Map();
let terminalSpecifiedTaskNamingSessions = null;
let terminalSpecifiedTaskNamingPath = "";
let terminalSpecifiedTaskNamingLoading = false;
let terminalSpecifiedTaskNamingRequestToken = 0;
let terminalSpecifiedTaskFutureMode = false;
let terminalSpecifiedTaskFutureEdit = null;
let terminalSpecifiedTaskAgentCommands = [];

function terminalSpecifiedTaskDefaultAgentCommands() {
  if (typeof cloneDefaultTerminalAgentCommands === "function") {
    return cloneDefaultTerminalAgentCommands();
  }
  return [];
}

function terminalSpecifiedTaskNormalizePath(pathValue) {
  if (typeof normalizeTerminalPath === "function") {
    return normalizeTerminalPath(pathValue);
  }
  const rawPath = String(pathValue || "").trim();
  if (!rawPath || rawPath === "/") {
    return "";
  }
  const parts = rawPath.split("/").filter(Boolean);
  return rawPath.startsWith("/") ? `/${parts.join("/")}` : parts.join("/");
}

function terminalSpecifiedTaskSessionPath(session) {
  if (typeof sessionPath === "function") {
    return sessionPath(session);
  }
  return terminalSpecifiedTaskNormalizePath(session?.path || "");
}

function terminalSpecifiedTaskDisplayPath(pathValue) {
  if (typeof terminalDisplayPath === "function") {
    return terminalDisplayPath(pathValue);
  }
  const normalized = terminalSpecifiedTaskNormalizePath(pathValue);
  return !normalized ? "/" : normalized.startsWith("/") ? normalized : `/${normalized}`;
}

function reportTerminalSpecifiedTaskPageStatus(message, tone) {
  if (typeof terminalSpecifiedTaskPageStatus === "function") {
    terminalSpecifiedTaskPageStatus(message, tone);
    return;
  }
  updateStatus(message, tone);
}

function terminalSpecifiedTaskDom() {
  return {
    button: document.getElementById("terminal-specified-task-button"),
    dialog: document.getElementById("terminal-specified-task-dialog"),
    form: document.getElementById("terminal-specified-task-form"),
    title: document.getElementById("terminal-specified-task-title"),
    preset: document.getElementById("terminal-specified-task-preset"),
    agentList: document.getElementById("terminal-specified-task-agent-list"),
    presetField: document.getElementById("terminal-specified-task-preset-field"),
    sessionOnly: document.getElementById("terminal-specified-task-session-only"),
    sessionOnlyLabel: document.querySelector(".terminal-specified-task-session-only-label"),
    otherCommandField: document.getElementById("terminal-specified-task-other-command-field"),
    otherCommand: document.getElementById("terminal-specified-task-other-command"),
    task: document.getElementById("terminal-specified-task-text"),
    timeout: document.getElementById("terminal-specified-task-timeout"),
    timeoutField: document.getElementById("terminal-specified-task-timeout-field"),
    schedule: document.getElementById("terminal-specified-task-schedule"),
    scheduleField: document.getElementById("terminal-specified-task-schedule-field"),
    path: document.getElementById("terminal-specified-task-path"),
    workspaceField: document.getElementById("terminal-specified-task-workspace-field"),
    workspace: document.getElementById("terminal-specified-task-workspace"),
    favorite: document.getElementById("terminal-specified-task-favorite"),
    workspaceBrowse: document.getElementById("terminal-specified-task-workspace-browse"),
    pathDialog: document.getElementById("terminal-specified-task-path-dialog"),
    pathForm: document.getElementById("terminal-specified-task-path-form"),
    pathInput: document.getElementById("terminal-specified-task-path-input"),
    pathBody: document.getElementById("terminal-specified-task-path-body"),
    pathSelected: document.getElementById("terminal-specified-task-path-selected"),
    pathCancel: document.getElementById("terminal-specified-task-path-cancel"),
    fixedOptions: document.getElementById("terminal-specified-task-fixed-options"),
    sessionIdField: document.getElementById("terminal-specified-task-session-id-field"),
    sessionId: document.getElementById("terminal-specified-task-session-id"),
    agyContinueLabel: document.getElementById("terminal-specified-task-agy-continue-label"),
    agyContinue: document.getElementById("terminal-specified-task-agy-continue"),
    autoCompact: document.getElementById("terminal-specified-task-auto-compact"),
    groupField: document.getElementById("terminal-specified-task-group-field"),
    joinGroup: document.getElementById("terminal-specified-task-join-group"),
    group: document.getElementById("terminal-specified-task-group"),
    groupRole: document.getElementById("terminal-specified-task-group-role"),
    newGroupField: document.getElementById("terminal-specified-task-new-group-field"),
    newGroup: document.getElementById("terminal-specified-task-new-group"),
    newGroupCount: document.getElementById("terminal-specified-task-new-group-count"),
    terminalName: document.getElementById("terminal-specified-task-terminal-name"),
    terminalNameSuffixField: document.getElementById("terminal-specified-task-terminal-name-suffix-field"),
    terminalNameSuffix: document.getElementById("terminal-specified-task-terminal-name-suffix"),
    useOriginalSuffix: document.getElementById("terminal-specified-task-use-original-suffix"),
    importTitle: document.getElementById("terminal-specified-task-import-title"),
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
  const value = document.querySelector('input[name="terminal-specified-task-agent"]:checked')?.value;
  return terminalSpecifiedTaskAgentCommand(value)?.key || "codex";
}

function terminalSpecifiedTaskAgentCommand(agent) {
  const normalized = String(agent || "").trim().toLowerCase();
  const commands = terminalSpecifiedTaskAgentCommands.length
    ? terminalSpecifiedTaskAgentCommands
    : terminalSpecifiedTaskDefaultAgentCommands();
  return commands.find((item) => item.key === normalized) || null;
}

function terminalSpecifiedTaskIsExternalAgent(agent = terminalSpecifiedTaskAgent()) {
  return agent !== "codex" && agent !== "claude";
}

function terminalSpecifiedTaskHasAccounts(agent = terminalSpecifiedTaskAgent()) {
  return ["codex", "claude", "codebuddy", "devin", "agy"].includes(agent);
}

function terminalSpecifiedTaskAccountLabel(agent = terminalSpecifiedTaskAgent()) {
  switch (agent) {
    case "codex": return "Codex 账号";
    case "claude": return "Claude 账号";
    case "codebuddy": return "CodeBuddy 账号";
    case "devin": return "Devin 账号";
    case "agy": return "AGY 账号";
    default: return `${agent} 账号`;
  }
}

async function applyTerminalSpecifiedAgentAccount(agent, accountId, presetKind) {
  if (agent === "codex") {
    const kind = presetKind === "oauth" ? "oauth" : "api";
    return await requestJson(
      `/api/auth/codex-accounts/${kind}/${encodeURIComponent(accountId)}/apply`,
      { method: "PUT" },
    );
  }
  if (agent === "claude") {
    return await requestJson(
      `/api/auth/claude-presets/${encodeURIComponent(accountId)}/apply`,
      { method: "PUT" },
    );
  }
  if (agent === "codebuddy") {
    return await requestJson(
      `/api/auth/codebuddy-accounts/${encodeURIComponent(accountId)}/switch`,
      { method: "PUT" },
    );
  }
  if (agent === "devin") {
    return await requestJson(
      `/api/auth/devin-accounts/${encodeURIComponent(accountId)}/switch`,
      { method: "PUT" },
    );
  }
  if (agent === "agy") {
    return await requestJson(
      `/api/agy/presets/${encodeURIComponent(accountId)}/apply`,
      { method: "PUT" },
    );
  }
}

function terminalSpecifiedTaskAgentLaunchCommand(sessionAction, sessionId) {
  const agent = terminalSpecifiedTaskAgent();
  if (!terminalSpecifiedTaskIsExternalAgent(agent)) return "";
  if (agent === "other") {
    const rawCommand = String(terminalSpecifiedTaskDom().otherCommand?.value || "").trim();
    if (!rawCommand && sessionAction !== "new") return "";
    if (sessionAction === "resume" || sessionAction === "fork") {
      const normalized = String(sessionId || "").trim();
      if (!normalized) return "";
      const quoted = shellQuoteSpecifiedPresetArgument(normalized);
      if (rawCommand.includes("{session_id}")) {
        return rawCommand.split("{session_id}").join(quoted);
      }
      return `${rawCommand} ${quoted}`.trim();
    }
    return rawCommand;
  }
  const configured = terminalSpecifiedTaskAgentCommand(agent);
  if (!configured) return "";
  if (sessionAction === "resume" || sessionAction === "fork") {
    if (agent === "agy" && sessionAction === "resume") {
      const dom = terminalSpecifiedTaskDom();
      if (dom.agyContinue?.checked) {
        return "agy -c";
      }
    }
    const normalized = String(sessionId || "").trim();
    const idPattern = typeof WEBCLX_EXTERNAL_AGENT_SESSION_ID_PATTERN !== "undefined"
      ? WEBCLX_EXTERNAL_AGENT_SESSION_ID_PATTERN
      : WEBCLX_AGENT_SESSION_ID_PATTERN;
    if (!idPattern.test(normalized)) return "";
    if (agent === "agy") {
      return `agy --conversation ${shellQuoteSpecifiedPresetArgument(normalized)}`;
    }
    const template = configured.resume_command || configured.new_command;
    if (!template.includes("{session_id}")) {
      return `${template} ${shellQuoteSpecifiedPresetArgument(normalized)}`.trim();
    }
    return template.split("{session_id}").join(shellQuoteSpecifiedPresetArgument(normalized));
  }
  return configured.new_command;
}

function terminalSpecifiedTaskSetAgentCommands(commands) {
  terminalSpecifiedTaskAgentCommands = typeof normalizeTerminalAgentCommands === "function"
    ? normalizeTerminalAgentCommands(
      commands,
      terminalSpecifiedTaskDefaultAgentCommands(),
    )
    : (Array.isArray(commands) && commands.length ? commands : terminalSpecifiedTaskDefaultAgentCommands());
  const agent = terminalSpecifiedTaskAgent();
  if (!terminalSpecifiedTaskAgentCommands.some((item) => item.key === agent)) {
    const fallback = terminalSpecifiedTaskAgentCommands.find((item) => item.key === "codex")
      || terminalSpecifiedTaskAgentCommands[0];
    terminalSpecifiedTaskSelectAgent(fallback?.key || "codex");
  }
  renderTerminalSpecifiedTaskAgents();
  syncTerminalSpecifiedTaskForm();
}

function renderTerminalSpecifiedTaskAgents() {
  const dom = terminalSpecifiedTaskDom();
  if (!dom.agentList) return;
  const currentAgent = terminalSpecifiedTaskAgent();
  dom.agentList.textContent = "";
  const commands = terminalSpecifiedTaskAgentCommands.length
    ? terminalSpecifiedTaskAgentCommands
    : terminalSpecifiedTaskDefaultAgentCommands();
  commands.forEach((command) => {
    const label = document.createElement("label");
    const input = document.createElement("input");
    input.type = "radio";
    input.name = "terminal-specified-task-agent";
    input.value = command.key;
    input.checked = command.key === currentAgent;
    input.addEventListener("change", async () => {
      syncTerminalSpecifiedTaskForm();
      await loadTerminalSpecifiedTaskPresets();
    });
    const text = document.createElement("span");
    text.textContent = command.label || command.key;
    label.append(input, text);
    dom.agentList.append(label);
  });
}

function terminalSpecifiedTaskSelectAgent(agent) {
  const command = terminalSpecifiedTaskAgentCommand(agent);
  const selectedKey = command?.key || "codex";
  document
    .querySelectorAll('input[name="terminal-specified-task-agent"]')
    .forEach((input) => {
      input.checked = input.value === selectedKey;
    });
  return selectedKey;
}

function terminalSpecifiedTaskMode() {
  return document.querySelector('input[name="terminal-specified-task-mode"]:checked')?.value || "fixed";
}

function terminalSpecifiedTaskTemporary() {
  const dom = terminalSpecifiedTaskDom();
  if (dom.sessionOnly) {
    return dom.sessionOnly.checked === true;
  }
  const scope = document.querySelector(
    'input[name="terminal-specified-task-preset-scope"]:checked',
  )?.value;
  return scope !== "global";
}

function terminalSpecifiedTaskSelectedPresetKind() {
  const { preset } = terminalSpecifiedTaskDom();
  const selectedKind = String(preset?.selectedOptions?.[0]?.dataset?.kind || "").trim();
  if (terminalSpecifiedTaskAgent() === "claude") {
    return "claude";
  }
  return selectedKind === "oauth" ? "oauth" : "api";
}

function terminalSpecifiedTaskSessionAction() {
  return document.querySelector('input[name="terminal-specified-task-session-action"]:checked')?.value || "new";
}

function terminalSpecifiedTaskSetQuestionSessionId(sessionId) {
  terminalSpecifiedTaskQuestionSessionId = String(sessionId || "").trim();
  const dom = terminalSpecifiedTaskDom();
  if (dom.terminalNameSuffix && !dom.terminalNameSuffix.value.trim()) {
    dom.terminalNameSuffix.value = "调用skill读取codex session并回答问题";
  }
  syncTerminalSpecifiedTaskImportedTitle();
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
  return session ? terminalSpecifiedTaskSessionPath(session) : state.currentPath;
}

function terminalSpecifiedTaskResolveWorkspacePath(pathValue) {
  const rawPath = String(pathValue || "").trim();
  if (rawPath.startsWith("/")) {
    return terminalSpecifiedTaskDisplayPath(rawPath);
  }
  const baseParts = String(state.workspaceDir || "")
    .split("/")
    .filter(Boolean);
  String(rawPath || state.currentPath || "")
    .split("/")
    .filter(Boolean)
    .forEach((part) => {
      if (part === ".") {
        return;
      }
      if (part === "..") {
        baseParts.pop();
        return;
      }
      baseParts.push(part);
    });
  return `/${baseParts.join("/")}` || "/";
}

// Workspace visits and terminal opens are separate persisted recency sources.
let terminalSpecifiedTaskWorkspaceSessions = null;
let terminalSpecifiedTaskWorkspaceRefreshToken = 0;

async function refreshTerminalSpecifiedTaskWorkspaceHistory() {
  if (typeof requestJson !== "function") return;
  const token = ++terminalSpecifiedTaskWorkspaceRefreshToken;
  const results = await Promise.allSettled([
    requestJson("/api/settings"),
    requestJson("/api/terminal/sessions?all=true&history=true"),
  ]);
  if (token !== terminalSpecifiedTaskWorkspaceRefreshToken) return;
  if (results[0].status === "fulfilled" && Array.isArray(results[0].value?.workspace_history)) {
    state.workspaceHistory = results[0].value.workspace_history;
  }
  terminalSpecifiedTaskWorkspaceSessions = results[1].status === "fulfilled"
    && Array.isArray(results[1].value?.sessions)
    ? results[1].value.sessions
    : null;
  results.forEach((result) => {
    if (result.status === "rejected") {
      console.warn("刷新新开工作目录最近使用记录失败", result.reason);
    }
  });
}

function terminalSpecifiedTaskWorkspaceChoices(sourcePath) {
  const pinnedPath = terminalSpecifiedTaskResolveWorkspacePath(sourcePath || terminalSpecifiedTaskCurrentPath());
  const byPath = new Map();
  const timestamp = (value) => {
    const number = Number(value);
    return Number.isFinite(number) && number > 0 ? number : 0;
  };
  const append = (pathValue, lastOpenedAt = 0) => {
    if (typeof pathValue !== "string" || !pathValue.trim()) return;
    const path = terminalSpecifiedTaskResolveWorkspacePath(pathValue);
    byPath.set(path, Math.max(byPath.get(path) || 0, timestamp(lastOpenedAt)));
  };
  (Array.isArray(state.workspaceHistory) ? state.workspaceHistory : []).forEach((item) => {
    append(typeof item === "string" ? item : item?.path, item?.last_opened_at);
  });
  const sessions = terminalSpecifiedTaskWorkspaceSessions
    || (Array.isArray(state.sessions) ? state.sessions : []);
  sessions.forEach((session) => {
    const path = terminalSpecifiedTaskSessionPath(session);
    append(path || state.workspaceDir, session.last_opened_at || session.created_at);
  });
  // Selection is preserved by the renderer, independently of sorting.
  append(sourcePath || terminalSpecifiedTaskCurrentPath() || state.workspaceDir);
  return [...byPath.entries()]
    .sort((left, right) => Number(right[0] === pinnedPath) - Number(left[0] === pinnedPath) || right[1] - left[1] || left[0].localeCompare(right[0]))
    .map(([path]) => path);
}

function terminalSpecifiedTaskWorkspaceLabel(pathValue) {
  const path = terminalSpecifiedTaskResolveWorkspacePath(pathValue);
  const name = path.split("/").filter(Boolean).pop() || "/";
  return `${name} · ${path}`;
}

function terminalSpecifiedTaskFavoriteChoices() {
  const seen = new Set();
  return (Array.isArray(state.favoritePaths) ? state.favoritePaths : [])
    .filter((item) => typeof item === "string" || !item?.kind || item.kind === "dir")
    .map((item) => terminalSpecifiedTaskResolveWorkspacePath(
      typeof item === "string" ? item : item?.path,
    ))
    .filter((path) => {
      if (!path || seen.has(path)) {
        return false;
      }
      seen.add(path);
      return true;
    });
}

function terminalSpecifiedTaskGroups() {
  if (typeof terminalMessageGroups !== "function") return [];
  return terminalMessageGroups(state.sessions);
}

function renderTerminalSpecifiedTaskGroupChoices() {
  const dom = terminalSpecifiedTaskDom();
  if (!dom.group) return;
  const groups = terminalSpecifiedTaskGroups();
  const selectedToken = String(dom.group.value || "");
  dom.group.replaceChildren();
  const empty = document.createElement("option");
  empty.value = "";
  empty.textContent = groups.length ? "选择当前群组" : "没有可用群组";
  dom.group.append(empty);
  groups.forEach((group) => {
    const option = document.createElement("option");
    option.value = group.token;
    option.textContent = `${terminalMessageGroupLabel(group)}（${group.members.length}）`;
    dom.group.append(option);
  });
  dom.group.value = groups.some((group) => group.token === selectedToken)
    ? selectedToken
    : (dom.group.options[1]?.value || "");
}

function renderTerminalSpecifiedTaskFavoriteChoices() {
  const { favorite } = terminalSpecifiedTaskDom();
  if (!favorite) {
    return;
  }
  favorite.replaceChildren();
  const paths = terminalSpecifiedTaskFavoriteChoices();
  const placeholder = document.createElement("option");
  placeholder.value = "";
  placeholder.textContent = paths.length > 0 ? "跳转收藏" : "暂无收藏路径";
  favorite.append(placeholder);
  paths.forEach((path) => {
    const option = document.createElement("option");
    option.value = path;
    option.textContent = path;
    option.title = path;
    favorite.append(option);
  });
  favorite.value = "";
}

function renderTerminalSpecifiedTaskWorkspaceChoices() {
  const { workspace } = terminalSpecifiedTaskDom();
  if (!workspace) {
    return;
  }
  const selectedPath = terminalSpecifiedTaskResolveWorkspacePath(
    terminalSpecifiedTaskSourcePath || terminalSpecifiedTaskCurrentPath(),
  );
  workspace.replaceChildren();
  terminalSpecifiedTaskWorkspaceChoices(selectedPath).forEach((path) => {
    const option = document.createElement("option");
    option.value = path;
    option.textContent = terminalSpecifiedTaskWorkspaceLabel(path);
    option.title = path;
    option.selected = path === selectedPath;
    workspace.append(option);
  });
}

async function setTerminalSpecifiedTaskSourcePath(pathValue) {
  const displayPath = terminalSpecifiedTaskDisplayPath(pathValue);
  terminalSpecifiedTaskSourcePath = terminalSpecifiedTaskResolveWorkspacePath(pathValue);
  const requestToken = ++terminalSpecifiedTaskNamingRequestToken;
  terminalSpecifiedTaskNamingPath = terminalSpecifiedTaskSourcePath;
  terminalSpecifiedTaskNamingSessions = null;
  terminalSpecifiedTaskNamingLoading = typeof requestJson === "function";
  const dom = terminalSpecifiedTaskDom();
  if (dom.path) {
    dom.path.textContent = displayPath;
  }
  if (dom.workspace) {
    const hasOption = Array.from(dom.workspace.options || []).some(
      (option) => option.value === terminalSpecifiedTaskSourcePath,
    );
    if (!hasOption) {
      renderTerminalSpecifiedTaskWorkspaceChoices();
    }
    dom.workspace.value = terminalSpecifiedTaskSourcePath;
  }
  renderTerminalSpecifiedTaskTerminalNamePreview();
  if (!terminalSpecifiedTaskNamingLoading) {
    return;
  }
  try {
    const relativePath = terminalSpecifiedTaskRelativeBrowsePath(terminalSpecifiedTaskSourcePath);
    const response = await requestJson(
      `/api/terminal/sessions?path=${encodeURIComponent(relativePath)}`,
    );
    if (
      requestToken !== terminalSpecifiedTaskNamingRequestToken
      || terminalSpecifiedTaskNamingPath !== terminalSpecifiedTaskSourcePath
    ) {
      return;
    }
    terminalSpecifiedTaskNamingSessions = Array.isArray(response?.sessions)
      ? response.sessions.slice()
      : [];
  } catch {
    if (requestToken !== terminalSpecifiedTaskNamingRequestToken) {
      return;
    }
    terminalSpecifiedTaskNamingSessions = null;
  } finally {
    if (requestToken === terminalSpecifiedTaskNamingRequestToken) {
      terminalSpecifiedTaskNamingLoading = false;
      renderTerminalSpecifiedTaskTerminalNamePreview();
    }
  }
}

function terminalSpecifiedTaskRelativeBrowsePath(pathValue) {
  const target = terminalSpecifiedTaskResolveWorkspacePath(pathValue)
    .split("/")
    .filter(Boolean);
  const base = terminalSpecifiedTaskResolveWorkspacePath(state.workspaceDir)
    .split("/")
    .filter(Boolean);
  let commonLength = 0;
  while (
    commonLength < base.length
    && commonLength < target.length
    && base[commonLength] === target[commonLength]
  ) {
    commonLength += 1;
  }
  return [
    ...new Array(base.length - commonLength).fill(".."),
    ...target.slice(commonLength),
  ].join("/");
}

function terminalSpecifiedTaskPathRow(label, path, kind) {
  const row = document.createElement("button");
  row.className = "path-browse-dialog-entry";
  row.type = "button";
  row.dataset.kind = kind;
  row.dataset.path = path;
  const icon = document.createElement("span");
  icon.className = "path-browse-dialog-entry-icon";
  icon.textContent = kind === "parent" ? "↑" : "/";
  const text = document.createElement("span");
  text.textContent = label;
  row.append(icon, text);
  row.addEventListener("click", () => loadTerminalSpecifiedTaskBrowsePath(path));
  return row;
}

async function loadTerminalSpecifiedTaskBrowsePath(pathValue, fallbackToDefault = false) {
  const dom = terminalSpecifiedTaskDom();
  if (!dom.pathBody) {
    return;
  }
  const requestPath = String(pathValue || "").startsWith("/")
    ? terminalSpecifiedTaskRelativeBrowsePath(pathValue)
    : String(pathValue || "");
  terminalSpecifiedTaskBrowseDisplayPath = "";
  if (dom.pathSelected) dom.pathSelected.textContent = "";
  dom.pathBody.textContent = "加载中…";
  try {
    const query = requestPath ? `?path=${encodeURIComponent(requestPath)}` : "";
    const directory = await requestJson(`/api/entries${query}`);
    terminalSpecifiedTaskBrowsePath = String(directory?.path || "");
    terminalSpecifiedTaskBrowseDisplayPath = String(
      directory?.display_path || terminalSpecifiedTaskResolveWorkspacePath(requestPath),
    );
    delete dom.pathBody.dataset.tone;
    if (dom.pathInput) {
      dom.pathInput.value = terminalSpecifiedTaskBrowseDisplayPath;
    }
    if (dom.pathSelected) {
      dom.pathSelected.textContent = terminalSpecifiedTaskBrowseDisplayPath;
    }
    dom.pathBody.replaceChildren();
    if (directory?.parent_path !== null && directory?.parent_path !== undefined) {
      dom.pathBody.append(
        terminalSpecifiedTaskPathRow("..", String(directory.parent_path || ""), "parent"),
      );
    }
    const directories = (Array.isArray(directory?.entries) ? directory.entries : [])
      .filter((entry) => entry?.kind === "dir");
    if (directories.length === 0) {
      const empty = document.createElement("div");
      empty.className = "meta-text terminal-specified-task-path-empty";
      empty.textContent = "此目录下没有子目录。";
      dom.pathBody.append(empty);
      return;
    }
    directories.forEach((entry) => {
      dom.pathBody.append(
        terminalSpecifiedTaskPathRow(String(entry.name || entry.path), String(entry.path || ""), "dir"),
      );
    });
  } catch (error) {
    if (fallbackToDefault && requestPath && /(?:路径|目录)不存在/.test(String(error?.message || error))) {
      return loadTerminalSpecifiedTaskBrowsePath("");
    }
    dom.pathBody.textContent = `读取目录失败：${error?.message || error}`;
    dom.pathBody.dataset.tone = "warn";
  }
}

function openTerminalSpecifiedTaskPathDialog() {
  const dom = terminalSpecifiedTaskDom();
  if (!dom.pathDialog) {
    return;
  }
  terminalSpecifiedTaskBrowsePath = "";
  terminalSpecifiedTaskBrowseDisplayPath = terminalSpecifiedTaskSourcePath;
  dom.pathDialog.showModal();
  return loadTerminalSpecifiedTaskBrowsePath(terminalSpecifiedTaskSourcePath, true);
}

function closeTerminalSpecifiedTaskPathDialog() {
  const { pathDialog, workspaceBrowse } = terminalSpecifiedTaskDom();
  if (pathDialog?.open) {
    pathDialog.close();
  }
  workspaceBrowse?.focus?.({ preventScroll: true });
}

function terminalSpecifiedTaskAutoNameIndices(name) {
  const indices = [];
  const pattern = /[_#](\d+)(?=$|[\s_])/g;
  const normalized = String(name || "");
  let match;
  while ((match = pattern.exec(normalized)) !== null) {
    const index = Number(match[1]);
    if (Number.isSafeInteger(index) && index > 0) {
      indices.push(index);
    }
  }
  return indices;
}

function terminalSpecifiedTaskAutoNamePreview() {
  const targetPath = terminalSpecifiedTaskNormalizePath(
    terminalSpecifiedTaskSourcePath || terminalSpecifiedTaskCurrentPath(),
  );
  if (terminalSpecifiedTaskNamingPath === targetPath && terminalSpecifiedTaskNamingLoading) {
    return "正在计算编号…";
  }
  if (
    terminalSpecifiedTaskNamingPath === targetPath
    && !Array.isArray(terminalSpecifiedTaskNamingSessions)
  ) {
    return "创建时自动分配";
  }
  const sessions = terminalSpecifiedTaskNamingPath === targetPath
    ? terminalSpecifiedTaskNamingSessions
    : Array.isArray(state.sessions) ? state.sessions : [];
  const usedNames = new Set();
  const usedIndices = new Set();

  for (const session of sessions) {
    const name = String(session?.name || "").trim();
    if (name) {
      usedNames.add(name);
    }
    if (
      terminalSpecifiedTaskNormalizePath(
        terminalSpecifiedTaskResolveWorkspacePath(terminalSpecifiedTaskSessionPath(session)),
      ) === targetPath
    ) {
      for (const index of terminalSpecifiedTaskAutoNameIndices(name)) {
        usedIndices.add(index);
      }
    }
  }

  const highestIndex = usedIndices.size > 0 ? Math.max(...usedIndices) : 0;
  let index = highestIndex + 1;
  if (index > 100) {
    index = 10;
  }
  index = Math.max(1, index);
  const candidateCount = index < 10 ? 100 - index + 1 : 100 - 10 + 1;
  const pathParts = targetPath.split("/").filter(Boolean);
  const directoryLabel = String(pathParts[pathParts.length - 1] || "根目录")
    .replace(/[\u0000-\u001f\u007f]/g, "")
    .trim() || "根目录";

  for (let attempt = 0; attempt < candidateCount; attempt += 1) {
    const candidate = `${directoryLabel}_${index}`;
    if (!usedNames.has(candidate) && !usedIndices.has(index)) {
      return candidate;
    }
    index = index === 100 ? 10 : index + 1;
  }
  return "编号已用完";
}

function terminalSpecifiedTaskNameWithoutAutoIndices(name) {
  return String(name || "")
    .trim()
    .replace(/[_#]\d+(?=$|[\s_])/g, "")
    .replace(/_{2,}/g, "_")
    .replace(/\s{2,}/g, " ")
    .replace(/^[_\s]+|[_\s]+$/g, "");
}

function terminalSpecifiedTaskExtractOriginalSuffix(name) {
  const normalized = String(name || "").trim();
  const marker = /[_#]\d+(?=$|[\s_])/.exec(normalized);
  if (!marker) {
    return "";
  }
  return normalized
    .slice(marker.index + marker[0].length)
    .replace(/^[_\s]+/, "")
    .trim();
}

function terminalSpecifiedTaskNormalizedSuffix(value) {
  return String(value || "")
    .trim()
    .replace(/^_+/, "")
    .trim();
}

function terminalSpecifiedTaskSyncImportedTitleValue(task, previousTitle, title, enabled) {
  let nextTask = String(task || "");
  const normalizedPreviousTitle = terminalSpecifiedTaskNormalizedSuffix(previousTitle);
  const normalizedTitle = enabled ? terminalSpecifiedTaskNormalizedSuffix(title) : "";
  if (normalizedPreviousTitle) {
    if (nextTask === normalizedPreviousTitle) {
      nextTask = "";
    } else if (nextTask.endsWith(`\n${normalizedPreviousTitle}`)) {
      nextTask = nextTask.slice(0, -(normalizedPreviousTitle.length + 1));
    }
  }
  if (normalizedTitle) {
    nextTask = nextTask.trim()
      ? `${nextTask.trimEnd()}\n${normalizedTitle}`
      : normalizedTitle;
  }
  return { task: nextTask, importedTitle: normalizedTitle };
}

function syncTerminalSpecifiedTaskImportedTitle() {
  const dom = terminalSpecifiedTaskDom();
  if (!dom.task) {
    return;
  }
  const sessionAction = terminalSpecifiedTaskSessionAction();
  const enabled = terminalSpecifiedTaskMode() === "fixed"
    && (sessionAction === "new" || sessionAction === "ask")
    && Boolean(dom.importTitle?.checked);
  const importedTitle = sessionAction === "ask" && terminalSpecifiedTaskQuestionSessionId
    ? `${dom.terminalNameSuffix?.value || "调用skill读取codex session并回答问题"}：${terminalSpecifiedTaskQuestionSessionId}`
    : dom.terminalNameSuffix?.value;
  const synced = terminalSpecifiedTaskSyncImportedTitleValue(
    dom.task.value,
    terminalSpecifiedTaskImportedTitle,
    importedTitle,
    enabled,
  );
  if (dom.task.value !== synced.task) {
    dom.task.value = synced.task;
  }
  terminalSpecifiedTaskImportedTitle = synced.importedTitle;
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
  const namingAction = terminalSpecifiedTaskNamingAction || terminalSpecifiedTaskSessionAction();
  if (namingAction === "ask") {
    return terminalSpecifiedTaskUniqueFinalName(terminalSpecifiedTaskQuestionTerminalName());
  }
  if (namingAction === "new") {
    const { terminalNameSuffix } = terminalSpecifiedTaskDom();
    const suffix = terminalSpecifiedTaskNormalizedSuffix(terminalNameSuffix?.value);
    if (!suffix) {
      return "";
    }
    const autoName = terminalSpecifiedTaskAutoNamePreview();
    if (!autoName || ["编号已用完", "正在计算编号…", "创建时自动分配"].includes(autoName)) {
      return "";
    }
    const availableSuffixLength = Math.max(0, 120 - autoName.length - 1);
    const boundedSuffix = suffix.slice(0, availableSuffixLength).trim();
    return boundedSuffix
      ? terminalSpecifiedTaskUniqueFinalName(`${autoName}_${boundedSuffix}`)
      : "";
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
  const finalName = specifiedPresetTerminalName({
    sourceTerminalName: namingBase,
    sessionAction: namingAction,
  });
  return terminalSpecifiedTaskUniqueFinalName(finalName);
}

function terminalSpecifiedTaskQuestionTerminalName() {
  const dom = terminalSpecifiedTaskDom();
  const suffix = terminalSpecifiedTaskQuestionNameSuffix(
    dom.terminalName?.value || terminalSpecifiedTaskSourceName,
    dom.terminalNameSuffix?.value,
  );
  const autoName = terminalSpecifiedTaskAutoNamePreview();
  if (!autoName || ["编号已用完", "正在计算编号…", "创建时自动分配"].includes(autoName)) {
    return "";
  }
  const availableSuffixLength = Math.max(0, 120 - autoName.length - 1);
  const boundedSuffix = suffix.slice(0, availableSuffixLength).trim();
  return boundedSuffix
    ? terminalSpecifiedTaskUniqueFinalName(`${autoName}_${boundedSuffix}`)
    : "";
}

function terminalSpecifiedTaskQuestionNameSuffix(sourceName, suffixValue) {
  const sourceBase = terminalSpecifiedTaskNameWithoutAutoIndices(sourceName);
  const suffix = terminalSpecifiedTaskNormalizedSuffix(suffixValue)
    || "调用skill读取codex session并回答问题";
  return sourceBase ? `原终端:${sourceBase}_${suffix}` : `原终端:${suffix}`;
}

function renderTerminalSpecifiedTaskTerminalNamePreview() {
  const { terminalNamePreview } = terminalSpecifiedTaskDom();
  if (!terminalNamePreview) {
    return;
  }
  const finalName = terminalSpecifiedTaskFinalTerminalName();
  const namingAction = terminalSpecifiedTaskNamingAction || terminalSpecifiedTaskSessionAction();
  terminalNamePreview.textContent = finalName
    || (namingAction === "new" ? terminalSpecifiedTaskAutoNamePreview() : "自动分配");
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
  const isExternal = terminalSpecifiedTaskIsExternalAgent(agent);
  const isOther = agent === "other";
  let mode = terminalSpecifiedTaskMode();
  if ((agent === "claude" || isExternal) && mode !== "fixed") {
    const fixed = dom.form?.querySelector('input[name="terminal-specified-task-mode"][value="fixed"]');
    if (fixed) {
      fixed.checked = true;
      mode = "fixed";
    }
  }

  dom.form?.querySelectorAll('input[name="terminal-specified-task-mode"]').forEach((input) => {
    input.disabled = Boolean(terminalSpecifiedTaskId) || ((agent === "claude" || isExternal) && input.value !== "fixed");
  });
  const fixed = mode === "fixed";
  const forkActionInput = dom.form?.querySelector('input[name="terminal-specified-task-session-action"][value="fork"]');
  if (forkActionInput) {
    forkActionInput.disabled = agent === "codebuddy" || agent === "cdx";
    if ((agent === "codebuddy" || agent === "cdx") && forkActionInput.checked) {
      const newActionInput = dom.form?.querySelector(
        'input[name="terminal-specified-task-session-action"][value="new"]',
      );
      if (newActionInput) {
        newActionInput.checked = true;
      }
    }
  }
  const sessionAction = terminalSpecifiedTaskSessionAction();
  dom.form?.querySelectorAll('input[name="terminal-specified-task-agent"]').forEach((input) => {
    input.disabled = Boolean(terminalSpecifiedTaskId) || terminalSpecifiedTaskAgentPinned;
  });
  if (dom.fixedOptions) {
    dom.fixedOptions.hidden = !fixed;
  }
  const hasAccounts = terminalSpecifiedTaskHasAccounts(agent);
  if (dom.presetField) dom.presetField.hidden = !hasAccounts;
  const presetLabel = dom.presetField?.querySelector("span");
  if (presetLabel) presetLabel.textContent = terminalSpecifiedTaskAccountLabel(agent);
  if (dom.sessionOnlyLabel) dom.sessionOnlyLabel.hidden = !fixed || !hasAccounts;
  if (dom.preset) dom.preset.required = hasAccounts;
  if (dom.otherCommandField) dom.otherCommandField.hidden = !isOther;
  if (dom.otherCommand) dom.otherCommand.required = isOther && sessionAction !== "new";
  if (dom.sessionIdField) {
    dom.sessionIdField.hidden = !fixed
      || ((sessionAction === "new" || sessionAction === "ask") && !terminalSpecifiedTaskSessionFieldPinned);
  }
  const isAgyResume = agent === "agy" && sessionAction === "resume";
  if (dom.agyContinueLabel) {
    dom.agyContinueLabel.hidden = !fixed || !isAgyResume;
  }
  const agyContinueActive = isAgyResume && Boolean(dom.agyContinue?.checked);
  if (dom.sessionId) {
    dom.sessionId.disabled = agyContinueActive;
    dom.sessionId.required = fixed && !agyContinueActive && (sessionAction === "resume" || sessionAction === "fork");
    dom.sessionId.placeholder = agyContinueActive
      ? "默认恢复最后一次会话 (-c)"
      : "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx";
  }
  if (dom.terminalNameSuffixField) {
    dom.terminalNameSuffixField.hidden = !fixed
      || (sessionAction !== "new" && sessionAction !== "ask");
  }
  const workspaceSelectable = !terminalSpecifiedTaskId;
  if (dom.workspaceField) {
    dom.workspaceField.hidden = false;
  }
  if (dom.workspace) {
    dom.workspace.disabled = !workspaceSelectable;
  }
  if (dom.workspaceBrowse) {
    dom.workspaceBrowse.disabled = !workspaceSelectable;
  }
  if (dom.favorite) {
    dom.favorite.disabled = !workspaceSelectable
      || terminalSpecifiedTaskFavoriteChoices().length === 0;
  }
  const groupSelectable = fixed && sessionAction === "new";
  if (dom.groupField) {
    dom.groupField.hidden = !groupSelectable;
  }
  if (dom.joinGroup) {
    dom.joinGroup.disabled = Boolean(terminalSpecifiedTaskId) || !groupSelectable;
  }
  if (dom.group) {
    dom.group.disabled = Boolean(terminalSpecifiedTaskId)
      || !groupSelectable
      || dom.joinGroup.checked !== true
      || terminalSpecifiedTaskGroups().length === 0;
  }
  if (dom.groupRole) {
    dom.groupRole.disabled = Boolean(terminalSpecifiedTaskId)
      || !groupSelectable
      || dom.joinGroup.checked !== true;
  }
  if (dom.newGroup) {
    dom.newGroup.disabled = Boolean(terminalSpecifiedTaskId)
      || !groupSelectable
      || dom.joinGroup.checked === true;
  }
  if (dom.newGroupCount) {
    dom.newGroupCount.disabled = Boolean(terminalSpecifiedTaskId)
      || !groupSelectable
      || dom.newGroup.checked !== true;
  }
  if (dom.timeoutField) {
    dom.timeoutField.hidden = fixed;
  }
  if (dom.task) {
    dom.task.required = !fixed;
    dom.task.placeholder = fixed ? "可选：启动后立即交给 Agent 的任务" : "输入任务要求";
  }
  if (dom.taskLabel) {
    dom.taskLabel.textContent = terminalSpecifiedTaskFutureMode ? "提示词" : fixed ? "初始提示词" : "任务";
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
    ? `；任务终端${record.terminal_closed ? "已关闭" : "关闭失败"}`
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

function terminalSpecifiedTaskPresetKey(id, kind = "api") {
  return JSON.stringify([String(kind || "api"), String(id || "")]);
}

function terminalSpecifiedTaskReadRecentPresets(agent) {
  try {
    const stored = localStorage.getItem(`webclx.new-terminal.recent-presets.${agent}`);
    if (stored !== null) {
      const parsed = JSON.parse(stored);
      if (Array.isArray(parsed)) {
        const recent = [...new Set(parsed.filter((key) => typeof key === "string"))];
        terminalSpecifiedTaskRecentPresets.set(agent, recent);
        return recent;
      }
    }
  } catch {
    // Browser storage can be unavailable; keep recency for this page as well.
  }
  return terminalSpecifiedTaskRecentPresets.get(agent) || [];
}

function terminalSpecifiedTaskRememberPreset(agent, id, kind) {
  if (!id || !terminalSpecifiedTaskHasAccounts(agent)) return;
  const key = terminalSpecifiedTaskPresetKey(id, kind);
  const recent = [key, ...terminalSpecifiedTaskReadRecentPresets(agent).filter((item) => item !== key)];
  terminalSpecifiedTaskRecentPresets.set(agent, recent);
  try {
    localStorage.setItem(`webclx.new-terminal.recent-presets.${agent}`, JSON.stringify(recent));
  } catch {
    // Successful launches must not fail because browser storage is disabled.
  }
}

function terminalSpecifiedTaskSortPresets(presets, agent) {
  const ranks = new Map(terminalSpecifiedTaskReadRecentPresets(agent).map((key, index) => [key, index]));
  const rank = (item) => ranks.get(terminalSpecifiedTaskPresetKey(
    item.id, item.kind || (agent === "claude" ? "claude" : agent === "codex" ? "api" : agent),
  )) ?? Infinity;
  return presets.slice().sort((left, right) => {
    const leftRank = rank(left);
    const rightRank = rank(right);
    return leftRank === rightRank ? 0 : leftRank < rightRank ? -1 : 1;
  });
}

async function loadTerminalSpecifiedTaskPresets() {
  const { preset } = terminalSpecifiedTaskDom();
  if (!preset) {
    return;
  }
  const agent = terminalSpecifiedTaskAgent();
  if (!terminalSpecifiedTaskHasAccounts(agent)) {
    preset.disabled = true;
    return;
  }
  const previous = preset.dataset.agent === agent ? preset.value : "";
  preset.disabled = true;
  try {
    let presets = [];
    if (agent === "codex") {
      const response = await requestJson("/api/auth/codex-accounts");
      const accounts = Array.isArray(response?.accounts) ? response.accounts : [];
      presets = accounts
        .slice()
        .sort((left, right) => Number(right.favorite === true) - Number(left.favorite === true));
    } else if (agent === "claude") {
      const response = await requestJson("/api/auth/claude-presets");
      const raw = Array.isArray(response?.presets) ? response.presets : [];
      presets = raw
        .filter((item) => item?.id && ![
          item.name, item.provider_name, item.account, item.base_url, item.endpoint, item.management_url,
        ].some((value) => /sub2api/i.test(String(value || ""))))
        .slice()
        .sort((left, right) => Number(right.favorite === true) - Number(left.favorite === true));
    } else if (agent === "codebuddy") {
      const response = await requestJson("/api/auth/codebuddy-accounts");
      const raw = Array.isArray(response) ? response : (Array.isArray(response?.accounts) ? response.accounts : []);
      presets = raw.map((item) => ({
        id: item.id,
        name: item.nickname || item.name || item.email || item.enterpriseName || item.id,
        active: Boolean(item.active),
        kind: "codebuddy",
      }));
    } else if (agent === "devin") {
      const response = await requestJson("/api/auth/devin-accounts");
      const raw = Array.isArray(response) ? response : (Array.isArray(response?.accounts) ? response.accounts : []);
      presets = raw.map((item) => ({
        id: item.id,
        name: item.name || item.email || item.id,
        active: Boolean(item.active),
        kind: "devin",
      }));
    } else if (agent === "agy") {
      const response = await requestJson("/api/agy/presets");
      const raw = Array.isArray(response) ? response : (Array.isArray(response?.presets) ? response.presets : []);
      presets = raw.map((item) => ({
        id: item.id,
        name: item.name || item.email || item.id,
        active: Boolean(item.current || item.active),
        kind: "agy",
      }));
    }

    presets = terminalSpecifiedTaskSortPresets(presets, agent);
    const recent = new Set(terminalSpecifiedTaskReadRecentPresets(agent));
    const first = presets[0];
    const recentId = first && recent.has(terminalSpecifiedTaskPresetKey(
      first.id, first.kind || (agent === "claude" ? "claude" : agent === "codex" ? "api" : agent),
    )) ? first.id : "";
    let activeId = "";
    for (const item of presets) {
      if (item.active) {
        activeId = item.id;
        break;
      }
    }
    const preferredId = agent === "agy"
      ? (activeId || (presets.some((item) => item.id === previous) ? previous : recentId))
      : (presets.some((item) => item.id === previous) ? previous : recentId);
    preset.dataset.activeId = activeId;

    preset.replaceChildren();
    for (const item of presets) {
      const option = document.createElement("option");
      const model = specifiedPresetModel(item, agent);
      option.value = item.id;
      option.dataset.kind = item.kind || (agent === "claude" ? "claude" : "api");
      const kindLabel = agent === "codex" ? `[${option.dataset.kind === "oauth" ? "OAuth" : "API"}] ` : "";
      const currentMarker = item.active ? " (当前)" : "";
      option.textContent = `${kindLabel}${item.name || item.id}${model ? ` · ${model}` : ""}${currentMarker}`;
      option.selected = item.id === preferredId || (!preferredId && item.active);
      preset.append(option);
    }
    if (!preset.value && presets[0]) {
      preset.value = presets[0].id;
    }
    if (presets.length === 0) {
      const option = document.createElement("option");
      option.value = "";
      option.textContent = "没有可用账号";
      preset.append(option);
    }
    preset.dataset.agent = agent;
    preset.disabled = Boolean(terminalSpecifiedTaskId);
  } catch (error) {
    setTerminalSpecifiedTaskStatus(`读取账号失败：${error.message}`, "warn");
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
  terminalSpecifiedTaskQuestionSessionId = String(options.questionSessionId || "").trim();
  terminalSpecifiedTaskLaunchTerminal = typeof options.launchTerminal === "function"
    ? options.launchTerminal
    : launchTerminalSpecifiedPreset;
  terminalSpecifiedTaskPageStatus = typeof options.updatePageStatus === "function"
    ? options.updatePageStatus
    : null;
  terminalSpecifiedTaskFutureMode = options.futureTask === true;
  terminalSpecifiedTaskFutureEdit = options.futureTaskEdit || null;
  if (dom.scheduleField) {
    dom.scheduleField.hidden = !terminalSpecifiedTaskFutureMode;
  }
  terminalSpecifiedTaskTrigger = trigger;
  if (options.resetPreset === true && dom.preset) {
    dom.preset.value = "";
    delete dom.preset.dataset.agent;
  }
  terminalSpecifiedTaskSelectAgent(options.agent || terminalSpecifiedTaskAgent() || "codex");
  if (options.mode) {
    const mode = dom.form?.querySelector(
      `input[name="terminal-specified-task-mode"][value="${options.mode}"]`,
    );
    if (mode) {
      mode.checked = true;
    }
  }
  if (dom.sessionOnly) {
    dom.sessionOnly.checked = options.temporary === true;
  }
  if (options.temporary !== undefined) {
    const scope = dom.form?.querySelector(
      `input[name="terminal-specified-task-preset-scope"][value="${options.temporary === true ? "terminal" : "global"}"]`,
    );
    if (scope) {
      scope.checked = true;
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
  if (dom.agyContinue) {
    dom.agyContinue.checked = options.agyContinue !== undefined
      ? options.agyContinue === true
      : (!dom.sessionId?.value);
  }
  if (dom.task && terminalSpecifiedTaskImportedTitle) {
    const cleared = terminalSpecifiedTaskSyncImportedTitleValue(
      dom.task.value,
      terminalSpecifiedTaskImportedTitle,
      "",
      false,
    );
    dom.task.value = cleared.task;
  }
  terminalSpecifiedTaskImportedTitle = "";
  if (dom.task && options.resetTask === true) {
    dom.task.value = "";
  }
  if (dom.task && options.initialTask !== undefined) {
    dom.task.value = String(options.initialTask || "");
  }
  if (dom.title) {
    dom.title.textContent = options.title ? String(options.title) : "指定预设临时执行";
  }
  await refreshTerminalSpecifiedTaskWorkspaceHistory();
  renderTerminalSpecifiedTaskWorkspaceChoices();
  renderTerminalSpecifiedTaskFavoriteChoices();
  if (dom.terminalNameSuffix) {
    dom.terminalNameSuffix.value = terminalSpecifiedTaskSessionAction() === "ask"
      ? "调用skill读取codex session并回答问题"
      : "";
  }
  await setTerminalSpecifiedTaskSourcePath(terminalSpecifiedTaskSourcePath);
  if (dom.terminalName) {
    dom.terminalName.value = terminalSpecifiedTaskSourceName;
    dom.terminalName.title = terminalSpecifiedTaskSourceName;
  }
  if (dom.importTitle) {
    dom.importTitle.checked = true;
  }
  if (dom.task && terminalSpecifiedTaskSessionAction() === "ask") {
    syncTerminalSpecifiedTaskImportedTitle();
  }
  if (dom.autoCompact) {
    dom.autoCompact.checked = true;
  }
  if (dom.group) {
    renderTerminalSpecifiedTaskGroupChoices();
    dom.joinGroup.checked = false;
    dom.group.disabled = true;
  }
  if (dom.newGroup) {
    dom.newGroup.checked = false;
  }
  if (dom.newGroupCount) {
    dom.newGroupCount.value = "2";
    dom.newGroupCount.disabled = true;
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
  (terminalSpecifiedTaskAgent() === "other" ? dom.otherCommand : dom.preset)?.focus();
}

function closeTerminalSpecifiedTaskDialog() {
  const { dialog, sessionId } = terminalSpecifiedTaskDom();
  if (dialog?.open) {
    dialog.close();
  }
  // The terminal page restores its editable IME context from the close event.
  // Refocusing the menu button here would race with that recovery on Windows.
  if (typeof restoreTerminalFocusAfterDialogClose !== "function") {
    terminalSpecifiedTaskTrigger?.focus?.({ preventScroll: true });
  }
  terminalSpecifiedTaskTrigger = null;
  if (terminalSpecifiedTaskSessionFieldPinned && sessionId) {
    sessionId.value = "";
  }
  if (terminalSpecifiedTaskDom().agyContinue) {
    terminalSpecifiedTaskDom().agyContinue.checked = true;
  }
  terminalSpecifiedTaskSessionFieldPinned = false;
  terminalSpecifiedTaskAgentPinned = false;
  terminalSpecifiedTaskTerminalNameOverride = "";
  terminalSpecifiedTaskNamingAction = "";
  terminalSpecifiedTaskQuestionSessionId = "";
  terminalSpecifiedTaskLaunchTerminal = null;
  terminalSpecifiedTaskPageStatus = null;
  terminalSpecifiedTaskFutureMode = false;
  terminalSpecifiedTaskFutureEdit = null;
  if (terminalSpecifiedTaskDom().scheduleField) {
    terminalSpecifiedTaskDom().scheduleField.hidden = true;
  }
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
  const createdSessions = [];
  try {
    const newGroupCount = Number(options.createGroupCount || 0);
    const createCount = Number.isFinite(newGroupCount) && options.createGroup === true
      ? Math.min(8, Math.max(2, Math.trunc(newGroupCount)))
      : 1;
    for (let index = 0; index < createCount; index += 1) {
      created = await createSession({
        autoSelect: index === createCount - 1,
        suppressLoadingStatus: true,
        pushHistoryOnSelect: index === createCount - 1,
        enableQuickStart: false,
        throwOnError: true,
        path: cwd,
        origin: options.origin || "agent",
        ownerKey: options.ownerKey || "",
        codexApiPresetId: options.codexApiPresetId || "",
      });
      createdSessions.push(created);
    }
    if (!created) {
      throw new Error("交互终端创建失败。");
    }
    if (options.autoCompact === false) {
      disableAutoCompactForSession(created.id);
    }
    let terminalName = String(options.terminalName || "").trim();
    const terminalNameSuffix = terminalSpecifiedTaskNormalizedSuffix(options.terminalNameSuffix);
    if (terminalNameSuffix) {
      const availableSuffixLength = Math.max(0, 120 - String(created.name || "").length - 1);
      const boundedSuffix = terminalNameSuffix.slice(0, availableSuffixLength).trim();
      terminalName = boundedSuffix ? `${created.name}_${boundedSuffix}` : "";
    }
    if (terminalName) {
      created = await renameTerminalForTool(created.id, terminalName, cwd);
    }
    const groupId = String(options.groupId || "").trim();
    const groupRole = String(options.groupRole || "").trim();
    if (options.createGroup === true && createCount >= 2) {
      const namedSessions = [];
      for (const [index, session] of createdSessions.entries()) {
        const role = index === 0 ? "管理者" : "执行者";
        const suffix = terminalSpecifiedTaskNormalizedSuffix(
          `${options.terminalNameSuffix || ""}_${role}_${index + 1}`,
        );
        const desiredName = [
          String(options.terminalName || created.name || "").trim(),
          suffix,
        ].filter(Boolean).join("_").slice(0, 120);
        namedSessions.push(await renameTerminalForTool(session.id, desiredName, cwd));
      }
      createdSessions.splice(0, namedSessions.length, ...namedSessions);
      created = namedSessions.at(-1);
    }
    if (options.createGroup === true && createCount >= 2) {
      await requestJson(`/api/terminal/sessions/${encodeURIComponent(created.id)}/group`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          member_ids: createdSessions.map((session) => session.id),
          group_name: terminalSpecifiedTaskNewGroupName(createdSessions),
          group_roles: createdSessions.map((session, index) => ({
            session_id: session.id,
            role: index === 0 ? "manager" : "executor",
          })),
        }),
      });
      created = { ...created, group_token: `new:${created.id}` };
      await loadSessions({
        preferredSessionId: created.id,
        preserveCurrentList: true,
        forcePreferredSession: true,
      });
    } else if (groupId) {
      const group = terminalSpecifiedTaskGroups().find((item) => item.token === groupId);
      if (!group) {
        throw new Error("所选终端群组不存在，请刷新后重试。");
      }
      const memberIds = [
        ...terminalMessageGroupMemberIds(group).filter((id) => id !== created.id),
        created.id,
      ];
      await requestJson(`/api/terminal/sessions/${encodeURIComponent(created.id)}/group`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          member_ids: memberIds,
          fixed_order: group.members[0]?.group_fixed_order === true,
          group_color: terminalSpecifiedTaskGroupColor(group),
          group_name: group.name || undefined,
          group_roles: [{ session_id: created.id, role: groupRole || "executor" }],
        }),
      });
      created = { ...created, group_token: group.token };
      await loadSessions({
        preferredSessionId: created.id,
        preserveCurrentList: true,
        forcePreferredSession: true,
      });
    }
    await waitForTerminalToolSessionReady(created.id);
    const runCommand = String(options.runCommand || "").trim();
    if (runCommand || !options.allowEmptyCommand) {
      const sent = await sendTerminalAutoTypedInput(runCommand, {
        sessionId: created.id,
        throwOnError: true,
      });
      if (!sent) {
        throw new Error("Agent 启动命令为空，未发送。");
      }
    }
    return created;
  } catch (launchError) {
    if (createdSessions.length > 1) {
      const deletedIds = new Set();
      for (const session of [...createdSessions].reverse()) {
        if (!session?.id || deletedIds.has(session.id)) continue;
        deletedIds.add(session.id);
        await requestJson(
          `/api/terminal/sessions/${encodeURIComponent(session.id)}`,
          {
            method: "DELETE",
            headers: {
              "X-WebClx-Confirm-Session": session.id,
              "X-WebClx-Delete-Source": "specified-preset-launch",
            },
          },
        ).catch(() => {});
      }
      await loadSessions({
        preferredSessionId: previousSessionId,
        forcePreferredSession: true,
      });
      throw new Error(`自组群启动失败，已清理 ${deletedIds.size} 个新终端。`);
    }
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

function terminalSpecifiedTaskSelectedGroup() {
  const dom = terminalSpecifiedTaskDom();
  return dom.joinGroup?.checked === true ? String(dom.group?.value || "").trim() : "";
}

function terminalSpecifiedTaskSelectedGroupRole() {
  const dom = terminalSpecifiedTaskDom();
  if (dom.joinGroup?.checked !== true) return "";
  const value = String(dom.groupRole?.value || "").trim();
  return ["manager", "executor", "reviewer"].includes(value) ? value : "executor";
}

function terminalSpecifiedTaskCreateGroup() {
  const dom = terminalSpecifiedTaskDom();
  return dom.joinGroup?.checked !== true && dom.newGroup?.checked === true;
}

function terminalSpecifiedTaskCreateGroupCount() {
  const dom = terminalSpecifiedTaskDom();
  if (terminalSpecifiedTaskCreateGroup() !== true) return 0;
  const value = Number.parseInt(dom.newGroupCount?.value, 10);
  return Number.isFinite(value)
    ? Math.min(8, Math.max(2, Math.trunc(value)))
    : 2;
}

function terminalSpecifiedTaskNewGroupName(sessions) {
  const names = (Array.isArray(sessions) ? sessions : [])
    .map((session) => String(session?.name || "").trim())
    .filter(Boolean);
  if (!names.length) return "";
  const prefixes = names.map((name) => name.split("_")[0]).filter(Boolean);
  const counts = new Map();
  prefixes.forEach((prefix) => counts.set(prefix, (counts.get(prefix) || 0) + 1));
  const base = [...counts.entries()].sort((left, right) => right[1] - left[1])[0]?.[0];
  return base ? `${base}_自组群` : "自组群";
}

function terminalSpecifiedTaskGroupColor(group) {
  const color = String(group?.color || "").trim();
  return /^#[0-9a-fA-F]{6}$/.test(color) ? color.toLowerCase() : undefined;
}

async function submitTerminalSpecifiedTask() {
  if (terminalSpecifiedTaskFutureMode) {
    return submitTerminalSpecifiedFutureTask();
  }
  const dom = terminalSpecifiedTaskDom();
  const agent = terminalSpecifiedTaskAgent();
  const mode = terminalSpecifiedTaskMode();
  const selectedPresetId = String(dom.preset?.value || "");
  const selectedPresetKind = dom.preset?.selectedOptions?.[0]?.dataset?.kind
    || (agent === "claude" ? "claude" : agent === "codex" ? "api" : agent);
  const rememberPreset = () => terminalSpecifiedTaskRememberPreset(agent, selectedPresetId, selectedPresetKind);
  const task = String(dom.task?.value || "").trim();
  const sessionAction = terminalSpecifiedTaskSessionAction();
  if (sessionAction === "ask") {
    if (mode !== "fixed") {
      setTerminalSpecifiedTaskStatus("“提问”动作仅支持交互终端。", "warn");
      return;
    }
    const sourceSessionId = String(state.activeSessionId || "").trim();
    const sourceContext = typeof activeTerminalContext === "object" ? activeTerminalContext : null;
    if (typeof activeTerminalContext === "undefined" || !sourceSessionId || sourceContext?.sessionId !== sourceSessionId) {
      setTerminalSpecifiedTaskStatus("当前终端尚未连接，无法生成提问提示词。", "warn");
      return;
    }
    if (terminalSpecifiedTaskHasAccounts(agent) && !dom.preset?.value) {
      setTerminalSpecifiedTaskStatus(`请选择 ${terminalSpecifiedTaskAccountLabel(agent)}。`, "warn");
      return;
    }
    const command = terminalSpecifiedTaskAgentLaunchCommand("new", "");
    if ((agent === "other") && !command) {
      setTerminalSpecifiedTaskStatus("请输入终端命令。", "warn");
      return;
    }
    setTerminalSpecifiedTaskBusy(true);
    try {
      const detected = await detectAgentResumeIdComplete(sourceSessionId, sourceContext);
      if (!detected?.resumeId) {
        throw new Error("没找到当前会话 ID，无法生成提问提示词。");
      }
      terminalSpecifiedTaskSetQuestionSessionId(detected.resumeId);
      const prompt = buildSessionQuestionPrompt(detected.resumeId);
      const questionNameSuffix = terminalSpecifiedTaskQuestionNameSuffix(
        dom.terminalName?.value || terminalSpecifiedTaskSourceName,
        dom.terminalNameSuffix?.value,
      );
      let launchResult;
      if (agent === "codex" || agent === "claude") {
        const result = await executeSpecifiedPreset({
          action: "launch",
          agent,
          presetKind: terminalSpecifiedTaskSelectedPresetKind(),
          presetId: dom.preset.value,
          cwd: terminalSpecifiedTaskSourcePath || terminalSpecifiedTaskCurrentPath(),
          temporary: terminalSpecifiedTaskTemporary(),
          sessionAction: "new",
          task: "",
          terminalName: terminalSpecifiedTaskQuestionTerminalName(),
          terminalNameSuffix: questionNameSuffix,
          origin: "agent",
          ownerKey: "manual-question-terminal",
          quickStart: false,
          autoCompact: dom.autoCompact?.checked !== false,
          groupId: sessionAction === "new" ? terminalSpecifiedTaskSelectedGroup() : "",
          groupRole: sessionAction === "new" ? terminalSpecifiedTaskSelectedGroupRole() : "",
          launchTerminal: terminalSpecifiedTaskLaunchTerminal || launchTerminalSpecifiedPreset,
        });
        launchResult = result.launchResult;
        if (launchResult) rememberPreset();
      } else {
        let shouldRestore = false;
        let previousActiveId = "";
        if (terminalSpecifiedTaskHasAccounts(agent) && dom.preset?.value) {
          const selectedAccountId = dom.preset.value;
          const temporary = terminalSpecifiedTaskTemporary();
          previousActiveId = dom.preset.dataset.activeId || "";
          shouldRestore = agent !== "agy" && temporary && previousActiveId && previousActiveId !== selectedAccountId;
          await applyTerminalSpecifiedAgentAccount(
            agent,
            selectedAccountId,
            dom.preset.selectedOptions?.[0]?.dataset?.kind,
          );
        }
        launchResult = await (terminalSpecifiedTaskLaunchTerminal || launchTerminalSpecifiedPreset)(
          terminalSpecifiedTaskSourcePath || terminalSpecifiedTaskCurrentPath(),
          {
            runCommand: command,
            quickStart: false,
            origin: "agent",
            ownerKey: "manual-question-terminal",
            terminalName: terminalSpecifiedTaskQuestionTerminalName(),
            terminalNameSuffix: questionNameSuffix,
            groupId: sessionAction === "new" ? terminalSpecifiedTaskSelectedGroup() : "",
            groupRole: sessionAction === "new" ? terminalSpecifiedTaskSelectedGroupRole() : "",
          },
        );
        if (launchResult) rememberPreset();
        // 启动请求完成后立即关闭对话框，等待就绪与自动输入在后台继续。
        closeTerminalSpecifiedTaskDialog();
        reportTerminalSpecifiedTaskPageStatus(`已发起终端启动：${launchResult?.name || "新终端"}。`, "info");
        if (shouldRestore && previousActiveId) {
          setTimeout(async () => {
            try {
              await applyTerminalSpecifiedAgentAccount(agent, previousActiveId);
            } catch (error) {
              console.warn("Failed to restore agent account:", error);
            }
          }, 3000);
        }
      }
      if (typeof waitForTerminalToolSessionReady !== "function" || typeof sendTerminalAutoTypedInput !== "function") {
        throw new Error("当前页面不支持在新终端启动后自动发送提问提示词。");
      }
      await waitForTerminalToolSessionReady(launchResult.id);
      if (!await sendTerminalAutoTypedInput(prompt, { sessionId: launchResult.id, throwOnError: true })) {
        throw new Error("终端已启动，但提问提示词发送失败。");
      }
      setTerminalSpecifiedTaskStatus(`已启动并提问 ${launchResult?.name || "新终端"}。`, "ok");
      reportTerminalSpecifiedTaskPageStatus("已启动新终端并发送提问提示词。", "ok");
      closeTerminalSpecifiedTaskDialog();
    } catch (error) {
      setTerminalSpecifiedTaskStatus(error.message || "新终端提问失败。", "warn");
    } finally {
      setTerminalSpecifiedTaskBusy(false);
    }
    return;
  }
  if (terminalSpecifiedTaskIsExternalAgent(agent)) {
    if (terminalSpecifiedTaskHasAccounts(agent) && !dom.preset?.value) {
      setTerminalSpecifiedTaskStatus(`请选择 ${terminalSpecifiedTaskAccountLabel(agent)}。`, "warn");
      return;
    }
    const sessionAction = terminalSpecifiedTaskSessionAction();
    const command = terminalSpecifiedTaskAgentLaunchCommand(
      sessionAction,
      dom.sessionId?.value,
    );
    if (!command && sessionAction !== "new") {
      const command_ = terminalSpecifiedTaskAgentCommand(agent);
      setTerminalSpecifiedTaskStatus(
        command_?.resume_command || command_?.new_command
          ? `恢复 ${command_.label || command_.key} 会话需要有效的 session ID。`
          : "请输入终端命令。",
        "warn",
      );
      return;
    }
    const needsPrompt = sessionAction === "ask";
    if (needsPrompt && typeof activeTerminalContext === "undefined") {
      setTerminalSpecifiedTaskStatus("请从终端管理页使用“提问”动作。", "warn");
      return;
    }
    const sourceSessionId = String(state.activeSessionId || "").trim();
    const sourceContext = typeof activeTerminalContext === "object" ? activeTerminalContext : null;
    if (needsPrompt && (!sourceSessionId || sourceContext?.sessionId !== sourceSessionId)) {
      setTerminalSpecifiedTaskStatus("当前终端尚未连接，无法生成 Session 提示词。", "warn");
      return;
    }
    setTerminalSpecifiedTaskBusy(true);
    try {
      let prompt = "";
      if (needsPrompt) {
        const detected = await detectAgentResumeIdComplete(sourceSessionId, sourceContext);
        if (!detected?.resumeId) {
          throw new Error("没找到当前会话 ID，无法生成提问提示词。");
        }
        terminalSpecifiedTaskSetQuestionSessionId(detected.resumeId);
        prompt = buildSessionQuestionPrompt(detected.resumeId);
      }
      let shouldRestore = false;
      let previousActiveId = "";
      if (terminalSpecifiedTaskHasAccounts(agent) && dom.preset?.value) {
        const selectedAccountId = dom.preset.value;
        const temporary = terminalSpecifiedTaskTemporary();
        previousActiveId = dom.preset.dataset.activeId || "";
        shouldRestore = agent !== "agy" && temporary && previousActiveId && previousActiveId !== selectedAccountId;
        await applyTerminalSpecifiedAgentAccount(
          agent,
          selectedAccountId,
          dom.preset.selectedOptions?.[0]?.dataset?.kind,
        );
      }
      const launchResult = await (terminalSpecifiedTaskLaunchTerminal || launchTerminalSpecifiedPreset)(
        terminalSpecifiedTaskSourcePath || terminalSpecifiedTaskCurrentPath(),
        {
          runCommand: command,
          allowEmptyCommand: agent === "other" && sessionAction === "new",
          quickStart: false,
          origin: "agent",
          ownerKey: "manual-other-terminal",
          groupId: sessionAction === "new" ? terminalSpecifiedTaskSelectedGroup() : "",
          groupRole: sessionAction === "new" ? terminalSpecifiedTaskSelectedGroupRole() : "",
          createGroup: sessionAction === "new" && terminalSpecifiedTaskCreateGroup(),
          createGroupCount: terminalSpecifiedTaskCreateGroupCount(),
          terminalName: sessionAction === "ask"
            ? terminalSpecifiedTaskQuestionTerminalName()
            : terminalSpecifiedTaskFinalTerminalName(),
          terminalNameSuffix: sessionAction === "ask"
            ? terminalSpecifiedTaskQuestionNameSuffix(
                dom.terminalName?.value || terminalSpecifiedTaskSourceName,
                dom.terminalNameSuffix?.value,
              )
            : "",
        },
      );
      if (launchResult) rememberPreset();
      // 终端创建请求已发出后立即关闭对话框，后续启动/提问进度通过页面状态提示。
      closeTerminalSpecifiedTaskDialog();
      reportTerminalSpecifiedTaskPageStatus(`已发起终端启动：${launchResult?.name || "新终端"}。`, "info");
      if (shouldRestore && previousActiveId) {
        setTimeout(async () => {
          try {
            await applyTerminalSpecifiedAgentAccount(agent, previousActiveId);
          } catch (error) {
            console.warn("Failed to restore agent account:", error);
          }
        }, 3000);
      }
      if (needsPrompt && (typeof waitForTerminalToolSessionReady !== "function" || typeof sendTerminalAutoTypedInput !== "function")) {
        throw new Error("当前页面不支持在新终端启动后自动发送提示词，请从终端管理页新开。");
      }
      if (needsPrompt) {
        await waitForTerminalToolSessionReady(launchResult.id);
        const sent = await sendTerminalAutoTypedInput(prompt, {
          sessionId: launchResult.id,
          throwOnError: true,
        });
        if (!sent) {
          throw new Error("终端已启动，但提问提示词发送失败。");
        }
      } else if (task && sessionAction === "new") {
        if (typeof waitForTerminalToolSessionReady === "function" && typeof sendTerminalAutoTypedInput === "function") {
          await waitForTerminalToolSessionReady(launchResult.id);
          await sendTerminalAutoTypedInput(task, {
            sessionId: launchResult.id,
            throwOnError: false,
          });
        }
      }
      if (terminalSpecifiedTaskHasAccounts(agent)) {
        const label = terminalSpecifiedTaskAccountLabel(agent);
        setTerminalSpecifiedTaskStatus(`已启动 ${label.replace("账号", "").trim()} 终端 ${launchResult?.name || ""}。`, "ok");
        const temporary = terminalSpecifiedTaskTemporary();
        reportTerminalSpecifiedTaskPageStatus(
          temporary
            ? `已在本终端使用指定 ${label}；全局账号未改变。`
            : `已切换到指定 ${label} 并启动终端。`,
          "ok",
        );
      } else {
        const displayLabel = agent === "cdx" ? "cdx" : (terminalSpecifiedTaskAgentCommand(agent)?.label || "其它");
        setTerminalSpecifiedTaskStatus(`已启动 ${displayLabel} 终端 ${launchResult?.name || ""}。`, "ok");
        reportTerminalSpecifiedTaskPageStatus(
          needsPrompt ? `已启动 ${displayLabel} 终端：${command}，已发送提问提示词。` : `已启动 ${displayLabel} 终端：${command}`,
          "ok",
        );
      }
      closeTerminalSpecifiedTaskDialog();
    } catch (error) {
      setTerminalSpecifiedTaskStatus(error.message || "其它终端启动失败。", "warn");
    } finally {
      setTerminalSpecifiedTaskBusy(false);
    }
    return;
  }
  if (!dom.preset?.value) {
    setTerminalSpecifiedTaskStatus(`请选择 ${terminalSpecifiedTaskAccountLabel(agent)}。`, "warn");
    return;
  }
  if (agent === "codex" && mode !== "fixed" && terminalSpecifiedTaskSelectedPresetKind() === "oauth") {
    setTerminalSpecifiedTaskStatus("Codex OAuth 账号当前仅支持交互终端。", "warn");
    return;
  }
  if (mode !== "fixed" && !task) {
    setTerminalSpecifiedTaskStatus("请输入交给 Codex 的任务。", "warn");
    return;
  }
  if (agent === "claude" && mode !== "fixed") {
    setTerminalSpecifiedTaskStatus("Claude API 预设当前仅支持交互终端。", "warn");
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
      const temporary = terminalSpecifiedTaskTemporary();
      const terminalNameSuffix = sessionAction === "new"
        ? terminalSpecifiedTaskNormalizedSuffix(dom.terminalNameSuffix?.value)
        : sessionAction === "ask"
          ? terminalSpecifiedTaskQuestionNameSuffix(
              dom.terminalName?.value || terminalSpecifiedTaskSourceName,
              dom.terminalNameSuffix?.value,
            )
          : "";
      const createGroup = sessionAction === "new" && terminalSpecifiedTaskCreateGroup();
      const result = await executeSpecifiedPreset({
        action: "launch",
        agent,
        presetKind: terminalSpecifiedTaskSelectedPresetKind(),
        presetId: dom.preset.value,
        cwd: terminalSpecifiedTaskSourcePath || terminalSpecifiedTaskCurrentPath(),
        temporary,
        sessionAction,
        sessionId: dom.sessionId?.value,
        terminalName: terminalSpecifiedTaskFinalTerminalName(),
        terminalNameSuffix,
        sourceTerminalName: dom.terminalName?.value || terminalSpecifiedTaskSourceName,
        task,
        origin: "agent",
        ownerKey: "manual-specified-preset",
        quickStart: false,
        autoCompact: dom.autoCompact?.checked !== false,
        groupId: sessionAction === "new" ? terminalSpecifiedTaskSelectedGroup() : "",
        groupRole: sessionAction === "new" ? terminalSpecifiedTaskSelectedGroupRole() : "",
        createGroup,
        createGroupCount: terminalSpecifiedTaskCreateGroupCount(),
        launchTerminal: terminalSpecifiedTaskLaunchTerminal || launchTerminalSpecifiedPreset,
      });
      const launched = result.launchResult;
      // 已拿到新终端后立即关闭对话框，预设应用与后续状态通过 toast/页面状态反馈。
      if (launched) {
        rememberPreset();
        closeTerminalSpecifiedTaskDialog();
        reportTerminalSpecifiedTaskPageStatus(`已发起终端启动：${launched.name || launched.id || "新终端"}。`, "info");
      }
      const deferred = Boolean(result.applied?.deferred);
      if (deferred) {
        setTerminalSpecifiedTaskStatus(
          "永久切换已排队；当前临时任务结束并应用目标预设后，再启动新终端。",
          "info",
        );
        reportTerminalSpecifiedTaskPageStatus("永久切换已排队，未使用旧配置启动 Agent。", "info");
        return;
      }
      setTerminalSpecifiedTaskStatus(
        createGroup
          ? `已启动 ${terminalSpecifiedTaskCreateGroupCount()} 个终端并组成自组群。`
          : `已启动交互终端 ${launched?.name || launched?.id || ""}。`,
        "ok",
      );
      reportTerminalSpecifiedTaskPageStatus(
        temporary
          ? `已在本终端使用指定 ${agent === "claude" ? "Claude" : "Codex"} 预设；全局预设未改变。`
          : `已全局切换到指定 ${agent === "claude" ? "Claude" : "Codex"} 预设并启动交互终端。`,
        "ok",
      );
      closeTerminalSpecifiedTaskDialog();
      return;
    }

    const record = await executeSpecifiedPreset({
      action: "task",
      agent,
      presetKind: terminalSpecifiedTaskSelectedPresetKind(),
      mode,
      presetId: dom.preset.value,
      cwd: terminalSpecifiedTaskSourcePath || terminalSpecifiedTaskCurrentPath(),
      task,
      timeoutSecs,
      onCreated(created) {
        terminalSpecifiedTaskId = created.id;
        rememberPreset();
      },
      onProgress: renderTerminalSpecifiedTaskRecord,
    });
    reportTerminalSpecifiedTaskPageStatus(
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
    setTerminalSpecifiedTaskStatus("已请求取消，正在关闭任务进程和任务终端。", "info");
  } catch (error) {
    setTerminalSpecifiedTaskStatus(`取消失败：${error.message}`, "warn");
    if (cancelTask) {
      cancelTask.disabled = false;
    }
  }
}

function bindTerminalSpecifiedTaskDialog() {
  renderTerminalSpecifiedTaskAgents();
  terminalSpecifiedTaskSelectAgent(terminalSpecifiedTaskAgent());
  const dom = terminalSpecifiedTaskDom();
  if (!dom.dialog || !dom.form || dom.dialog.dataset.bound === "true") {
    return;
  }
  dom.dialog.dataset.bound = "true";
  dom.form.addEventListener("submit", (event) => {
    event.preventDefault();
    submitTerminalSpecifiedTask();
  });
  dom.form.addEventListener("change", async (event) => {
    if (event.target?.name === "terminal-specified-task-agent") {
      syncTerminalSpecifiedTaskForm();
      await loadTerminalSpecifiedTaskPresets();
    }
  });
  dom.form.querySelectorAll('input[name="terminal-specified-task-agent"]').forEach((input) => {
    input.addEventListener("change", async () => {
      syncTerminalSpecifiedTaskForm();
      await loadTerminalSpecifiedTaskPresets();
    });
  });
  dom.sessionOnly?.addEventListener("change", () => {
    const scopeVal = dom.sessionOnly.checked ? "terminal" : "global";
    const scopeInput = dom.form?.querySelector(
      `input[name="terminal-specified-task-preset-scope"][value="${scopeVal}"]`,
    );
    if (scopeInput) {
      scopeInput.checked = true;
    }
  });
  dom.agyContinue?.addEventListener("change", () => {
    syncTerminalSpecifiedTaskForm();
    if (!dom.agyContinue.checked) {
      dom.sessionId?.focus();
    }
  });
  dom.form.querySelectorAll('input[name="terminal-specified-task-preset-scope"]').forEach((input) => {
    input.addEventListener("change", () => {
      if (dom.sessionOnly) {
        dom.sessionOnly.checked = input.value === "terminal";
      }
    });
  });
  dom.form.querySelectorAll(
    'input[name="terminal-specified-task-mode"], input[name="terminal-specified-task-session-action"]',
  ).forEach((input) => input.addEventListener("change", async () => {
    syncTerminalSpecifiedTaskForm();
    syncTerminalSpecifiedTaskImportedTitle();
    if (input.name !== "terminal-specified-task-session-action" || input.value !== "ask" || terminalSpecifiedTaskQuestionSessionId) {
      return;
    }
    const sourceSessionId = String(state.activeSessionId || "").trim();
    const sourceContext = typeof activeTerminalContext === "object" ? activeTerminalContext : null;
    if (!sourceSessionId || sourceContext?.sessionId !== sourceSessionId) {
      setTerminalSpecifiedTaskStatus("当前终端尚未连接，无法提问。", "warn");
      return;
    }
    setTerminalSpecifiedTaskStatus("正在读取当前 Session…", "info");
    try {
      const detected = await detectAgentResumeIdComplete(sourceSessionId, sourceContext);
      if (!detected?.resumeId) {
        throw new Error("没找到当前会话 ID，无法提问。");
      }
      terminalSpecifiedTaskSetQuestionSessionId(detected.resumeId);
      syncTerminalSpecifiedTaskForm();
      setTerminalSpecifiedTaskStatus(`已读取 Session：${detected.resumeId}`, "ok");
    } catch (error) {
      setTerminalSpecifiedTaskStatus(error.message || "读取 Session 失败。", "warn");
    }
  }));
  dom.workspace?.addEventListener("change", () => {
    void setTerminalSpecifiedTaskSourcePath(dom.workspace.value);
  });
  dom.favorite?.addEventListener("change", () => {
    if (dom.favorite.value) {
      void setTerminalSpecifiedTaskSourcePath(dom.favorite.value);
    }
    dom.favorite.value = "";
  });
  dom.joinGroup?.addEventListener("change", () => {
    if (dom.joinGroup.checked) {
      renderTerminalSpecifiedTaskGroupChoices();
    }
    if (dom.joinGroup.checked && dom.newGroup?.checked) {
      dom.newGroup.checked = false;
    }
    syncTerminalSpecifiedTaskForm();
  });
  dom.newGroup?.addEventListener("change", () => {
    if (dom.newGroup.checked && dom.joinGroup?.checked) {
      dom.joinGroup.checked = false;
      if (dom.group) dom.group.disabled = true;
    }
    syncTerminalSpecifiedTaskForm();
  });
  dom.workspaceBrowse?.addEventListener("click", openTerminalSpecifiedTaskPathDialog);
  dom.pathForm?.addEventListener("submit", (event) => {
    event.preventDefault();
    if (terminalSpecifiedTaskBrowseDisplayPath) {
      void setTerminalSpecifiedTaskSourcePath(terminalSpecifiedTaskBrowseDisplayPath);
    }
    closeTerminalSpecifiedTaskPathDialog();
  });
  dom.pathInput?.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      loadTerminalSpecifiedTaskBrowsePath(dom.pathInput.value.trim());
    }
  });
  dom.pathCancel?.addEventListener("click", closeTerminalSpecifiedTaskPathDialog);
  dom.pathDialog?.addEventListener("cancel", (event) => {
    event.preventDefault();
    closeTerminalSpecifiedTaskPathDialog();
  });
  dom.terminalNameSuffix?.addEventListener("input", () => {
    renderTerminalSpecifiedTaskTerminalNamePreview();
    syncTerminalSpecifiedTaskImportedTitle();
  });
  dom.importTitle?.addEventListener("change", syncTerminalSpecifiedTaskImportedTitle);
  dom.useOriginalSuffix?.addEventListener("click", () => {
    const suffix = terminalSpecifiedTaskExtractOriginalSuffix(terminalSpecifiedTaskSourceName);
    if (!suffix) {
      setTerminalSpecifiedTaskStatus("原终端名没有可沿用的后缀。", "warn");
      dom.terminalNameSuffix?.focus();
      return;
    }
    if (dom.terminalNameSuffix) {
      dom.terminalNameSuffix.value = suffix;
      dom.terminalNameSuffix.focus();
      dom.terminalNameSuffix.setSelectionRange?.(suffix.length, suffix.length);
    }
    setTerminalSpecifiedTaskStatus("已提取原终端后缀，可继续输入。", "ok");
    renderTerminalSpecifiedTaskTerminalNamePreview();
    syncTerminalSpecifiedTaskImportedTitle();
  });
  dom.close?.addEventListener("click", closeTerminalSpecifiedTaskDialog);
  dom.dialog.addEventListener("close", () => {
    if (typeof restoreTerminalFocusAfterDialogClose === "function") {
      restoreTerminalFocusAfterDialogClose({ desktopInput: true });
    }
  });
  dom.cancelTask?.addEventListener("click", cancelTerminalSpecifiedTask);
  dom.dialog.addEventListener("cancel", (event) => {
    event.preventDefault();
    closeTerminalSpecifiedTaskDialog();
  });
  syncTerminalSpecifiedTaskForm();
}
