// webClx terminal output queue, backlog replay, page scroll, and scroll rail helpers.
// Extracted from terminal.js as global function declarations.
// Contains no top-level setup.

function refreshTerminalInputVisibilityAfterPageResume() {
  const sessionId = state.activeSessionId;
  if (!sessionId || terminalSessionInitializing() || shouldDeferSessionListRender()) {
    return;
  }

  // Returning from another app and closing/opening the IME both emit focus and
  // visibility events. Preserve the user's current terminal position across
  // the resulting layout refresh instead of treating every resume as a request
  // to follow the live output at the bottom.
  const scrollSnapshot = captureTerminalScrollSnapshotForSession(sessionId)
    || terminalScrollPositions.get(sessionId)
    || null;

  const followUpDelays = [0, 80, 180, 360, 720, 1200, 1800];
  followUpDelays.forEach((delay) => {
    window.setTimeout(() => {
      if (
        sessionId !== state.activeSessionId ||
        document.visibilityState === "hidden" ||
        shouldDeferSessionListRender()
      ) {
        return;
      }
      preservePageScrollDuringLayout(() => {
        refreshTerminalViewportLayout({ requireConnected: true });
      });
      restoreTerminalScrollSnapshot(scrollSnapshot);
      syncTerminalCursorCorrection();
    }, delay);
  });
}

function terminalShouldStickToBottomForOutput(context = activeTerminalContext) {
  if (!context) {
    return true;
  }
  if (context !== activeTerminalContext) {
    return context.followOutput !== false;
  }
  if (context.backlogReplayActive) {
    return true;
  }

  const metrics = terminalScrollMetrics();
  if (!terminalScrollSaveSuppressed()) {
    return metrics?.atBottom ?? true;
  }

  const saved = context.sessionId ? terminalScrollPositions.get(context.sessionId) : null;
  return saved?.atBottom ?? metrics?.atBottom ?? true;
}

function beginTerminalBacklogReplay(context = activeTerminalContext) {
  if (!context || context.disposed) {
    return;
  }
  context.initialReplayPending = false;
  if (context.backlogReplayInterrupted) {
    syncActiveTerminalContextAliases(context);
    return;
  }

  context.backlogReplayActive = true;
  context.backlogReplayEndQueued = false;
  context.backlogReplayInterrupted = false;
  syncActiveTerminalContextAliases(context);
  if (context !== activeTerminalContext) {
    return;
  }
  if (context.retainedTerm) {
    terminalHost?.classList.remove("terminal-host-replaying");
    updateTerminalScrollBottomButton();
    return;
  }
  hideTerminalCursorCorrection();
  terminalHost?.classList.add("terminal-host-replaying");
  updateTerminalScrollBottomButton();
}

function endTerminalBacklogReplay(
  { preserveInterrupted = false } = {},
  context = activeTerminalContext,
) {
  if (!context || context.disposed) {
    return;
  }
  context.initialReplayPending = false;
  context.hasLoadedOutput = true;
  completeTerminalContextReconnect(context);
  if (!context.backlogReplayActive) {
    if (!preserveInterrupted) {
      context.backlogReplayEndQueued = false;
      context.backlogReplayInterrupted = false;
    }
    syncActiveTerminalContextAliases(context);
    if (context === activeTerminalContext) {
      maybeRunTerminalStartupActions();
    }
    return;
  }

  const keepEndQueued = preserveInterrupted && context.backlogReplayEndQueued;
  context.backlogReplayActive = false;
  context.backlogReplayEndQueued = keepEndQueued;
  context.backlogReplayInterrupted = preserveInterrupted && context.backlogReplayInterrupted;
  syncActiveTerminalContextAliases(context);
  if (context !== activeTerminalContext) {
    if (context.followOutput !== false) {
      context.term.scrollToBottom?.();
    }
    return;
  }

  restoreTerminalScrollPositionForSession(context.sessionId, { defaultToBottom: true });
  if (!preserveInterrupted) {
    scheduleTerminalBottomAnchorAfterReplay(context.sessionId);
  }
  if (activeSessionPageScrollRestore?.sessionId === context.sessionId) {
    activeSessionPageScrollRestore.startedAt = Date.now();
  }
  scheduleTerminalRenderRefresh(context);
  terminalHost?.classList.remove("terminal-host-replaying");
  hideTerminalSwitchPlaceholder();
  restoreSessionPageScrollIfActive();
  scheduleSessionPageScrollRestore();
  updateTerminalScrollBottomButton();
  syncTerminalCursorCorrection();
  maybeRunTerminalStartupActions();
}


