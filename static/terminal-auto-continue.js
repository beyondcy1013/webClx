function syncAutoContinueHandledErrors() {
  const activeIds = new Set(state.sessions.map((session) => session.id).filter(Boolean));
  state.autoContinueHandledErrors.forEach((_key, sessionId) => {
    if (!activeIds.has(sessionId)) {
      state.autoContinueHandledErrors.delete(sessionId);
      clearAutoContinueSchedule(sessionId);
    }
  });

  state.sessions.forEach((session) => {
    if (!isSessionErrorState(session)) {
      state.autoContinueHandledErrors.delete(session.id);
      clearAutoContinueSchedule(session.id);
    }
  });
}

function terminalRecentLinesContainCompact(sessionId, lineCount = 20) {
  const context = ensureTerminalSessionCache()?.get?.(sessionId);
  const buffer = context?.term?.buffer?.active;
  if (!buffer || typeof buffer.length !== "number" || typeof buffer.getLine !== "function") {
    return false;
  }
  const start = Math.max(0, buffer.length - Math.max(1, Math.trunc(lineCount)));
  for (let index = start; index < buffer.length; index += 1) {
    const line = buffer.getLine(index);
    const text = line?.translateToString?.(true) || "";
    if (/\/compact\b/i.test(text)) {
      return true;
    }
  }
  return false;
}

function terminalRecentLinesContainCompacted(sessionId, lineCount = 20) {
  const context = ensureTerminalSessionCache()?.get?.(sessionId);
  const buffer = context?.term?.buffer?.active;
  if (!buffer || typeof buffer.length !== "number" || typeof buffer.getLine !== "function") return false;
  const start = Math.max(0, buffer.length - Math.max(1, Math.trunc(lineCount)));
  for (let index = start; index < buffer.length; index += 1) {
    const text = buffer.getLine(index)?.translateToString?.(true) || "";
    if (/Context compacted|上下文已压缩|上下文压缩完成/i.test(text)) return true;
  }
  return false;
}

function terminalSessionHasRecentUserInput(sessionId, now = Date.now()) {
  const sentAt = Number(state.terminalUserInputAt?.get(sessionId) || 0);
  return Number.isFinite(sentAt) && sentAt > 0 && now - sentAt < 60000;
}

function terminalRecentLinesContainQueuedInput(sessionId, lineCount = 20) {
  const context = ensureTerminalSessionCache()?.get?.(sessionId);
  const buffer = context?.term?.buffer?.active;
  if (!buffer || typeof buffer.length !== "number" || typeof buffer.getLine !== "function") return false;
  const start = Math.max(0, buffer.length - Math.max(1, Math.trunc(lineCount)));
  for (let index = start; index < buffer.length; index += 1) {
    const text = buffer.getLine(index)?.translateToString?.(true) || "";
    if (text.includes("Messages to be submitted at end of turn") || text.includes("Queued follow-up inputs")) return true;
  }
  return false;
}

// "压缩中" comes from the backend activity state, which observes the agent's
// real compaction progress line. The selector therefore never has to guess
// from a sent /compact, and a command that was typed but never executed (a bare
// shell prompt, a busy agent) stays out of the compacting state. Only the
// finished transition is tracked here: the compacting state disappearing for a
// terminal we auto-compacted means the compaction really ran.
function syncAutoCompactCompletionStatuses() {
  const previousStates = state.autoCompactPreviousActivityStates;
  const activeIds = new Set();
  state.sessions.forEach((session) => {
    const sessionId = String(session?.id || "").trim();
    if (!sessionId) return;
    activeIds.add(sessionId);
    const currentState = sessionActivityState(session);
    if (
      previousStates.get(sessionId) === "compacting"
      && currentState !== "compacting"
      && state.autoCompactLastSent.has(sessionId)
      // A terminal the user is already watching needs no "come look" badge.
      && sessionId !== state.activeSessionId
    ) {
      state.autoCompactStatusSessionIds.add(sessionId);
      storeTerminalAutoCompactStatusSessionIds(state.autoCompactStatusSessionIds);
    }
    previousStates.set(sessionId, currentState);
  });
  Array.from(previousStates.keys()).forEach((sessionId) => {
    if (!activeIds.has(sessionId)) {
      previousStates.delete(sessionId);
    }
  });
}

