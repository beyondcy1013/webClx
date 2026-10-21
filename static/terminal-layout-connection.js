// webClx terminal layout, reconnect, WebSocket, and session selection helpers.
// Extracted from terminal.js as global function declarations.
// Keep top-level ResizeObserver setup in terminal.js because it depends on terminalHost.

// ── Reconnect failure tracking & automatic server re-evaluation ──────────────
// When the WebSocket disconnects repeatedly, the current server may be
// unreachable.  Instead of blindly retrying the same host forever, we trigger
// TERMINAL_SERVER_PROBE.reevaluate() after a few consecutive failures.
// reevaluate() itself enforces a 60s grace window: a short outage (the webClx
// service restarting itself) resets as soon as the current origin answers
// again, and never triggers a switch to another server.
const RECONNECT_FAILURE_THRESHOLD = 3;
const TERMINAL_RECONNECT_MAX_DELAY_MS = 30000;
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

function terminalLocalFitSize(context = activeTerminalContext) {
  if (!context?.fitAddon || typeof context.fitAddon.proposeDimensions !== "function") {
    return null;
  }

  const previousZoom = terminalHost?.style.zoom || "";
  const previousWidth = terminalHost?.style.width || "";
  const previousMaxWidth = terminalHost?.style.maxWidth || "";
  if (terminalHost) {
    terminalHost.style.zoom = "";
    terminalHost.style.width = "";
    terminalHost.style.maxWidth = "";
  }
  const proposed = context.fitAddon.proposeDimensions();
  if (terminalHost) {
    terminalHost.style.zoom = previousZoom;
    terminalHost.style.width = previousWidth;
    terminalHost.style.maxWidth = previousMaxWidth;
  }
  if (!proposed) {
    return null;
  }
  return {
    cols: Math.max(Math.trunc(Number(proposed.cols) || 0), 2),
    rows: Math.max(Math.trunc(Number(proposed.rows) || 0), 2),
  };
}