// After backlog replay ends, fitAddon.fit() (called during size-settle) can
// reflow the xterm buffer when rows/cols change and reset the viewport to the
// top. Re-anchor the terminal viewport across that short window. Explicit
// user scroll intent cancels this via cancelTerminalBottomAnchor().
let terminalBottomAnchorToken = 0;
let terminalBottomAnchorSessionId = null;

function scheduleTerminalBottomAnchorAfterReplay(sessionId) {
  if (!sessionId || terminalBacklogReplayInterrupted) {
    return;
  }

  terminalBottomAnchorToken += 1;
  const token = terminalBottomAnchorToken;
  terminalBottomAnchorSessionId = sessionId;

  const delays = [0, 80, 180, 360, 720, 1200];
  delays.forEach((delay) => {
    window.setTimeout(() => {
      if (
        token !== terminalBottomAnchorToken ||
        sessionId !== state.activeSessionId ||
        sessionId !== terminalBottomAnchorSessionId
      ) {
        return;
      }
      suppressTerminalScrollSaveUntilNextFrame();
      scrollTerminalToBottom();
      saveTerminalScrollPositionForSession(sessionId);
    }, delay);
  });
}

function cancelTerminalBottomAnchor() {
  terminalBottomAnchorToken += 1;
  terminalBottomAnchorSessionId = null;
}

function resetTerminalBacklogReplay(context = activeTerminalContext) {
  if (!context) {
    return;
  }
  context.initialReplayPending = false;
  context.backlogReplayActive = false;
  context.backlogReplayEndQueued = false;
  context.backlogReplayInterrupted = false;
  syncActiveTerminalContextAliases(context);
  if (context === activeTerminalContext) {
    terminalHost?.classList.remove("terminal-host-replaying");
    hideTerminalSwitchPlaceholder();
    updateTerminalScrollBottomButton();
  }
}

function resetTerminalOutputQueue(context = activeTerminalContext) {
  if (!context) {
    return;
  }
  clearTerminalOutputDrainTimer(context);
  context.codexStatusOutputTransformer?.reset?.();
  context.codexStatusOutputTransformer = null;
  context.synchronizedOutputTransformer?.reset?.();
  context.synchronizedOutputTransformer = null;
  context.outputQueue = [];
  context.outputWriteInFlight = false;
  context.outputWriteId += 1;
  syncActiveTerminalContextAliases(context);
}

function ensureSynchronizedOutputTransformer(context = activeTerminalContext) {
  if (!context || context.disposed) {
    return null;
  }
  if (!context.synchronizedOutputTransformer) {
    const createSynchronizedOutputTransformer =
      globalThis.WebClxTerminalSynchronizedOutput?.createSynchronizedOutputTransformer;
    if (typeof createSynchronizedOutputTransformer === "function") {
      context.synchronizedOutputTransformer = createSynchronizedOutputTransformer();
    }
  }
  return context.synchronizedOutputTransformer || null;
}

function transformTerminalSynchronizedOutput(bytes, context = activeTerminalContext) {
  const transformer = ensureSynchronizedOutputTransformer(context);
  return transformer ? transformer.transform(bytes) : bytes;
}

function flushTerminalSynchronizedOutput(context = activeTerminalContext) {
  return context?.synchronizedOutputTransformer?.flush?.() || new Uint8Array();
}

function ensureCodexStatusOutputTransformer(context = activeTerminalContext) {
  if (!context || context.disposed) {
    return null;
  }
  if (!context.codexStatusOutputTransformer) {
    const createCodexStatusOutputTransformer =
      globalThis.WebClxCodexStatusOutput?.createCodexStatusOutputTransformer;
    if (typeof createCodexStatusOutputTransformer === "function") {
      context.codexStatusOutputTransformer = createCodexStatusOutputTransformer();
    }
  }
  return context.codexStatusOutputTransformer || null;
}

function transformTerminalCodexStatusOutput(bytes, context = activeTerminalContext) {
  const transformer = ensureCodexStatusOutputTransformer(context);
  return transformer ? transformer.transform(bytes) : bytes;
}

function flushCodexStatusOutputTransformer(context = activeTerminalContext) {
  return context?.codexStatusOutputTransformer?.flush?.() || new Uint8Array();
}

