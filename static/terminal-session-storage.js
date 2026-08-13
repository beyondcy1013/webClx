(function attachTerminalSessionStorage(root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
    return;
  }

  root.WebClxTerminalSessionStorage = factory(root);
})(typeof globalThis !== "undefined" ? globalThis : this, function createTerminalSessionStorage(root) {
  const SESSION_STORAGE_KEY = "webclx:last-terminal-sessions";
  const GLOBAL_SESSION_PREFERENCE_KEY = "__global_last_opened__";
  const SESSION_EVENT_STORAGE_KEY = "webclx:terminal-session-event";
  const PASSIVE_SESSION_REFRESH_ACTIONS = Object.freeze(["created", "deleted", "idle", "restored"]);

  // 跨页面事件键：首页 app.js 与终端页 terminal.js 共用，集中在此避免双份定义漂移。
  const SETTINGS_EVENT_STORAGE_KEY = "webclx:settings-event";
  const RESUME_ARCHIVE_EVENT_STORAGE_KEY = "webclx:codex-resume-archive-event";

  function sessionPreferenceKey(pathValue) {
    return pathValue || "__root__";
  }

  function readSessionPreferences() {
    try {
      const raw = root.localStorage.getItem(SESSION_STORAGE_KEY);
      if (!raw) {
        return {};
      }

      const parsed = JSON.parse(raw);
      return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : {};
    } catch {
      return {};
    }
  }

  function writeSessionPreferences(preferences) {
    try {
      root.localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(preferences));
    } catch {
      // Keep working even if localStorage is unavailable.
    }
  }

  function getStoredSessionId(pathValue) {
    const value = readSessionPreferences()[sessionPreferenceKey(pathValue)];
    return typeof value === "string" ? value : "";
  }

  function getStoredGlobalSessionId() {
    const value = readSessionPreferences()[GLOBAL_SESSION_PREFERENCE_KEY];
    return typeof value === "string" ? value : "";
  }

  function storeSessionId(pathValue, sessionId) {
    const preferences = readSessionPreferences();
    const key = sessionPreferenceKey(pathValue);
    if (sessionId) {
      preferences[key] = sessionId;
    } else {
      delete preferences[key];
    }
    writeSessionPreferences(preferences);
  }

  function storeGlobalSessionId(sessionId) {
    const preferences = readSessionPreferences();
    if (sessionId) {
      preferences[GLOBAL_SESSION_PREFERENCE_KEY] = sessionId;
    } else {
      delete preferences[GLOBAL_SESSION_PREFERENCE_KEY];
    }
    writeSessionPreferences(preferences);
  }

  function announceSessionMutation(action, session = {}, currentPath = "") {
    try {
      root.localStorage.setItem(
        SESSION_EVENT_STORAGE_KEY,
        JSON.stringify({
          action,
          session_id: session.id || "",
          path: session.path || currentPath || "",
          at: Date.now(),
        }),
      );
    } catch {
      // Keep working even if localStorage is unavailable.
    }
  }

  function parseSessionMutationEvent(rawValue) {
    if (!rawValue) {
      return null;
    }

    try {
      return JSON.parse(rawValue);
    } catch {
      return null;
    }
  }

  function shouldRefreshForSessionMutation(action) {
    return PASSIVE_SESSION_REFRESH_ACTIONS.includes(String(action || "").trim());
  }

  return {
    GLOBAL_SESSION_PREFERENCE_KEY,
    PASSIVE_SESSION_REFRESH_ACTIONS,
    RESUME_ARCHIVE_EVENT_STORAGE_KEY,
    SESSION_EVENT_STORAGE_KEY,
    SESSION_STORAGE_KEY,
    SETTINGS_EVENT_STORAGE_KEY,
    announceSessionMutation,
    getStoredGlobalSessionId,
    getStoredSessionId,
    parseSessionMutationEvent,
    readSessionPreferences,
    sessionPreferenceKey,
    shouldRefreshForSessionMutation,
    storeGlobalSessionId,
    storeSessionId,
  };
});
