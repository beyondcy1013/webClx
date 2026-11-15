(function attachTerminalTouchSelectionPolicy(root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
    return;
  }

  root.WebClxTerminalTouchSelectionPolicy = factory();
})(typeof globalThis !== "undefined" ? globalThis : this, function createTerminalTouchSelectionPolicy() {
  const TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS = 2000;
  const TERMINAL_TOUCH_SELECTION_DRAG_CANCEL_PX = 8;
  const TERMINAL_TOUCH_DOUBLE_TAP_MAX_DELAY_MS = 300;
  const TERMINAL_TOUCH_DOUBLE_TAP_MAX_DISTANCE_PX = 24;
  const TERMINAL_TOUCH_WORD_SEPARATORS = " ()[]{}',\"`";

  function finiteNumber(value, fallback = 0) {
    const number = Number(value);
    return Number.isFinite(number) ? number : fallback;
  }

  function normalizeLongPressMs(value) {
    return Math.round(
      Math.min(10000, Math.max(2000, finiteNumber(value, TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS)))
    );
  }

  function terminalTouchSelectionMoveAction(state) {
    const offsetX = finiteNumber(state?.offsetX);
    const offsetY = finiteNumber(state?.offsetY);
    const distance = Math.hypot(offsetX, offsetY);
    if (distance <= TERMINAL_TOUCH_SELECTION_DRAG_CANCEL_PX) {
      return "keep";
    }

    return "cancel";
  }

  function terminalTouchSelectionContextMenuAction(state) {
    const elapsedMs = finiteNumber(state?.elapsedMs);
    return elapsedMs >= normalizeLongPressMs(state?.longPressMs) ? "select" : "ignore";
  }

  function terminalTouchScrollStep(state) {
    const rowHeight = Math.max(finiteNumber(state?.rowHeight, 1), 1);
    const pixelsPerLine = Math.max(rowHeight, 8);
    const totalPixels =
      finiteNumber(state?.remainderPixels) + finiteNumber(state?.deltaPixels);
    const lines = Math.trunc(totalPixels / pixelsPerLine);
    return {
      lines,
      remainderPixels: totalPixels - lines * pixelsPerLine,
    };
  }

  function normalizeTouchSelectionCell(point, columns) {
    const cols = Math.max(Math.trunc(finiteNumber(columns, 1)), 1);
    return {
      column: Math.min(Math.max(Math.trunc(finiteNumber(point?.column)), 0), cols - 1),
      row: Math.max(Math.trunc(finiteNumber(point?.row)), 0),
    };
  }

  function terminalTouchSelectionInitialRange(point, columns) {
    const cell = normalizeTouchSelectionCell(point, columns);
    return {
      column: cell.column,
      row: cell.row,
      length: 1,
    };
  }

  function terminalTouchSelectionRangeBetweenCells(anchorPoint, focusPoint, columns) {
    const cols = Math.max(Math.trunc(finiteNumber(columns, 1)), 1);
    const anchor = normalizeTouchSelectionCell(anchorPoint, cols);
    const focus = normalizeTouchSelectionCell(focusPoint, cols);
    const anchorIndex = anchor.row * cols + anchor.column;
    const focusIndex = focus.row * cols + focus.column;
    const startIndex = Math.min(anchorIndex, focusIndex);
    const endIndex = Math.max(anchorIndex, focusIndex);

    return {
      column: startIndex % cols,
      row: Math.floor(startIndex / cols),
      length: endIndex - startIndex + 1,
    };
  }

  function terminalTouchSelectionIsDoubleTap(lastTap, currentTap, options = {}) {
    if (!lastTap || !currentTap) {
      return false;
    }

    const maxDelay = finiteNumber(
      options.maxDelayMs,
      TERMINAL_TOUCH_DOUBLE_TAP_MAX_DELAY_MS,
    );
    const maxDistance = finiteNumber(
      options.maxDistancePx,
      TERMINAL_TOUCH_DOUBLE_TAP_MAX_DISTANCE_PX,
    );

    const elapsed = finiteNumber(currentTap.time) - finiteNumber(lastTap.time);
    if (elapsed < 0 || elapsed > maxDelay) {
      return false;
    }

    const dx = finiteNumber(currentTap.clientX) - finiteNumber(lastTap.clientX);
    const dy = finiteNumber(currentTap.clientY) - finiteNumber(lastTap.clientY);
    const distance = Math.hypot(dx, dy);

    return distance <= maxDistance;
  }

  function terminalTouchSelectionWordRange(point, lineText, columns, wordSeparators = TERMINAL_TOUCH_WORD_SEPARATORS) {
    const cols = Math.max(Math.trunc(finiteNumber(columns, 1)), 1);
    const cell = normalizeTouchSelectionCell(point, cols);
    const text = typeof lineText === "string" ? lineText : "";
    const col = Math.min(cell.column, cols - 1);

    if (!text || col >= text.length) {
      return {
        column: col,
        row: cell.row,
        length: 1,
      };
    }

    const char = text.charAt(col);
    const separators = typeof wordSeparators === "string" ? wordSeparators : TERMINAL_TOUCH_WORD_SEPARATORS;

    if (/\s/.test(char)) {
      let start = col;
      while (start > 0 && /\s/.test(text.charAt(start - 1))) {
        start--;
      }
      let end = col;
      while (end + 1 < text.length && end + 1 < cols && /\s/.test(text.charAt(end + 1))) {
        end++;
      }
      return {
        column: start,
        row: cell.row,
        length: end - start + 1,
      };
    }

    if (separators.includes(char)) {
      return {
        column: col,
        row: cell.row,
        length: 1,
      };
    }

    let start = col;
    while (start > 0) {
      const prevChar = text.charAt(start - 1);
      if (separators.includes(prevChar) || /\s/.test(prevChar)) {
        break;
      }
      start--;
    }

    let end = col;
    while (end + 1 < text.length && end + 1 < cols) {
      const nextChar = text.charAt(end + 1);
      if (separators.includes(nextChar) || /\s/.test(nextChar)) {
        break;
      }
      end++;
    }

    return {
      column: start,
      row: cell.row,
      length: end - start + 1,
    };
  }

  return {
    TERMINAL_TOUCH_DOUBLE_TAP_MAX_DELAY_MS,
    TERMINAL_TOUCH_DOUBLE_TAP_MAX_DISTANCE_PX,
    TERMINAL_TOUCH_SELECTION_DRAG_CANCEL_PX,
    TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS,
    TERMINAL_TOUCH_WORD_SEPARATORS,
    terminalTouchScrollStep,
    terminalTouchSelectionContextMenuAction,
    terminalTouchSelectionInitialRange,
    terminalTouchSelectionIsDoubleTap,
    terminalTouchSelectionMoveAction,
    terminalTouchSelectionRangeBetweenCells,
    terminalTouchSelectionWordRange,
  };
});