function clearTerminalOutputDrainTimer(context) {
  if (!context || context.outputDrainTimer === null) {
    return;
  }
  window.clearTimeout(context.outputDrainTimer);
  context.outputDrainTimer = null;
}

function scheduleTerminalRenderRefresh(context = activeTerminalContext) {
  if (
    !context ||
    context.disposed ||
    context !== activeTerminalContext ||
    context.renderRefreshFrame !== null
  ) {
    return;
  }

  context.renderRefreshFrame = window.requestAnimationFrame(() => {
    context.renderRefreshFrame = null;
    if (context.disposed || context !== activeTerminalContext) {
      return;
    }
    const rows = Math.max(Number(context.term?.rows) || 0, 1);
    context.term.refresh(0, rows - 1);
  });
}

function scheduleTerminalOutputDrain(context) {
  if (
    !context ||
    context.disposed ||
    context.outputWriteInFlight ||
    context.outputDrainTimer !== null
  ) {
    return;
  }

  context.outputDrainTimer = window.setTimeout(() => {
    context.outputDrainTimer = null;
    drainTerminalOutputQueue(context);
  }, TERMINAL_LIVE_OUTPUT_COALESCE_MS);
}

function mergeQueuedTerminalOutputItem(firstItem, context = activeTerminalContext) {
  if (!firstItem || firstItem.kind !== "output") {
    return firstItem;
  }

  const chunks = [firstItem.bytes];
  let totalBytes = firstItem.bytes.length;
  const maxBytes = firstItem.replay
    ? TERMINAL_REPLAY_OUTPUT_MERGE_MAX_BYTES
    : TERMINAL_LIVE_OUTPUT_MERGE_MAX_BYTES;
  while (context.outputQueue.length > 0) {
    const candidate = context.outputQueue[0];
    if (
      candidate.kind !== "output" ||
      candidate.token !== firstItem.token ||
      candidate.replay !== firstItem.replay ||
      totalBytes + candidate.bytes.length > maxBytes
    ) {
      break;
    }
    context.outputQueue.shift();
    chunks.push(candidate.bytes);
    totalBytes += candidate.bytes.length;
  }

  if (chunks.length === 1) {
    return firstItem;
  }

  const bytes = new Uint8Array(totalBytes);
  let offset = 0;
  chunks.forEach((chunk) => {
    bytes.set(chunk, offset);
    offset += chunk.length;
  });

  return { ...firstItem, bytes };
}

function drainTerminalOutputQueue(context = activeTerminalContext) {
  if (!context || context.disposed || context.outputWriteInFlight) {
    return;
  }

  clearTerminalOutputDrainTimer(context);

  let nextItem = context.outputQueue.shift();
  if (!nextItem) {
    return;
  }

  if (nextItem.token !== context.connectionToken) {
    drainTerminalOutputQueue(context);
    return;
  }

  if (nextItem.kind === "backlog_replay_end") {
    endTerminalBacklogReplay({}, context);
    drainTerminalOutputQueue(context);
    return;
  }

  nextItem = mergeQueuedTerminalOutputItem(nextItem, context);
  context.outputWriteInFlight = true;
  const writeId = ++context.outputWriteId;
  const stickToBottom = terminalShouldStickToBottomForOutput(context);
  context.hasLoadedOutput = true;
  syncActiveTerminalContextAliases(context);
  if (context === activeTerminalContext) {
    noteTerminalAutoResponseQueries(nextItem.bytes);
  }
  context.term.write(nextItem.bytes, () => {
    if (
      context.disposed ||
      writeId !== context.outputWriteId ||
      nextItem.token !== context.connectionToken
    ) {
      return;
    }

    context.outputWriteInFlight = false;
    context.followOutput = stickToBottom;
    syncActiveTerminalContextAliases(context);
    scheduleTerminalRenderRefresh(context);
    if (context === activeTerminalContext) {
      if (context.backlogReplayActive || stickToBottom) {
        scrollTerminalToBottom();
      } else {
        updateTerminalScrollBottomButton();
      }
      if (!context.backlogReplayActive) {
        syncTerminalCursorCorrection();
      }
    } else if (stickToBottom) {
      context.term.scrollToBottom?.();
    }
    const followingItem = context.outputQueue[0];
    if (followingItem?.kind === "output" && !followingItem.replay) {
      scheduleTerminalOutputDrain(context);
    } else {
      drainTerminalOutputQueue(context);
    }
  });
}

