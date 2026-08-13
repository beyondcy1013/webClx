let terminalCursorCorrectionEl = null;
let terminalCursorCorrectionActive = false;
let terminalCursorCorrectionDeactivateCount = 0;
let terminalCursorCorrectionLastTarget = null;
let terminalCursorCorrectionLastStyleKey = "";
let terminalCursorCorrectionFrame = 0;
let terminalSoftKeyboardCursorEl = null;
let terminalSoftKeyboardCursorLastStyleKey = "";

function isTerminalCursorHidden() {
  return term?._core?.coreService?.isCursorHidden === true;
}

function terminalEditableViewportRowRange() {
  const activeBuffer = term.buffer?.active;
  if (
    !activeBuffer ||
    typeof activeBuffer.cursorY !== "number" ||
    !Number.isFinite(activeBuffer.cursorY)
  ) {
    return null;
  }

  const rows = Math.max(Number(term.rows) || 0, 1);
  const baseY = Number.isFinite(activeBuffer.baseY) ? activeBuffer.baseY : 0;
  let start = Math.min(Math.max(Math.trunc(activeBuffer.cursorY), 0), rows - 1);
  let end = start;

  if (typeof activeBuffer.getLine === "function") {
    while (start > 0) {
      const line = activeBuffer.getLine(baseY + start);
      if (!line?.isWrapped) {
        break;
      }
      start -= 1;
    }

    while (end + 1 < rows) {
      const nextLine = activeBuffer.getLine(baseY + end + 1);
      if (!nextLine?.isWrapped) {
        break;
      }
      end += 1;
    }
  }

  return { start, end };
}

function setTerminalCursorHiddenForCorrection(hidden, { force = false } = {}) {
  const nextHidden = Boolean(hidden);
  if (!force && terminalCursorCorrectionActive === nextHidden) {
    return;
  }

  terminalCursorCorrectionActive = nextHidden;
  term.options.theme = terminalThemeForCursorState(nextHidden);
}

function terminalViewportLineText(activeBuffer, row) {
  if (!activeBuffer || typeof activeBuffer.getLine !== "function") {
    return "";
  }

  const baseY = Number.isFinite(activeBuffer.baseY) ? activeBuffer.baseY : 0;
  const line = activeBuffer.getLine(baseY + row);
  return typeof line?.translateToString === "function" ? line.translateToString(true) : "";
}

function terminalCellAttr(cell) {
  if (Number.isFinite(cell?.fg)) {
    return cell.fg;
  }

  const charData = cell?.getAsCharData?.();
  return Array.isArray(charData) ? Number(charData[0]) || 0 : 0;
}

function terminalCellBgAttr(cell) {
  return Number.isFinite(cell?.bg) ? cell.bg : 0;
}

function terminalViewportLineHasApplicationCursor(activeBuffer, row) {
  if (!activeBuffer || typeof activeBuffer.getLine !== "function") {
    return false;
  }

  const baseY = Number.isFinite(activeBuffer.baseY) ? activeBuffer.baseY : 0;
  const line = activeBuffer.getLine(baseY + row);
  if (!line || typeof line.getCell !== "function") {
    return false;
  }

  const columns = Math.max(Number(term.cols) || 0, 0);
  const length = Math.min(
    columns,
    Number.isFinite(line.length) ? Math.max(Math.trunc(line.length), 0) : columns,
  );
  for (let column = 0; column < length; column += 1) {
    const cell = line.getCell(column);
    if ((terminalCellAttr(cell) & XTERM_CELL_ATTR_INVERSE_MASK) !== 0) {
      return true;
    }
  }

  return false;
}

