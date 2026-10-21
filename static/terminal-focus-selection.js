// webClx terminal focus, IME, touch selection, and auto-response helpers.
// Extracted from terminal.js as global declarations.
// Contains only helper state and function declarations; no top-level setup.

function focusTerminalSoon() {
  window.requestAnimationFrame(() => {
    focusTerminalIfAllowed();
  });
}

function terminalAutoFocusSuppressedByEditor() {
  return Boolean(
    state.renamingSessionId ||
      terminalPasteDialogEl?.open ||
      terminalAgentsDocDialogEl?.open ||
      terminalInputHistoryDialogEl?.open ||
      (typeof terminalCommandCollectionsMenuEl !== "undefined" && terminalCommandCollectionsMenuEl && !terminalCommandCollectionsMenuEl.hidden) ||
      terminalQuotaDialogEl?.open,
  );
}

let terminalActionEditableFocusBlocked = false;
let terminalActionNativeKeyboardWasVisible = false;
let terminalActionKeyboardPolicyActive = false;

function terminalNativeKeyboardVisible() {
  try {
    if (typeof window.WebClxAndroid?.isSystemKeyboardVisible === "function") {
      return window.WebClxAndroid.isSystemKeyboardVisible() === true;
    }
  } catch {}
  if (typeof terminalHelperTextareaFocused === "function" && terminalHelperTextareaFocused() && terminalSystemImeEnabled) {
    return true;
  }
  const viewportHeight = Number(window.visualViewport?.height);
  const layoutHeight = Number(window.innerHeight);
  if (
    Number.isFinite(viewportHeight) &&
    Number.isFinite(layoutHeight) &&
    layoutHeight - viewportHeight >= 80
  ) {
    return true;
  }
  const screenHeight = Number(window.screen?.height);
  if (
    Number.isFinite(screenHeight) &&
    Number.isFinite(layoutHeight) &&
    screenHeight - layoutHeight >= 150 &&
    terminalSystemImeEnabled
  ) {
    return true;
  }
  return false;
}

function terminalEditableFocusTarget(target) {
  if (target instanceof HTMLTextAreaElement) {
    return true;
  }
  if (target instanceof HTMLElement && target.isContentEditable) {
    return true;
  }
  if (!(target instanceof HTMLInputElement)) {
    return false;
  }
  return !["button", "checkbox", "color", "file", "hidden", "image", "radio", "range", "reset", "submit"].includes(
    String(target.type || "text").toLowerCase(),
  );
}

function terminalExplicitSystemKeyboardGestureTarget(target) {
  if (!(target instanceof Element)) {
    return false;
  }
  return Boolean(
    target === terminalSystemKeyboardCheckboxEl ||
      target.closest("label")?.contains(terminalSystemKeyboardCheckboxEl),
  );
}

function terminalActionControlTarget(target) {
  if (!(target instanceof Element)) {
    return null;
  }
  if (target.closest("select")) {
    return null;
  }
  return target.closest(
    'button, [role="button"], input[type="button"], input[type="submit"], input[type="reset"], input[type="checkbox"], label',
  );
}

function guardTerminalActionEditableFocus(event) {
  const target = event.target;
  if (terminalEditableFocusTarget(target)) {
    if (event.isTrusted || event.type === "pointerdown") {
      terminalActionKeyboardPolicyActive = false;
      terminalActionEditableFocusBlocked = false;
      terminalActionNativeKeyboardWasVisible = terminalNativeKeyboardVisible();
    }
    return;
  }
  if (terminalExplicitSystemKeyboardGestureTarget(target)) {
    terminalActionKeyboardPolicyActive = false;
    terminalActionEditableFocusBlocked = false;
    terminalActionNativeKeyboardWasVisible = terminalNativeKeyboardVisible();
    return;
  }
  if (!terminalActionControlTarget(target)) {
    return;
  }
  terminalActionNativeKeyboardWasVisible = terminalNativeKeyboardVisible();
  terminalActionKeyboardPolicyActive = true;
  terminalActionEditableFocusBlocked = !terminalActionNativeKeyboardWasVisible;
  if (terminalActionEditableFocusBlocked) {
    if (!(typeof softKeyboardFocusPreservingControl === "function" && softKeyboardFocusPreservingControl(target))) {
      blurTerminalHelperTextarea();
    }
  }
  if (event.type === "pointerdown" || event.type === "mousedown") {
    event.preventDefault();
  }
}

function terminalActionPreservesVisibleNativeKeyboard() {
  return terminalActionNativeKeyboardWasVisible && !terminalActionEditableFocusBlocked;
}

function terminalActionFocusGateExempt(target) {
  return Boolean(
    target instanceof HTMLElement &&
      (target.dataset?.terminalClipboardHelper === "true" ||
        target.id === "session-rename-input" ||
        terminalAgentsDocDialogEl?.contains(target)),
  );
}

function installTerminalProgrammaticFocusGate() {
  const nativeFocus = HTMLElement.prototype.focus;
  if (nativeFocus.__webclxTerminalImeGate) {
    return;
  }
  const guardedFocus = function (...args) {
    if (terminalActionFocusGateExempt(this)) {
      return nativeFocus.apply(this, args);
    }
    if (terminalActionKeyboardPolicyActive) {
      if (terminalActionEditableFocusBlocked && terminalEditableFocusTarget(this)) {
        return;
      }
      const activeElement = document.activeElement;
      if (
        terminalActionNativeKeyboardWasVisible &&
        terminalEditableFocusTarget(activeElement) &&
        this !== activeElement
      ) {
        return;
      }
    }
    return nativeFocus.apply(this, args);
  };
  guardedFocus.__webclxTerminalImeGate = true;
  HTMLElement.prototype.focus = guardedFocus;
}

function terminalDialogEditableControls(dialog) {
  return Array.from(dialog.querySelectorAll("input, textarea, [contenteditable=true]"))
    .filter((element) => terminalEditableFocusTarget(element));
}

function installTerminalDialogFocusGate() {
  for (const methodName of ["show", "showModal"]) {
    const nativeShow = HTMLDialogElement.prototype[methodName];
    if (typeof nativeShow !== "function" || nativeShow.__webclxTerminalImeGate) {
      continue;
    }
    const guardedShow = function (...args) {
      if (!terminalActionKeyboardPolicyActive) {
        return nativeShow.apply(this, args);
      }
      const controls = terminalDialogEditableControls(this);
      const inertStates = controls.map((element) => element.hasAttribute("inert"));
      controls.forEach((element) => element.setAttribute("inert", ""));
      try {
        return nativeShow.apply(this, args);
      } finally {
        window.requestAnimationFrame(() => {
          controls.forEach((element, index) => {
            if (!inertStates[index]) {
              element.removeAttribute("inert");
            }
          });
        });
      }
    };
    guardedShow.__webclxTerminalImeGate = true;
    HTMLDialogElement.prototype[methodName] = guardedShow;
  }
}

