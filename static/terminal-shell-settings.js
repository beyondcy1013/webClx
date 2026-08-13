// webClx terminal shell, settings, theme, and xterm instance helpers.
// Extracted from terminal.js as global declarations.
// Top-level mountTerminalInstance() remains in terminal.js.

function focusTextInputToEnd(input) {
  if (!input) {
    return;
  }

  input.focus();
  if (typeof input.setSelectionRange === "function") {
    const cursor = input.value.length;
    input.setSelectionRange(cursor, cursor);
  }
}

function sessionRenameDraftName(sessionName) {
  const base = String(sessionName || "").trim();
  return `${base}_`;
}

function sessionRenameSavedName(sessionName) {
  return String(sessionName || "").trim().replace(/_+$/, "");
}

function applyTerminalTouchSelectionLongPress(value = state.terminalTouchSelectionLongPressMs) {
  state.terminalTouchSelectionLongPressMs = normalizeTerminalTouchSelectionLongPressMs(value);
  return state.terminalTouchSelectionLongPressMs;
}

function applyTerminalScrollbackLines(value = state.terminalScrollbackLines) {
  const normalized = normalizeTerminalScrollbackLines(value);
  state.terminalScrollbackLines = normalized;
  forEachTerminalSessionContext((context) => {
    context.term.options.scrollback = normalized;
  });
  return normalized;
}

function applyTerminalSoftKeyboardScale(value = state.terminalSoftKeyboardScale) {
  const normalized = normalizeTerminalSoftKeyboardScale(value);
  state.terminalSoftKeyboardScale = normalized;
  const rootStyle = document.documentElement.style;
  const setPx = (name, base) => {
    rootStyle.setProperty(name, `${Math.round(base * normalized * 10) / 10}px`);
  };
  rootStyle.setProperty("--terminal-soft-keyboard-scale", String(normalized));
  rootStyle.setProperty(
    "--terminal-key-font-size",
    `${Math.round((state.fontSizeTiers?.[2] || DEFAULT_FONT_SIZE_TIER_3) * normalized * 100) / 100}rem`,
  );
  setPx("--terminal-soft-keyboard-gap", 4);
  setPx("--terminal-soft-keyboard-row-gap", 2);
  setPx("--terminal-soft-keyboard-padding-y", 1);
  setPx("--terminal-soft-keyboard-padding-x", 3);
  setPx("--terminal-soft-keyboard-radius", 10);
  setPx("--terminal-key-min-width", 43);
  setPx("--terminal-key-padding-y", 6);
  setPx("--terminal-key-padding-x", 8);
  setPx("--terminal-key-radius", 8);
  setPx("--terminal-key-select-min-width", 48);
  setPx("--terminal-key-select-max-width", 64);
  setPx("--terminal-key-select-padding-left", 7);
  setPx("--terminal-key-select-padding-right", 18);
  setPx("--terminal-key-select-caret-x1", 11);
  setPx("--terminal-key-select-caret-x2", 7);
  setPx("--terminal-key-select-caret-size", 4);
  setPx("--terminal-slash-command-width", 52);
  setPx("--terminal-number-select-width", 48);
  syncTerminalHostHeight();
  syncScrollTopButtonOffset();
  updateScrollTopButton();
  updateTerminalScrollBottomButton();
  return normalized;
}

function applyTerminalFloatingButtonOffset(value = state.terminalFloatingButtonOffsetVh) {
  const normalized = normalizeTerminalFloatingButtonOffsetVh(value);
  state.terminalFloatingButtonOffsetVh = normalized;
  document.documentElement.style.setProperty("--terminal-floating-bottom-offset", `${normalized}vh`);
  syncTerminalStickyOffsets();
  syncScrollTopButtonOffset();
  updateScrollTopButton();
  updateTerminalScrollBottomButton();
  return normalized;
}