function terminalViewportPlaceholderRanges(activeBuffer, row) {
  if (!activeBuffer || typeof activeBuffer.getLine !== "function") {
    return [];
  }

  const baseY = Number.isFinite(activeBuffer.baseY) ? activeBuffer.baseY : 0;
  const line = activeBuffer.getLine(baseY + row);
  if (!line || typeof line.getCell !== "function") {
    return [];
  }

  const columns = Math.max(Number(term.cols) || 0, 0);
  const length = Math.min(
    columns,
    Number.isFinite(line.length) ? Math.max(Math.trunc(line.length), 0) : columns,
  );
  const ranges = [];
  let startColumn = -1;
  for (let column = 0; column < length; column += 1) {
    const cell = line.getCell(column);
    const text = typeof cell?.getChars === "function" ? cell.getChars() : "";
    const isPlaceholderCell =
      text.length > 0 && (terminalCellBgAttr(cell) & XTERM_CELL_ATTR_DIM_MASK) !== 0;
    if (isPlaceholderCell && startColumn < 0) {
      startColumn = column;
    } else if (!isPlaceholderCell && startColumn >= 0) {
      ranges.push({ row, startColumn, endColumn: column });
      startColumn = -1;
    }
  }

  if (startColumn >= 0) {
    ranges.push({ row, startColumn, endColumn: length });
  }

  return ranges;
}

function terminalCursorCorrectionTarget() {
  if (
    !terminalCursorGuard ||
    typeof terminalCursorGuard.detectBottomStatusCursorCorrection !== "function" ||
    isTerminalCursorHidden()
  ) {
    return null;
  }

  const activeBuffer = term.buffer?.active;
  if (
    !activeBuffer ||
    typeof activeBuffer.cursorY !== "number" ||
    !Number.isFinite(activeBuffer.cursorY)
  ) {
    return null;
  }

  const rows = Math.max(Number(term.rows) || 0, 0);
  const columns = Math.max(Number(term.cols) || 0, 1);
  const lines = [];
  for (let row = 0; row < rows; row += 1) {
    lines.push(terminalViewportLineText(activeBuffer, row));
  }
  const inputRow = rows - 3;
  const applicationCursorRows =
    inputRow >= 0 && terminalViewportLineHasApplicationCursor(activeBuffer, inputRow) ? [inputRow] : [];
  const placeholderRanges =
    inputRow >= 0 ? terminalViewportPlaceholderRanges(activeBuffer, inputRow) : [];

  return terminalCursorGuard.detectBottomStatusCursorCorrection({
    cursorRow: activeBuffer.cursorY,
    cursorColumn: activeBuffer.cursorX,
    rows,
    columns,
    lines,
    applicationCursorRows,
    placeholderRanges,
  });
}

function terminalCellDimensions() {
  const dimensions = term?._core?._renderService?.dimensions;
  const width = dimensions?.actualCellWidth;
  const height = dimensions?.actualCellHeight;
  if (!Number.isFinite(width) || width <= 0 || !Number.isFinite(height) || height <= 0) {
    return null;
  }

  return { width, height };
}

function terminalCursorCorrectionElement() {
  const screen = term.element?.querySelector(".xterm-screen");
  if (!(screen instanceof HTMLElement)) {
    return null;
  }

  if (!terminalCursorCorrectionEl) {
    terminalCursorCorrectionEl = document.createElement("span");
    terminalCursorCorrectionEl.className = "terminal-cursor-correction";
    terminalCursorCorrectionEl.setAttribute("aria-hidden", "true");
    terminalCursorCorrectionEl.hidden = true;
  }

  if (terminalCursorCorrectionEl.parentElement !== screen) {
    screen.appendChild(terminalCursorCorrectionEl);
  }

  return terminalCursorCorrectionEl;
}

function terminalSoftKeyboardVisible() {
  return (
    document.body?.dataset.terminalSoftKeyboard === "open" ||
    terminalSoftKeyboardAutoVisible() ||
    state.temporaryDesktopTerminalSoftKeyboardVisible
  );
}

function shouldShowTerminalSoftKeyboardCursor() {
  return Boolean(
    state.activeSessionId &&
      terminalSoftKeyboardVisible() &&
      !terminalSystemImeEnabled &&
      !terminalCursorCorrectionActive &&
      !terminalBacklogReplayActive &&
      !terminalHost?.classList.contains("terminal-host-switching")
  );
}

