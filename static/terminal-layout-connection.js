// webClx terminal layout, reconnect, WebSocket, and session selection helpers.
// Extracted from terminal.js as global function declarations.
// Keep top-level ResizeObserver setup in terminal.js because it depends on terminalHost.

// ── Reconnect failure tracking & automatic server re-evaluation ──────────────
// When the WebSocket disconnects repeatedly, the current server may be
// unreachable.  Instead of blindly retrying the same host forever, we trigger
// TERMINAL_SERVER_PROBE.reevaluate() after a few consecutive failures to find
// and switch to a faster reachable candidate.
const RECONNECT_FAILURE_THRESHOLD = 3;
let terminalReconnectFailures = 0;
let terminalServerProbeInFlight = false;

function resetTerminalReconnectFailures() {
  terminalReconnectFailures = 0;
}

async function maybeReevaluateServer(reason) {
  if (terminalServerProbeInFlight) return;
  if (typeof TERMINAL_SERVER_PROBE === "undefined") return;
  terminalServerProbeInFlight = true;
  try {
    await TERMINAL_SERVER_PROBE.reevaluate(reason);
  } finally {
    terminalServerProbeInFlight = false;
  }
}

window.addEventListener("online", () => {
  console.debug("server-probe: browser online event, re-evaluating");
  maybeReevaluateServer("online");
});

function syncTerminalSize({ force = false } = {}) {
  const context = activeTerminalContext;
  if (!context || context.term !== term) {
    return;
  }
  const nextSize = {
    cols: Math.max(term.cols, 2),
    rows: Math.max(term.rows, 2),
  };

  if (
    !force &&
    context.lastTerminalSize &&
    context.lastTerminalSize.cols === nextSize.cols &&
    context.lastTerminalSize.rows === nextSize.rows
  ) {
    return;
  }

  context.lastTerminalSize = nextSize;
  lastTerminalSize = nextSize;
  pendingTerminalSize = { context, size: nextSize };
  if (terminalSizeFlushTimer !== null) {
    window.clearTimeout(terminalSizeFlushTimer);
  }
  terminalSizeFlushTimer = window.setTimeout(() => {
    terminalSizeFlushTimer = null;
    flushTerminalSize();
  }, force ? 0 : TERMINAL_RESIZE_FLUSH_DELAY_MS);
}

function flushTerminalSize() {
  if (terminalSizeFlushTimer !== null) {
    window.clearTimeout(terminalSizeFlushTimer);
    terminalSizeFlushTimer = null;
  }
  if (!pendingTerminalSize) {
    return;
  }
  const pending = pendingTerminalSize;
  pendingTerminalSize = null;
  if (pending.context !== activeTerminalContext || pending.context.term !== term) {
    return;
  }
  const nextSize = pending.size;
  sendMessage({
    type: "resize",
    cols: nextSize.cols,
    rows: nextSize.rows,
  });
}

function syncTerminalOverlayBounds() {
  if (!term.element) {
    return;
  }

  bindTerminalViewportScroll();
  const viewport = term.element.querySelector(".xterm-viewport");
  const screen = term.element.querySelector(".xterm-screen");
  if (!(viewport instanceof HTMLElement) || !(screen instanceof HTMLElement)) {
    return;
  }

  // Derive width from the renderer canvas so the screen element matches the
  // exact pixel grid xterm.js paints into.  Using viewport.clientWidth can
  // disagree with the internal canvas by 1-2 px, which misaligns box-drawing
  // characters across the width of the terminal.
  let width = 0;
  const renderer = term._core?._renderService;
  if (renderer?.dimensions) {
    width = Math.ceil(renderer.dimensions.actualCellWidth * term.cols);
  }
  if (!width) {
    const canvas = screen.querySelector("canvas");
    if (canvas instanceof HTMLCanvasElement && canvas.width > 0) {
      width = canvas.width;
    }
  }
  if (!width) {
    width = Math.ceil(viewport.clientWidth || viewport.getBoundingClientRect().width);
  }
  if (!width) {
    return;
  }

  screen.style.width = `${width}px`;
  term.element.querySelectorAll(".xterm-accessibility, .xterm-message").forEach((overlay) => {
    if (!(overlay instanceof HTMLElement)) {
      return;
    }
    overlay.style.width = `${width}px`;
  });
  syncTerminalSelectionHandles();
}