function applyTerminalFabAppearance(
  color = state.terminalFabActionColor,
  opacity = state.terminalFabActionOpacity,
) {
  const normalizedColor = normalizeTerminalFabActionColor(color);
  const normalizedOpacity = normalizeTerminalFabActionOpacity(opacity);
  state.terminalFabActionColor = normalizedColor;
  state.terminalFabActionOpacity = normalizedOpacity;
  document.documentElement.style.setProperty("--terminal-fab-action-color", normalizedColor);
  document.documentElement.style.setProperty("--terminal-fab-action-opacity", String(normalizedOpacity));
  return { color: normalizedColor, opacity: normalizedOpacity };
}

function readCssCustomProperty(name, fallback = "") {
  const value = window.getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value || fallback;
}

function persistThemeMode(themeMode) {
  try {
    window.localStorage.setItem(
      globalThis.WebClxTerminalSettings.THEME_MODE_STORAGE_KEY,
      globalThis.WebClxTerminalSettings.normalizeThemeMode(themeMode),
    );
  } catch {
    // Ignore storage write failures; runtime theme still updates.
  }
}

function applyThemeMode(themeMode = state.themeMode, { persist = false } = {}) {
  const normalized = globalThis.WebClxTerminalSettings.normalizeThemeMode(themeMode);
  const effective = globalThis.WebClxTerminalSettings.resolveThemeMode(normalized);
  state.themeMode = normalized;
  document.documentElement.dataset.theme = effective;
  document.documentElement.style.colorScheme = effective;
  if (persist) {
    persistThemeMode(normalized);
  }
  return normalized;
}

function hasPrimaryTouchInput() {
  return (
    window.matchMedia?.("(hover: none) and (pointer: coarse)")?.matches ||
    window.matchMedia?.("(pointer: coarse)")?.matches
  );
}

function isNarrowTouchViewport() {
  const width = Math.min(window.innerWidth || 0, window.screen?.width || window.innerWidth || 0);
  return hasPrimaryTouchInput() && width > 0 && width <= 900;
}

function terminalSoftKeyboardAutoVisible() {
  return state.desktopTerminalSoftKeyboardEnabled || isNarrowTouchViewport();
}

function terminalSoftKeyboardVisible() {
  return terminalSoftKeyboardAutoVisible() || state.temporaryDesktopTerminalSoftKeyboardVisible;
}

function syncTerminalSoftKeyboardToggleButton() {
  if (!terminalSoftKeyboardToggleButton) {
    return;
  }

  const autoVisible = terminalSoftKeyboardAutoVisible();
  terminalSoftKeyboardToggleButton.hidden = false;
  const expanded = state.temporaryDesktopTerminalSoftKeyboardVisible;
  const kbLabel = terminalSoftKeyboardToggleButton.querySelector(".terminal-fab-item-label");
  if (kbLabel) {
    kbLabel.textContent = expanded ? "收起" : "键盘";
  } else {
    terminalSoftKeyboardToggleButton.textContent = expanded ? "收起" : "键盘";
  }
  terminalSoftKeyboardToggleButton.setAttribute("aria-label", expanded ? "收起软键盘" : "显示软键盘");
  terminalSoftKeyboardToggleButton.setAttribute("aria-pressed", expanded ? "true" : "false");
  terminalSoftKeyboardToggleButton.title = expanded ? "收起软键盘" : "显示软键盘";
  terminalSoftKeyboardToggleButton.classList.toggle("is-active", expanded);
  terminalSoftKeyboardToggleButton.classList.toggle("is-auto", autoVisible);
}

function syncTerminalInputHistoryButton() {
  if (!terminalInputHistoryButton) {
    return;
  }

  const disabled = !state.activeSessionId;
  const changed = terminalInputHistoryButton.hidden || terminalInputHistoryButton.disabled !== disabled;
  terminalInputHistoryButton.hidden = false;
  terminalInputHistoryButton.disabled = disabled;
  terminalInputHistoryButton.setAttribute("aria-disabled", disabled ? "true" : "false");
  terminalInputHistoryButton.title = disabled ? "当前没有活动终端" : "对话史";
  if (changed) {
    syncScrollTopButtonOffset();
  }
}

