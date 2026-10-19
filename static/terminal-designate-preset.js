let terminalDesignateForkRunning = false;

async function openTerminalNewSessionDialog(trigger = null, sourcePath = state.currentPath, options = {}) {
  const sessionAction = options.sessionAction || "new";
  const sessionId = String(options.sessionId || "").trim();
  let questionSessionId = String(options.questionSessionId || "").trim();
  const titleBySessionAction = {
    fork: "Fork 会话",
    resume: "进入会话",
    new: "新开终端",
    ask: "临时/新终端/提问",
  };
  await openTerminalSpecifiedTaskDialog(trigger, {
    agent: options.agent || "codex",
    launchTerminal: options.launchTerminal,
    lockAgent: Boolean(sessionId),
    mode: "fixed",
    resetPreset: true,
    resetTask: true,
    sessionAction,
    sessionId,
    terminalName: options.terminalName,
    questionSessionId,
    showSessionField: Boolean(sessionId),
    temporary: options.temporary === undefined ? false : options.temporary,
    sourceTerminalName: options.sourceTerminalName,
    sourcePath,
    title: titleBySessionAction[sessionAction] || "新开终端",
    updatePageStatus: options.updatePageStatus,
  });

  // 先显示对话框，再读取 Session。读取链可能需要等待终端输出，不能阻塞对话框打开。
  if (sessionAction === "ask" && !questionSessionId) {
    const sourceSessionId = String(state.activeSessionId || "").trim();
    const sourceContext = typeof activeTerminalContext === "object" ? activeTerminalContext : null;
    if (!sourceSessionId || sourceContext?.sessionId !== sourceSessionId) {
      updateStatus("当前终端尚未连接，无法提问。", "warn");
      return;
    }
    updateStatus("正在读取当前 Session…", "info");
    try {
      const detected = await detectAgentResumeIdComplete(sourceSessionId, sourceContext);
      questionSessionId = String(detected?.resumeId || "").trim();
      if (!questionSessionId) {
        throw new Error("没找到当前会话 ID，无法提问。");
      }
      terminalSpecifiedTaskSetQuestionSessionId(questionSessionId);
      syncTerminalSpecifiedTaskImportedTitle();
      updateStatus(`已读取 Session：${questionSessionId}`, "ok");
    } catch (error) {
      updateStatus(error?.message || "读取当前 Session 失败。", "warn");
    }
  }
}

// 指定预设与“高级复制”共用屏幕 -> /status -> 后端完整探测链。
async function detectTerminalDesignateResumeId(sourceSessionId, sourceContext) {
  return detectAgentResumeIdComplete(sourceSessionId, sourceContext);
}

async function openTerminalDesignatePresetDialog({
  cwd = state.currentPath,
  program = "codex",
  sessionId = "",
  sourceTerminalName,
  terminalName = "",
  namingAction = "",
  trigger = null,
} = {}) {
  const normalizedSessionId = String(sessionId || "").trim();
  const requestedAction = String(
    namingAction || (normalizedSessionId ? "resume" : "new"),
  ).trim().toLowerCase();
  const resolvedNamingAction = ["new", "resume", "fork"].includes(requestedAction)
    ? requestedAction
    : "new";
  // 对话框标题跟随实际命名动作，避免 resume 入口被显示为 fork。
  const titleByNamingAction = {
    fork: "指定预设+fork",
    resume: "指定预设+resume",
    new: "指定预设终端",
  };
  const dialogOptions = {
    agent: specifiedPresetAgent(program),
    lockAgent: Boolean(normalizedSessionId),
    mode: "fixed",
    resetTask: true,
    temporary: true,
    sessionAction: resolvedNamingAction,
    sessionId: normalizedSessionId,
    showSessionField: true,
    sourcePath: String(cwd ?? state.currentPath),
    title: titleByNamingAction[resolvedNamingAction] || "指定预设终端",
  };
  if (sourceTerminalName !== undefined) {
    dialogOptions.sourceTerminalName = sourceTerminalName;
  }
  if (terminalName) {
    dialogOptions.terminalName = terminalName;
  }
  if (namingAction) {
    dialogOptions.namingAction = namingAction;
  }
  await openTerminalSpecifiedTaskDialog(trigger, dialogOptions);
}