function terminalSoftKeyboardCursorElement() {
  const screen = term.element?.querySelector(".xterm-screen");
  if (!(screen instanceof HTMLElement)) {
    return null;
  }

  if (!terminalSoftKeyboardCursorEl) {
    terminalSoftKeyboardCursorEl = document.createElement("span");
    terminalSoftKeyboardCursorEl.className = "terminal-soft-keyboard-cursor";
    terminalSoftKeyboardCursorEl.setAttribute("aria-hidden", "true");
    terminalSoftKeyboardCursorEl.hidden = true;
  }

  if (terminalSoftKeyboardCursorEl.parentElement !== screen) {
    screen.appendChild(terminalSoftKeyboardCursorEl);
  }

  return terminalSoftKeyboardCursorEl;
}

function hideTerminalSoftKeyboardCursor() {
  terminalSoftKeyboardCursorLastStyleKey = "";
  if (terminalSoftKeyboardCursorEl) {
    terminalSoftKeyboardCursorEl.hidden = true;
  }
}

function terminalSoftKeyboardCursorTarget() {
  if (!shouldShowTerminalSoftKeyboardCursor()) {
    return null;
  }

  const activeBuffer = term.buffer?.active;
  if (
    !activeBuffer ||
    typeof activeBuffer.cursorY !== "number" ||
    !Number.isFinite(activeBuffer.cursorY)
  ) {
    return null;
  }

  const rows = Math.max(Number(term.rows) || 0, 1);
  const columns = Math.max(Number(term.cols) || 0, 1);
  return {
    row: Math.min(Math.max(Math.trunc(activeBuffer.cursorY), 0), rows - 1),
    column: Math.min(Math.max(Math.trunc(activeBuffer.cursorX) || 0, 0), columns - 1),
  };
}

function syncTerminalSoftKeyboardCursor() {
  const target = terminalSoftKeyboardCursorTarget();
  const dimensions = target ? terminalCellDimensions() : null;
  if (!target || !dimensions) {
    hideTerminalSoftKeyboardCursor();
    return;
  }

  const marker = terminalSoftKeyboardCursorElement();
  if (!marker) {
    hideTerminalSoftKeyboardCursor();
    return;
  }

  const width = Math.max(2, Math.min(dimensions.width, Number(term.options?.cursorWidth) || 2));
  const height = dimensions.height;
  const x = target.column * dimensions.width;
  const y = target.row * dimensions.height;
  const styleKey = `${width}:${height}:${x}:${y}`;
  marker.hidden = false;
  if (styleKey !== terminalSoftKeyboardCursorLastStyleKey) {
    marker.style.width = `${width}px`;
    marker.style.height = `${height}px`;
    marker.style.transform = `translate(${x}px, ${y}px)`;
    terminalSoftKeyboardCursorLastStyleKey = styleKey;
  }
}

function hideTerminalCursorCorrection() {
  terminalCursorCorrectionDeactivateCount = 0;
  terminalCursorCorrectionLastTarget = null;
  terminalCursorCorrectionLastStyleKey = "";
  if (terminalCursorCorrectionEl) {
    terminalCursorCorrectionEl.hidden = true;
  }
  setTerminalCursorHiddenForCorrection(false);
}

// During an active mouse/touch text selection xterm re-renders every frame to
// repaint the selection highlight. If cursor correction also re-evaluates then,
// false positives can force full xterm redraws and make the terminal blink.
function terminalSelectionBlockingCursorCorrection() {
  return (
    terminalSelectionHandleDrag !== null ||
    (typeof term.hasSelection === "function" && term.hasSelection())
  );
}