function syncTerminalSoftKeyboardVisibility() {
  const visible = terminalSoftKeyboardVisible();
  if (document.body) {
    document.body.dataset.terminalSoftKeyboard = visible ? "open" : "closed";
  }
  syncTerminalSoftKeyboardToggleButton();
  syncTerminalSoftKeyboardCursor();
  return visible;
}

function applyDesktopTerminalSoftKeyboardSetting(enabled = state.desktopTerminalSoftKeyboardEnabled) {
  const normalized = Boolean(enabled);
  state.desktopTerminalSoftKeyboardEnabled = normalized;
  if (document.body) {
    document.body.dataset.desktopTerminalSoftKeyboard = normalized ? "enabled" : "disabled";
  }
  syncTerminalSoftKeyboardVisibility();
  return normalized;
}

function applyTypographySettings(fontSizeTiers = state.fontSizeTiers) {
  const tiers = normalizeFontSizeTiers(fontSizeTiers);
  state.fontSizeTiers = tiers;
  const rootStyle = document.documentElement.style;
  rootStyle.setProperty("--font-size-tier-1", `${tiers[0]}rem`);
  rootStyle.setProperty("--font-size-tier-2", `${tiers[1]}rem`);
  rootStyle.setProperty("--font-size-tier-3", `${tiers[2]}rem`);
  rootStyle.setProperty("--font-size-tier-4", `${tiers[3]}rem`);
  rootStyle.setProperty(
    "--terminal-key-font-size",
    `${Math.round(tiers[2] * state.terminalSoftKeyboardScale * 100) / 100}rem`,
  );
  return tiers;
}

function fontTierPx(value) {
  return Math.max(10, Math.round(normalizeFontSizeTier(value, DEFAULT_FONT_SIZE_TIER_4) * 16));
}

function enableCanvasReadbackOptimization() {
  const prototype = window.HTMLCanvasElement?.prototype;
  if (!prototype || typeof prototype.getContext !== "function") {
    return;
  }

  if (prototype[CANVAS_READBACK_PATCH_FLAG]) {
    return;
  }

  const originalGetContext = prototype.getContext;
  prototype.getContext = function patchedGetContext(contextType, options, ...rest) {
    let nextOptions = options;
    if (contextType === "2d") {
      if (options && typeof options === "object" && !Array.isArray(options)) {
        nextOptions = {
          ...options,
          willReadFrequently: true,
        };
      } else if (options == null) {
        nextOptions = {
          willReadFrequently: true,
        };
      }
    }

    return originalGetContext.call(this, contextType, nextOptions, ...rest);
  };

  Object.defineProperty(prototype, CANVAS_READBACK_PATCH_FLAG, {
    value: true,
    configurable: true,
  });
}

function getDirectoryListingHref(path = state.currentPath) {
  return path ? `/?path=${encodeURIComponent(path)}` : "/";
}

function syncTerminalNavScroll({ forceEnd = false } = {}) {
  if (!terminalNavScrollEl) {
    return;
  }

  window.requestAnimationFrame(() => {
    const maxScrollLeft = Math.max(terminalNavScrollEl.scrollWidth - terminalNavScrollEl.clientWidth, 0);
    if (forceEnd) {
      terminalNavScrollEl.scrollLeft = maxScrollLeft;
    }
    terminalNavScrollEl.classList.toggle("is-scrollable", maxScrollLeft > 0);
  });
}