function rejectTerminalActionEditableFocus(event) {
  const target = event.target;
  if (terminalActionFocusGateExempt(target)) {
    return;
  }
  if (
    !terminalActionEditableFocusBlocked ||
    !terminalEditableFocusTarget(target)
  ) {
    return;
  }
  target.blur();
  syncTerminalImePolicy();
}

function focusTerminalAfterSoftKeyboardInput() {
  window.requestAnimationFrame(() => {
    syncTerminalImePolicy();
    syncTerminalSoftKeyboardCursor();
  });
}

function focusTerminalForUserInput() {
  if (terminalAutoFocusSuppressedByEditor()) {
    return;
  }

  if (terminalSoftKeyboardVisible()) {
    if (terminalActionPreservesVisibleNativeKeyboard()) {
      focusTerminalForDirectInput();
      return;
    }
    focusTerminalAfterSoftKeyboardInput();
    return;
  }

  focusTerminalForDirectInput();
}

function focusTerminalForUserInputSoon() {
  window.requestAnimationFrame(() => {
    focusTerminalForUserInput();
  });
}

let terminalImeResetInputEl = null;

function terminalImeResetInput() {
  if (terminalImeResetInputEl instanceof HTMLTextAreaElement) {
    return terminalImeResetInputEl;
  }

  const input = document.createElement("textarea");
  input.setAttribute("aria-hidden", "true");
  input.setAttribute("data-terminal-ime-reset", "true");
  input.setAttribute("inputmode", "text");
  input.setAttribute("autocomplete", "off");
  input.setAttribute("autocapitalize", "none");
  input.setAttribute("autocorrect", "off");
  input.tabIndex = -1;
  input.spellcheck = false;
  input.style.position = "fixed";
  input.style.left = "0";
  input.style.top = "0";
  input.style.width = "1px";
  input.style.height = "1px";
  input.style.opacity = "0";
  input.style.pointerEvents = "none";
  input.style.zIndex = "-1";
  document.body.appendChild(input);
  terminalImeResetInputEl = input;
  return input;
}

function focusTerminalImeResetTarget({ desktopInput = false } = {}) {
  // Desktop soft keys do not imply a touch IME. Windows still needs an
  // editable focus transition to rebuild the input context after a dialog.
  if (!hasPrimaryTouchInput() && (desktopInput || !terminalSoftKeyboardVisible())) {
    const input = terminalImeResetInput();
    input.value = "";
    input.focus({ preventScroll: true });
    input.setSelectionRange(0, 0);
    return;
  }

  const hadTabIndex = document.body.hasAttribute("tabindex");
  const previousTabIndex = document.body.getAttribute("tabindex");
  document.body.setAttribute("tabindex", "-1");
  document.body.focus({ preventScroll: true });
  if (hadTabIndex) {
    document.body.setAttribute("tabindex", previousTabIndex);
  } else {
    document.body.removeAttribute("tabindex");
  }
}

function resetTerminalImeFocusContext({ desktopInput = false } = {}) {
  if (!hasPrimaryTouchInput() && desktopInput) {
    // Clear the action gate before focusing the neutral textarea, rather
    // than after a blocked focus attempt. Native IME visibility is a touch
    // keyboard policy and must not skip desktop input-context recovery.
    terminalActionKeyboardPolicyActive = false;
    terminalActionEditableFocusBlocked = false;
    terminalActionNativeKeyboardWasVisible = false;
  }
  if (terminalActionPreservesVisibleNativeKeyboard()) {
    syncTerminalImePolicy();
    return;
  }
  const activeElement = document.activeElement;
  if (
    activeElement instanceof HTMLElement &&
    (activeElement === terminalAgentsDocEditorEl ||
      activeElement === terminalAgentsDocSelectEl ||
      activeElement === terminalHelperTextarea())
  ) {
    activeElement.blur();
  }

  const helper = terminalHelperTextarea();
  if (helper) {
    helper.blur();
  }
  focusTerminalImeResetTarget({ desktopInput });
  syncTerminalImePolicy();
}

let terminalDialogFocusRecoveryRevision = 0;

function installTerminalDialogCloseRecovery() {
  // close does not bubble. Observe it in capture phase, then let the dialog's
  // own handlers finish (including opening a replacement/parent dialog).
  document.addEventListener("close", (event) => {
    if (!(event.target instanceof HTMLDialogElement) || hasPrimaryTouchInput()) {
      return;
    }
    const revision = terminalDialogFocusRecoveryRevision;
    queueMicrotask(() => {
      if (revision === terminalDialogFocusRecoveryRevision) {
        restoreTerminalFocusAfterDialogClose({ desktopInput: true });
      }
    });
  }, true);
}

function restoreTerminalFocusAfterDialogClose({ desktopInput = false } = {}) {
  if (terminalAutoFocusSuppressedByEditor() || document.querySelector("dialog[open]")) {
    return;
  }

  terminalDialogFocusRecoveryRevision += 1;
  const restoreDesktopInput = !hasPrimaryTouchInput();
  resetTerminalImeFocusContext({ desktopInput: restoreDesktopInput });
  const sessionId = state.activeSessionId;
  const restore = () => {
    if (
      state.activeSessionId !== sessionId ||
      terminalAutoFocusSuppressedByEditor() ||
      document.querySelector("dialog[open]")
    ) {
      return;
    }
    if (!restoreDesktopInput) {
      focusTerminalForUserInput();
      return;
    }
    const activeElement = document.activeElement;
    if (
      terminalActionKeyboardPolicyActive ||
      terminalHelperTextareaFocused() ||
      (activeElement !== document.body && activeElement !== terminalImeResetInputEl)
    ) {
      return;
    }
    focusTerminalForDirectInput();
  };
  window.requestAnimationFrame(() => {
    window.requestAnimationFrame(restore);
  });
  window.setTimeout(restore, 80);
  window.setTimeout(restore, 180);
}

function focusTerminalAfterTransientControl() {
  if (terminalAutoFocusSuppressedByEditor()) {
    return;
  }

  if (terminalSoftKeyboardVisible()) {
    focusTerminalAfterSoftKeyboardInput();
    return;
  }

  window.requestAnimationFrame(() => {
    focusTerminalIfAllowed();
  });
}

let terminalSessionSwitchFocusInProgress = false;

function terminalWorkspaceIconSelectParts(sourceSelect) {
  if (!(sourceSelect instanceof HTMLSelectElement)) {
    return null;
  }
  // The custom dropdown host is inserted as a sibling before the native
  // select, not as an ancestor, so closest() cannot reach the trigger.
  const trigger = sourceSelect.parentElement?.querySelector(
    ":scope > .workspace-icon-select .workspace-icon-select-trigger"
  );
  const menu = sourceSelect.ownerDocument?.getElementById(
    `${sourceSelect.id || "workspace-icon-select"}-image-menu`
  );
  if (!(trigger instanceof HTMLElement) && !(menu instanceof HTMLElement)) {
    return null;
  }
  return { trigger, menu };
}

