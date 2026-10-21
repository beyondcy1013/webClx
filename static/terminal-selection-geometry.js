(function attachTerminalSelectionGeometry(root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
    return;
  }

  root.WebClxTerminalSelectionGeometry = factory();
})(typeof globalThis !== "undefined" ? globalThis : this, function createTerminalSelectionGeometry() {
  function finiteNumber(value, fallback = 0) {
    return Number.isFinite(value) ? value : fallback;
  }

  function clamp(value, min, max) {
    return Math.min(Math.max(value, min), max);
  }

  function clampTerminalSelectionPoint(point, columns, maxRow) {
    const maxColumn = Math.max(Math.trunc(finiteNumber(columns, 0)), 0);
    const lastRow = Math.max(Math.trunc(finiteNumber(maxRow, 0)), 0);
    return {
      column: clamp(Math.trunc(finiteNumber(point?.column, 0)), 0, maxColumn),
      row: clamp(Math.trunc(finiteNumber(point?.row, 0)), 0, lastRow),
    };
  }

  function compareTerminalSelectionPoints(a, b) {
    if (a.row !== b.row) {
      return a.row - b.row;
    }
    return a.column - b.column;
  }

  function terminalSelectionRangeFromPoints(anchorPoint, focusPoint, columns) {
    const cols = Math.max(Math.trunc(finiteNumber(columns, 0)), 1);
    let start = {
      column: Math.trunc(finiteNumber(anchorPoint?.column, 0)),
      row: Math.trunc(finiteNumber(anchorPoint?.row, 0)),
    };
    let end = {
      column: Math.trunc(finiteNumber(focusPoint?.column, 0)),
      row: Math.trunc(finiteNumber(focusPoint?.row, 0)),
    };

    if (compareTerminalSelectionPoints(start, end) > 0) {
      const nextStart = end;
      end = start;
      start = nextStart;
    }

    return {
      column: start.column,
      row: start.row,
      length: Math.max((end.row - start.row) * cols + (end.column - start.column), 0),
    };
  }

  function terminalSelectionPointFromClient(pointer, metrics) {
    const columns = Math.max(Math.trunc(finiteNumber(metrics?.columns, 0)), 1);
    const rows = Math.max(Math.trunc(finiteNumber(metrics?.rows, 0)), 1);
    const width = Math.max(finiteNumber(metrics?.width, 0), 1);
    const height = Math.max(finiteNumber(metrics?.height, 0), 1);
    const cellWidth = width / columns;
    const cellHeight = height / rows;
    const localX = finiteNumber(pointer?.clientX, 0) - finiteNumber(metrics?.left, 0);
    const localY = finiteNumber(pointer?.clientY, 0) - finiteNumber(metrics?.top, 0);
    const viewportY = Math.trunc(finiteNumber(metrics?.viewportY, 0));
    const maxRow = Math.max(Math.trunc(finiteNumber(metrics?.maxRow, viewportY + rows)), 0);

    return clampTerminalSelectionPoint(
      {
        column: Math.round(localX / cellWidth),
        row: viewportY + Math.floor(localY / cellHeight),
      },
      columns,
      maxRow,
    );
  }

  return {
    clampTerminalSelectionPoint,
    compareTerminalSelectionPoints,
    terminalSelectionRangeFromPoints,
    terminalSelectionPointFromClient,
  };
});
