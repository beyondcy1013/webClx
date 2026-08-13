// webClx terminal navigation, viewport geometry, and terminal-scroll layout helpers.
// Extracted from terminal.js as global declarations; no top-level DOM setup.

function toggleTerminalSoftKeyboard() {
  if (terminalSoftKeyboardAutoVisible()) {
    return;
  }

  state.temporaryDesktopTerminalSoftKeyboardVisible =
    !state.temporaryDesktopTerminalSoftKeyboardVisible;
  syncTerminalSoftKeyboardVisibility();
  fitTerminal({ force: true });
  scheduleTerminalSizeSettle();
  syncTerminalStickyOffsets();
  syncTerminalNavScroll({ forceEnd: true });
  syncScrollTopButtonOffset();
  updateScrollTopButton();
  updateTerminalScrollBottomButton();
  updatePageScrollRail();
  updateStatus(
    state.temporaryDesktopTerminalSoftKeyboardVisible ? "已显示软键盘。" : "已收起软键盘。",
    "muted",
  );
}

function buildTerminalUrl(sessionId, pathValue = state.currentPath) {
  const nextParams = new URLSearchParams();
  const normalizedPath = normalizeTerminalPath(pathValue);
  if (normalizedPath) {
    nextParams.set("path", normalizedPath);
  }
  if (sessionId) {
    nextParams.set("session", sessionId);
  }
  const query = nextParams.toString();
  return query ? `/terminal?${query}` : "/terminal";
}

function buildWorkspaceUrl(homePath = "/workspace") {
  const nextParams = new URLSearchParams();
  const normalizedPath = normalizeTerminalPath(state.currentPath);
  if (normalizedPath) {
    nextParams.set("path", normalizedPath);
  }
  if (state.activeSessionId) {
    nextParams.set("terminal_session", state.activeSessionId);
  }
  const query = nextParams.toString();
  return `${homePath || "/"}${query ? `?${query}` : ""}`;
}

function currentLocationSessionId() {
  return readLocationState().sessionId;
}

function shouldCreateInitialTerminalSession() {
  if (initialLocation.sessionId) {
    return false;
  }
  return Boolean(initialLocation.fresh || initialLocation.runCommand);
}

function syncTopNavigation() {
  topNavLinks.forEach((link) => {
    link.href = buildWorkspaceUrl(link.dataset.homePath || "/workspace");
  });
}

function updateNavigationButtons() {
  if (navigateBackButton) {
    navigateBackButton.disabled = window.history.length <= 1 && state.historyIndex <= 0;
  }
  if (navigateForwardButton) {
    navigateForwardButton.disabled = state.historyIndex >= state.historyMaxIndex;
  }
}

function syncTerminalStickyOffsets() {
  const height = terminalPageNavEl?.getBoundingClientRect().height || 0;
  document.documentElement.style.setProperty("--terminal-page-nav-height", `${Math.ceil(height)}px`);
}