function focusTerminalAfterSessionSwitch(sourceSelect) {
  // Native select popups can finish closing after their change event. Blur the
  // control first, then retry while that transition may still own focus.
  // The workspace-icon custom dropdown hides the native select behind a
  // trigger button and its own menu: the controller closes the menu and
  // re-focuses the trigger right after dispatching the change event, so real
  // focus starts on those elements and they must count as part of the select
  // for the retry guard below.
  const customParts = terminalWorkspaceIconSelectParts(sourceSelect);
  if (sourceSelect instanceof HTMLSelectElement) {
    sourceSelect.blur();
  }
  customParts?.trigger?.blur();

  if (terminalAutoFocusSuppressedByEditor()) {
    return;
  }

  const restore = () => {
    const activeElement = document.activeElement;
    const helper = terminalHelperTextarea();
    const activeInCustomSelect = Boolean(
      (customParts?.trigger &&
        (activeElement === customParts.trigger ||
          customParts.trigger.contains(activeElement))) ||
        (customParts?.menu &&
          (activeElement === customParts.menu ||
            customParts.menu.contains(activeElement)))
    );
    if (
      activeElement !== sourceSelect &&
      activeElement !== document.body &&
      activeElement !== helper &&
      !activeInCustomSelect
    ) {
      return;
    }
    terminalSessionSwitchFocusInProgress = true;
    try {
      // Clicking a custom dropdown option <button> marks the action-focus
      // gate as blocked; a session switch must still land focus on the
      // terminal helper, so clear the gate before focusing.
      terminalActionKeyboardPolicyActive = false;
      terminalActionEditableFocusBlocked = false;
      term.focus();
    } finally {
      terminalSessionSwitchFocusInProgress = false;
    }
    syncTerminalImePolicy();
  };

  window.requestAnimationFrame(restore);
  window.setTimeout(restore, 0);
  window.setTimeout(restore, 80);
  window.setTimeout(restore, 180);
}

let pendingNewTerminalLaunchFocusSessionId = "";

function focusTerminalAfterNewTerminalLaunch(sessionId, sourceControl = null) {
  const targetSessionId = String(sessionId || "").trim();
  if (!targetSessionId) {
    return;
  }

  pendingNewTerminalLaunchFocusSessionId = targetSessionId;
  if (sourceControl instanceof HTMLElement && document.activeElement === sourceControl) {
    sourceControl.blur();
  }
  focusTerminalForDirectInput();

  const restore = () => {
    if (pendingNewTerminalLaunchFocusSessionId !== targetSessionId) {
      return;
    }
    if (state.activeSessionId !== targetSessionId) {
      return;
    }
    if (!terminalContextSocketOpen(activeTerminalContext)) {
      return;
    }
    if (!terminalInitialReplaySettled(activeTerminalContext)) {
      return;
    }

    const activeElement = document.activeElement;
    const helper = terminalHelperTextarea();
    if (
      activeElement !== sourceControl &&
      activeElement !== document.body &&
      activeElement !== helper
    ) {
      pendingNewTerminalLaunchFocusSessionId = "";
      return;
    }

    pendingNewTerminalLaunchFocusSessionId = "";
    terminalSessionSwitchFocusInProgress = true;
    try {
      terminalActionKeyboardPolicyActive = false;
      terminalActionEditableFocusBlocked = false;
      term.focus();
    } finally {
      terminalSessionSwitchFocusInProgress = false;
    }
    syncTerminalImePolicy();
  };

  window.requestAnimationFrame(restore);
  window.setTimeout(restore, 0);
  window.setTimeout(restore, 80);
  window.setTimeout(restore, 280);
  window.setTimeout(restore, 800);
}

function preventPointerFocus(element) {
  if (!element) {
    return;
  }

  element.addEventListener("pointerdown", (event) => {
    if (event.pointerType === "mouse" && event.button !== 0) {
      return;
    }
    event.preventDefault();
  });
  element.addEventListener("mousedown", (event) => {
    if (event.button !== 0) {
      return;
    }
    event.preventDefault();
  });
}

function preserveSystemImeStateForControl(element) {
  if (!element) {
    return;
  }
  element.addEventListener("pointerdown", (event) => {
    if (event.pointerType === "mouse" && event.button !== 0) {
      return;
    }
    event.preventDefault();
  });
}

function terminalHelperTextarea() {
  if (term?.textarea instanceof HTMLTextAreaElement) {
    return term.textarea;
  }

  const helper = term.element?.querySelector(".xterm-helper-textarea");
  return helper instanceof HTMLTextAreaElement ? helper : null;
}

function terminalSystemImeSuppressedBySoftKeyboardMode() {
  return !terminalSystemImeEnabled && terminalSoftKeyboardVisible();
}

function terminalSystemImeFocusSuppressed() {
  return (
    terminalSoftKeyboardVisible() &&
    !terminalImePolicy.terminalImeFocusAllowed({
      now: Date.now(),
      suppressedUntil: terminalSystemImeSuppressedUntil,
    })
  );
}

function syncTerminalImePolicy() {
  const helper = terminalHelperTextarea();
  if (!helper) {
    syncTerminalImeToggleButton();
    syncTerminalSoftKeyboardCursor();
    return;
  }

  installTerminalCursorCorrectionImeAnchor(helper);

  helper.setAttribute(
    "inputmode",
    terminalSystemImeSuppressedBySoftKeyboardMode() ? "none" : "text",
  );
  helper.setAttribute("enterkeyhint", "enter");
  helper.setAttribute("autocomplete", "off");
  helper.setAttribute("autocapitalize", "none");
  helper.setAttribute("autocorrect", "off");
  helper.spellcheck = false;
  syncTerminalImeToggleButton();
  syncTerminalSoftKeyboardCursor();
}

function syncTerminalImeToggleButton() {
  if (!terminalImeToggleButton) {
    return;
  }

  terminalImeToggleButton.classList.toggle("is-active", terminalSystemImeEnabled);
  terminalImeToggleButton.setAttribute("aria-pressed", terminalSystemImeEnabled ? "true" : "false");
  terminalImeToggleButton.title = terminalSystemImeEnabled
    ? "当前为系统输入法模式，点击切回软键盘模式"
    : "当前为软键盘模式，点击切到系统输入法模式";
}

function blurTerminalHelperTextarea() {
  const helper = terminalHelperTextarea();
  if (!helper) {
    return;
  }

  helper.blur();
  syncTerminalImePolicy();
}

function terminalHelperTextareaFocused() {
  const helper = terminalHelperTextarea();
  return Boolean(helper && document.activeElement === helper);
}