function applyTerminalWideModeLayout() {
  if (state.terminalWideMode) {
    document.body.dataset.terminalWideMode = "enabled";
  } else {
    delete document.body.dataset.terminalWideMode;
  }

  if (!terminalHost) {
    return;
  }

  if (!state.terminalWideMode) {
    terminalHost.style.width = "";
    terminalHost.style.maxWidth = "";
    if (terminalScrollShellEl) {
      terminalScrollShellEl.scrollLeft = 0;
    }
    return;
  }

  const baseWidth = Math.max(
    Math.round(
      terminalScrollShellEl?.clientWidth ||
        terminalPanelEl?.clientWidth ||
        terminalHost.getBoundingClientRect().width ||
        window.innerWidth ||
        0,
    ),
    1,
  );
  const wideWidth = Math.min(
    Math.max(Math.round(baseWidth * TERMINAL_WIDE_MODE_WIDTH_RATIO), TERMINAL_WIDE_MODE_MIN_WIDTH_PX),
    TERMINAL_WIDE_MODE_MAX_WIDTH_PX,
  );
  terminalHost.style.width = `${wideWidth}px`;
  terminalHost.style.maxWidth = "none";
}

function setTerminalWideMode(enabled, { announce = true } = {}) {
  const nextEnabled = Boolean(enabled);
  if (state.terminalWideMode === nextEnabled) {
    applyTerminalWideModeLayout();
    return;
  }

  state.terminalWideMode = nextEnabled;
  storeTerminalWideMode(nextEnabled);
  applyTerminalWideModeLayout();
  fitTerminal({ force: true });
  syncTerminalStickyOffsets();
  syncTerminalNavScroll();
  updateTerminalScrollBottomButton();
  updatePageScrollRail();
  if (announce) {
    updateStatus(
      nextEnabled ? "已开启宽屏：可横向滑动查看更长的行。" : "已恢复普通宽度。",
      "ok",
    );
  }
}

function toggleTerminalWideMode() {
  setTerminalWideMode(!state.terminalWideMode);
  focusTerminalSoon();
}

// 在 systemd 关机流程之前由用户主动触发：先在后端把活动 agent 会话的 resume
// 记录落盘（此时 tmux server 与 codex/claude 进程仍存活，检测才能成功），再调度
// 系统关机。直接靠 systemd 关机的 graceful shutdown 保存会因为 tmux 被先杀而失效。
function saveAndPoweroff() {
  if (!window.confirm("将保存当前活动会话并关闭系统，确定继续吗？")) {
    return;
  }
  updateStatus("正在保存会话并准备关机…", "info");
  requestJson("/api/system/save-and-poweroff", { method: "POST" })
    .then((result) => {
      const saved = Number.isFinite(result?.saved) ? result.saved : 0;
      updateStatus(
        saved > 0
          ? `已保存 ${saved} 个会话，系统即将关机。`
          : "没有需要保存的活动会话，系统即将关机。",
        "ok",
      );
    })
    .catch((error) => {
      updateStatus(`保存会话或关机失败：${error?.message || error}`, "warn");
    });
}

// 与「保存会话并关机」对称：先在后端把活动 agent 会话的 resume 记录落盘，再杀掉
// webClx 的 tmux scope 并调度 webclx.service 重启。服务重启后走「从恢复记录重建终端」
// 的完整路径，与关机一致；显式杀 scope 是因为 systemd scope 隔离会让 tmux 续命，
// 否则重启只是重连、不会触发恢复链路。
function saveAndRestartService() {
  if (!window.confirm("将保存当前活动会话、结束终端 tmux 进程并重启 webclx 服务，确定继续吗？")) {
    return;
  }
  updateStatus("正在保存会话并准备重启服务…", "info");
  requestJson("/api/system/save-and-restart", { method: "POST" })
    .then((result) => {
      const saved = Number.isFinite(result?.saved) ? result.saved : 0;
      updateStatus(
        saved > 0
          ? `已保存 ${saved} 个会话，webclx 服务即将重启。`
          : "没有需要保存的活动会话，webclx 服务即将重启。",
        "ok",
      );
    })
    .catch((error) => {
      updateStatus(`保存会话或重启服务失败：${error?.message || error}`, "warn");
    });
}