function terminalOutputLineCount(sessionId) {
  const context = ensureTerminalSessionCache()?.get?.(sessionId);
  const length = context?.term?.buffer?.active?.length;
  return typeof length === "number" && Number.isFinite(length) ? length : null;
}

// AGY does not support /compact. Identify AGY terminals by their reported
// activity agent or by the terminal name containing "agy" (case-insensitive).
function isSessionAgyTerminal(session) {
  const agent = String(session?.activity_agent || "").toLowerCase();
  if (agent.includes("agy")) return true;
  const name = String(session?.name || "").toLowerCase();
  return name === "agy" || name.startsWith("agy ") || name.startsWith("agy-");
}

function isSessionCompactCapableAgent(session) {
  // These agents must not receive webClx-generated /compact, regardless of
  // legacy settings or a previously enabled per-session compact toggle.
  const agent = String(session?.activity_agent || "").toLowerCase();
  return !agent.includes("cline") && !agent.includes("agy")
    && !agent.includes("antigravity") && !agent.includes("codebuddy")
    && !isSessionAgyTerminal(session);
}

function terminalCompactCommandAllowed(command, sessionId) {
  if (String(command || "").trim().split(/\s+/)[0] !== "/compact") return true;
  const session = state.sessions?.find((item) => item?.id === sessionId);
  if (isSessionCompactCapableAgent(session)) return true;
  updateStatus(`${session?.activity_agent || "AGY"} 终端禁止由 webClx 发送 /compact。`, "warn");
  return false;
}

function maybeAutoCompactCompletedSession(session) {
  if (!session?.id || !state.autoCompactSessionIds.has(session.id)) return false;
  // Check before scheduling and again at send time after agent detection.
  if (!isSessionCompactCapableAgent(session) || isSessionAgyTerminal(session)) return false;
  // A terminal that is currently open is already being viewed. Its backend
  // activity can briefly lag behind the viewed acknowledgement, so never
  // inject /compact into the user's active terminal.
  if (session.id === state.activeSessionId) {
    state.autoCompactHandledSessions.delete(session.id);
    const activeTimer = state.autoCompactScheduledTimers.get(session.id);
    if (activeTimer) {
      window.clearTimeout(activeTimer);
      state.autoCompactScheduledTimers.delete(session.id);
    }
    return false;
  }
  const completed = sessionActivityState(session) === "completed" || sessionActivityLabel(session) === "待查看";
  if (!completed) {
    return false;
  }
  // Auto-typed /compact is deliberately absent from terminal history. Its
  // completion output is therefore the reliable nearby-output guard after the
  // compacted status has been cleared by viewing the terminal.
  if (terminalSessionHasRecentUserInput(session.id)) return false;
  if (terminalRecentLinesContainQueuedInput(session.id, 20)) return false;
  if (terminalRecentLinesContainCompacted(session.id, 20)) {
    state.autoCompactHandledSessions.add(session.id);
    const completedTimer = state.autoCompactScheduledTimers.get(session.id);
    if (completedTimer) {
      window.clearTimeout(completedTimer);
      state.autoCompactScheduledTimers.delete(session.id);
    }
    return false;
  }
  const lastSent = state.autoCompactLastSent.get(session.id);
  if (lastSent) {
    const currentLineCount = terminalOutputLineCount(session.id);
    const linesSinceCompact = currentLineCount === null
      ? 0
      : Math.max(0, currentLineCount - Number(lastSent.lineCount || currentLineCount));
    if (linesSinceCompact < 20) {
      state.autoCompactHandledSessions.add(session.id);
      return false;
    }
    state.autoCompactLastSent.delete(session.id);
    state.autoCompactHandledSessions.delete(session.id);
  }
  if (terminalRecentLinesContainCompact(session.id, 20)) {
    state.autoCompactHandledSessions.add(session.id);
    return false;
  }
  if (state.autoCompactHandledSessions.has(session.id)) return false;
  state.autoCompactHandledSessions.add(session.id);
  const sessionLabel = autoContinueSessionLabel(session);
  updateStatus(`${sessionLabel}处于“待查看”，持续 5 秒后再进入压缩倒计时。`, "info");
  const timerId = window.setTimeout(() => {
    state.autoCompactScheduledTimers.delete(session.id);
    const latest = state.sessions.find((item) => item?.id === session.id);
    if (
      !latest
      || session.id === state.activeSessionId
      || !state.autoCompactSessionIds.has(session.id)
      || !isSessionCompactCapableAgent(latest)
      || (sessionActivityState(latest) !== "completed" && sessionActivityLabel(latest) !== "待查看")
    ) {
      state.autoCompactHandledSessions.delete(session.id);
      return;
    }
    updateStatus(`${sessionLabel}已持续“待查看”5 秒，2 秒后自动压缩。`, "info");
    const sendTimerId = window.setTimeout(() => {
      state.autoCompactScheduledTimers.delete(session.id);
      resolveAutoCompactAfterDelay(session.id, sessionLabel);
    }, 2000);
    state.autoCompactScheduledTimers.set(session.id, sendTimerId);
  }, 5000);
  state.autoCompactScheduledTimers.set(session.id, timerId);
  return true;
}

