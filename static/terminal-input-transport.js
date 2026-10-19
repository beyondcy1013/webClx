function websocketUrl(context = activeTerminalContext) {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  const query = new URLSearchParams();
  const path = context?.path ?? state.currentPath;
  const sessionId = context?.sessionId ?? state.activeSessionId;
  if (path) {
    query.set("path", path);
  }
  if (sessionId && sessionId !== TERMINAL_EMPTY_CONTEXT_ID) {
    query.set("session_id", sessionId);
  }
  query.set("visible", terminalContextOutputVisible(context) ? "true" : "false");
  // seq 帧协议：服务端确认后二进制输出帧带 8 字节 seq 前缀，客户端借此
  // 记录已应用水位，重连时可按水位增量续传而不必清空终端全量回放。
  query.set("seq_framed", "1");
  if (
    context?.hasLoadedOutput &&
    !context.outputStreamGap &&
    Number(context.lastOutputSeq) > 0 &&
    context.outputInstanceId
  ) {
    query.set("resume_seq", String(context.lastOutputSeq));
    query.set("resume_instance", String(context.outputInstanceId));
  }
  return `${protocol}//${window.location.host}/api/terminal/ws?${query.toString()}`;
}

function terminalContextOutputVisible(context) {
  return Boolean(
    context &&
      context === activeTerminalContext &&
      document.visibilityState === "visible",
  );
}

function sendTerminalContextVisibility(context, visible) {
  const contextSocket = context?.socket;
  if (contextSocket && contextSocket.readyState === WebSocket.OPEN) {
    contextSocket.send(
      JSON.stringify({
        type: "visibility",
        visible: Boolean(visible) && terminalContextOutputVisible(context),
      }),
    );
  }
}

function syncActiveTerminalContextOutputVisibility() {
  sendTerminalContextVisibility(activeTerminalContext, true);
}

const TERMINAL_PENDING_INPUT_MAX_CHARS = 65536;

function sendMessage(message, context = activeTerminalContext) {
  const contextSocket = context?.socket;
  if (!contextSocket || contextSocket.readyState !== WebSocket.OPEN) {
    return false;
  }
  try {
    contextSocket.send(JSON.stringify(message));
    return true;
  } catch {
    return false;
  }
}

function flushTerminalInputQueue(context = activeTerminalContext) {
  if (!context) {
    return false;
  }
  if (context.inputFlushTimer !== null) {
    window.clearTimeout(context.inputFlushTimer);
    context.inputFlushTimer = null;
  }
  if (context.pendingInputChars === 0 || context.pendingInputQueue.length === 0) {
    return true;
  }
  if (!terminalContextSocketOpen(context)) {
    return false;
  }

  const data = context.pendingInputQueue.join("");
  if (!sendMessage({ type: "input", data }, context)) {
    return false;
  }
  context.pendingInputQueue = [];
  context.pendingInputChars = 0;
  if (typeof trimTerminalSessionCache === "function") {
    trimTerminalSessionCache();
  }
  return true;
}

function scheduleTerminalInputFlush(context) {
  if (!context || context.inputFlushTimer !== null) {
    return;
  }
  context.inputFlushTimer = window.setTimeout(() => {
    context.inputFlushTimer = null;
    flushTerminalInputQueue(context);
  }, TERMINAL_INPUT_FLUSH_DELAY_MS);
}

function queueTerminalInput(data, context = activeTerminalContext) {
  if (!context || typeof data !== "string" || data.length === 0) {
    return false;
  }
  const available = TERMINAL_PENDING_INPUT_MAX_CHARS - context.pendingInputChars;
  if (data.length > available) {
    updateSessionStatus("待发送输入已达上限，本次输入未发送，请等待重连后重试。", "warn");
    return false;
  }
  context.pendingInputQueue.push(data);
  context.pendingInputChars += data.length;
  scheduleTerminalInputFlush(context);
  return true;
}

// Returns acceptance, not a server/Agent acknowledgement. requireConnected
// callers must never queue a paste that the UI will invite the user to retry.
function sendTerminalInput(data, options = {}) {
  if (typeof data !== "string" || !data) {
    return false;
  }

  if (!options.terminalReport) {
    interruptTerminalBacklogReplayForInput();
    if (options.refreshVisibility !== false) {
      refreshTerminalInputVisibilityAfterUserInput();
    }
  }
  const context = activeTerminalContext;
  if (!context) {
    updateSessionStatus("当前没有可接收输入的终端。", "warn");
    return false;
  }
  if (options.requireConnected && !terminalContextSocketOpen(context)) {
    return false;
  }
  if (terminalContextSocketOpen(context) &&
      (options.flush || data.length >= 1024 || /[\r\n\u0003\u0004]/.test(data))) {
    // Flush older keystrokes first; send this input as one complete message.
    // Never pass a connected paste through the bounded reconnect queue.
    return flushTerminalInputQueue(context) && sendMessage({ type: "input", data }, context);
  }
  return queueTerminalInput(data, context);
}

function sendTerminalInputToSession(data, sessionId) {
  const targetSessionId = String(sessionId || "").trim();
  if (!data || !targetSessionId) {
    return false;
  }
  const context = ensureTerminalSessionCache().get(targetSessionId);
  if (!terminalContextSocketOpen(context)) {
    return false;
  }
  if (context === activeTerminalContext) {
    interruptTerminalBacklogReplayForInput();
    refreshTerminalInputVisibilityAfterUserInput();
  }
  context.socket.send(JSON.stringify({ type: "input", data }));
  return true;
}

function clearRunCommandFromUrl() {
  const url = new URL(window.location.href);
  if (!url.searchParams.has("run") && !url.searchParams.has("command")) {
    return;
  }

  url.searchParams.delete("run");
  url.searchParams.delete("command");
  window.history.replaceState({}, "", `${url.pathname}${url.search}${url.hash}`);
}

function runPendingTerminalCommand() {
  const command = state.pendingRunCommand;
  if (!command || !isTerminalConnected()) {
    return false;
  }
  if (!terminalInitialReplaySettled()) {
    return false;
  }

  state.pendingRunCommand = "";
  cancelNewSessionQuickStart();
  clearRunCommandFromUrl();
  window.setTimeout(async () => {
    if (!isTerminalConnected()) {
      return;
    }
    const sent = await sendTerminalAutoTypedInput(command);
    updateStatus(sent ? `已按当前预设执行：${command}` : "命令发送失败。", sent ? "ok" : "warn");
    focusTerminalSoon();
  }, 150);
  return true;
}

function terminalInitialReplaySettled(context = activeTerminalContext) {
  if (!context) {
    return false;
  }
  return (
    !context.initialReplayPending &&
    !context.backlogReplayActive &&
    !context.backlogReplayEndQueued &&
    !context.outputReplayWriteInFlight &&
    !context.outputQueue.some((item) => item.replay || item.kind === "backlog_replay_end")
  );
}

function maybeRunTerminalStartupActions() {
  if (!isTerminalConnected() || !terminalInitialReplaySettled()) {
    return false;
  }

  if (runPendingTerminalCommand()) {
    return true;
  }

  activateNewSessionQuickStart();
  return Boolean(pendingNewSessionQuickStart);
}
