const WEBCLX_CODEX_TASK_FINAL_STATUSES = new Set([
  "succeeded",
  "failed",
  "timed_out",
  "cancelled",
]);

const WEBCLX_CODEX_TASK_POLL_INTERVAL_MS = 800;
const WEBCLX_AGENT_SESSION_ID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function specifiedPresetAgent(agent) {
  return String(agent || "codex").trim().toLowerCase() === "claude" ? "claude" : "codex";
}

function specifiedPresetModel(preset, agent = "codex") {
  if (specifiedPresetAgent(agent) === "claude") {
    const overrides = Array.isArray(preset?.config_overrides) ? preset.config_overrides : [];
    for (let index = overrides.length - 1; index >= 0; index -= 1) {
      const key = String(overrides[index]?.key || "").trim().toUpperCase();
      if (key === "ANTHROPIC_MODEL" || key === "MODEL") {
        return String(overrides[index]?.value || "").trim();
      }
    }
    return String(
      preset?.third_party_model
      || preset?.default_sonnet_model
      || preset?.default_opus_model
      || preset?.default_haiku_model
      || "",
    ).trim();
  }

  const overrides = Array.isArray(preset?.config_overrides) ? preset.config_overrides : [];
  for (let index = overrides.length - 1; index >= 0; index -= 1) {
    if (String(overrides[index]?.key || "").trim().toLowerCase() === "model") {
      return String(overrides[index]?.value || "").trim();
    }
  }
  return String(preset?.config_key || "").trim().toLowerCase() === "model"
    ? String(preset?.config_value || "").trim()
    : "";
}

function specifiedPresetListEndpoint(agent = "codex") {
  return specifiedPresetAgent(agent) === "claude"
    ? "/api/auth/claude-presets"
    : "/api/auth/api-presets";
}

function resolveSpecifiedPreset(presets, options = {}) {
  const list = Array.isArray(presets) ? presets : [];
  const selector = String(options.selector || "").trim();
  const match = String(options.match || "unique_contains").trim();
  if (!selector) {
    throw new Error("预设选择器不能为空。");
  }

  const normalizedSelector = selector.toLocaleLowerCase("en-US");
  const normalizeName = (item) =>
    String(item?.name || "").trim().toLocaleLowerCase("en-US");

  if (match === "id") {
    const found = list.find((item) => String(item?.id || "").trim() === selector);
    if (!found) {
      throw new Error(`没有找到匹配 ${selector} 的预设 ID。`);
    }
    return found;
  }

  if (match === "exact_name") {
    const found = list.find((item) => normalizeName(item) === normalizedSelector);
    if (!found) {
      throw new Error(`没有找到名称为 ${selector} 的预设。`);
    }
    return found;
  }

  // unique_contains: prefer exact name match, then unique substring
  const exactMatch = list.find((item) => normalizeName(item) === normalizedSelector);
  if (exactMatch) {
    return exactMatch;
  }
  const substringMatches = list.filter((item) =>
    normalizeName(item).includes(normalizedSelector),
  );
  if (substringMatches.length === 0) {
    throw new Error(`没有找到匹配 ${selector} 的 Codex API 预设。`);
  }
  if (substringMatches.length > 1) {
    const names = substringMatches
      .map((item) => String(item?.name || item?.id || "").trim())
      .join("、");
    throw new Error(`找到多个 ${selector} 预设（${names}），请保留唯一匹配项。`);
  }
  return substringMatches[0];
}

function specifiedPresetSessionAction(action = "new") {
  const normalized = String(action || "new").trim().toLowerCase();
  if (["new", "resume", "fork"].includes(normalized)) {
    return normalized;
  }
  throw new Error(`不支持的会话动作：${normalized || "空"}`);
}

function specifiedPresetSessionId(sessionId, sessionAction = "new") {
  const action = specifiedPresetSessionAction(sessionAction);
  const normalized = String(sessionId || "").trim();
  if (action === "new") {
    return "";
  }
  if (!WEBCLX_AGENT_SESSION_ID_PATTERN.test(normalized)) {
    throw new Error(`${action === "fork" ? "Fork" : "恢复"}需要有效的 session ID。`);
  }
  return normalized;
}