function setTerminalPathExpanded(expanded, { forceEnd = false } = {}) {
  if (!terminalNavScrollEl || !terminalNavToggleButton) {
    return;
  }

  terminalNavScrollEl.hidden = !expanded;
  terminalNavToggleButton.setAttribute("aria-expanded", expanded ? "true" : "false");
  terminalNavToggleButton.classList.toggle("is-active", expanded);

  try {
    window.localStorage.setItem(TERMINAL_PATH_EXPANDED_STORAGE_KEY, expanded ? "1" : "0");
  } catch {
    // Ignore storage failures and keep the in-memory toggle working.
  }

  if (expanded) {
    syncTerminalNavScroll({ forceEnd });
  }
}

function restoreTerminalPathExpanded() {
  let expanded = false;
  try {
    expanded = window.localStorage.getItem(TERMINAL_PATH_EXPANDED_STORAGE_KEY) === "1";
  } catch {
    expanded = false;
  }
  setTerminalPathExpanded(expanded, { forceEnd: expanded });
}

function terminalThemeFromCss() {
  return {
    background: readCssCustomProperty("--terminal-bg", "#0b1110"),
    foreground: readCssCustomProperty("--terminal-fg", "#d5e2da"),
    cursor: readCssCustomProperty("--terminal-cursor", "#90f0cf"),
    selection: readCssCustomProperty(
      "--terminal-selection-bg",
      "rgba(99, 179, 237, 0.55)"
    ),
    black: readCssCustomProperty("--terminal-ansi-black", "#7c8b85"),
    brightBlack: readCssCustomProperty("--terminal-ansi-bright-black", "#a4b6af"),
  };
}

function terminalThemeForCursorState(cursorHidden = false) {
  const theme = terminalThemeFromCss();
  if (cursorHidden) {
    theme.cursor = "rgba(0, 0, 0, 0)";
  }
  return theme;
}

let term = null;
let fitAddon = null;
let terminalInstanceEventDisposables = [];
let activeTerminalContext = null;
let terminalSessionCache = null;
const TERMINAL_EMPTY_CONTEXT_ID = "__webclx_empty_terminal__";

function terminalRendererType(
  userAgent = navigator.userAgent,
  androidBridge = globalThis.WebClxAndroid,
) {
  const value = String(userAgent || "");
  const explicitAndroidClient =
    /(?:^|\s)webClxAndroid\//.test(value) || androidBridge !== null && androidBridge !== undefined;
  const legacyAndroidWebView =
    /\bAndroid\b/i.test(value) &&
    (/(?:^|[;\s])wv(?:[)\s;]|$)/i.test(value) ||
      /\bVersion\/4\.0\b.*\bChrome\/.*\bMobile Safari\//i.test(value));
  return explicitAndroidClient || legacyAndroidWebView ? "dom" : "canvas";
}

function createTerminalInstance() {
  return new Terminal({
    cursorBlink: true,
    cursorStyle: "bar",
    cursorWidth: 2,
    fontFamily: readCssCustomProperty(
      "--font-family-mono",
      '"IBM Plex Mono", "SFMono-Regular", Consolas, monospace'
    ),
    fontSize: fontTierPx(state.fontSizeTiers?.[3] || DEFAULT_FONT_SIZE_TIER_4),
    lineHeight: 1.3,
    rendererType: terminalRendererType(),
    allowTransparency: true,
    altClickMovesCursor: false,
    minimumContrastRatio: 4.5,
    scrollback: normalizeTerminalScrollbackLines(state.terminalScrollbackLines),
    theme: terminalThemeForCursorState(false),
  });
}

function disposeTerminalInstanceEventHandlers(context = activeTerminalContext) {
  const disposables = context?.eventDisposables || terminalInstanceEventDisposables;
  disposables.forEach((disposable) => {
    try {
      disposable?.dispose?.();
    } catch {
      // Ignore stale xterm listener disposal failures during context cleanup.
    }
  });
  if (context) {
    context.eventDisposables = [];
  }
  if (context === activeTerminalContext) {
    terminalInstanceEventDisposables = [];
  }
}