function syncTerminalCursorCorrection() {
  if (terminalSelectionBlockingCursorCorrection()) {
    return;
  }
  const target = terminalCursorCorrectionTarget();
  const dimensions = target ? terminalCellDimensions() : null;
  if (!target || !dimensions) {
    if (!terminalCursorCorrectionActive) {
      hideTerminalCursorCorrection();
      syncTerminalSoftKeyboardCursor();
      return;
    }
    terminalCursorCorrectionDeactivateCount += 1;
    if (terminalCursorCorrectionDeactivateCount < 4) {
      hideTerminalSoftKeyboardCursor();
      return;
    }
    hideTerminalCursorCorrection();
    syncTerminalSoftKeyboardCursor();
    return;
  }

  hideTerminalSoftKeyboardCursor();
  terminalCursorCorrectionDeactivateCount = 0;
  terminalCursorCorrectionLastTarget = target;

  const marker = terminalCursorCorrectionElement();
  if (!marker) {
    hideTerminalCursorCorrection();
    return;
  }

  marker.hidden = false;
  const geometry =
    typeof terminalCursorGuard.cursorCorrectionMarkerGeometry === "function"
      ? terminalCursorGuard.cursorCorrectionMarkerGeometry({
          cellWidth: dimensions.width,
          cellHeight: dimensions.height,
          column: target.column,
          row: target.row,
        })
      : {
          width: 2,
          height: dimensions.height,
          x: target.column * dimensions.width,
          y: target.row * dimensions.height,
        };
  const styleKey = `${geometry.width}:${geometry.height}:${geometry.x}:${geometry.y}`;
  if (styleKey !== terminalCursorCorrectionLastStyleKey || marker.hidden) {
    marker.style.width = `${geometry.width}px`;
    marker.style.height = `${geometry.height}px`;
    marker.style.transform = `translate(${geometry.x}px, ${geometry.y}px)`;
    terminalCursorCorrectionLastStyleKey = styleKey;
  }
  setTerminalCursorHiddenForCorrection(true);
}

function scheduleTerminalCursorCorrection() {
  // While a large batched term.write() is in flight (e.g. a 20-64 KiB Codex
  // TUI redraw merged into one atomic write), xterm paints the buffer across
  // multiple animation frames.  Each intermediate frame fires onRender, which
  // would re-evaluate cursor correction against a half-updated buffer.  The
  // detection result oscillates between "found" and "not found", toggling
  // term.options.theme on every frame and causing a full-canvas repaint loop
  // that the user sees as irregular 2-3s flicker.  Skip scheduling during the
  // write; drainTerminalOutputQueue already calls syncTerminalCursorCorrection
  // in the write callback once the buffer is in a stable final state.
  if (terminalOutputWriteInFlight) {
    return;
  }

  if (terminalCursorCorrectionFrame) {
    return;
  }

  terminalCursorCorrectionFrame = window.requestAnimationFrame(() => {
    terminalCursorCorrectionFrame = 0;
    syncTerminalCursorCorrection();
  });
}

function terminalMouseButtonBase(buttonCode) {
  return buttonCode & ~(4 | 8 | 16);
}

function isTerminalMouseWheelReport(buttonCode) {
  const base = terminalMouseButtonBase(buttonCode);
  return base >= 64 && base <= 67;
}

function shouldForwardTerminalMouseReport(row, buttonCode) {
  if (isTerminalMouseWheelReport(buttonCode)) {
    return true;
  }

  const rows = Math.max(Number(term.rows) || 0, 1);
  if (row < 0 || row >= rows) {
    return false;
  }

  if (isTerminalCursorHidden()) {
    return false;
  }

  const editableRows = terminalEditableViewportRowRange();
  if (!editableRows) {
    return true;
  }

  return row >= editableRows.start && row <= editableRows.end;
}

function filterTerminalMouseInput(data) {
  if (!data) {
    return "";
  }

  return data
    .replace(
      TERMINAL_SGR_MOUSE_SEQUENCE_PATTERN,
      (sequence, buttonCodeText, _colText, rowText) => {
        const buttonCode = Number(buttonCodeText);
        const row = Number(rowText) - 1;
        return shouldForwardTerminalMouseReport(row, buttonCode) ? sequence : "";
      }
    )
    .replace(TERMINAL_X10_MOUSE_SEQUENCE_PATTERN, (sequence, buttonChar, _colChar, rowChar) => {
      const buttonCode = buttonChar.charCodeAt(0) - 32;
      const row = rowChar.charCodeAt(0) - 33;
      return shouldForwardTerminalMouseReport(row, buttonCode) ? sequence : "";
    });
}
