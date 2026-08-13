(function initTerminalSessionCache(root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  root.WebClxTerminalSessionCache = api;
})(typeof globalThis !== "undefined" ? globalThis : this, function createApi() {
  function createTerminalSessionCache({ createContext, activateContext, disposeContext }) {
    if (
      typeof createContext !== "function" ||
      typeof activateContext !== "function" ||
      typeof disposeContext !== "function"
    ) {
      throw new TypeError("terminal session cache callbacks are required");
    }

    const contexts = new Map();
    let activeContext = null;

    function normalizedSessionId(sessionId) {
      return typeof sessionId === "string" ? sessionId.trim() : "";
    }

    function get(sessionId) {
      return contexts.get(normalizedSessionId(sessionId)) || null;
    }

    function activate(sessionId) {
      const normalized = normalizedSessionId(sessionId);
      if (!normalized) {
        throw new TypeError("terminal session id is required");
      }

      let context = contexts.get(normalized);
      if (!context) {
        context = createContext(normalized);
        contexts.set(normalized, context);
      }

      const previousContext = activeContext;
      activeContext = context;
      activateContext(context, previousContext);
      return context;
    }

    function remove(sessionId) {
      const normalized = normalizedSessionId(sessionId);
      const context = contexts.get(normalized);
      if (!context) {
        return false;
      }

      contexts.delete(normalized);
      if (activeContext === context) {
        activeContext = null;
      }
      disposeContext(context);
      return true;
    }

    function prune(allowedSessionIds) {
      const allowed = allowedSessionIds instanceof Set
        ? allowedSessionIds
        : new Set(allowedSessionIds || []);
      for (const sessionId of Array.from(contexts.keys())) {
        if (!allowed.has(sessionId)) {
          remove(sessionId);
        }
      }
    }

    function clear() {
      for (const sessionId of Array.from(contexts.keys())) {
        remove(sessionId);
      }
    }

    function forEach(callback) {
      contexts.forEach(callback);
    }

    return {
      activate,
      clear,
      forEach,
      get,
      prune,
      remove,
      get activeContext() {
        return activeContext;
      },
      get size() {
        return contexts.size;
      },
    };
  }

  return { createTerminalSessionCache };
});