function fitTerminal({ force = false } = {}) {
  preserveTerminalScrollDuringLayout(() => {
    syncTerminalHostHeight();
    applyTerminalWideModeLayout();
    fitAddon.fit();
    syncTerminalOverlayBounds();
    syncTerminalSize({ force });
    updateTerminalScrollBottomButton();
    syncTerminalCursorCorrection();
  });
}

function ensureFontsReady() {
  if (document.fonts?.ready) {
    return document.fonts.ready;
  }
  return Promise.resolve();
}

function resetTerminalSizeSync() {
  cancelTerminalSizeSettle();
  lastTerminalSize = null;
  if (activeTerminalContext) {
    activeTerminalContext.lastTerminalSize = null;
  }
  pendingTerminalSize = null;
  if (terminalSizeFlushTimer !== null) {
    window.clearTimeout(terminalSizeFlushTimer);
    terminalSizeFlushTimer = null;
  }
  // Clear forced inline width so a reconnect starts from clean DOM state.
  const screen = term.element?.querySelector(".xterm-screen");
  if (screen instanceof HTMLElement) {
    screen.style.width = "";
  }
}

function cancelTerminalSizeSettle() {
  terminalSizeSettleToken += 1;
  if (terminalSizeSettleTimer !== null) {
    window.clearTimeout(terminalSizeSettleTimer);
    terminalSizeSettleTimer = null;
  }
}

function scheduleTerminalSizeSettle({ frames = TERMINAL_SIZE_SETTLE_FRAMES } = {}) {
  if (!terminalHost || frames <= 0) {
    return;
  }

  const token = ++terminalSizeSettleToken;
  let remaining = Math.max(Math.trunc(Number(frames) || 0), 1);

  const run = () => {
    if (token !== terminalSizeSettleToken) {
      return;
    }

    // Refit while layout settles, but only publish a PTY resize when the cell
    // grid actually changes. The connection path performs its own handshake.
    fitTerminal();
    remaining -= 1;
    if (remaining <= 0) {
      terminalSizeSettleTimer = null;
      return;
    }

    terminalSizeSettleTimer = window.setTimeout(run, TERMINAL_SIZE_SETTLE_INTERVAL_MS);
  };

  if (terminalSizeSettleTimer !== null) {
    window.clearTimeout(terminalSizeSettleTimer);
    terminalSizeSettleTimer = null;
  }

  window.requestAnimationFrame(run);
}

function terminalContextSocketOpen(context) {
  return Boolean(context?.socket && context.socket.readyState === WebSocket.OPEN);
}

function terminalContextSocketConnecting(context) {
  return Boolean(context?.socket && context.socket.readyState === WebSocket.CONNECTING);
}

function scheduleReconnect(context = activeTerminalContext) {
  if (!context || context.disposed || context.reconnectTimer || !context.sessionId) {
    return;
  }

  terminalReconnectFailures++;
  const shouldProbe = terminalReconnectFailures >= RECONNECT_FAILURE_THRESHOLD;

  context.reconnectTimer = window.setTimeout(() => {
    context.reconnectTimer = null;
    syncActiveTerminalContextAliases(context);
    if (context.disposed) {
      return;
    }
    if (shouldProbe) {
      maybeReevaluateServer("reconnect-failures-" + terminalReconnectFailures);
    }
    connectTerminal(context);
  }, 1500);
  syncActiveTerminalContextAliases(context);
}