function setTerminalSystemImeEnabled(enabled, { suppressMs = 0, clearSuppression = false } = {}) {
  terminalSystemImeEnabled = Boolean(enabled);
  if (clearSuppression || terminalSystemImeEnabled) {
    terminalSystemImeSuppressedUntil = 0;
  } else if (suppressMs > 0) {
    terminalSystemImeSuppressedUntil = Date.now() + suppressMs;
  }
  syncTerminalImePolicy();
  if (terminalSystemImeSuppressedBySoftKeyboardMode()) {
    blurTerminalHelperTextarea();
  }
}

function focusTerminalForDirectInput() {
  setTerminalSystemImeEnabled(true, { clearSuppression: true });
  term.focus();
  syncTerminalImePolicy();
}

function focusTerminalFromTerminalTap() {
  terminalActionKeyboardPolicyActive = false;
  terminalActionEditableFocusBlocked = false;
  terminalActionNativeKeyboardWasVisible = terminalNativeKeyboardVisible();
  const state = {
    now: Date.now(),
    suppressedUntil: terminalSystemImeSuppressedUntil,
  };
  const action =
    typeof terminalImePolicy.terminalImeDirectFocusAction === "function"
      ? terminalImePolicy.terminalImeDirectFocusAction(state)
      : terminalImePolicy.terminalImeFocusAllowed(state)
        ? "focus"
        : "blocked";
  if (action === "blocked") {
    focusTerminalIfAllowed();
    return;
  }

  focusTerminalForDirectInput();
}

function focusTerminalIfAllowed() {
  if (terminalAutoFocusSuppressedByEditor()) {
    syncTerminalImePolicy();
    return;
  }

  if (terminalSystemImeFocusSuppressed()) {
    blurTerminalHelperTextarea();
    return;
  }

  if (terminalSystemImeSuppressedBySoftKeyboardMode()) {
    blurTerminalHelperTextarea();
    return;
  }

  term.focus();
  syncTerminalImePolicy();
}

function isTerminalEventTarget(target) {
  return Boolean(target instanceof Node && term.element?.contains(target));
}

let terminalTouchScrollGesture = null;

function rememberTerminalTouchScrollGesture(event) {
  const touch = event.touches?.[0];
  if (
    event.touches?.length !== 1 ||
    !touch ||
    !isTerminalEventTarget(touch.target)
  ) {
    terminalTouchScrollGesture = null;
    return;
  }

  terminalTouchScrollGesture = {
    identifier: touch.identifier,
    lastY: touch.clientY,
    remainderPixels: 0,
  };
}

function clearTerminalTouchScrollGesture(identifier = null) {
  if (
    !terminalTouchScrollGesture ||
    (identifier !== null && terminalTouchScrollGesture.identifier !== identifier)
  ) {
    return;
  }
  terminalTouchScrollGesture = null;
}

function terminalCanScrollTouchLines(lines) {
  const buffer = term?.buffer?.active;
  if (!buffer || !lines) {
    return false;
  }
  return lines < 0
    ? Number(buffer.viewportY) > 0
    : Number(buffer.viewportY) < Number(buffer.baseY);
}

function handleTerminalTouchScrollMove(event) {
  if (!terminalTouchScrollGesture || terminalTouchSelection || terminalSelectionHandleDrag) {
    return false;
  }

  const touch = terminalTouchByIdentifier(
    event.changedTouches,
    terminalTouchScrollGesture.identifier,
  );
  if (!touch) {
    return false;
  }

  const deltaPixels = terminalTouchScrollGesture.lastY - touch.clientY;
  terminalTouchScrollGesture.lastY = touch.clientY;
  if (!deltaPixels) {
    return false;
  }

  const step = terminalTouchSelectionPolicy.terminalTouchScrollStep({
    deltaPixels,
    remainderPixels: terminalTouchScrollGesture.remainderPixels,
    rowHeight: terminalViewportRowHeight(),
  });
  const intendedLines = step.lines || Math.sign(step.remainderPixels || deltaPixels);
  const canConsumeGesture =
    term?.buffer?.active?.type === "alternate"
      ? true
      : terminalCanScrollTouchLines(intendedLines);
  if (!canConsumeGesture) {
    terminalTouchScrollGesture.remainderPixels = 0;
    // At the boundary, let the browser continue page scrolling. Consuming the
    // gesture here strands the page whenever the terminal itself cannot move.
    return false;
  }

  terminalTouchScrollGesture.remainderPixels = step.remainderPixels;
  if (step.lines) {
    noteTerminalUserScrollIntent();
    term.scrollLines(step.lines);
    saveTerminalScrollPositionForSession(state.activeSessionId, { userIntent: true });
    clearTerminalTouchSelectionCandidate(touch.identifier);
  }
  event.preventDefault();
  event.stopPropagation();
  return true;
}

function rememberTerminalTouchSelectionCandidate(event) {
  if (terminalTouchSelectionDisabled) {
    return;
  }

  const touch = event.touches?.[0];
  if (
    event.touches?.length !== 1 ||
    !touch ||
    !isTerminalEventTarget(touch.target)
  ) {
    clearTerminalTouchSelectionCandidate();
    return;
  }

  clearTerminalTouchSelectionCandidate();

  const candidate = {
    identifier: touch.identifier,
    startedAt: Date.now(),
    target: touch.target,
    startX: touch.clientX,
    startY: touch.clientY,
    lastX: touch.clientX,
    lastY: touch.clientY,
    longPressTimer: null,
  };
  candidate.longPressTimer = window.setTimeout(() => {
    if (terminalTouchSelectionCandidate !== candidate || terminalTouchSelection) {
      return;
    }
    beginTerminalTouchSelection(candidate.identifier, candidate.lastX, candidate.lastY);
  }, state.terminalTouchSelectionLongPressMs);
  terminalTouchSelectionCandidate = candidate;
}

function clearTerminalTouchSelectionCandidate(identifier = null) {
  if (
    !terminalTouchSelectionCandidate ||
    (identifier !== null && terminalTouchSelectionCandidate.identifier !== identifier)
  ) {
    return;
  }

  if (terminalTouchSelectionCandidate.longPressTimer !== null) {
    window.clearTimeout(terminalTouchSelectionCandidate.longPressTimer);
  }
  terminalTouchSelectionCandidate = null;
}

function terminalTouchItems(touches) {
  return touches ? Array.from(touches) : [];
}

function terminalTouchByIdentifier(touches, identifier) {
  for (const touch of terminalTouchItems(touches)) {
    if (identifier === null || touch.identifier === identifier) {
      return touch;
    }
  }

  return null;
}