function mountTerminalContextInstance(context) {
  context.term = createTerminalInstance();
  context.fitAddon = new FitAddon.FitAddon();
  context.eventDisposables = [];
  context.term.loadAddon(context.fitAddon);
  context.term.open(terminalHost);
  if (context.term.element) {
    context.term.element.hidden = true;
    context.term.element.dataset.terminalSessionId = context.sessionId;
  }
  registerTerminalInstanceEventHandlers(context);
}

function createTerminalSessionContext(sessionId) {
  const context = {
    sessionId,
    term: null,
    fitAddon: null,
    retainedTerm: null,
    retainedFitAddon: null,
    eventDisposables: [],
    socket: null,
    connectionToken: 0,
    reconnectTimer: null,
    outputQueue: [],
    outputWriteInFlight: false,
    outputDrainTimer: null,
    outputWriteId: 0,
    synchronizedOutputTransformer: null,
    renderRefreshFrame: null,
    initialReplayPending: false,
    backlogReplayActive: false,
    backlogReplayEndQueued: false,
    backlogReplayInterrupted: false,
    followOutput: true,
    hasLoadedOutput: false,
    disconnected: false,
    disposed: false,
    path: "",
    terminalOutputTail: "",
    terminalInputTail: "",
    pendingDeviceAttributeResponses: 0,
    lastTerminalSize: null,
  };
  mountTerminalContextInstance(context);
  return context;
}

function captureTerminalContextAliases(context = activeTerminalContext) {
  if (!context || context !== activeTerminalContext) {
    return;
  }
  context.socket = socket;
  context.connectionToken = connectionToken;
  context.reconnectTimer = reconnectTimer;
  context.initialReplayPending = terminalInitialReplayPending;
  context.backlogReplayActive = terminalBacklogReplayActive;
  context.backlogReplayEndQueued = terminalBacklogReplayEndQueued;
  context.backlogReplayInterrupted = terminalBacklogReplayInterrupted;
  context.outputQueue = terminalOutputQueue;
  context.outputWriteInFlight = terminalOutputWriteInFlight;
  context.outputWriteId = terminalOutputWriteId;
  context.terminalOutputTail = terminalOutputTail;
  context.terminalInputTail = terminalInputTail;
  context.pendingDeviceAttributeResponses = pendingDeviceAttributeResponses;
}

function syncActiveTerminalContextAliases(context = activeTerminalContext) {
  if (!context || context !== activeTerminalContext) {
    return;
  }
  term = context.term;
  fitAddon = context.fitAddon;
  terminalInstanceEventDisposables = context.eventDisposables;
  socket = context.socket;
  connectionToken = context.connectionToken;
  reconnectTimer = context.reconnectTimer;
  terminalInitialReplayPending = context.initialReplayPending;
  terminalBacklogReplayActive = context.backlogReplayActive;
  terminalBacklogReplayEndQueued = context.backlogReplayEndQueued;
  terminalBacklogReplayInterrupted = context.backlogReplayInterrupted;
  terminalOutputQueue = context.outputQueue;
  terminalOutputWriteInFlight = context.outputWriteInFlight;
  terminalOutputWriteId = context.outputWriteId;
  terminalOutputTail = context.terminalOutputTail;
  terminalInputTail = context.terminalInputTail;
  pendingDeviceAttributeResponses = context.pendingDeviceAttributeResponses;
}

function restoreCachedTerminalViewport(context) {
  if (!context || context !== activeTerminalContext || context.sessionId === TERMINAL_EMPTY_CONTEXT_ID) {
    return;
  }
  suppressTerminalScrollSaveUntilNextFrame();
  restoreTerminalScrollPositionForSession(context.sessionId, { defaultToBottom: true });
  if (context.followOutput) {
    scrollTerminalToBottom();
  }
  const refresh = () => {
    if (context === activeTerminalContext && !context.disposed) {
      context.term.refresh?.(0, Math.max(context.term.rows - 1, 0));
    }
  };
  refresh();
  window.requestAnimationFrame(refresh);
  updateTerminalScrollBottomButton();
}