function syncTerminalSize({ force = false, size = null } = {}) {
  const context = activeTerminalContext;
  if (!context || context.term !== term) {
    return;
  }
  const nextSize = size || terminalLocalFitSize(context) || {
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

function applyTerminalCanonicalViewportLayout(context = activeTerminalContext) {
  if (!terminalHost || !terminalScrollShellEl || context !== activeTerminalContext) {
    return;
  }
  const canonical = context?.canonicalTerminalSize;
  const local = context?.localTerminalFitSize;
  const overflow = Boolean(canonical && local && canonical.cols > local.cols);
  terminalHost.style.zoom = "";
  terminalScrollShellEl.classList.toggle("terminal-shared-grid-overflow", overflow);
  if (!overflow) {
    if (!state.terminalWideMode) {
      terminalHost.style.width = "";
      terminalHost.style.maxWidth = "";
    }
    return;
  }

  const dimensions = context.term?._core?._renderService?.dimensions;
  const cellWidth = Number(dimensions?.actualCellWidth) || 0;
  if (cellWidth <= 0) {
    return;
  }
  const horizontalPadding = 8;
  const width = Math.ceil(cellWidth * canonical.cols + horizontalPadding);
  terminalHost.style.width = `${width}px`;
  terminalHost.style.maxWidth = "none";
}

function terminalReflowCurrentSession() {
  const context = activeTerminalContext;
  if (!context || context.disposed || !context.term || context.term !== term) {
    updateStatus("当前没有可重排的活动终端。", "warn");
    return false;
  }

  const sessionId = String(context.sessionId || "").trim();
  if (sessionId && sessionId !== TERMINAL_EMPTY_CONTEXT_ID) {
    state.terminalSessionReflowIds.add(sessionId);
    storeTerminalSessionReflowIds(state.terminalSessionReflowIds);
  }

  applyTerminalSessionReflow(context);
  updateStatus("已按当前手机宽度重排，并记住该终端的当前视口。", "ok");
  return true;
}

function terminalSessionReflowEnabled(context = activeTerminalContext) {
  const sessionId = String(context?.sessionId || "").trim();
  return Boolean(sessionId) && state.terminalSessionReflowIds.has(sessionId);
}

function applyTerminalSessionReflow(context = activeTerminalContext) {
  if (!context || context.disposed || !context.term) {
    return false;
  }

  const isForeground = context === activeTerminalContext;
  if (isForeground && context.term !== term) {
    syncActiveTerminalContextAliases(context);
  }

  // A stale canonical width can keep the PTY wider than the phone viewport.
  // Reflow is an explicit recovery action, so ignore that canonical size and
  // republish the local fit result once.
  context.canonicalTerminalSize = null;
  if (isForeground) {
    context.lastTerminalSize = null;
    lastTerminalSize = null;
    terminalScrollShellEl?.classList.remove("terminal-shared-grid-overflow");
    if (!state.terminalWideMode) {
      terminalHost.style.width = "";
      terminalHost.style.maxWidth = "";
    }
  }

  const proposed = terminalLocalFitSize(context);
  context.localTerminalFitSize = proposed;
  if (proposed && context.term) {
    if (context.term.cols !== proposed.cols || context.term.rows !== proposed.rows) {
      context.term.resize(proposed.cols, proposed.rows);
    }
    if (isForeground) {
      context.lastTerminalSize = proposed;
      lastTerminalSize = proposed;
    }
    sendMessage({
      type: "resize",
      cols: proposed.cols,
      rows: proposed.rows,
    }, context);
  } else if (isForeground) {
    syncTerminalSize({ force: true });
  }

  if (isForeground) {
    applyTerminalCanonicalViewportLayout(context);
    syncTerminalOverlayBounds();
    context.term.refresh(0, Math.max(context.term.rows - 1, 0));
    syncTerminalCursorCorrection();
  }

  if (!proposed) {
    return false;
  }

  if (isForeground) {
    window.requestAnimationFrame(() => {
      if (context !== activeTerminalContext || context.term !== term || context.disposed) {
        return;
      }
      context.canonicalTerminalSize = null;
      applyTerminalCanonicalViewportLayout(context);
      syncTerminalOverlayBounds();
      context.term.refresh(0, Math.max(context.term.rows - 1, 0));
    });
  }
  return true;
}

function applyTerminalCanonicalViewportSize(size, context = activeTerminalContext) {
  if (!context || context.disposed || !context.term) {
    return;
  }
  if (terminalSessionReflowEnabled(context)) {
    applyTerminalSessionReflow(context);
    return;
  }
  const canonical = {
    cols: Math.max(Math.trunc(Number(size?.cols) || 0), 2),
    rows: Math.max(Math.trunc(Number(size?.rows) || 0), 2),
  };
  context.canonicalTerminalSize = canonical;
  const localFitSize = context.localTerminalFitSize || terminalLocalFitSize(context);
  const targetRows =
    localFitSize?.rows
      ? Math.min(canonical.rows, localFitSize.rows)
      : canonical.rows;
  if (context.term.cols !== canonical.cols || context.term.rows !== targetRows) {
    context.term.resize(canonical.cols, targetRows);
  }
  if (context === activeTerminalContext) {
    applyTerminalCanonicalViewportLayout(context);
    syncTerminalOverlayBounds();
    updateTerminalScrollBottomButton();
    syncTerminalCursorCorrection();
  }
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
    const context = activeTerminalContext;
    const localFitSize = terminalLocalFitSize(context);
    if (context) {
      context.localTerminalFitSize = localFitSize;
    }
    if (terminalSessionReflowEnabled(context)) {
      applyTerminalSessionReflow(context);
    } else if (context?.canonicalTerminalSize) {
      const canonical = context.canonicalTerminalSize;
      const targetRows =
        localFitSize?.rows
          ? Math.min(canonical.rows, localFitSize.rows)
          : canonical.rows;
      if (term.cols !== canonical.cols || term.rows !== targetRows) {
        if (targetRows === canonical.rows) {
          term.resize(canonical.cols, canonical.rows);
        } else {
          term.resize(canonical.cols, targetRows);
        }
      }
      applyTerminalCanonicalViewportLayout(context);
    } else {
      fitAddon.fit();
    }
    syncTerminalOverlayBounds();
    syncTerminalSize({ force, size: localFitSize });
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

function terminalReconnectDelayMs(context) {
  const sessionId = String(context?.sessionId || "");
  let hash = 0;
  for (let index = 0; index < sessionId.length; index += 1) {
    hash = (hash * 31 + sessionId.charCodeAt(index)) >>> 0;
  }
  const jitter = hash % 1200;
  const attempts = Math.max(0, Math.trunc(Number(context?.reconnectAttempts) || 0));
  const base = !context || context === activeTerminalContext ? 1500 : 1800;
  const scaled = base * 2 ** Math.min(attempts, 10);
  return Math.min(TERMINAL_RECONNECT_MAX_DELAY_MS, scaled + jitter);
}

function terminalReconnectBlocked() {
  const hidden = typeof document !== "undefined" && document.visibilityState === "hidden";
  const offline = typeof navigator !== "undefined" && navigator.onLine === false;
  return hidden || offline;
}

function scheduleReconnect(context = activeTerminalContext, { immediate = false } = {}) {
  if (!context || context.disposed || context.reconnectTimer || !context.sessionId) {
    return;
  }

  // While the page is hidden or the device is offline a reconnect can never
  // succeed; defer instead of burning attempts against a dead network.  Even
  // "immediate" resumes only run when the page can actually reach the server.
  if (terminalReconnectBlocked()) {
    context.reconnectDeferred = true;
    return;
  }
  context.reconnectDeferred = false;
  const delay = immediate ? 0 : terminalReconnectDelayMs(context);
  context.reconnectAttempts = Math.max(0, Math.trunc(Number(context.reconnectAttempts) || 0)) + 1;

  terminalReconnectFailures++;
  const shouldProbe = terminalReconnectFailures >= RECONNECT_FAILURE_THRESHOLD;

  context.reconnectTimer = window.setTimeout(() => {
    context.reconnectTimer = null;
    syncActiveTerminalContextAliases(context);
    if (context.disposed) {
      return;
    }
    // The environment may have changed while the timer slept.
    if (terminalReconnectBlocked()) {
      context.reconnectDeferred = true;
      return;
    }
    if (shouldProbe) {
      maybeReevaluateServer("reconnect-failures-" + terminalReconnectFailures);
    }
    connectTerminal(context);
  }, delay);
  syncActiveTerminalContextAliases(context);
}

// Going hidden or offline cancels every sleeping reconnect timer and marks
// the context deferred — attempts are not consumed while we cannot connect.
function deferPendingReconnects() {
  if (typeof forEachTerminalSessionContext !== "function") {
    return;
  }
  forEachTerminalSessionContext((context) => {
    if (!context || context.disposed) {
      return;
    }
    if (context.reconnectTimer) {
      window.clearTimeout(context.reconnectTimer);
      context.reconnectTimer = null;
    }
    if (context.disconnected || context.reconnectDeferred) {
      context.reconnectDeferred = true;
    }
    syncActiveTerminalContextAliases(context);
  });
}

// Deferred and interrupted reconnects resume as soon as the page can reach the
// network again: the active context retries immediately (a sleeping timer is
// cancelled and re-armed at 0), background contexts keep their normal backoff.
function resumeTerminalReconnects() {
  if (terminalReconnectBlocked()) {
    return;
  }
  if (typeof forEachTerminalSessionContext !== "function") {
    return;
  }
  forEachTerminalSessionContext((context) => {
    if (!context || context.disposed) {
      return;
    }
    if (context.reconnectTimer) {
      if (context !== activeTerminalContext) {
        return;
      }
      window.clearTimeout(context.reconnectTimer);
      context.reconnectTimer = null;
    } else if (!context.reconnectDeferred && !context.disconnected) {
      return;
    }
    context.reconnectDeferred = false;
    scheduleReconnect(context, { immediate: context === activeTerminalContext });
  });
}

if (typeof document !== "undefined" && typeof document.addEventListener === "function") {
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") {
      resumeTerminalReconnects();
    } else {
      deferPendingReconnects();
    }
  });
}
window.addEventListener("online", resumeTerminalReconnects);
window.addEventListener("offline", deferPendingReconnects);

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
  mergePendingSessionRefresh({
    ...nextRequest,
    preserveCurrentList: true,
  });
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

  if (message?.type === "terminal_output_framing") {
    // 服务端确认 seq 帧协议：此后二进制输出帧带 8 字节大端 seq 前缀。
    // 该控制消息先于一切二进制输出到达，晚于它的帧才可按 seq 帧解析。
    if (context && !context.disposed && token === context.connectionToken) {
      context.seqFramingActive = Number(message.version) >= 2;
    }
    return true;
  }

  if (message?.type === "terminal_viewport_size") {
    queueTerminalViewportSize(message, token, context);
    return true;
  }

  if (message?.type === "terminal_connection_error") {
    if (!context || context.disposed || token !== context.connectionToken) {
      return true;
    }
    context.connectionError = message.message || "终端连接失败。";
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

  // 断线重连优先尝试 seq 水位增量续传：保留现有终端缓冲区，服务端只补发
  // 缺失 chunk，App 前后台切换不再清空终端、全量重拉历史。是否真正重建
  // 终端推迟到 backlog_replay start 到达时决定——服务端不接受续传时才走
  // 既有的 retainedTerm 全量回放路径。
  const resumeWanted =
    context.disconnected &&
    context.hasLoadedOutput &&
    !context.outputStreamGap &&
    Number(context.lastOutputSeq) > 0 &&
    Boolean(context.outputInstanceId);
  context.pendingDeltaReplay = resumeWanted;
  context.seqFramingActive = false;
  context.deltaReplayActive = false;
  if (context.disconnected && context.hasLoadedOutput && !resumeWanted) {
    resetTerminalContextInstance(context);
  }
  resetTerminalBacklogReplay(context);
  resetTerminalOutputQueue(context);
  const token = ++context.connectionToken;
  context.disconnected = false;
  context.initialReplayPending = true;
  context.connectionError = "";
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
    // Reconnect backoff resets in endTerminalBacklogReplay once a replay has
    // actually completed — a bare socket open that flaps closed again must keep
    // growing the delay instead of resetting it.
    if (typeof TERMINAL_SERVER_PROBE !== "undefined") {
      TERMINAL_SERVER_PROBE.notifyServerReachable();
    }
    syncActiveTerminalContextAliases(context);
    sendTerminalContextVisibility(context, context === activeTerminalContext);
    flushTerminalInputQueue(context);
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
    let bytes = new Uint8Array(buffer);
    if (context.seqFramingActive) {
      if (bytes.byteLength < 8) {
        return;
      }
      context.lastOutputSeq = Number(
        new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getBigUint64(0, false),
      );
      bytes = bytes.subarray(8);
      if (bytes.byteLength === 0) {
        return;
      }
    }
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