function readCssPixels(value) {
  const parsed = Number.parseFloat(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

function currentViewportBounds() {
  const visualViewport = window.visualViewport;
  const visualViewportHeight = visualViewport?.height;
  if (Number.isFinite(visualViewportHeight) && visualViewportHeight > 0) {
    const visualViewportTop = Number.isFinite(visualViewport.offsetTop) ? Math.max(visualViewport.offsetTop, 0) : 0;
    const top = Math.round(visualViewportTop);
    const height = Math.round(visualViewportHeight);
    return {
      top,
      bottom: top + height,
      height,
    };
  }

  return {
    top: 0,
    bottom: window.innerHeight,
    height: window.innerHeight,
  };
}

function visibleViewportOverlap(rect, viewportBounds = currentViewportBounds()) {
  if (!rect || viewportBounds.height <= 0) {
    return 0;
  }

  const top = Math.max(rect.top, viewportBounds.top);
  const bottom = Math.min(rect.bottom, viewportBounds.bottom);
  return Math.max(bottom - top, 0);
}

function currentMobileKeyboardReserve(viewportBounds = currentViewportBounds()) {
  if (!mobileKeysEl) {
    return 0;
  }

  const styles = window.getComputedStyle(mobileKeysEl);
  if (styles.display === "none" || styles.visibility === "hidden") {
    return 0;
  }

  const rect = mobileKeysEl.getBoundingClientRect();
  if (rect.height <= 0) {
    return 0;
  }

  if (styles.position === "fixed") {
    return visibleViewportOverlap(rect, viewportBounds);
  }

  return rect.height + readCssPixels(styles.marginTop);
}

function syncTerminalFloatingButtonRight() {
  document.documentElement.style.setProperty(
    "--terminal-floating-right-offset",
    "0px",
  );
}

function syncTerminalHostHeight() {
  if (!terminalHost) {
    return;
  }

  const rect = terminalHost.getBoundingClientRect();
  const viewportBounds = currentViewportBounds();
  const hostTop = Math.max(rect.top, viewportBounds.top);
  const pageStyles = terminalPageEl ? window.getComputedStyle(terminalPageEl) : null;
  const panelStyles = terminalPanelEl ? window.getComputedStyle(terminalPanelEl) : null;
  const mobileKeyboardStyles = mobileKeysEl ? window.getComputedStyle(mobileKeysEl) : null;
  const mobileKeyboardReserve = currentMobileKeyboardReserve(viewportBounds);
  const mobileKeyboardFixed =
    mobileKeyboardStyles &&
    mobileKeyboardStyles.display !== "none" &&
    mobileKeyboardStyles.visibility !== "hidden" &&
    mobileKeyboardStyles.position === "fixed" &&
    mobileKeyboardReserve > 0;

  let reserve = mobileKeyboardReserve;
  if (mobileKeyboardFixed) {
    reserve += 2;
  } else {
    reserve += 8;
    if (pageStyles) {
      reserve += readCssPixels(pageStyles.marginBottom);
    }
    if (panelStyles) {
      reserve += readCssPixels(panelStyles.paddingBottom);
    }
  }

  document.documentElement.style.setProperty(
    "--terminal-mobile-keys-height",
    `${Math.ceil(mobileKeyboardReserve)}px`,
  );

  const nextHeight = Math.max(Math.round(viewportBounds.bottom - hostTop - reserve), 0);
  const nextValue = `${nextHeight}px`;
  if (nextValue === lastTerminalHostHeight) {
    return;
  }

  document.documentElement.style.setProperty("--terminal-host-height", nextValue);
  lastTerminalHostHeight = nextValue;
}

function syncScrollTopButtonOffset() {
  syncTerminalFloatingButtonRight();

  const viewportBounds = currentViewportBounds();
  document.documentElement.style.setProperty(
    "--terminal-visible-viewport-top",
    `${Math.ceil(viewportBounds.top)}px`,
  );
  document.documentElement.style.setProperty(
    "--terminal-visible-viewport-height",
    `${Math.ceil(viewportBounds.height)}px`,
  );
  const terminalOutputTop = Math.max(
    terminalHost?.getBoundingClientRect().top ?? viewportBounds.top,
    viewportBounds.top,
  );
  document.documentElement.style.setProperty(
    "--terminal-output-visible-top",
    `${Math.ceil(terminalOutputTop)}px`,
  );

  let offset = 0;
  if (mobileKeysEl) {
    const styles = window.getComputedStyle(mobileKeysEl);
    if (styles.display !== "none" && styles.visibility !== "hidden") {
      const rect = mobileKeysEl.getBoundingClientRect();
      if (rect.height > 0 && rect.bottom > viewportBounds.bottom - 24) {
        offset = Math.max(viewportBounds.bottom - Math.max(rect.top, viewportBounds.top) + 12, 0);
      }
    }
  }
  document.documentElement.style.setProperty("--terminal-scroll-top-offset", `${Math.ceil(offset)}px`);

  // FAB mode: no per-button stacking needed, all buttons live inside the FAB menu.
  // Set all stack-height vars to 0 so legacy CSS references resolve cleanly.
  document.documentElement.style.setProperty("--terminal-floating-toggle-stack-height", "0px");
  document.documentElement.style.setProperty("--terminal-input-history-stack-height", "0px");
  document.documentElement.style.setProperty("--terminal-scroll-bottom-stack-height", "0px");
  document.documentElement.style.setProperty("--terminal-scroll-terminal-top-stack-height", "0px");
  document.documentElement.style.setProperty("--terminal-scroll-terminal-stack-height", "0px");
}

function updateScrollTopButton() {
  if (!scrollPageTopButton) {
    return;
  }

  const scrollTop = window.scrollY || document.documentElement.scrollTop || 0;
  scrollPageTopButton.hidden = scrollTop < PAGE_SCROLL_TOP_THRESHOLD;
}

function scrollPageToTop() {
  const prefersReducedMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches;
  window.scrollTo({
    top: 0,
    behavior: prefersReducedMotion ? "auto" : "smooth",
  });
}

function terminalViewportElement() {
  const viewport = term?.element?.querySelector(".xterm-viewport");
  return viewport instanceof HTMLElement ? viewport : null;
}

function terminalScrollMetrics() {
  const viewport = terminalViewportElement();
  if (!viewport) {
    return null;
  }

  const rowHeight = typeof terminalViewportRowHeight === "function"
    ? terminalViewportRowHeight()
    : 0;
  const buffer = term?.buffer?.active;
  const logicalScrollTop = buffer && rowHeight
    ? buffer.viewportY * rowHeight
    : null;
  const logicalMaxScroll = buffer && rowHeight
    ? buffer.baseY * rowHeight
    : null;
  const maxScroll = Number.isFinite(logicalMaxScroll)
    ? Math.max(logicalMaxScroll, 0)
    : Math.max(viewport.scrollHeight - viewport.clientHeight, 0);
  const scrollTop = Number.isFinite(logicalScrollTop)
    ? clampNumber(logicalScrollTop, 0, maxScroll)
    : Math.max(viewport.scrollTop, 0);
  return {
    viewport,
    maxScroll,
    scrollTop,
    atTop: scrollTop <= 6,
    atBottom: maxScroll <= 0 || maxScroll - scrollTop <= 6,
  };
}

function updateTerminalScrollBottomButton() {
  if (!scrollTerminalBottomButton && !scrollTerminalTopButton) {
    return;
  }

  const metrics = terminalScrollMetrics();
  const hasScrollableContent = Boolean(metrics && metrics.maxScroll > 0);
  let changed = false;

  if (scrollTerminalBottomButton) {
    changed = changed || scrollTerminalBottomButton.hidden || scrollTerminalBottomButton.disabled === hasScrollableContent;
    scrollTerminalBottomButton.hidden = false;
    scrollTerminalBottomButton.disabled = !hasScrollableContent;
    scrollTerminalBottomButton.setAttribute("aria-disabled", hasScrollableContent ? "false" : "true");
    scrollTerminalBottomButton.title = hasScrollableContent ? "跳底部" : "终端内容暂时无需滚动";
  }
  if (scrollTerminalTopButton) {
    changed = changed || scrollTerminalTopButton.hidden || scrollTerminalTopButton.disabled === hasScrollableContent;
    scrollTerminalTopButton.hidden = false;
    scrollTerminalTopButton.disabled = !hasScrollableContent;
    scrollTerminalTopButton.setAttribute("aria-disabled", hasScrollableContent ? "false" : "true");
    scrollTerminalTopButton.title = hasScrollableContent ? "跳顶部" : "终端内容暂时无需滚动";
  }

  if (changed) {
    syncScrollTopButtonOffset();
  }
}

function handleTerminalViewportScroll() {
  if (!terminalBacklogReplayActive && !terminalScrollSaveSuppressed()) {
    saveTerminalScrollPositionForSession(state.activeSessionId);
  }
  updateTerminalScrollBottomButton();
  syncTerminalSelectionHandles();
}

function saveTerminalScrollPositionForSession(sessionId) {
  if (!sessionId) {
    return;
  }

  const metrics = terminalScrollMetrics();
  if (!metrics) {
    return;
  }

  terminalScrollPositions.set(sessionId, {
    scrollTop: metrics.scrollTop,
    maxScroll: metrics.maxScroll,
    atBottom: metrics.atBottom,
  });
  const context = terminalSessionCache?.get(sessionId);
  if (context) {
    context.followOutput = metrics.atBottom;
  }
}

function captureTerminalScrollSnapshotForSession(sessionId) {
  if (!sessionId || terminalBacklogReplayActive) {
    return null;
  }

  const metrics = terminalScrollMetrics();
  if (!metrics) {
    return null;
  }

  return {
    sessionId,
    scrollTop: metrics.scrollTop,
    atBottom: metrics.atBottom,
  };
}

function restoreTerminalScrollPositionForSession(sessionId, { defaultToBottom = false } = {}) {
  const metrics = terminalScrollMetrics();
  if (!metrics) {
    return;
  }

  const saved = sessionId ? terminalScrollPositions.get(sessionId) : null;
  if (!saved) {
    if (defaultToBottom) {
      scrollTerminalToBottom();
    } else {
      updateTerminalScrollBottomButton();
    }
    return;
  }

  if (saved.atBottom) {
    scrollTerminalToBottom();
    return;
  }

  scrollTerminalToDomScrollTop(saved.scrollTop, metrics.maxScroll);
  updateTerminalScrollBottomButton();
}

function suppressTerminalScrollSaveUntilNextFrame() {
  terminalScrollSaveSuppressionDepth += 1;
  const release = () => {
    terminalScrollSaveSuppressionDepth = Math.max(terminalScrollSaveSuppressionDepth - 1, 0);
  };
  if (typeof window.requestAnimationFrame === "function") {
    window.requestAnimationFrame(release);
  } else {
    window.setTimeout(release, 0);
  }
}

function terminalScrollSaveSuppressed() {
  return terminalScrollSaveSuppressionDepth > 0 || Date.now() < terminalScrollSaveSuppressedUntil;
}

function suppressTerminalScrollSaveForLayout(ms = TERMINAL_LAYOUT_SCROLL_SUPPRESSION_MS) {
  const until = Date.now() + Math.max(Number(ms) || 0, 0);
  terminalScrollSaveSuppressedUntil = Math.max(terminalScrollSaveSuppressedUntil, until);
  if (terminalScrollSaveSuppressionTimer !== null) {
    window.clearTimeout(terminalScrollSaveSuppressionTimer);
  }
  terminalScrollSaveSuppressionTimer = window.setTimeout(() => {
    terminalScrollSaveSuppressionTimer = null;
    if (Date.now() >= terminalScrollSaveSuppressedUntil) {
      terminalScrollSaveSuppressedUntil = 0;
    }
  }, Math.max(terminalScrollSaveSuppressedUntil - Date.now(), 0) + 16);
}

function restoreTerminalScrollSnapshot(snapshot) {
  if (!snapshot || snapshot.sessionId !== state.activeSessionId || terminalBacklogReplayActive) {
    return;
  }

  const metrics = terminalScrollMetrics();
  if (!metrics) {
    return;
  }

  suppressTerminalScrollSaveUntilNextFrame();
  if (snapshot.atBottom) {
    scrollTerminalToBottom();
  } else {
    scrollTerminalToDomScrollTop(snapshot.scrollTop, metrics.maxScroll);
    updateTerminalScrollBottomButton();
  }
  saveTerminalScrollPositionForSession(snapshot.sessionId);
}

function scheduleTerminalScrollSnapshotRestore(snapshot) {
  if (!snapshot) {
    return;
  }

  const token = ++terminalScrollLayoutRestoreToken;
  const restore = () => {
    if (token !== terminalScrollLayoutRestoreToken) {
      return;
    }
    restoreTerminalScrollSnapshot(snapshot);
  };

  if (typeof window.requestAnimationFrame === "function") {
    window.requestAnimationFrame(restore);
  } else {
    window.setTimeout(restore, 0);
  }
}

function preserveTerminalScrollDuringLayout(layoutCallback) {
  const snapshot = captureTerminalScrollSnapshotForSession(state.activeSessionId);
  suppressTerminalScrollSaveUntilNextFrame();
  suppressTerminalScrollSaveForLayout();
  try {
    layoutCallback();
  } finally {
    restoreTerminalScrollSnapshot(snapshot);
    scheduleTerminalScrollSnapshotRestore(snapshot);
  }
}

function scrollTerminalToBottom() {
  const metrics = terminalScrollMetrics();
  if (!metrics?.atBottom) {
    if (typeof term.scrollToBottom === "function") {
      term.scrollToBottom();
    } else if (metrics) {
      metrics.viewport.scrollTop = metrics.maxScroll;
    }
  }

  scheduleTerminalViewportDomSync();
  updateTerminalScrollBottomButton();
}

function refreshTerminalInputVisibilityAfterPaste() {
  const sessionId = state.activeSessionId;
  if (!sessionId) {
    return;
  }
  const startedAt = Date.now();
  const followUpDelays = [80, 180, 360, 720, 1200, 1800];

  const refresh = () => {
    if (sessionId !== state.activeSessionId) {
      return;
    }
    scheduleTerminalRenderRefresh();
    suppressTerminalScrollSaveUntilNextFrame();
    scrollTerminalToBottom();
    saveTerminalScrollPositionForSession(sessionId);
    syncTerminalCursorCorrection();
  };

  refresh();
  window.requestAnimationFrame(refresh);
  followUpDelays.forEach((delay) => {
    window.setTimeout(() => {
      if (Date.now() - startedAt <= delay + 250) {
        refresh();
      }
    }, delay);
  });
}

function refreshTerminalInputVisibilityAfterUserInput() {
  const sessionId = state.activeSessionId;
  if (!sessionId) {
    return;
  }

  const startedAt = Date.now();
  const followUpDelays = [60, 140, 280, 560];

  const refresh = () => {
    if (sessionId !== state.activeSessionId) {
      return;
    }
    scheduleTerminalRenderRefresh();
    suppressTerminalScrollSaveUntilNextFrame();
    scrollTerminalToBottom();
    saveTerminalScrollPositionForSession(sessionId);
    syncTerminalCursorCorrection();
  };

  refresh();
  window.requestAnimationFrame(refresh);
  followUpDelays.forEach((delay) => {
    window.setTimeout(() => {
      if (Date.now() - startedAt <= delay + 160) {
        refresh();
      }
    }, delay);
  });
}

// ===== FAB menu toggle =====
let terminalFabInitialized = false;
function initTerminalFab() {
  if (terminalFabInitialized) return;
  const fabToggle = document.getElementById("terminal-fab-toggle");
  const fabMenu = document.getElementById("terminal-fab-menu");
  const fabTopMenu = document.getElementById("terminal-fab-top-menu");
  const fab = document.getElementById("terminal-fab");
  if (!fabToggle || !fabMenu || !fabTopMenu || !fab) return;
  const fabMenus = [fabTopMenu, fabMenu];
  terminalFabInitialized = true;

  // Backdrop element for closing on outside tap
  const backdrop = document.createElement("div");
  backdrop.className = "terminal-fab-backdrop";
  backdrop.setAttribute("aria-hidden", "true");
  fab.parentElement.insertBefore(backdrop, fab);

  let expanded = false;
  // When true the blocking backdrop is permanently disabled: the FAB menu
  // can toggle open/closed while the rest of the UI stays fully interactive
  // on every cycle, not just the initial auto-expand. Set by the
  // "auto-expand FAB" setting.
  let backdropDisabled = false;

  function setExpanded(value, { auto = false } = {}) {
    expanded = value;
    fabToggle.setAttribute("aria-expanded", String(expanded));
    fabMenus.forEach((menu) => {
      menu.hidden = !expanded;
    });
    // Show the click-to-close backdrop only when it is not permanently
    // disabled by the auto-expand setting.
    backdrop.classList.toggle("is-visible", expanded && !backdropDisabled);
  }

 fabToggle.addEventListener("click", (e) => {
   e.stopPropagation();
    // When the backdrop is disabled (auto-expand on), never collapse the FAB
    // on toggle click: keep the menu always open. Otherwise behave as a
    // normal expand/collapse toggle.
    if (!backdropDisabled) {
      setExpanded(!expanded, { auto: false });
    }
 });

  backdrop.addEventListener("click", () => setExpanded(false));

 // Close menu when any menu item is clicked (scroll/history actions are transient)
 fabMenus.forEach((menu) => {
   menu.addEventListener("click", (e) => {
     const item = e.target.closest(".terminal-fab-item");
      // In auto-expand mode the FAB stays pinned open; never collapse it.
      if (backdropDisabled) return;
     if (item && item.id !== "terminal-soft-keyboard-toggle") {
       // Soft keyboard toggle is a toggle state, keep menu open
       setExpanded(false);
     }
   });
 });

 // Close on Escape
 document.addEventListener("keydown", (e) => {
    // In auto-expand mode the FAB stays pinned open.
    if (backdropDisabled) return;
   if (e.key === "Escape" && expanded) {
     setExpanded(false);
     fabToggle.focus();
   }
 });

  // Close on page navigation / session switch
  window.addEventListener("pagehide", () => setExpanded(false));

  // Expose for applyTerminalFabAutoExpand
  window.__terminalFabSetExpanded = setExpanded;
  // Expose a setter so the auto-expand setting can permanently disable the
  // blocking backdrop (keeping the UI interactive) across all toggles.
  window.__terminalFabSetBackdropDisabled = (disabled) => {
    backdropDisabled = Boolean(disabled);
    backdrop.classList.toggle("is-visible", expanded && !backdropDisabled);
  };
}

function applyTerminalFabAutoExpand(enabled) {
  state.terminalFabAutoExpand = enabled !== false;
  // Permanently disable the backdrop while the setting is on so the UI
  // stays interactive on every expand/collapse, not only the first one.
  if (typeof window.__terminalFabSetBackdropDisabled === "function") {
    window.__terminalFabSetBackdropDisabled(Boolean(enabled));
  }
  if (state.terminalFabAutoExpand && typeof window.__terminalFabSetExpanded === "function") {
    // Auto-expand: keep menu open without the blocking backdrop.
    window.__terminalFabSetExpanded(true, { auto: true });
  }
}

// Initialize on DOMContentLoaded or immediately if already loaded
if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", initTerminalFab);
} else {
  initTerminalFab();
}