function setTerminalContextRenderVisibility(context, visible) {
  if (!context) {
    return;
  }
  if (context.retainedTerm?.element) {
    context.retainedTerm.element.hidden = !visible;
  }
  if (context.term?.element) {
    context.term.element.hidden = Boolean(context.retainedTerm) || !visible;
  }
}

function completeTerminalContextReconnect(context) {
  if (!context?.retainedTerm) {
    return false;
  }

  context.retainedTerm.dispose?.();
  context.retainedTerm = null;
  context.retainedFitAddon = null;
  setTerminalContextRenderVisibility(context, context === activeTerminalContext);
  if (context === activeTerminalContext) {
    syncActiveTerminalContextAliases(context);
    terminalViewportScrollEl = null;
    bindTerminalViewportScroll();
    context.term.refresh?.(0, Math.max(context.term.rows - 1, 0));
  }
  return true;
}

function activateTerminalContext(context, previousContext) {
  if (previousContext && previousContext !== context) {
    captureTerminalContextAliases(previousContext);
    sendTerminalContextVisibility(previousContext, false);
    setTerminalContextRenderVisibility(previousContext, false);
  }

  activeTerminalContext = context;
  pendingTerminalSize = null;
  setTerminalContextRenderVisibility(context, true);
  context.term.options.theme = terminalThemeForCursorState(false);
  syncActiveTerminalContextAliases(context);
  sendTerminalContextVisibility(context, true);
  terminalHost?.classList.toggle(
    "terminal-host-replaying",
    Boolean(context.backlogReplayActive && !context.retainedTerm),
  );
  terminalViewportScrollEl = null;
  bindTerminalViewportScroll();
}

function disposeTerminalContext(context) {
  context.disposed = true;
  context.connectionToken += 1;
  if (context.reconnectTimer !== null) {
    window.clearTimeout(context.reconnectTimer);
    context.reconnectTimer = null;
  }
  const currentSocket = context.socket;
  context.socket = null;
  currentSocket?.close?.();
  if (context.outputDrainTimer !== null) {
    window.clearTimeout(context.outputDrainTimer);
    context.outputDrainTimer = null;
  }
  if (context.renderRefreshFrame !== null) {
    window.cancelAnimationFrame(context.renderRefreshFrame);
    context.renderRefreshFrame = null;
  }
  context.outputQueue = [];
  context.synchronizedOutputTransformer?.reset?.();
  context.synchronizedOutputTransformer = null;
  context.outputWriteId += 1;
  context.outputWriteInFlight = false;
  disposeTerminalInstanceEventHandlers(context);
  context.term?.dispose?.();
  context.retainedTerm?.dispose?.();
  context.retainedTerm = null;
  context.retainedFitAddon = null;
  if (activeTerminalContext === context) {
    activeTerminalContext = null;
  }
}

function ensureTerminalSessionCache() {
  if (terminalSessionCache) {
    return terminalSessionCache;
  }
  const createCache = globalThis.WebClxTerminalSessionCache?.createTerminalSessionCache;
  if (typeof createCache !== "function") {
    throw new Error("terminal session cache is unavailable");
  }
  terminalSessionCache = createCache({
    createContext: createTerminalSessionContext,
    activateContext: activateTerminalContext,
    disposeContext: disposeTerminalContext,
  });
  return terminalSessionCache;
}