function resolveAutoCompactAfterDelay(sessionId, sessionLabel) {
  const session = state.sessions.find((item) => item?.id === sessionId);
  if (!session) {
    state.autoCompactHandledSessions.delete(sessionId);
    return;
  }
  Promise.resolve()
    .then(() => {
      const latest = state.sessions.find((item) => item?.id === sessionId);
      if (
        !state.autoCompactSessionIds.has(sessionId)
        || sessionId === state.activeSessionId
        || !latest
        || (sessionActivityState(latest) !== "completed" && sessionActivityLabel(latest) !== "待查看")
      ) {
        state.autoCompactHandledSessions.delete(sessionId);
        return null;
      }
      // Re-check the live agent type after the delay; a session can finish its
      // first turn only after the process detector has identified Cline.
      if (!isSessionCompactCapableAgent(latest) || isSessionAgyTerminal(latest)) {
        state.autoCompactHandledSessions.delete(sessionId);
        return null;
      }
      if (terminalSessionHasRecentUserInput(sessionId)) {
        state.autoCompactHandledSessions.delete(sessionId);
        return null;
      }
      if (terminalRecentLinesContainQueuedInput(sessionId, 20)) {
        state.autoCompactHandledSessions.delete(sessionId);
        return null;
      }
      if (terminalRecentLinesContainCompacted(sessionId, 20)) {
        state.autoCompactHandledSessions.delete(sessionId);
        state.autoCompactLastSent.delete(sessionId);
        storeTerminalAutoCompactLastSent(state.autoCompactLastSent);
        return null;
      }
      return sendTerminalAutoTypedInput("/compact", {
        sessionId,
        submitEnters: 1,
        throwOnError: true,
      });
    })
    .then((sent) => {
      if (sent === null) return;
      if (!sent) {
        state.autoCompactHandledSessions.delete(sessionId);
        return;
      }
      state.autoCompactLastSent.set(sessionId, {
        lineCount: terminalOutputLineCount(sessionId),
        sentAt: Date.now(),
      });
      storeTerminalAutoCompactLastSent(state.autoCompactLastSent);
      // Do not claim "压缩中" here: writing the bytes only means the command was
      // delivered. The compacting state starts when the agent's own compaction
      // progress line reaches the backend activity probe.
      updateStatus(`${sessionLabel}已发送 /compact，等待压缩开始。`, "info");
    })
    .catch((error) => {
      state.autoCompactHandledSessions.delete(sessionId);
      updateStatus(error.message || `${sessionLabel}自动压缩失败。`, "warn");
    });
  return true;
}

function maybeAutoCompactCompletedSessions() {
  syncAutoCompactCompletionStatuses();
  let sent = false;
  state.sessions
    .filter((session) => session?.id && state.autoCompactSessionIds.has(session.id))
    .forEach((session) => {
      sent = maybeAutoCompactCompletedSession(session) || sent;
    });
  return sent;
}

function autoContinueHandledKey(entry) {
  if (!entry) {
    return "";
  }
  if (typeof entry === "string") {
    return entry;
  }
  return typeof entry.key === "string" ? entry.key : "";
}