function mergePendingSessionRefresh(nextRequest = {}) {
  const nextPreferredSessionId =
    typeof nextRequest.preferredSessionId === "string" ? nextRequest.preferredSessionId : "";
  const nextPushHistoryOnSelect = Boolean(nextRequest.pushHistoryOnSelect);
  const nextPreserveCurrentList = Boolean(nextRequest.preserveCurrentList);
  const nextForcePreferredSession = Boolean(nextRequest.forcePreferredSession);

  if (!state.pendingSessionRefresh) {
    state.pendingSessionRefresh = {
      preferredSessionId: nextPreferredSessionId,
      pushHistoryOnSelect: nextPushHistoryOnSelect,
      preserveCurrentList: nextPreserveCurrentList,
      forcePreferredSession: nextForcePreferredSession,
    };
    return;
  }

  state.pendingSessionRefresh = {
    preferredSessionId:
      nextPreferredSessionId || state.pendingSessionRefresh.preferredSessionId || "",
    pushHistoryOnSelect:
      state.pendingSessionRefresh.pushHistoryOnSelect || nextPushHistoryOnSelect,
    preserveCurrentList:
      state.pendingSessionRefresh.preserveCurrentList || nextPreserveCurrentList,
    forcePreferredSession:
      state.pendingSessionRefresh.forcePreferredSession || nextForcePreferredSession,
  };
}

function syncQueuedSessionRefresh(nextRequest = {}) {
  if (!state.pendingSessionRefresh && sessionEventRefreshTimer === null) {
    return;
  }

  mergePendingSessionRefresh(nextRequest);
}

function scheduleSessionEventRefresh(
  nextRequest = {},
  delayMs = TERMINAL_SESSION_EVENT_REFRESH_DELAY_MS,
) {
  mergePendingSessionRefresh(nextRequest);
  if (sessionEventRefreshTimer !== null) {
    if (delayMs > 0) {
      return;
    }

    window.clearTimeout(sessionEventRefreshTimer);
    sessionEventRefreshTimer = null;
  }

  const runRefresh = () => {
    sessionEventRefreshTimer = null;
    const pendingRefresh = state.pendingSessionRefresh;
    state.pendingSessionRefresh = null;
    loadSessions(pendingRefresh || {});
  };

  if (delayMs <= 0) {
    sessionEventRefreshTimer = window.setTimeout(runRefresh, 0);
    return;
  }

  sessionEventRefreshTimer = window.setTimeout(runRefresh, delayMs);
}

function handleServerControlMessage(
  rawMessage,
  token = activeTerminalContext?.connectionToken,
  context = activeTerminalContext,
) {
  let message;
  try {
    message = JSON.parse(rawMessage);
  } catch {
    return false;
  }

  if (handleTerminalBacklogReplayControl(message, token, context)) {
    return true;
  }

  if (message?.type === "terminal_connection_error") {
    resetTerminalBacklogReplay(context);
    resetTerminalOutputQueue(context);
    context.disconnected = true;
    if (context === activeTerminalContext) {
      updateStatus(message.message || "终端连接失败。", "warn");
    }
    return true;
  }

  if (message?.type === "toast") {
    if (!message.session_id || message.session_id === state.activeSessionId) {
      updateStatus(message.message || "", message.tone || "info");
    }
    return true;
  }

  if (message?.type !== "session_list_changed") {
    return false;
  }

  if (message.action === "connected") {
    const connectedSessionId =
      typeof message.session_id === "string" ? message.session_id.trim() : "";
    if (connectedSessionId && context === activeTerminalContext) {
      if (connectedSessionId !== state.activeSessionId) {
        state.activeSessionId = connectedSessionId;
        storeSessionId(state.currentPath, connectedSessionId);
        storeGlobalSessionId(connectedSessionId);
        syncHistory();
      }
      clearConnectingStatusForSession(connectedSessionId);
      scheduleSessionEventRefresh(
        {
          preferredSessionId: connectedSessionId,
          preserveCurrentList: true,
          forcePreferredSession: true,
        },
        0,
      );
    } else if (connectedSessionId) {
      scheduleSessionEventRefresh({
        preferredSessionId: state.activeSessionId,
        preserveCurrentList: true,
      });
    }
    return true;
  }

  if (
    message.action === "opened" &&
    context === activeTerminalContext &&
    message.session_id === state.activeSessionId
  ) {
    clearConnectingStatusForSession(message.session_id);
    scheduleSessionEventRefresh(
      {
        preferredSessionId: state.activeSessionId,
        preserveCurrentList: true,
        forcePreferredSession: true,
      },
      0,
    );
    return true;
  }

  if (message.action === "opened") {
    return true;
  }

  if (!shouldRefreshForSessionMutation(message.action)) {
    return true;
  }

  scheduleSessionEventRefresh({ preferredSessionId: state.activeSessionId });
  return true;
}

