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

function sendMessage(message) {
  const activeSocket = activeTerminalContext?.socket;
  if (activeSocket && activeSocket.readyState === WebSocket.OPEN) {
    activeSocket.send(JSON.stringify(message));
  }
}

function flushTerminalInputQueue() {
  if (terminalInputFlushTimer !== null) {
    window.clearTimeout(terminalInputFlushTimer);
    terminalInputFlushTimer = null;
  }
  if (terminalInputQueue.length === 0) {
    return;
  }

  const data = terminalInputQueue.join("");
  terminalInputQueue = [];
  sendMessage({
    type: "input",
    data,
  });
}

function scheduleTerminalInputFlush() {
  if (terminalInputFlushTimer !== null) {
    return;
  }
  terminalInputFlushTimer = window.setTimeout(() => {
    terminalInputFlushTimer = null;
    flushTerminalInputQueue();
  }, TERMINAL_INPUT_FLUSH_DELAY_MS);
}

function queueTerminalInput(data) {
  if (typeof data !== "string" || data.length === 0) {
    return;
  }
  terminalInputQueue.push(data);
  scheduleTerminalInputFlush();
}

function sendTerminalInput(data, options = {}) {
  if (!data) {
    return;
  }

  interruptTerminalBacklogReplayForInput();
  if (options.refreshVisibility !== false) {
    refreshTerminalInputVisibilityAfterUserInput();
  }
  if (options.flush || data.length >= 1024 || /[\r\n\u0003\u0004]/.test(data)) {
    terminalInputQueue.push(data);
    flushTerminalInputQueue();
    return;
  }
  queueTerminalInput(data);
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
    !context.outputWriteInFlight &&
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