function shellQuoteSpecifiedPresetArgument(value) {
  return `'${String(value).replace(/'/g, `'"'"'`)}'`;
}

function specifiedPresetLaunchCommand(options = {}) {
  const agent = specifiedPresetAgent(options.agent);
  const sessionAction = specifiedPresetSessionAction(options.sessionAction);
  const sessionId = specifiedPresetSessionId(options.sessionId, sessionAction);
  const prompt = String(options.task ?? options.prompt ?? "").trim();
  const args = [agent];

  if (agent === "claude") {
    if (sessionAction !== "new") {
      args.push("--resume", sessionId);
    }
    if (sessionAction === "fork") {
      args.push("--fork-session");
    }
  } else if (sessionAction === "resume") {
    args.push("resume", sessionId);
  } else if (sessionAction === "fork") {
    args.push("fork", sessionId);
  }

  if (prompt) {
    args.push(shellQuoteSpecifiedPresetArgument(prompt));
  }
  return args.join(" ");
}

function specifiedPresetRunCommand(agent, presetId, command) {
  const normalizedAgent = specifiedPresetAgent(agent);
  const normalizedPresetId = String(presetId || "").trim();
  const launchCommand = String(command || "").trim();
  if (!normalizedPresetId) {
    throw new Error("指定预设 ID 不能为空。");
  }
  if (!launchCommand) {
    throw new Error("Agent 启动命令为空。");
  }
  const presetKind = normalizedAgent === "claude" ? "claude" : "api";
  return `webclx run ${presetKind} ${shellQuoteSpecifiedPresetArgument(normalizedPresetId)} -- ${launchCommand}`;
}

function specifiedPresetLeaseKind(agent) {
  return specifiedPresetAgent(agent) === "claude" ? "claude" : "api";
}

async function acquireSpecifiedPresetLease(options = {}) {
  const response = await requestJson("/api/auth/preset-run-leases", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      preset_kind: specifiedPresetLeaseKind(options.agent),
      preset_id: String(options.presetId || "").trim(),
      project_path: String(options.projectPath || "").trim(),
      owner: String(options.owner || "terminal-specified-preset").trim() || "terminal-specified-preset",
    }),
  });
  if (!response?.lease_id) {
    throw new Error("临时预设租约未返回 ID。");
  }
  return response;
}

async function releaseSpecifiedPresetLease(leaseId) {
  await requestJson(
    `/api/auth/preset-run-leases/${encodeURIComponent(leaseId)}`,
    { method: "DELETE" },
  );
}

async function waitForSpecifiedPresetAgentStart(
  sessionId,
  leaseId,
  timeoutMs = 120000,
) {
  const targetSessionId = String(sessionId || "").trim();
  if (!targetSessionId) {
    return false;
  }
  const startedAt = Date.now();
  let lastHeartbeat = 0;
  while (Date.now() - startedAt < timeoutMs) {
    try {
      const response = await requestJson(
        `/api/terminal/sessions/${encodeURIComponent(targetSessionId)}/agent-session`,
      );
      if (response?.resume_id || response?.command) {
        return true;
      }
    } catch {}
    if (leaseId && Date.now() - lastHeartbeat >= 15000) {
      try {
        await requestJson(
          `/api/auth/preset-run-leases/${encodeURIComponent(leaseId)}/heartbeat`,
          { method: "PUT" },
        );
        lastHeartbeat = Date.now();
      } catch {}
    }
    await new Promise((resolve) => window.setTimeout(resolve, 1000));
  }
  return false;
}

