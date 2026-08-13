(function attachTerminalTouchSelectionPolicy(root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
    return;
  }

  root.WebClxTerminalTouchSelectionPolicy = factory();
})(typeof globalThis !== "undefined" ? globalThis : this, function createTerminalTouchSelectionPolicy() {
  const TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS = 2000;
  const TERMINAL_TOUCH_SELECTION_DRAG_CANCEL_PX = 8;

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

  return {
    TERMINAL_TOUCH_SELECTION_DRAG_CANCEL_PX,
    TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS,
    terminalTouchScrollStep,
    terminalTouchSelectionContextMenuAction,
    terminalTouchSelectionInitialRange,
    terminalTouchSelectionMoveAction,
    terminalTouchSelectionRangeBetweenCells,
  };
});