function updateTerminalTouchSelectionCandidateFromMove(event) {
  if (!terminalTouchSelectionCandidate || terminalTouchSelection) {
    return;
  }

  const touch = terminalTouchByIdentifier(
    event.changedTouches,
    terminalTouchSelectionCandidate.identifier
  );
  if (!touch) {
    return;
  }

  terminalTouchSelectionCandidate.lastX = touch.clientX;
  terminalTouchSelectionCandidate.lastY = touch.clientY;

  const offsetX = touch.clientX - terminalTouchSelectionCandidate.startX;
  const offsetY = touch.clientY - terminalTouchSelectionCandidate.startY;
  const action = terminalTouchSelectionPolicy.terminalTouchSelectionMoveAction({
    elapsedMs: Date.now() - terminalTouchSelectionCandidate.startedAt,
    offsetX,
    offsetY,
    longPressMs: state.terminalTouchSelectionLongPressMs,
  });
  if (action !== "keep") {
    if (action === "select") {
      beginTerminalTouchSelection(
        terminalTouchSelectionCandidate.identifier,
        terminalTouchSelectionCandidate.startX,
        terminalTouchSelectionCandidate.startY
      );
    } else {
      clearTerminalTouchSelectionCandidate(touch.identifier);
    }
  }
}

function terminalTouchFromEvent(event) {
  if (!terminalTouchSelection || !event.changedTouches?.length) {
    return null;
  }

  return terminalTouchByIdentifier(event.changedTouches, terminalTouchSelection.identifier);
}

function terminalSelectionDispatchTarget(clientX, clientY) {
  if (!term.element) {
    return null;
  }

  const pointTarget = document.elementFromPoint(clientX, clientY);
  if (isTerminalEventTarget(pointTarget)) {
    return pointTarget;
  }

  const screen = term.element.querySelector(".xterm-screen");
  if (screen instanceof EventTarget) {
    return screen;
  }

  return term.element;
}

function dispatchSyntheticTerminalMouseEvent(type, eventInit) {
  const target = terminalSelectionDispatchTarget(eventInit.clientX, eventInit.clientY);
  if (!(target instanceof EventTarget)) {
    return;
  }

  target.dispatchEvent(
    new MouseEvent(type, {
      bubbles: true,
      cancelable: true,
      composed: true,
      view: window,
      ...eventInit,
    }),
  );
}

function preventNativeTerminalTouchSelection(event) {
  if (
    !terminalTouchSelection &&
    !terminalTouchSelectionCandidate &&
    !terminalSelectionHandleDrag
  ) {
    return;
  }
  if (
    event?.target &&
    !isTerminalEventTarget(event.target) &&
    !terminalSelectionStartHandle?.contains(event.target) &&
    !terminalSelectionEndHandle?.contains(event.target)
  ) {
    return;
  }

  try {
    document.getSelection()?.removeAllRanges();
  } catch {
    // Some embedded browsers can reject selection access while the touch menu is opening.
  }
  event.preventDefault();
  event.stopPropagation();
}

function terminalSelectionHandlesReady() {
  return Boolean(
    terminalSelectionStartHandle &&
      terminalSelectionEndHandle &&
      typeof term.getSelectionPosition === "function" &&
      typeof term.select === "function" &&
      typeof terminalSelectionRangeFromPoints === "function" &&
      typeof terminalSelectionPointFromClient === "function" &&
      typeof clampTerminalSelectionPoint === "function",
  );
}

function hideTerminalSelectionHandles() {
  if (terminalSelectionStartHandle) {
    terminalSelectionStartHandle.hidden = true;
    terminalSelectionStartHandle.classList.remove("dragging");
  }
  if (terminalSelectionEndHandle) {
    terminalSelectionEndHandle.hidden = true;
    terminalSelectionEndHandle.classList.remove("dragging");
  }
}

function currentTerminalSelectionPosition() {
  const position = typeof term.getSelectionPosition === "function" ? term.getSelectionPosition() : null;
  if (!position) {
    return null;
  }

  return {
    start: {
      column: position.startColumn,
      row: position.startRow,
    },
    end: {
      column: position.endColumn,
      row: position.endRow,
    },
  };
}

function currentTerminalSelectionMetrics() {
  const screen = term.element?.querySelector(".xterm-screen");
  const shell = terminalHost?.closest(".terminal-scroll-shell");
  if (!(screen instanceof HTMLElement) || !(shell instanceof HTMLElement)) {
    return null;
  }

  const screenRect = screen.getBoundingClientRect();
  const shellRect = shell.getBoundingClientRect();
  if (screenRect.width <= 0 || screenRect.height <= 0) {
    return null;
  }

  const activeBuffer = term.buffer?.active;
  const viewportY = Number.isFinite(activeBuffer?.viewportY)
    ? Math.trunc(activeBuffer.viewportY)
    : Math.trunc(Number(activeBuffer?.baseY) || 0);
  const bufferLength = Number.isFinite(activeBuffer?.length)
    ? Math.trunc(activeBuffer.length)
    : viewportY + Math.max(Number(term.rows) || 1, 1);
  const columns = Math.max(Number(term.cols) || 1, 1);
  const rows = Math.max(Number(term.rows) || 1, 1);

  return {
    left: screenRect.left,
    top: screenRect.top,
    width: screenRect.width,
    height: screenRect.height,
    shellLeft: shellRect.left,
    shellTop: shellRect.top,
    shellWidth: shellRect.width,
    shellHeight: shellRect.height,
    columns,
    rows,
    viewportY,
    maxRow: Math.max(bufferLength - 1, viewportY + rows - 1, 0),
  };
}

function resetTerminalSelectionCopyButtonPosition() {
  terminalSelectionCopyButton?.style.removeProperty("left");
  terminalSelectionCopyButton?.style.removeProperty("top");
  terminalSelectionCopyButton?.style.removeProperty("bottom");
  terminalSelectionCopyButton?.style.removeProperty("visibility");
}

function positionTerminalSelectionHandle(handle, point, metrics) {
  const viewportRow = point.row - metrics.viewportY;
  if (viewportRow < 0 || viewportRow >= metrics.rows) {
    handle.hidden = true;
    return;
  }

  const cellWidth = metrics.width / metrics.columns;
  const cellHeight = metrics.height / metrics.rows;
  const left = metrics.left - metrics.shellLeft + point.column * cellWidth;
  const top = metrics.top - metrics.shellTop + (viewportRow + 1) * cellHeight;

  handle.style.left = `${Math.round(left)}px`;
  handle.style.top = `${Math.round(top)}px`;
  handle.hidden = false;
}

function syncTerminalSelectionHandles() {
  if (!terminalSelectionHandlesReady()) {
    hideTerminalSelectionHandles();
    return;
  }

  const selection = currentTerminalSelectionPosition();
  const metrics = currentTerminalSelectionMetrics();
  if (!selection || !metrics) {
    hideTerminalSelectionHandles();
    return;
  }

  const start = clampTerminalSelectionPoint(selection.start, metrics.columns, metrics.maxRow);
  const end = clampTerminalSelectionPoint(selection.end, metrics.columns, metrics.maxRow);
  positionTerminalSelectionHandle(terminalSelectionStartHandle, start, metrics);
  positionTerminalSelectionHandle(terminalSelectionEndHandle, end, metrics);
}