async function launchSpecifiedPresetTemporary(options, launchTerminal, terminalOptions) {
  const lease = await acquireSpecifiedPresetLease({
    agent: specifiedPresetAgent(options.agent),
    presetId: options.presetId,
    projectPath: options.projectPath || options.cwd,
    owner: options.ownerKey,
  });
  let launchResult = null;
  try {
    launchResult = await launchTerminal(String(options.cwd || ""), terminalOptions);
    const sessionId = String(
      launchResult?.id || launchResult?.sessionId || "",
    ).trim();
    await waitForSpecifiedPresetAgentStart(sessionId, lease.lease_id);
  } catch (error) {
    try {
      await releaseSpecifiedPresetLease(lease.lease_id);
    } catch {}
    throw error;
  }
  await releaseSpecifiedPresetLease(lease.lease_id);
  return {
    applied: {
      deferred: false,
      temporary: true,
      name: lease.name,
      preset_id: lease.preset_id,
    },
    lease_id: lease.lease_id,
    launchResult,
  };
}

function specifiedPresetTerminalName(options = {}) {
  const explicitName = String(options.terminalName || "").trim();
  if (explicitName) {
    return explicitName;
  }
  const sourceName = String(options.sourceTerminalName || "").trim();
  if (!sourceName) {
    return "";
  }
  const sessionAction = specifiedPresetSessionAction(options.sessionAction);
  if (sessionAction === "fork") {
    return `${sourceName}_fork`;
  }
  if (sessionAction === "resume") {
    return `${sourceName}_resume`;
  }
  return sourceName;
}

function normalizeSpecifiedPresetSelector(options = {}) {
  const supplied = options.preset && typeof options.preset === "object"
    ? options.preset
    : {
        id: options.presetId,
        name: options.presetName,
        model: options.presetModel,
      };
  const selector = {};
  for (const key of ["id", "name", "model"]) {
    const value = String(supplied[key] || "").trim();
    if (value) {
      selector[key] = value;
    }
  }
  if (Object.keys(selector).length !== 1) {
    throw new Error("必须且只能通过 id、名称或模型指定一个预设。");
  }
  return selector;
}

function specifiedPresetApplyEndpoint({ agent, presetId, projectPath, respectSavedProxyPreference }) {
  const normalizedAgent = specifiedPresetAgent(agent);
  const base = normalizedAgent === "claude"
    ? `/api/auth/claude-presets/${encodeURIComponent(presetId)}/apply`
    : `/api/auth/api-presets/${encodeURIComponent(presetId)}/apply`;
  if (normalizedAgent === "claude") {
    return base;
  }

  const params = new URLSearchParams();
  const normalizedProjectPath = String(projectPath || "").trim();
  if (normalizedProjectPath) {
    params.set("project_path", normalizedProjectPath);
  }
  if (respectSavedProxyPreference === false) {
    params.set("respect_saved_proxy_preference", "false");
  }
  const query = params.toString();
  return query ? `${base}?${query}` : base;
}

async function waitForSpecifiedCodexTask(
  taskId,
  { onProgress = null, pollIntervalMs = WEBCLX_CODEX_TASK_POLL_INTERVAL_MS, signal = null } = {},
) {
  const id = String(taskId || "").trim();
  if (!id) {
    throw new Error("Codex 任务未返回任务 ID。");
  }
  while (true) {
    if (signal?.aborted) {
      throw new Error("已停止等待 Codex 任务结果。");
    }
    const record = await requestJson(`/api/codex/tasks/${encodeURIComponent(id)}`);
    onProgress?.(record);
    if (WEBCLX_CODEX_TASK_FINAL_STATUSES.has(record.status)) {
      return record;
    }
    await new Promise((resolve) => window.setTimeout(resolve, pollIntervalMs));
  }
}

