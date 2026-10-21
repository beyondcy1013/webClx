(function attachTerminalCursorGuard(root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
    return;
  }

  root.WebClxTerminalCursorGuard = factory();
})(typeof globalThis !== "undefined" ? globalThis : this, function createTerminalCursorGuard() {
  const INTERACTIVE_STATUS_LINE_PATTERN =
    /(?:\b(?:esc|ctrl\+?c|enter|shift\+?enter|tab|help|quit|exit|cancel|context)\b|[?？]|帮助|退出|取消|发送|回车|上下文)/i;
  const BUSY_STATUS_LINE_PATTERN =
    /(?:\b(?:working|thinking|loading|running|executing|generating|streaming)\b|工作中|思考中|执行中|生成中|加载中)/i;

  function finiteNumber(value, fallback = 0) {
    return Number.isFinite(value) ? value : fallback;
  }

  function lineText(lines, row) {
    const value = Array.isArray(lines) ? lines[row] : "";
    return typeof value === "string" ? value : "";
  }

  function rowListIncludes(rows, row) {
    return Array.isArray(rows) && rows.some((value) => Math.trunc(finiteNumber(value, -1)) === row);
  }

  function placeholderStartColumn(ranges, row) {
    if (!Array.isArray(ranges)) {
      return null;
    }

    let startColumn = null;
    for (const range of ranges) {
      const rangeRow = Math.trunc(finiteNumber(range?.row, -1));
      const start = Math.trunc(finiteNumber(range?.startColumn, -1));
      const end = Math.trunc(finiteNumber(range?.endColumn, -1));
      if (rangeRow !== row || start < 0 || end <= start) {
        continue;
      }
      startColumn = startColumn === null ? start : Math.min(startColumn, start);
    }

    return startColumn;
  }

  function isZeroWidthCodePoint(codePoint) {
    return (
      codePoint === 0x200d ||
      (codePoint >= 0xfe00 && codePoint <= 0xfe0f) ||
      (codePoint >= 0xe0100 && codePoint <= 0xe01ef)
    );
  }

  function isWideCodePoint(codePoint) {
    return (
      codePoint >= 0x1100 &&
      (codePoint <= 0x115f ||
        codePoint === 0x2329 ||
        codePoint === 0x232a ||
        (codePoint >= 0x2e80 && codePoint <= 0xa4cf && codePoint !== 0x303f) ||
        (codePoint >= 0xac00 && codePoint <= 0xd7a3) ||
        (codePoint >= 0xf900 && codePoint <= 0xfaff) ||
        (codePoint >= 0xfe10 && codePoint <= 0xfe19) ||
        (codePoint >= 0xfe30 && codePoint <= 0xfe6f) ||
        (codePoint >= 0xff00 && codePoint <= 0xff60) ||
        (codePoint >= 0xffe0 && codePoint <= 0xffe6) ||
        (codePoint >= 0x1f300 && codePoint <= 0x1f64f) ||
        (codePoint >= 0x1f900 && codePoint <= 0x1f9ff) ||
        (codePoint >= 0x20000 && codePoint <= 0x3fffd))
    );
  }

  function visibleLength(text) {
    const value = String(text || "").replace(/\s+$/g, "");
    let width = 0;
    for (const char of value) {
      const codePoint = char.codePointAt(0) || 0;
      if (
        codePoint === 0 ||
        codePoint < 32 ||
        (codePoint >= 0x7f && codePoint < 0xa0) ||
        isZeroWidthCodePoint(codePoint) ||
        /\p{Mark}/u.test(char)
      ) {
        continue;
      }
      width += isWideCodePoint(codePoint) ? 2 : 1;
    }
    return width;
  }

  function isBlank(text) {
    return visibleLength(text) === 0;
  }

  function isLikelyStatusLine(text) {
    const normalized = String(text || "").trim();
    return (
      normalized.length > 0 &&
      INTERACTIVE_STATUS_LINE_PATTERN.test(normalized) &&
      !BUSY_STATUS_LINE_PATTERN.test(normalized)
    );
  }

  function isLikelyCodexInputLine(text) {
    return /^\s*\u203a(?:\s|$)/u.test(String(text || ""));
  }

  // Shell prompts like `[root@host dir]#`, `user@host:~$`, `> `, `# ` should never
  // be mistaken for a Codex/Claude interactive status line.  When the cursor sits
  // on the bottom row and that row reads like a shell prompt, the corrected cursor
  // must not be drawn — otherwise a leftover `›` Codex input line two rows above
  // (visible while a TUI is exiting and the prompt has not yet fully overwritten
  // the viewport) can fool detection into toggling the cursor theme every frame.
  function isLikelyShellPrompt(text) {
    const value = String(text || "").trim();
    if (value.length === 0) {
      return false;
    }
    // `[user@host dir]#` optionally followed by a typed command, e.g.
    // `[root@openeuler stockInfo]#`, `[root@host dir]# ~`.
    if (/^\[[^\]]*\]\s*[#$]\s*/u.test(value)) {
      return true;
    }
    // `user@host:path$` / `user@host:path#` optionally followed by a command.
    if (/^[^\s@]+@[^\s:]+:[^\s]*[#$]\s*/u.test(value)) {
      return true;
    }
    // bare prompt suffixes such as `# `, `$ `, `> ` with little else
    if (/^[#$>]\s*$/u.test(value)) {
      return true;
    }
    return false;
  }

  function detectBottomStatusCursorCorrection(snapshot) {
    const rows = Math.max(Math.trunc(finiteNumber(snapshot?.rows, 0)), 0);
    const columns = Math.max(Math.trunc(finiteNumber(snapshot?.columns, 0)), 1);
    const cursorRow = Math.trunc(finiteNumber(snapshot?.cursorRow, -1));
    if (rows < 3 || cursorRow !== rows - 1) {
      return null;
    }

    const statusLine = lineText(snapshot?.lines, rows - 1);
    const separatorLine = lineText(snapshot?.lines, rows - 2);
    const inputLine = lineText(snapshot?.lines, rows - 3);
    const inputRow = rows - 3;
    if (
      !isLikelyStatusLine(statusLine) ||
      isLikelyShellPrompt(statusLine) ||
      !isBlank(separatorLine) ||
      !isLikelyCodexInputLine(inputLine)
    ) {
      return null;
    }

    if (rowListIncludes(snapshot?.applicationCursorRows, inputRow)) {
      return null;
    }

    const placeholderColumn = placeholderStartColumn(snapshot?.placeholderRanges, inputRow);
    const inputEndColumn =
      placeholderColumn === null ? visibleLength(inputLine) : Math.min(visibleLength(inputLine), placeholderColumn);

    return {
      row: inputRow,
      column: Math.min(inputEndColumn, columns - 1),
    };
  }

  function cursorCorrectionMarkerGeometry(target) {
    const cellWidth = Math.max(finiteNumber(target?.cellWidth, 0), 0);
    const cellHeight = Math.max(finiteNumber(target?.cellHeight, 0), 0);
    const column = Math.max(Math.trunc(finiteNumber(target?.column, 0)), 0);
    const row = Math.max(Math.trunc(finiteNumber(target?.row, 0)), 0);
    const width = Math.min(Math.max(Math.round(cellWidth * 0.14), 2), 4);

    return {
      width,
      height: cellHeight,
      x: column * cellWidth,
      y: row * cellHeight,
    };
  }

  return {
    cursorCorrectionMarkerGeometry,
    detectBottomStatusCursorCorrection,
    isLikelyStatusLine,
    isLikelyCodexInputLine,
    isLikelyShellPrompt,
  };
});
