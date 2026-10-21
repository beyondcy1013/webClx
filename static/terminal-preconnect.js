const TERMINAL_PRECONNECT_MAX_SESSIONS = 4;
const TERMINAL_PRECONNECT_STAGGER_MS = 300;
const TERMINAL_PRECONNECT_SCHEDULE_DELAY_MS = 50;

let terminalPreconnectScheduleTimer = null;
const terminalPreconnectSessionIds = new Set();
const terminalPreconnectTimers = new Map();

function terminalSessionEligibleForPreconnect(sessionId) {
  const normalizedSessionId = String(sessionId || "").trim();
  return Boolean(
    normalizedSessionId &&
      normalizedSessionId !== state.activeSessionId &&
      state.sessions.some((session) => session.id === normalizedSessionId) &&
      !isIdleSession(normalizedSessionId),
  );
}

function releaseTerminalPreconnectSession(sessionId) {
  const timer = terminalPreconnectTimers.get(sessionId);
  if (timer !== undefined) {
    window.clearTimeout(timer);
    terminalPreconnectTimers.delete(sessionId);
  }
  terminalPreconnectSessionIds.delete(sessionId);
}

function reconcileTerminalPreconnectSessions() {
  const cache = ensureTerminalSessionCache();
  for (const sessionId of Array.from(terminalPreconnectSessionIds)) {
    if (!terminalSessionEligibleForPreconnect(sessionId)) {
      releaseTerminalPreconnectSession(sessionId);
      continue;
    }

    const context = cache.get(sessionId);
    if (!terminalPreconnectTimers.has(sessionId) && (!context || context.disposed)) {
      terminalPreconnectSessionIds.delete(sessionId);
    }
  }
}

// Automatic background preconnecting keeps extra WebViews/xterm buffers warm.
// On Android (dom renderer) the visited-context cache is bounded, so warming
// sockets for sessions the user never opened would just churn evictions; only
// explicit session switches may create background contexts there.
function terminalBackgroundPreconnectEnabled() {
  return typeof terminalRendererType !== "function" || terminalRendererType() !== "dom";
}

function scheduleTerminalBackgroundPreconnect() {
  if (
    !terminalBackgroundPreconnectEnabled() ||
    document.visibilityState === "hidden" ||
    terminalPreconnectScheduleTimer !== null
  ) {
    return;
  }

  terminalPreconnectScheduleTimer = window.setTimeout(() => {
    terminalPreconnectScheduleTimer = null;
    runTerminalBackgroundPreconnect();
  }, TERMINAL_PRECONNECT_SCHEDULE_DELAY_MS);
}

function runTerminalBackgroundPreconnect() {
  reconcileTerminalPreconnectSessions();
  if (!terminalBackgroundPreconnectEnabled() || document.visibilityState === "hidden") {
    return;
  }

  const cache = ensureTerminalSessionCache();
  let availableSlots = TERMINAL_PRECONNECT_MAX_SESSIONS - terminalPreconnectSessionIds.size;
  let staggerIndex = 0;

  for (const session of state.sessions || []) {
    if (availableSlots <= 0) {
      break;
    }
    if (
      !terminalSessionEligibleForPreconnect(session.id) ||
      terminalPreconnectSessionIds.has(session.id)
    ) {
      continue;
    }

    const existingContext = cache.get(session.id);
    if (
      terminalContextSocketOpen(existingContext) ||
      terminalContextSocketConnecting(existingContext)
    ) {
      continue;
    }

    const sessionId = session.id;
    terminalPreconnectSessionIds.add(sessionId);
    availableSlots -= 1;
    const timer = window.setTimeout(() => {
      terminalPreconnectTimers.delete(sessionId);
      if (
        document.visibilityState === "hidden" ||
        !terminalPreconnectSessionIds.has(sessionId) ||
        !terminalSessionEligibleForPreconnect(sessionId)
      ) {
        terminalPreconnectSessionIds.delete(sessionId);
        scheduleTerminalBackgroundPreconnect();
        return;
      }

      const context = getOrCreateBackgroundTerminalContext(sessionId);
      if (!context || context.disposed) {
        terminalPreconnectSessionIds.delete(sessionId);
        scheduleTerminalBackgroundPreconnect();
        return;
      }
      if (
        terminalContextSocketOpen(context) ||
        terminalContextSocketConnecting(context)
      ) {
        return;
      }

      try {
        connectTerminal(context);
      } catch (error) {
        terminalPreconnectSessionIds.delete(sessionId);
        console.warn(`terminal background preconnect failed for ${sessionId}`, error);
        scheduleTerminalBackgroundPreconnect();
      }
    }, staggerIndex * TERMINAL_PRECONNECT_STAGGER_MS);
    terminalPreconnectTimers.set(sessionId, timer);
    staggerIndex += 1;
  }
}

function cancelTerminalBackgroundPreconnect() {
  if (terminalPreconnectScheduleTimer !== null) {
    window.clearTimeout(terminalPreconnectScheduleTimer);
    terminalPreconnectScheduleTimer = null;
  }
  for (const timer of terminalPreconnectTimers.values()) {
    window.clearTimeout(timer);
  }
  terminalPreconnectTimers.clear();
  terminalPreconnectSessionIds.clear();
}