async function openTerminalDesignatePresetForkDialog(trigger = null) {
  if (terminalDesignateForkRunning) {
    updateStatus("正在等待 /fork 生成新的 Session…", "info");
    return;
  }
  const sourceSessionId = String(state.activeSessionId || "").trim();
  const sourceSession = state.sessions.find((session) => session.id === sourceSessionId);
  if (!sourceSessionId || !sourceSession) {
    updateStatus("当前没有可 fork 的活动终端。", "warn");
    return;
  }
  const sourcePath = String(sessionPath(sourceSession) ?? state.currentPath);
  const sourceTerminalName = String(sourceSession.name || "").trim();
  terminalDesignateForkRunning = true;
  updateStatus("正在读取当前 Session 并执行 /fork…", "info");
  try {
    await waitForTerminalToolSessionReady(sourceSessionId);
    const sourceContext = ensureTerminalSessionCache().get(sourceSessionId);
    if (!sourceContext) {
      throw new Error("无法监听原终端的 /fork 输出。");
    }
    // 只读取当前 Session。真正的 fork 在新终端执行，源终端保持不变。
    const baseline = await detectTerminalDesignateResumeId(sourceSessionId, sourceContext);
    if (!baseline?.resumeId) {
      throw new Error("无法提取当前 Session，未启动 fork。");
    }
    updateStatus(
      `fork 已就绪：新终端将复制 ${shortResumeId(baseline.resumeId)}`,
      "ok",
    );
    await openTerminalDesignatePresetDialog({
      cwd: sourcePath,
      program: baseline.program,
      sessionId: baseline.resumeId,
      sourceTerminalName,
      namingAction: "fork",
      trigger,
    });
  } catch (error) {
    updateStatus(error?.message || "指定预设 fork 失败。", "warn");
  } finally {
    terminalDesignateForkRunning = false;
  }
}


// 指定预设+resume：通过统一完整链检测当前 Session 后在当前项目目录
// 新建终端并以 codex resume / claude --resume 恢复原会话，不改名原终端。
// 命名动作为 resume，预览/实际名称都使用 <原终端名>_resume。
async function openTerminalDesignatePresetResumeDialog(trigger = null) {
  const sourceSessionId = String(state.activeSessionId || "").trim();
  const sourceSession = state.sessions.find((session) => session.id === sourceSessionId);
  if (!sourceSessionId || !sourceSession) {
    updateStatus("当前没有可恢复的活动终端。", "warn");
    return;
  }
  const sourcePath = String(sessionPath(sourceSession) ?? state.currentPath);
  const sourceTerminalName = String(sourceSession.name || "").trim();
  updateStatus("正在读取当前 Session…", "info");
  try {
    await waitForTerminalToolSessionReady(sourceSessionId);
    const sourceContext = ensureTerminalSessionCache().get(sourceSessionId) || null;
    const detected = await detectTerminalDesignateResumeId(sourceSessionId, sourceContext);
    if (!detected?.resumeId) {
      throw new Error("无法提取当前 Session，请确认 codex/claude 已显示 Session。");
    }
    updateStatus(
      `resume 已就绪：新终端将恢复 ${shortResumeId(detected.resumeId)}`,
      "ok",
    );
    await openTerminalDesignatePresetDialog({
      cwd: sourcePath,
      program: detected.program,
      sessionId: detected.resumeId,
      sourceTerminalName,
      namingAction: "resume",
      trigger,
    });
  } catch (error) {
    updateStatus(error?.message || "指定预设 resume 失败。", "warn");
  }
}