function syncTerminalSelectionCopyButton() {
  if (!terminalSelectionCopyButton) {
    return;
  }

  const hasSelection =
    (typeof term.hasSelection === "function" && term.hasSelection()) ||
    Boolean(typeof term.getSelection === "function" && term.getSelection());
  if (!hasSelection) {
    terminalSelectionCopyButton.hidden = true;
    return;
  }

  const selection = currentTerminalSelectionPosition();
  const metrics = currentTerminalSelectionMetrics();
  if (!selection || !metrics) {
    resetTerminalSelectionCopyButtonPosition();
    terminalSelectionCopyButton.hidden = false;
    return;
  }

  const cellWidth = metrics.width / metrics.columns;
  const cellHeight = metrics.height / metrics.rows;
  const start = clampTerminalSelectionPoint(selection.start, metrics.columns, metrics.maxRow);
  const end = clampTerminalSelectionPoint(selection.end, metrics.columns, metrics.maxRow);
  const firstRow = Math.min(start.row, end.row);
  const lastRow = Math.max(start.row, end.row);
  const visibleFirstRow = Math.max(firstRow, metrics.viewportY);
  const visibleLastRow = Math.min(lastRow, metrics.viewportY + metrics.rows - 1);
  if (visibleFirstRow > visibleLastRow) {
    resetTerminalSelectionCopyButtonPosition();
    terminalSelectionCopyButton.hidden = false;
    return;
  }

  const screenLeft = metrics.left - metrics.shellLeft;
  const isSingleRow = firstRow === lastRow;
  const firstColumn = isSingleRow ? Math.min(start.column, end.column) : 0;
  const lastColumn = isSingleRow ? Math.max(start.column, end.column) : metrics.columns - 1;
  const selectionLeft = screenLeft + firstColumn * cellWidth;
  const selectionRight = screenLeft + (lastColumn + 1) * cellWidth;
  const lowerSelectionTop =
    metrics.top - metrics.shellTop + (visibleLastRow - metrics.viewportY) * cellHeight;
  const selectionBottom = lowerSelectionTop + cellHeight;
  const gap = 8;

  // Measure after showing the button, then keep it inside the terminal host.
  terminalSelectionCopyButton.hidden = false;
  terminalSelectionCopyButton.style.visibility = "hidden";
  const buttonWidth = terminalSelectionCopyButton.offsetWidth;
  const buttonHeight = terminalSelectionCopyButton.offsetHeight;
  const minLeft = 8;
  const maxLeft = Math.max(minLeft, metrics.shellWidth - buttonWidth - 8);
  const centeredLeft = (selectionLeft + selectionRight - buttonWidth) / 2;
  const left = Math.min(Math.max(centeredLeft, minLeft), maxLeft);
  const belowTop = selectionBottom + gap;
  const maxTop = Math.max(8, metrics.shellHeight - buttonHeight - 8);
  const top = belowTop <= maxTop
    ? belowTop
    : Math.max(8, lowerSelectionTop - buttonHeight - gap);

  terminalSelectionCopyButton.style.left = `${Math.round(left)}px`;
  terminalSelectionCopyButton.style.top = `${Math.round(Math.min(top, maxTop))}px`;
  terminalSelectionCopyButton.style.bottom = "auto";
  terminalSelectionCopyButton.style.visibility = "";
}

function syncTerminalSelectionControls() {
  syncTerminalSelectionCopyButton();
  syncTerminalSelectionHandles();
}

function terminalSelectionDragPoint(event, metrics) {
  return terminalSelectionPointFromClient(
    {
      clientX: event.clientX,
      clientY: event.clientY,
    },
    metrics,
  );
}

function terminalTouchSelectionCellFromClient(pointer, metrics) {
  const columns = Math.max(Math.trunc(Number(metrics?.columns) || 0), 1);
  const rows = Math.max(Math.trunc(Number(metrics?.rows) || 0), 1);
  const width = Math.max(Number(metrics?.width) || 0, 1);
  const height = Math.max(Number(metrics?.height) || 0, 1);
  const cellWidth = width / columns;
  const cellHeight = height / rows;
  const viewportY = Math.trunc(Number(metrics?.viewportY) || 0);
  const maxRow = Math.max(Math.trunc(Number(metrics?.maxRow) || viewportY), viewportY);
  const localX = (Number(pointer?.clientX) || 0) - (Number(metrics?.left) || 0);
  const localY = (Number(pointer?.clientY) || 0) - (Number(metrics?.top) || 0);
  const column = Math.min(Math.max(Math.floor(localX / cellWidth), 0), columns - 1);
  const row = Math.min(Math.max(viewportY + Math.floor(localY / cellHeight), viewportY), maxRow);

  return { column, row };
}

function applyTerminalSelectionHandleDrag(event) {
  if (!terminalSelectionHandleDrag || !terminalSelectionHandlesReady()) {
    return;
  }

  const metrics = currentTerminalSelectionMetrics();
  if (!metrics) {
    hideTerminalSelectionHandles();
    return;
  }

  const movingPoint = terminalSelectionDragPoint(event, metrics);
  const range = terminalSelectionRangeFromPoints(
    terminalSelectionHandleDrag.anchor,
    movingPoint,
    metrics.columns,
  );
  if (range.length <= 0) {
    return;
  }

  term.select(range.column, range.row, range.length);
  syncTerminalSelectionControls();
}

function startTerminalSelectionHandleDrag(event, handleName) {
  if (
    !terminalSelectionHandlesReady() ||
    (event.pointerType === "mouse" && event.button !== 0)
  ) {
    return;
  }

  const selection = currentTerminalSelectionPosition();
  const metrics = currentTerminalSelectionMetrics();
  if (!selection || !metrics) {
    return;
  }

  terminalSelectionHandleDrag = {
    pointerId: event.pointerId,
    handleName,
    anchor: handleName === "start" ? selection.end : selection.start,
  };

  event.currentTarget.classList.add("dragging");
  if (typeof event.currentTarget.setPointerCapture === "function") {
    event.currentTarget.setPointerCapture(event.pointerId);
  }
  event.preventDefault();
  event.stopPropagation();
}

function handleTerminalSelectionHandleMove(event) {
  if (!terminalSelectionHandleDrag || terminalSelectionHandleDrag.pointerId !== event.pointerId) {
    return;
  }

  applyTerminalSelectionHandleDrag(event);
  event.preventDefault();
  event.stopPropagation();
}

function stopTerminalSelectionHandleDrag(event) {
  if (!terminalSelectionHandleDrag || terminalSelectionHandleDrag.pointerId !== event.pointerId) {
    return;
  }

  terminalSelectionHandleDrag = null;
  terminalSelectionStartHandle?.classList.remove("dragging");
  terminalSelectionEndHandle?.classList.remove("dragging");
  syncTerminalSelectionControls();
  event.preventDefault();
  event.stopPropagation();
}