function closeTerminalContextSocket(context, { suppressEvents = false } = {}) {
  if (!context?.socket) {
    return;
  }

  if (suppressEvents) {
    context.connectionToken += 1;
  }
  const currentSocket = context.socket;
  context.socket = null;
  context.disconnected = true;
  syncActiveTerminalContextAliases(context);
  currentSocket.close();
}

function closeSocket({ suppressEvents = false } = {}) {
  closeTerminalContextSocket(activeTerminalContext, { suppressEvents });
}

function connectTerminal(targetContext = null) {
  if (!targetContext && !state.activeSessionId) {
    updateStatus("请先选择一个终端会话。", "muted");
    return;
  }

  const context = targetContext || activateTerminalSessionContext(state.activeSessionId);
  const isForeground = context === activeTerminalContext;
  const session = state.sessions.find((item) => item.id === context.sessionId) || null;
  if (session) {
    context.path = sessionPath(session);
  }

  if (context.reconnectTimer) {
    window.clearTimeout(context.reconnectTimer);
    context.reconnectTimer = null;
    syncActiveTerminalContextAliases(context);
  }

  if (terminalContextSocketOpen(context)) {
    if (isForeground) {
      state.hasConnectedOnce = true;
      hideTerminalSwitchPlaceholder();
      terminalHost?.classList.remove("terminal-host-replaying");
      updateStatus("", "ok");
      bindSessionPageScrollRestoreToConnection(context.sessionId, context.connectionToken);
      restoreCachedTerminalViewport(context);
      resetTerminalSizeSync();
      ensureFontsReady().then(() => {
        if (context !== activeTerminalContext || !terminalContextSocketOpen(context)) {
          return;
        }
        fitTerminal({ force: true });
        scheduleTerminalSizeSettle();
        restoreCachedTerminalViewport(context);
        restoreSessionPageScrollIfActive();
        focusTerminalIfAllowed();
        maybeRunTerminalStartupActions();
      });
    }
    return;
  }

  if (terminalContextSocketConnecting(context)) {
    if (isForeground) {
      restoreCachedTerminalViewport(context);
    }
    return;
  }

  if (context.disconnected && context.hasLoadedOutput) {
    resetTerminalContextInstance(context);
  }
  resetTerminalBacklogReplay(context);
  resetTerminalOutputQueue(context);
  const token = ++context.connectionToken;
  context.disconnected = false;
  context.initialReplayPending = true;
  syncActiveTerminalContextAliases(context);
  if (isForeground) {
    bindSessionPageScrollRestoreToConnection(context.sessionId, token);
  }

  if (isForeground) {
    updateStatus(`正在连接 ${session ? session.name : "终端"}…`, "info");
    if (!context.hasLoadedOutput) {
      showTerminalSwitchPlaceholder(`正在打开 ${session ? session.name : "终端"}…`);
    }
    restoreSessionPageScrollIfActive();
    hideTerminalCursorCorrection();
    resetTerminalAutoResponseState();
    captureTerminalContextAliases(context);
  }

  const contextSocket = new WebSocket(websocketUrl(context));
  context.socket = contextSocket;
  contextSocket.binaryType = "arraybuffer";
  syncActiveTerminalContextAliases(context);

  contextSocket.addEventListener("open", async () => {
    if (context.disposed || token !== context.connectionToken) {
      return;
    }

    context.everConnected = true;
    context.disconnected = false;
    state.hasConnectedOnce = true;
    resetTerminalReconnectFailures();
    syncActiveTerminalContextAliases(context);
    sendTerminalContextVisibility(context, context === activeTerminalContext);
    if (context !== activeTerminalContext) {
      return;
    }
    resetTerminalSizeSync();
    updateStatus("", "ok");
    await ensureFontsReady();
    if (context !== activeTerminalContext || token !== context.connectionToken) {
      return;
    }
    fitTerminal({ force: true });
    scheduleTerminalSizeSettle();
    restoreSessionPageScrollIfActive();
    focusTerminalIfAllowed();
    maybeRunTerminalStartupActions();
    maybeAutoContinueErroredSessions();
  });

  contextSocket.addEventListener("message", async (event) => {
    if (context.disposed || token !== context.connectionToken) {
      return;
    }

    if (typeof event.data === "string") {
      handleServerControlMessage(event.data, token, context);
      return;
    }

    const buffer = event.data instanceof Blob ? await event.data.arrayBuffer() : event.data;
    if (context.disposed || token !== context.connectionToken) {
      return;
    }
    const bytes = new Uint8Array(buffer);
    queueTerminalOutput(bytes, token, context);
  });

  contextSocket.addEventListener("close", () => {
    if (context.disposed || token !== context.connectionToken) {
      return;
    }
    context.socket = null;
    context.disconnected = true;
    syncActiveTerminalContextAliases(context);

    // 从未成功连接过即关闭，可能是认证失败，检查会话状态。
    if (!context.everConnected) {
      fetch("/api/auth/session")
        .then((r) => r.json())
        .then((info) => {
          if (!info.authenticated) {
            redirectToLogin();
          } else {
            scheduleReconnect(context);
          }
        })
        .catch(() => scheduleReconnect(context));
      if (context === activeTerminalContext) {
        updateStatus("连接失败，正在检查登录状态。", "warn");
      }
      return;
    }

    resetTerminalBacklogReplay(context);
    if (context === activeTerminalContext) {
      updateStatus("连接已断开，正在同步终端会话并自动重连。", "warn");
      loadSessions({ preferredSessionId: state.activeSessionId, preserveCurrentList: true });
    }
    scheduleReconnect(context);
  });

  contextSocket.addEventListener("error", () => {
    if (context.disposed || token !== context.connectionToken) {
      return;
    }

    if (context === activeTerminalContext) {
      updateStatus("终端连接失败。", "warn");
    }
    resetTerminalBacklogReplay(context);
  });
}