function queueTerminalOutput(bytes, token, context = activeTerminalContext) {
  if (!context || context.disposed || !(bytes instanceof Uint8Array) || bytes.length === 0) {
    return;
  }
  if (context.backlogReplayInterrupted && !context.backlogReplayEndQueued) {
    return;
  }

  const synchronizedBytes = transformTerminalSynchronizedOutput(bytes, context);
  if (synchronizedBytes.length === 0) {
    return;
  }
  const replay = context.backlogReplayActive && !context.backlogReplayEndQueued;
  const transformedBytes = replay
    ? transformTerminalCodexStatusOutput(synchronizedBytes, context)
    : synchronizedBytes;
  if (transformedBytes.length === 0) {
    return;
  }

  context.outputQueue.push({ kind: "output", bytes: transformedBytes, token, replay });
  syncActiveTerminalContextAliases(context);
  if (
    replay ||
    context !== activeTerminalContext ||
    document.visibilityState === "hidden"
  ) {
    drainTerminalOutputQueue(context);
  } else {
    scheduleTerminalOutputDrain(context);
  }
}

function queueTerminalBacklogReplayEnd(token, context = activeTerminalContext) {
  if (!context || context.disposed) {
    return;
  }
  const synchronizedPendingBytes = flushTerminalSynchronizedOutput(context);
  const transformedSynchronizedPendingBytes =
    synchronizedPendingBytes.length > 0
      ? transformTerminalCodexStatusOutput(synchronizedPendingBytes, context)
      : new Uint8Array();
  if (transformedSynchronizedPendingBytes.length > 0) {
    context.outputQueue.push({
      kind: "output",
      bytes: transformedSynchronizedPendingBytes,
      token,
      replay: context.backlogReplayActive && !context.backlogReplayEndQueued,
    });
  }
  const pendingBytes = flushCodexStatusOutputTransformer(context);
  if (pendingBytes.length > 0) {
    context.outputQueue.push({
      kind: "output",
      bytes: pendingBytes,
      token,
      replay: context.backlogReplayActive && !context.backlogReplayEndQueued,
    });
  }
  context.backlogReplayEndQueued = true;
  context.outputQueue.push({ kind: "backlog_replay_end", token });
  syncActiveTerminalContextAliases(context);
  drainTerminalOutputQueue(context);
}

function interruptTerminalBacklogReplayForInput() {
  const context = activeTerminalContext;
  if (!context) {
    return;
  }
  if (!context.backlogReplayActive) {
    if (context.initialReplayPending) {
      context.initialReplayPending = false;
      context.backlogReplayInterrupted = true;
      context.outputQueue = context.outputQueue.filter((item) => !item.replay);
      syncActiveTerminalContextAliases(context);
    }
    return;
  }

  context.backlogReplayInterrupted = true;
  context.outputQueue = context.outputQueue.filter((item) => !item.replay);
  syncActiveTerminalContextAliases(context);
  endTerminalBacklogReplay({ preserveInterrupted: true }, context);
}

function handleTerminalBacklogReplayControl(
  message,
  token = activeTerminalContext?.connectionToken,
  context = activeTerminalContext,
) {
  if (message?.type !== "terminal_backlog_replay") {
    return false;
  }

  if (message.action === "start") {
    beginTerminalBacklogReplay(context);
  } else if (message.action === "end") {
    queueTerminalBacklogReplayEnd(token, context);
  }
  return true;
}

function scrollTerminalToTop() {
  if (typeof term.scrollToTop === "function") {
    term.scrollToTop();
  } else {
    const metrics = terminalScrollMetrics();
    if (metrics) {
      metrics.viewport.scrollTop = 0;
    }
  }

  scheduleTerminalViewportDomSync();
  updateTerminalScrollBottomButton();
}

// xterm.js treats buffer viewportY/ydisp as the logical scroll position, then
// mirrors it to DOM scrollTop from Viewport._refresh in requestAnimationFrame.
// webClx calls scrollToBottom()/scrollToTop() in several programmatic paths
// (session replay, output writes, layout fixes). Those calls update viewportY
// immediately but leave scrollTop and sometimes _scrollArea height stale until
// the next frame. Use the buffer position as the canonical value and keep the
// DOM projection synchronized before xterm interprets more scroll input.
function syncTerminalViewportScrollBeforeWheel() {
  syncTerminalViewportDomToBuffer();
}