function beginTerminalTouchSelection(identifier, clientX, clientY) {
  if (terminalTouchSelectionDisabled) {
    return;
  }
  if (terminalTouchSelection) {
    return false;
  }

  if (terminalSelectionHandlesReady()) {
    const metrics = currentTerminalSelectionMetrics();
    if (metrics) {
      const cell = terminalTouchSelectionCellFromClient({ clientX, clientY }, metrics);
      const range = terminalTouchSelectionPolicy.terminalTouchSelectionInitialRange(
        cell,
        metrics.columns,
      );
      terminalTouchSelection = {
        identifier,
        anchor: {
          column: range.column,
          row: range.row,
        },
      };
      try {
        document.getSelection()?.removeAllRanges();
      } catch {
        // xterm selection should be the only active selection surface.
      }
      term.select(range.column, range.row, range.length);
      syncTerminalSelectionControls();
      return true;
    }
  }

  terminalTouchSelection = {
    identifier,
  };

  dispatchSyntheticTerminalMouseEvent("mousedown", {
    button: 0,
    buttons: 1,
    clientX,
    clientY,
  });
  return true;
}

function handleTerminalContextMenuSelection(event) {
  if (terminalContextMenuEventIsTouch(event)) {
    if (terminalTouchSelection) {
      preventNativeTerminalTouchSelection(event);
      return;
    }

    startTerminalTouchSelection(event);
    return;
  }

  event.preventDefault();
  event.stopPropagation();
  openTerminalContextMenu(event.clientX, event.clientY);
}

function terminalContextMenuEventIsTouch(event) {
  return Boolean(
    event?.pointerType === "touch" ||
      event?.sourceCapabilities?.firesTouchEvents ||
      terminalTouchSelectionCandidate ||
      terminalTouchSelection,
  );
}

function closeTerminalContextMenu() {
  if (!terminalContextMenuEl || terminalContextMenuEl.hidden) {
    terminalContextMenuCopyAction = null;
    return;
  }

  terminalContextMenuEl.hidden = true;
  terminalContextMenuCopyAction = null;
}

function openTerminalContextMenu(clientX, clientY, copyAction, container = document.body) {
  if (!terminalContextMenuEl) {
    return;
  }
  if (container instanceof Node && terminalContextMenuEl.parentElement !== container) {
    container.appendChild(terminalContextMenuEl);
  }

  terminalContextMenuCopyAction = typeof copyAction === "function" ? copyAction : null;
  const contextAction = terminalContextMenuCopyAction?.() || null;
  const hasSelection = contextAction
    ? Boolean(String(contextAction.text || "").trim())
    : Boolean(typeof term?.hasSelection === "function" && term.hasSelection());
  if (terminalContextCopyAllButton) {
    terminalContextCopyAllButton.textContent = contextAction?.label || (hasSelection ? "复制选中文本" : "复制全部文本");
    terminalContextCopyAllButton.setAttribute(
      "aria-label",
      terminalContextCopyAllButton.textContent,
    );
  }
  if (terminalContextSelectAllButton) {
    terminalContextSelectAllButton.hidden = Boolean(hasSelection || contextAction);
  }
  const terminalContextAgentPromptButton = document.getElementById("terminal-context-agent-prompt");
  if (terminalContextAgentPromptButton) {
    terminalContextAgentPromptButton.hidden = !hasSelection || Boolean(contextAction);
  }
  terminalContextMenuEl.hidden = false;
  terminalContextMenuEl.style.left = "0px";
  terminalContextMenuEl.style.top = "0px";

  const margin = 8;
  const rect = terminalContextMenuEl.getBoundingClientRect();
  const left = Math.min(
    Math.max(Number(clientX) || 0, margin),
    Math.max(window.innerWidth - rect.width - margin, margin),
  );
  const top = Math.min(
    Math.max(Number(clientY) || 0, margin),
    Math.max(window.innerHeight - rect.height - margin, margin),
  );
  terminalContextMenuEl.style.left = `${left}px`;
  terminalContextMenuEl.style.top = `${top}px`;
  terminalContextCopyAllButton?.focus({ preventScroll: true });
}

async function copyTerminalContextMenuSelection() {
  const getAction = terminalContextMenuCopyAction;
  if (typeof getAction !== "function") {
    return false;
  }

  const action = getAction();
  if (!action || typeof action !== "object") {
    return false;
  }

  closeTerminalContextMenu();
  const copied = await copyTextToClipboard(String(action.text || ""));
  action.onCopied?.(copied);
  return true;
}

function closeTerminalContextMenuFromOutside(event) {
  if (!terminalContextMenuEl || terminalContextMenuEl.hidden) {
    return;
  }
  if (event?.target instanceof Node && terminalContextMenuEl.contains(event.target)) {
    return;
  }

  closeTerminalContextMenu();
}

function handleTerminalContextMenuKeydown(event) {
  if (event.key !== "Escape" || terminalContextMenuEl?.hidden !== false) {
    return;
  }

  event.preventDefault();
  event.stopPropagation();
  closeTerminalContextMenu();
  focusTerminalIfAllowed();
}

function applyTerminalTouchSelectionDrag(clientX, clientY) {
  if (!terminalTouchSelection?.anchor || !terminalSelectionHandlesReady()) {
    return false;
  }

  const metrics = currentTerminalSelectionMetrics();
  if (!metrics) {
    hideTerminalSelectionHandles();
    return false;
  }

  const focus = terminalTouchSelectionCellFromClient({ clientX, clientY }, metrics);
  const range = terminalTouchSelectionPolicy.terminalTouchSelectionRangeBetweenCells(
    terminalTouchSelection.anchor,
    focus,
    metrics.columns,
  );
  if (range.length <= 0) {
    return false;
  }

  term.select(range.column, range.row, range.length);
  syncTerminalSelectionControls();
  return true;
}

function startTerminalTouchSelection(event) {
  if (
    !isTerminalEventTarget(event.target) ||
    !(
      event.pointerType === "touch" ||
      event.sourceCapabilities?.firesTouchEvents ||
      terminalTouchSelectionCandidate
    )
  ) {
    return;
  }

  if (terminalTouchSelection) {
    event.preventDefault();
    event.stopPropagation();
    return;
  }

  const candidate = terminalTouchSelectionCandidate;
  if (
    terminalTouchSelectionPolicy.terminalTouchSelectionContextMenuAction({
      elapsedMs: candidate ? Date.now() - candidate.startedAt : 0,
      longPressMs: state.terminalTouchSelectionLongPressMs,
    }) === "ignore"
  ) {
    event.preventDefault();
    event.stopPropagation();
    return;
  }

  beginTerminalTouchSelection(
    candidate?.identifier ?? null,
    event.clientX,
    event.clientY
  );

  event.preventDefault();
  event.stopPropagation();
}

function endTerminalTouchSelection(identifier = null) {
  if (!terminalTouchSelection || (identifier !== null && terminalTouchSelection.identifier !== identifier)) {
    return false;
  }

  terminalTouchSelection = null;
  clearTerminalTouchSelectionCandidate(identifier);
  return true;
}