function activateTerminalSessionContext(sessionId = state.activeSessionId) {
  const normalizedSessionId = String(sessionId || TERMINAL_EMPTY_CONTEXT_ID).trim();
  const cache = ensureTerminalSessionCache();
  const context = cache.activate(normalizedSessionId);
  if (normalizedSessionId !== TERMINAL_EMPTY_CONTEXT_ID) {
    const session = state.sessions.find((item) => item.id === normalizedSessionId);
    context.path = session ? sessionPath(session) : state.currentPath;
    const emptyContext = cache.get(TERMINAL_EMPTY_CONTEXT_ID);
    if (emptyContext && emptyContext !== context) {
      cache.remove(TERMINAL_EMPTY_CONTEXT_ID);
    }
  }
  restoreCachedTerminalViewport(context);
  return context;
}

function disposeTerminalSessionContext(sessionId) {
  const cache = ensureTerminalSessionCache();
  const context = cache.get(sessionId);
  if (!context) {
    return false;
  }
  if (context === activeTerminalContext) {
    cache.activate(TERMINAL_EMPTY_CONTEXT_ID);
  }
  return cache.remove(sessionId);
}

function pruneTerminalSessionContexts(sessions = state.sessions) {
  const allowed = new Set(
    (sessions || []).filter((session) => session?.id && !session.idle).map((session) => session.id),
  );
  const staleSessionIds = [];
  ensureTerminalSessionCache().forEach((context, sessionId) => {
    if (sessionId !== TERMINAL_EMPTY_CONTEXT_ID && !allowed.has(sessionId)) {
      staleSessionIds.push(sessionId);
    }
  });
  staleSessionIds.forEach(disposeTerminalSessionContext);
}

function disposeAllTerminalSessionContexts() {
  ensureTerminalSessionCache().clear();
  activeTerminalContext = null;
}

function forEachTerminalSessionContext(callback) {
  if (!terminalSessionCache) {
    if (term) {
      callback({ term, sessionId: state.activeSessionId || TERMINAL_EMPTY_CONTEXT_ID });
    }
    return;
  }
  terminalSessionCache.forEach(callback);
}

function resetTerminalContextInstance(context) {
  disposeTerminalInstanceEventHandlers(context);
  if (context.retainedTerm) {
    context.term?.dispose?.();
  } else {
    context.retainedTerm = context.term;
    context.retainedFitAddon = context.fitAddon;
  }
  context.term = null;
  context.fitAddon = null;
  if (context.outputDrainTimer !== null) {
    window.clearTimeout(context.outputDrainTimer);
    context.outputDrainTimer = null;
  }
  if (context.renderRefreshFrame !== null) {
    window.cancelAnimationFrame(context.renderRefreshFrame);
    context.renderRefreshFrame = null;
  }
  context.outputQueue = [];
  context.synchronizedOutputTransformer?.reset?.();
  context.synchronizedOutputTransformer = null;
  context.outputWriteId += 1;
  context.outputWriteInFlight = false;
  context.initialReplayPending = false;
  context.backlogReplayActive = false;
  context.backlogReplayEndQueued = false;
  context.backlogReplayInterrupted = false;
  context.hasLoadedOutput = false;
  mountTerminalContextInstance(context);
  setTerminalContextRenderVisibility(context, context === activeTerminalContext);
  if (context === activeTerminalContext) {
    syncActiveTerminalContextAliases(context);
  }
}

function mountTerminalInstance(sessionId = state.activeSessionId || TERMINAL_EMPTY_CONTEXT_ID) {
  return activateTerminalSessionContext(sessionId);
}

function replaceTerminalInstance(sessionId = state.activeSessionId, { forceNew = false } = {}) {
  const normalizedSessionId = String(sessionId || TERMINAL_EMPTY_CONTEXT_ID).trim();
  const cache = ensureTerminalSessionCache();
  if (forceNew && cache.get(normalizedSessionId)) {
    disposeTerminalSessionContext(normalizedSessionId);
  }
  const context = activateTerminalSessionContext(normalizedSessionId);
  scheduleTerminalSizeSettle();
  syncTerminalOverlayBounds();
  updateTerminalScrollBottomButton();
  hideTerminalCursorCorrection();
  return context;
}