async function executeSpecifiedPreset(options = {}) {
  const selector = normalizeSpecifiedPresetSelector(options);
  const agent = specifiedPresetAgent(options.agent);
  const action = String(
    options.action || (options.task ? "task" : options.command ? "launch" : "apply"),
  ).trim().toLowerCase();

  if (action === "task") {
    if (agent !== "codex") {
      throw new Error("原生任务 API 当前只支持 Codex API 预设。");
    }
    const task = String(options.task || "").trim();
    if (!task) {
      throw new Error("请输入交给 Codex 的任务。");
    }
    const timeoutSecs = options.timeoutSecs === undefined
      ? 1800
      : Number(options.timeoutSecs);
    if (!Number.isFinite(timeoutSecs) || timeoutSecs < 1 || timeoutSecs > 7200) {
      throw new Error("超时时间必须在 1 到 7200 秒之间。");
    }
    const payload = {
      mode: options.mode === "terminal" ? "terminal" : "exec",
      preset: selector,
      cwd: String(options.cwd || ""),
      task,
      timeout_secs: timeoutSecs,
    };
    if (options.outputSchema !== undefined && options.outputSchema !== null) {
      payload.output_schema = options.outputSchema;
    }
    const created = await requestJson("/api/codex/tasks", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    options.onCreated?.(created);
    options.onProgress?.(created);
    if (options.waitForResult === false) {
      return created;
    }
    return waitForSpecifiedCodexTask(created.id, {
      onProgress: options.onProgress,
      pollIntervalMs: options.pollIntervalMs,
      signal: options.signal,
    });
  }

  if (!["apply", "launch"].includes(action)) {
    throw new Error(`不支持的指定预设动作：${action || "空"}`);
  }
  if (!selector.id) {
    throw new Error("应用或启动预设时必须提供预设 ID。");
  }
  let launchTerminal = null;
  let terminalOptions = null;
  let temporaryLaunch = false;
  if (action === "launch") {
    launchTerminal = options.launchTerminal
      || (typeof openFreshTerminalSession === "function" ? openFreshTerminalSession : null);
    if (typeof launchTerminal !== "function") {
      throw new Error("当前页面不能启动新终端。");
    }
    const requestedLaunchCommand = String(options.command || "").trim() || specifiedPresetLaunchCommand({
        agent,
        sessionAction: options.sessionAction,
        sessionId: options.sessionId,
        task: options.task ?? options.prompt,
      });
    temporaryLaunch = options.temporary === true;
    if (temporaryLaunch && requestedLaunchCommand.startsWith("webclx run ")) {
      throw new Error("临时切换不支持嵌套 webclx run 命令。");
    }
    const runCommand = temporaryLaunch
      ? requestedLaunchCommand
      : requestedLaunchCommand.startsWith("webclx run ")
        ? requestedLaunchCommand
        : specifiedPresetRunCommand(agent, selector.id, requestedLaunchCommand);
    terminalOptions = {
      runCommand,
      quickStart: options.quickStart === true,
      origin: options.origin,
      ownerKey: options.ownerKey,
      codexApiPresetId: agent === "codex" ? selector.id : "",
    };
    const terminalName = specifiedPresetTerminalName(options);
    if (terminalName) {
      terminalOptions.terminalName = terminalName;
    }
  }
  if (action === "apply") {
    const applied = await requestJson(specifiedPresetApplyEndpoint({
      agent,
      presetId: selector.id,
      projectPath: options.projectPath || options.cwd,
      respectSavedProxyPreference: options.respectSavedProxyPreference,
    }), { method: "PUT" });
    options.onApplied?.(applied);
    return applied;
  }
  if (temporaryLaunch) {
    return launchSpecifiedPresetTemporary(options, launchTerminal, terminalOptions);
  }
  // A deferred apply is already queued. The terminal still starts, but its
  // `webclx run` command waits until the queued preset is written before it
  // launches the agent, so no stale shared config is ever used.
  const applied = await requestJson(specifiedPresetApplyEndpoint({
    agent,
    presetId: selector.id,
    projectPath: options.projectPath || options.cwd,
    respectSavedProxyPreference: options.respectSavedProxyPreference,
  }), { method: "PUT" });
  options.onApplied?.(applied);
  const launchResult = await launchTerminal(String(options.cwd || ""), terminalOptions);
  return { applied, launchResult };
}