function syncTerminalViewportDomToBuffer() {
  if (!term) {
    return false;
  }
  const viewport = terminalViewportScrollEl || terminalViewportElement();
  const coreViewport = term?._core?.viewport;
  if (!viewport || !coreViewport) {
    return false;
  }

  const buffer = term.buffer?.active;
  const rowHeight = terminalViewportRowHeight();
  if (!buffer || !rowHeight) {
    return false;
  }

  const expectedTop = buffer.viewportY * rowHeight;

  const dimensions = term?._core?._renderService?.dimensions;
  const canvasHeight = dimensions?.canvasHeight;
  if (Number.isFinite(canvasHeight) && canvasHeight > 0) {
    const viewportHeight = viewport.offsetHeight;
    const desiredBufferHeight =
      Math.round(rowHeight * buffer.length) + (viewportHeight - canvasHeight);
    if (Number.isFinite(desiredBufferHeight) && desiredBufferHeight > 0) {
      const scrollArea = coreViewport._scrollArea;
      if (
        scrollArea instanceof HTMLElement &&
        Math.abs(scrollArea.offsetHeight - desiredBufferHeight) > 1
      ) {
        scrollArea.style.height = `${desiredBufferHeight}px`;
      }
    }
  }

  if (Math.abs(viewport.scrollTop - expectedTop) > 1) {
    viewport.scrollTop = expectedTop;
  }
  return true;
}

function terminalViewportRowHeight() {
  const dimensions = term?._core?._renderService?.dimensions;
  const fromViewport = term?._core?.viewport?._currentRowHeight;
  const candidate = Number.isFinite(fromViewport) && fromViewport > 0
    ? fromViewport
    : dimensions?.actualCellHeight;
  return Number.isFinite(candidate) && candidate > 0 ? candidate : 0;
}

function scrollTerminalToDomScrollTop(scrollTop, maxScroll = null) {
  const rowHeight = terminalViewportRowHeight();
  const viewport = terminalViewportScrollEl || terminalViewportElement();
  if (!viewport || !rowHeight) {
    return false;
  }

  const effectiveMaxScroll = Number.isFinite(maxScroll)
    ? Math.max(maxScroll, 0)
    : Math.max(viewport.scrollHeight - viewport.clientHeight, 0);
  const targetScrollTop = clampNumber(Number(scrollTop) || 0, 0, effectiveMaxScroll);
  const targetLine = Math.max(Math.round(targetScrollTop / rowHeight), 0);

  if (typeof term.scrollToLine === "function") {
    term.scrollToLine(targetLine);
  } else {
    viewport.scrollTop = targetScrollTop;
  }

  scheduleTerminalViewportDomSync();
  return true;
}

function scheduleTerminalViewportDomSync() {
  syncTerminalViewportDomToBuffer();
  const sync = () => {
    syncTerminalViewportDomToBuffer();
    updateTerminalScrollBottomButton();
  };
  if (typeof window.requestAnimationFrame === "function") {
    window.requestAnimationFrame(sync);
  } else {
    window.setTimeout(sync, 0);
  }
  window.setTimeout(sync, 80);
}

function bindTerminalViewportScroll() {
  const viewport = terminalViewportElement();
  if (!viewport || viewport === terminalViewportScrollEl) {
    return;
  }

  terminalViewportScrollEl?.removeEventListener("scroll", handleTerminalViewportScroll);
  terminalViewportScrollEl?.removeEventListener("wheel", syncTerminalViewportScrollBeforeWheel, true);
  terminalViewportScrollEl = viewport;
  terminalViewportScrollEl.addEventListener("scroll", handleTerminalViewportScroll, {
    passive: true,
  });
  terminalViewportScrollEl.addEventListener(
    "wheel",
    syncTerminalViewportScrollBeforeWheel,
    { passive: true, capture: true },
  );
}