function autoContinueRetryDue(entry) {
  const intervalSeconds = normalizeTerminalAutoContinueIntervalSeconds(
    state.terminalAutoContinueIntervalSeconds,
  );
  if (intervalSeconds <= 0) {
    return false;
  }
  const sentAt = typeof entry?.sentAt === "number" ? entry.sentAt : 0;
  return Date.now() - sentAt >= intervalSeconds * 1000;
}

function clearAutoContinueSchedule(sessionId) {
  const existing = state.autoContinueScheduledTimers.get(sessionId);
  if (existing?.timerId) {
    window.clearTimeout(existing.timerId);
  }
  state.autoContinueScheduledTimers.delete(sessionId);
}

// Cooldowns are session-scoped. Expire only the marker belonging to this
// terminal so another terminal can continue independently.
function scheduleAutoContinueCooldownCleanup(sessionId, sentAt, delayMs) {
  const existing = state.autoContinueScheduledTimers.get(sessionId);
  if (existing?.timerId) {
    window.clearTimeout(existing.timerId);
  }
  const timerId = window.setTimeout(() => {
    const current = state.autoContinueHandledErrors.get(sessionId);
    if (current && Number(current.sentAt) === sentAt) {
      state.autoContinueHandledErrors.delete(sessionId);
    }
    state.autoContinueScheduledTimers.delete(sessionId);
  }, Math.max(0, Number(delayMs) || 0));
  state.autoContinueScheduledTimers.set(sessionId, { timerId, sentAt });
}

function clearAutoContinueSchedules() {
  Array.from(state.autoContinueScheduledTimers.keys()).forEach(clearAutoContinueSchedule);
}

function sessionAutoContinueResetAt(session) {
  return String(
    session?.activity_error_auto_continue_at || session?.activityErrorAutoContinueAt || "",
  ).trim();
}

function autoContinueSessionLabel(session) {
  const name = String(session?.name || "").trim();
  if (name) {
    return `终端“${name}”`;
  }
  const sessionId = String(session?.id || "").trim();
  return sessionId ? `终端 ${sessionId}` : "当前终端";
}

function parseTerminalAutoContinueResetAt(value) {
  const text = String(value || "").trim();
  if (!text) {
    return Number.NaN;
  }
  const normalized = text.includes("T") ? text : text.replace(" ", "T");
  return Date.parse(normalized);
}

function scheduleAutoContinueAtResetTime(session, errorKey, resetAt) {
  const existing = state.autoContinueHandledErrors.get(session.id);
  const now = Date.now();
  if (existing?.resetAt === resetAt || hasAutoContinueScheduleAck(session.id, resetAt, now)) {
    if (existing?.resetAt !== resetAt) {
      state.autoContinueHandledErrors.set(session.id, {
        key: errorKey,
        sentAt: now,
        resetAt,
      });
    }
    return true;
  }
  const resetAtMs = parseTerminalAutoContinueResetAt(resetAt);
  if (!Number.isFinite(resetAtMs)) {
    state.autoContinueHandledErrors.set(session.id, { key: errorKey, sentAt: now, resetAt });
    rememberAutoContinueScheduleAck(session.id, resetAt, now);
    updateStatus(`检测到${autoContinueSessionLabel(session)}限额重置时间 ${resetAt}，已添加定时，将在重置后 1 分钟发送“继续”。`, "info");
    return true;
  }
  const dueAtMs = resetAtMs + 60 * 1000;
  clearAutoContinueSchedule(session.id);
  state.autoContinueHandledErrors.set(session.id, {
    key: errorKey,
    sentAt: now,
    resetAt,
    dueAt: new Date(dueAtMs).toISOString(),
  });
  rememberAutoContinueScheduleAck(session.id, resetAt, now);
  updateStatus(`检测到${autoContinueSessionLabel(session)}限额重置时间 ${resetAt}，已添加定时，将在重置后 1 分钟发送“继续”。`, "info");
  return true;
}

async function sendContinueToSession(session) {
  return requestJson(`/api/terminal/sessions/${encodeURIComponent(session.id)}/auto-continue`, {
    method: "POST",
  });
}

