(function initTerminalSessionCache(root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  root.WebClxTerminalSessionCache = api;
})(typeof globalThis !== "undefined" ? globalThis : this, function createApi() {
  function createTerminalSessionCache({
    createContext,
    activateContext,
    disposeContext,
    maxContexts = Infinity,
    canEvictContext = null,
  }) {
    if (
      typeof createContext !== "function" ||
      typeof activateContext !== "function" ||
      typeof disposeContext !== "function"
    ) {
      throw new TypeError("terminal session cache callbacks are required");
    }

    const contextLimit =
      maxContexts === Infinity ? Infinity : Math.max(1, Math.trunc(Number(maxContexts) || 0));
    // Map insertion order doubles as the LRU order: entries are re-inserted at
    // the end whenever they are created or activated.
    const contexts = new Map();
    let activeContext = null;

    function touch(sessionId, context) {
      contexts.delete(sessionId);
      contexts.set(sessionId, context);
    }

    function evictable(context, protectedContext) {
      if (!context || context === activeContext || context === protectedContext) {
        return false;
      }
      if (typeof canEvictContext === "function" && !canEvictContext(context)) {
        return false;
      }
      return true;
    }

    // Soft-bounded: if every candidate is pinned (active, just returned, or
    // busy with pending input/writes/replay) the cache stays over budget until
    // a later trim finds an evictable context. Evicted contexts are recreated
    // and replayed from scratch on the next visit — bytes are never dropped.
    function trim(protectedContext = null) {
      if (contexts.size <= contextLimit) {
        return;
      }
      for (const [sessionId, context] of Array.from(contexts)) {
        if (contexts.size <= contextLimit) {
          break;
        }
        if (!evictable(context, protectedContext)) {
          continue;
        }
        contexts.delete(sessionId);
        disposeContext(context);
      }
    }

    function normalizedSessionId(sessionId) {
      return typeof sessionId === "string" ? sessionId.trim() : "";
    }

    function get(sessionId) {
      return contexts.get(normalizedSessionId(sessionId)) || null;
    }

    function getOrCreate(sessionId) {
      const normalized = normalizedSessionId(sessionId);
      if (!normalized) {
        return null;
      }

      let context = contexts.get(normalized);
      if (!context) {
        context = createContext(normalized);
        contexts.set(normalized, context);
        trim(context);
      }
      return context;
    }

    function activate(sessionId) {
      const normalized = normalizedSessionId(sessionId);
      if (!normalized) {
        throw new TypeError("terminal session id is required");
      }

      const context = getOrCreate(normalized);
      touch(normalized, context);

      const previousContext = activeContext;
      activeContext = context;
      activateContext(context, previousContext);
      trim(context);
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
      getOrCreate,
      prune,
      remove,
      trim,
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