function clampNumber(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

function pageScrollMetrics() {
  if (!pageScrollRailEl) {
    return null;
  }

  const scrollingEl = document.scrollingElement || document.documentElement;
  const scrollHeight = Math.max(scrollingEl.scrollHeight, window.innerHeight);
  const maxScroll = Math.max(scrollHeight - window.innerHeight, 0);
  const trackHeight = pageScrollRailEl.clientHeight || 0;
  const usableHeight = Math.max(trackHeight - PAGE_SCROLL_RAIL_PADDING * 2, 0);
  const thumbHeight =
    usableHeight > 0
      ? clampNumber(
          Math.round((window.innerHeight / scrollHeight) * usableHeight),
          Math.min(PAGE_SCROLL_RAIL_MIN_THUMB_SIZE, usableHeight),
          usableHeight,
        )
      : 0;
  const travel = Math.max(usableHeight - thumbHeight, 0);
  const scrollTop = window.scrollY || scrollingEl.scrollTop || 0;
  const ratio = maxScroll > 0 ? clampNumber(scrollTop / maxScroll, 0, 1) : 0;

  return {
    maxScroll,
    thumbHeight,
    trackHeight,
    travel,
    ratio,
    usableHeight,
  };
}

function capturePageScrollSnapshotForLayout() {
  const scrollingEl = document.scrollingElement || document.documentElement;
  if (!scrollingEl) {
    return null;
  }

  const scrollHeight = Math.max(scrollingEl.scrollHeight, window.innerHeight);
  const maxScroll = Math.max(scrollHeight - window.innerHeight, 0);
  const scrollTop = window.scrollY || scrollingEl.scrollTop || 0;
  return {
    atBottom: maxScroll <= 0 || maxScroll - scrollTop <= TERMINAL_PAGE_SCROLL_BOTTOM_TOLERANCE_PX,
  };
}

function restorePageScrollSnapshotForLayout(snapshot) {
  if (!snapshot?.atBottom) {
    return;
  }

  const scrollingEl = document.scrollingElement || document.documentElement;
  if (!scrollingEl) {
    return;
  }

  const scrollHeight = Math.max(scrollingEl.scrollHeight, window.innerHeight);
  const maxScroll = Math.max(scrollHeight - window.innerHeight, 0);
  const currentScrollTop = window.scrollY || scrollingEl.scrollTop || 0;
  if (maxScroll - currentScrollTop <= TERMINAL_PAGE_SCROLL_BOTTOM_TOLERANCE_PX) {
    updatePageScrollRail();
    return;
  }
  window.scrollTo({
    top: maxScroll,
    behavior: "auto",
  });
  updatePageScrollRail();
}

function pageIsAtBottomForLayout() {
  return Boolean(capturePageScrollSnapshotForLayout()?.atBottom);
}

function schedulePageScrollSnapshotRestore(snapshot) {
  if (!snapshot?.atBottom) {
    return;
  }

  const token = ++pageScrollLayoutRestoreToken;
  const restore = () => {
    if (token !== pageScrollLayoutRestoreToken) {
      return;
    }
    restorePageScrollSnapshotForLayout(snapshot);
  };

  if (typeof window.requestAnimationFrame === "function") {
    window.requestAnimationFrame(restore);
  } else {
    window.setTimeout(restore, 0);
  }
  window.setTimeout(restore, 80);
  window.setTimeout(restore, TERMINAL_LAYOUT_SCROLL_SUPPRESSION_MS);
}

function preservePageScrollDuringLayout(layoutCallback) {
  const snapshot = capturePageScrollSnapshotForLayout();
  try {
    layoutCallback();
  } finally {
    restorePageScrollSnapshotForLayout(snapshot);
    schedulePageScrollSnapshotRestore(snapshot);
  }
}

function beginSessionPageScrollRestore(sessionId, connectionId = null) {
  if (!sessionId) {
    activeSessionPageScrollRestore = null;
    return null;
  }

  // Always restore to page bottom on session switch, regardless of where the
  // page was before. term.reset() collapses document height and the browser
  // moves scrollTop toward 0; without an active restore state the page stays
  // stranded at the top. The user expects the new terminal to appear at the
  // bottom after switching.
  const restoreState = {
    token: ++sessionPageScrollRestoreToken,
    sessionId,
    connectionId: Number.isInteger(connectionId) ? connectionId : null,
    snapshot: { atBottom: true },
    startedAt: Date.now(),
    lastRestoredAt: 0,
  };
  activeSessionPageScrollRestore = restoreState;
  return restoreState;
}

function sessionPageScrollRestoreStillActive(restoreState) {
  if (!restoreState || restoreState !== activeSessionPageScrollRestore) {
    return false;
  }
  if (restoreState.token !== sessionPageScrollRestoreToken) {
    return false;
  }
  if (restoreState.sessionId !== state.activeSessionId) {
    return false;
  }
  if (restoreState.connectionId !== null && restoreState.connectionId !== connectionToken) {
    return false;
  }
  return (
    terminalBacklogReplayActive ||
    Date.now() - restoreState.startedAt <= TERMINAL_SESSION_PAGE_SCROLL_RESTORE_MS
  );
}

function bindSessionPageScrollRestoreToConnection(sessionId, connectionId) {
  if (
    activeSessionPageScrollRestore?.sessionId === sessionId &&
    activeSessionPageScrollRestore.connectionId === null
  ) {
    activeSessionPageScrollRestore.connectionId = connectionId;
    scheduleSessionPageScrollRestore(activeSessionPageScrollRestore);
  }
}

function restoreSessionPageScrollIfActive(restoreState = activeSessionPageScrollRestore) {
  if (terminalBacklogReplayActive) {
    return;
  }

  if (!sessionPageScrollRestoreStillActive(restoreState)) {
    if (restoreState === activeSessionPageScrollRestore) {
      activeSessionPageScrollRestore = null;
    }
    return;
  }

  sessionPageScrollProgrammaticUntil = Date.now() + 120;
  restorePageScrollSnapshotForLayout(restoreState.snapshot);
  restoreState.lastRestoredAt = Date.now();
}

function cancelSessionPageScrollRestore() {
  if (!activeSessionPageScrollRestore) {
    return;
  }

  activeSessionPageScrollRestore = null;
  sessionPageScrollRestoreToken += 1;
}

function cancelSessionPageScrollRestoreForUserScrollIntent() {
  cancelTerminalBottomAnchor();
  if (!activeSessionPageScrollRestore || Date.now() <= sessionPageScrollProgrammaticUntil) {
    return;
  }
  cancelSessionPageScrollRestore();
}

function scheduleSessionPageScrollRestore(restoreState = activeSessionPageScrollRestore) {
  if (!restoreState?.snapshot?.atBottom) {
    return;
  }

  const restore = () => {
    restoreSessionPageScrollIfActive(restoreState);
  };

  restore();
  if (typeof window.requestAnimationFrame === "function") {
    window.requestAnimationFrame(restore);
  } else {
    window.setTimeout(restore, 0);
  }
  window.setTimeout(restore, 80);
  window.setTimeout(restore, TERMINAL_LAYOUT_SCROLL_SUPPRESSION_MS);
  window.setTimeout(restore, TERMINAL_SESSION_PAGE_SCROLL_RESTORE_MS);
}

function updatePageScrollRail() {
  if (!pageScrollRailEl || !pageScrollThumbEl) {
    return;
  }

  const metrics = pageScrollMetrics();
  const shouldHide = !metrics || metrics.trackHeight <= 0 || metrics.maxScroll <= 0;
  pageScrollRailEl.hidden = shouldHide;
  if (shouldHide) {
    return;
  }

  const offset = PAGE_SCROLL_RAIL_PADDING + metrics.travel * metrics.ratio;
  pageScrollThumbEl.style.height = `${Math.round(metrics.thumbHeight)}px`;
  pageScrollThumbEl.style.transform = `translateY(${Math.round(offset)}px)`;

  const percent = Math.round(metrics.ratio * 100);
  pageScrollRailEl.setAttribute("aria-valuenow", String(percent));
  if (percent <= 0) {
    pageScrollRailEl.setAttribute("aria-valuetext", "页面顶部");
  } else if (percent >= 100) {
    pageScrollRailEl.setAttribute("aria-valuetext", "页面底部");
  } else {
    pageScrollRailEl.setAttribute("aria-valuetext", `页面位置 ${percent}%`);
  }
}

function scrollPageToRatio(ratio) {
  const metrics = pageScrollMetrics();
  if (!metrics) {
    return;
  }

  window.scrollTo({
    top: Math.round(clampNumber(ratio, 0, 1) * metrics.maxScroll),
    behavior: "auto",
  });
}

function scrollPageByAmount(delta) {
  if (!delta) {
    return;
  }

  window.scrollBy({
    top: delta,
    behavior: "auto",
  });
}

function railRatioFromClientY(clientY) {
  const metrics = pageScrollMetrics();
  if (!metrics || !pageScrollRailEl) {
    return 0;
  }

  const rect = pageScrollRailEl.getBoundingClientRect();
  if (metrics.travel <= 0) {
    return clientY < rect.top + rect.height / 2 ? 0 : 1;
  }

  const offset = clientY - rect.top - PAGE_SCROLL_RAIL_PADDING - metrics.thumbHeight / 2;
  return clampNumber(offset / metrics.travel, 0, 1);
}

function handlePageScrollRailPointerDown(event) {
  if (!pageScrollRailEl || (event.pointerType !== "touch" && event.button !== 0)) {
    return;
  }

  event.preventDefault();
  pageScrollRailEl.classList.add("dragging");
  pageScrollRailDrag = {
    pointerId: event.pointerId,
  };
  if (typeof pageScrollRailEl.setPointerCapture === "function") {
    pageScrollRailEl.setPointerCapture(event.pointerId);
  }
  scrollPageToRatio(railRatioFromClientY(event.clientY));
}

function handlePageScrollRailPointerMove(event) {
  if (!pageScrollRailDrag || event.pointerId !== pageScrollRailDrag.pointerId) {
    return;
  }

  event.preventDefault();
  scrollPageToRatio(railRatioFromClientY(event.clientY));
}

function clearPageScrollRailDrag(pointerId = null) {
  if (!pageScrollRailDrag || (pointerId !== null && pointerId !== pageScrollRailDrag.pointerId)) {
    return;
  }

  if (
    pageScrollRailEl &&
    typeof pageScrollRailEl.hasPointerCapture === "function" &&
    pageScrollRailEl.hasPointerCapture(pageScrollRailDrag.pointerId)
  ) {
    pageScrollRailEl.releasePointerCapture(pageScrollRailDrag.pointerId);
  }
  pageScrollRailEl?.classList.remove("dragging");
  pageScrollRailDrag = null;
  focusTerminalAfterTransientControl();
}

function handlePageScrollRailWheel(event) {
  event.preventDefault();
  scrollPageByAmount(event.deltaY);
}

function handlePageScrollRailKeydown(event) {
  switch (event.key) {
    case "Home":
      event.preventDefault();
      scrollPageToTop();
      break;
    case "End":
      event.preventDefault();
      scrollPageToRatio(1);
      break;
    case "PageUp":
      event.preventDefault();
      scrollPageByAmount(-window.innerHeight * 0.8);
      break;
    case "PageDown":
    case " ":
      event.preventDefault();
      scrollPageByAmount(window.innerHeight * 0.8);
      break;
    case "ArrowUp":
      event.preventDefault();
      scrollPageByAmount(-120);
      break;
    case "ArrowDown":
      event.preventDefault();
      scrollPageByAmount(120);
      break;
  }
}

function handleTerminalNavScrollPointerDown(event) {
  if (!terminalNavScrollEl || (event.pointerType === "mouse" && event.button !== 0)) {
    return;
  }

  event.preventDefault();
  if (terminalNavScrollEl.scrollWidth <= terminalNavScrollEl.clientWidth + 1) {
    focusTerminalAfterTransientControl();
    return;
  }

  terminalNavScrollDrag = {
    pointerId: event.pointerId,
    startX: event.clientX,
    startScrollLeft: terminalNavScrollEl.scrollLeft,
  };
  terminalNavScrollEl.classList.add("dragging");
  if (typeof terminalNavScrollEl.setPointerCapture === "function") {
    terminalNavScrollEl.setPointerCapture(event.pointerId);
  }
}

function handleTerminalNavScrollPointerMove(event) {
  if (!terminalNavScrollEl || !terminalNavScrollDrag || event.pointerId !== terminalNavScrollDrag.pointerId) {
    return;
  }

  terminalNavScrollEl.scrollLeft =
    terminalNavScrollDrag.startScrollLeft - (event.clientX - terminalNavScrollDrag.startX);
  event.preventDefault();
}

function clearTerminalNavScrollDrag(pointerId = null) {
  if (
    !terminalNavScrollDrag ||
    (pointerId !== null && pointerId !== terminalNavScrollDrag.pointerId)
  ) {
    return;
  }

  if (
    terminalNavScrollEl &&
    typeof terminalNavScrollEl.hasPointerCapture === "function" &&
    terminalNavScrollEl.hasPointerCapture(terminalNavScrollDrag.pointerId)
  ) {
    terminalNavScrollEl.releasePointerCapture(terminalNavScrollDrag.pointerId);
  }
  terminalNavScrollEl?.classList.remove("dragging");
  terminalNavScrollDrag = null;
  focusTerminalAfterTransientControl();
}