function maybeAutoContinueErroredSession(session = activeSession()) {
  if (!state.autoContinueOnError || !session?.id) {
    return false;
  }
  if (!isSessionErrorState(session)) {
    state.autoContinueHandledErrors.delete(session.id);
    return false;
  }

  const errorKey = sessionErrorContinueKey(session);
  if (!errorKey) {
    return false;
  }
  const existing = state.autoContinueHandledErrors.get(session.id);
  const resetAt = sessionAutoContinueResetAt(session);
  if (resetAt && scheduleAutoContinueAtResetTime(session, errorKey, resetAt)) {
    return true;
  }
  if (existing && !autoContinueRetryDue(existing)) {
    return false;
  }

  const continueSent = Boolean(session?.activity_error_continue_sent || session?.activityErrorContinueSent);
  if (continueSent) {
    if (!existing || autoContinueHandledKey(existing) !== errorKey) {
      state.autoContinueHandledErrors.set(session.id, { key: errorKey, sentAt: Date.now() });
      return false;
    }
    if (!autoContinueRetryDue(existing)) {
      return false;
    }
  }
  const inputQueued = Boolean(session?.activity_error_input_queued || session?.activityErrorInputQueued);
  if (inputQueued) {
    return false;
  }

  const cooldownSentAt = Date.now();
  state.autoContinueHandledErrors.set(session.id, { key: errorKey, sentAt: cooldownSentAt });
  scheduleAutoContinueCooldownCleanup(
    session.id,
    cooldownSentAt,
    normalizeTerminalAutoContinueIntervalSeconds(state.terminalAutoContinueIntervalSeconds) * 1000,
  );
  const keyword = String(session.activity_error_keyword || session.activityErrorKeyword || "").trim();
  const sessionLabel = autoContinueSessionLabel(session);
  sendContinueToSession(session)
    .then((result) => {
      if (result?.sent === false) {
        const retrySeconds = Math.ceil(Number(result.retry_after_millis || 0) / 1000);
        const retrySentAt = Date.now();
        state.autoContinueHandledErrors.set(session.id, { key: errorKey, sentAt: retrySentAt });
        scheduleAutoContinueCooldownCleanup(session.id, retrySentAt, Number(result.retry_after_millis || 0));
        if (retrySeconds > 0) {
          updateStatus(`${sessionLabel}自动继续冷却中，约 ${retrySeconds} 秒后可再次发送。`, "info");
        }
        return;
      }
      if (result?.compact_sent) {
        updateStatus(
          keyword
            ? `检测到${sessionLabel}错误“${keyword}”，已发送 /compact。`
            : `检测到${sessionLabel}上下文窗口已满，已发送 /compact。`,
          "ok",
        );
        return;
      }
      updateStatus(
        keyword
          ? `检测到${sessionLabel}错误“${keyword}”，已发送“继续”。`
          : `检测到${sessionLabel}错误，已发送“继续”。`,
        "ok",
      );
    })
    .catch((error) => {
      if (autoContinueHandledKey(state.autoContinueHandledErrors.get(session.id)) === errorKey) {
        state.autoContinueHandledErrors.delete(session.id);
      }
      clearAutoContinueSchedule(session.id);
      updateStatus(error.message || `${sessionLabel}自动发送“继续”失败。`, "warn");
    });
  return true;
}

function maybeAutoContinueErroredSessions() {
  maybeAutoCompactCompletedSessions();
  let sent = false;
  const params = new URLSearchParams(window.location.search);
  const embeddedAgent = params.get("embedded") === "agent";
  const candidates = embeddedAgent
    ? [activeSession()].filter(Boolean)
    : state.sessions.filter((session) => session?.origin === "normal");
  candidates.forEach((session) => {
    sent = maybeAutoContinueErroredSession(session) || sent;
  });
  return sent;
}

function setAutoContinueOnError(enabled) {
  state.autoContinueOnError = Boolean(enabled);
  storeTerminalAutoContinueOnError(state.autoContinueOnError);
  persistTerminalAutoContinueOnError(state.autoContinueOnError).catch((error) => {
    updateStatus(error.message || "保存自动继续设置失败。", "warn");
  });
  if (sessionAutoContinueToggleEl) {
    sessionAutoContinueToggleEl.checked = state.autoContinueOnError;
  }
  if (!state.autoContinueOnError) {
    state.autoContinueHandledErrors.clear();
    clearAutoContinueSchedules();
    return;
  }
  syncAutoContinueHandledErrors();
  maybeAutoContinueErroredSessions();
}
