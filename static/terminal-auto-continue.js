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
            ? `检测到${sessionLabel}错误“${keyword}”，已发送 /compact 并继续。`
            : `检测到${sessionLabel}上下文窗口已满，已发送 /compact 并继续。`,
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