function selectSession(sessionId, { connect = true, pushHistory = false } = {}) {
  const session = state.sessions.find((item) => item.id === sessionId) || null;
  if (!session?.id) {
    return;
  }
  if (isIdleSession(session.id)) {
    renderSessions();
    return;
  }
  if (activeTerminalContext?.sessionId === state.activeSessionId) {
    saveTerminalScrollPositionForSession(state.activeSessionId);
  }
  const pageScrollRestore = beginSessionPageScrollRestore(session.id);

  if (pendingNewSessionQuickStart && pendingNewSessionQuickStart.sessionId !== session.id) {
    cancelNewSessionQuickStart();
  }

  state.activeSessionId = session.id;
  state.currentPath = sessionPath(session);
  syncQueuedSessionRefresh({
    preferredSessionId: session.id,
    pushHistoryOnSelect: pushHistory,
  });
  syncCurrentPathDisplay(sessionDisplayPath(session));
  storeSessionId(state.currentPath, session.id);
  storeGlobalSessionId(session.id);
  if (connect) {
    markSessionOpenedLocally(session.id);
  }
  syncHistory({ push: pushHistory });
  renderSessions();
  tickTerminalPasteScheduledCountdown();
  restoreSessionPageScrollIfActive(pageScrollRestore);

  if (connect) {
    connectTerminal();
  }
}