function handleTerminalTouchSelectionMove(event) {
  updateTerminalTouchSelectionCandidateFromMove(event);
  if (handleTerminalTouchScrollMove(event)) {
    return;
  }

  const touch = terminalTouchFromEvent(event);
  if (!touch) {
    return;
  }

  if (terminalTouchSelection?.anchor) {
    event.preventDefault();
    event.stopPropagation();
    return;
  }

  dispatchSyntheticTerminalMouseEvent("mousemove", {
    button: 0,
    buttons: 1,
    clientX: touch.clientX,
    clientY: touch.clientY,
  });
  event.preventDefault();
  event.stopPropagation();
}

function handleTerminalTouchSelectionEnd(event) {
  const touch = terminalTouchFromEvent(event);
  if (!touch) {
    return;
  }

  if (!terminalTouchSelection?.anchor) {
    dispatchSyntheticTerminalMouseEvent("mouseup", {
      button: 0,
      buttons: 0,
      clientX: touch.clientX,
      clientY: touch.clientY,
    });
  }

  if (endTerminalTouchSelection(touch.identifier)) {
    window.requestAnimationFrame(syncTerminalSelectionControls);
    event.preventDefault();
    event.stopPropagation();
  }
}

function terminalAsciiChunk(bytes) {
  let output = "";
  for (const byte of bytes) {
    if (byte === 0x1b || byte === 0x0a || byte === 0x0d || byte === 0x09 || (byte >= 0x20 && byte <= 0x7e)) {
      output += String.fromCharCode(byte);
    }
  }
  return output;
}

function noteTerminalAutoResponseQueries(bytes) {
  const asciiChunk = terminalAsciiChunk(bytes);
  if (!asciiChunk) {
    terminalOutputTail = "";
    return;
  }

  const scanWindow = terminalOutputTail + asciiChunk;
  let matchCount = 0;
  for (const match of scanWindow.matchAll(DEVICE_ATTRIBUTE_REQUEST_PATTERN)) {
    const matchStart = match.index ?? -1;
    const matchEnd = matchStart + match[0].length;
    if (matchStart >= 0 && matchEnd > terminalOutputTail.length) {
      matchCount += 1;
    }
  }

  if (matchCount > 0) {
    pendingDeviceAttributeResponses = Math.min(
      pendingDeviceAttributeResponses + matchCount,
      MAX_PENDING_DEVICE_ATTRIBUTE_RESPONSES,
    );
  }

  terminalOutputTail = scanWindow.slice(-DEVICE_ATTRIBUTE_REQUEST_TAIL_LENGTH);
}

function isPotentialDeviceAttributeResponsePrefix(data) {
  return DEVICE_ATTRIBUTE_RESPONSE_PREFIX_PATTERN.test(data);
}

function filterTerminalAutoResponse(data) {
  if (!data) {
    return "";
  }

  const combined = terminalInputTail + data;
  terminalInputTail = "";

  if (pendingDeviceAttributeResponses <= 0) {
    return combined;
  }

  let index = 0;
  let output = "";
  while (index < combined.length) {
    const slice = combined.slice(index);
    const matchedResponse = slice.match(DEVICE_ATTRIBUTE_RESPONSE_START_PATTERN);
    if (matchedResponse && pendingDeviceAttributeResponses > 0) {
      pendingDeviceAttributeResponses -= 1;
      index += matchedResponse[0].length;
      continue;
    }

    if (pendingDeviceAttributeResponses > 0 && isPotentialDeviceAttributeResponsePrefix(slice)) {
      terminalInputTail = slice;
      break;
    }

    output += combined[index];
    index += 1;
  }

  return output;
}

function resetTerminalAutoResponseState() {
  terminalOutputTail = "";
  terminalInputTail = "";
  pendingDeviceAttributeResponses = 0;
}

function readTerminalBufferTailTextFrom(
  terminalInstance,
  maxLines = TERMINAL_RESUME_SCAN_MAX_LINES,
) {
  const activeBuffer = terminalInstance?.buffer?.active;
  if (!activeBuffer || typeof activeBuffer.length !== "number" || typeof activeBuffer.getLine !== "function") {
    return "";
  }

  const lines = [];
  const startIndex = Math.max(activeBuffer.length - maxLines, 0);
  for (let index = startIndex; index < activeBuffer.length; index += 1) {
    const line = activeBuffer.getLine(index);
    if (!line || typeof line.translateToString !== "function") {
      continue;
    }

    const text = line.translateToString(true);
    if (line.isWrapped && lines.length > 0) {
      lines[lines.length - 1] += text;
      continue;
    }

    lines.push(text);
  }

  return lines.join("\n");
}

function readTerminalBufferTailText(maxLines = TERMINAL_RESUME_SCAN_MAX_LINES) {
  return readTerminalBufferTailTextFrom(term, maxLines);
}

function readTerminalAllText() {
  const activeBuffer = term.buffer?.active;
  if (!activeBuffer || typeof activeBuffer.length !== "number" || typeof activeBuffer.getLine !== "function") {
    return "";
  }

  const lines = [];
  for (let index = 0; index < activeBuffer.length; index += 1) {
    const line = activeBuffer.getLine(index);
    if (!line || typeof line.translateToString !== "function") {
      continue;
    }

    const text = line.translateToString(true);
    if (line.isWrapped && lines.length > 0) {
      lines[lines.length - 1] += text;
      continue;
    }
    lines.push(text);
  }

  return lines.join("\n").replace(/[\t ]+$/gm, "").trimEnd();
}

function readTerminalVisibleText() {
  const activeBuffer = term.buffer?.active;
  if (!activeBuffer || typeof activeBuffer.getLine !== "function") {
    return "";
  }

  const viewportY = Number.isFinite(activeBuffer.viewportY)
    ? Math.trunc(activeBuffer.viewportY)
    : Math.trunc(Number(activeBuffer.baseY) || 0);
  const rows = Math.max(Math.trunc(Number(term.rows) || 0), 1);
  const bufferLength = Number.isFinite(activeBuffer.length)
    ? Math.trunc(activeBuffer.length)
    : viewportY + rows;
  const endIndex = Math.min(viewportY + rows, bufferLength);
  const lines = [];

  for (let index = Math.max(viewportY, 0); index < endIndex; index += 1) {
    const line = activeBuffer.getLine(index);
    if (!line || typeof line.translateToString !== "function") {
      continue;
    }

    const text = line.translateToString(true);
    if (line.isWrapped && lines.length > 0) {
      lines[lines.length - 1] += text;
      continue;
    }
    lines.push(text);
  }

  return lines.join("\n").replace(/[\t ]+$/gm, "").trimEnd();
}

function currentTerminalSwitchPlaceholderText() {
  return terminalSwitchPlaceholderEl?.textContent || "";
}
