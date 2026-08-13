(function attachCodexStatusCompact(root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.WebClxCodexStatusCompact = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this, function createCodexStatusCompact() {
  const CODEX_STATUS_HEADING_RE = />_\s+OpenAI Codex\s+\(v([^)]+)\)/;
  const TOP_BORDER_RE = /^\s*╭─+╮\s*$/;
  const BOTTOM_BORDER_RE = /^\s*╰─+╯\s*$/;
  const MAX_SCAN_LINES = 240;

  function codePointCellWidth(codePoint, character) {
    if (codePoint === 0 || codePoint < 32 || (codePoint >= 0x7f && codePoint < 0xa0)) {
      return 0;
    }
    if (/\p{Mark}/u.test(character)) {
      return 0;
    }
    if (
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
        (codePoint >= 0x1f300 && codePoint <= 0x1faff) ||
        (codePoint >= 0x20000 && codePoint <= 0x3fffd))
    ) {
      return 2;
    }
    return 1;
  }

  function stringCellWidth(value) {
    let width = 0;
    for (const character of String(value || "")) {
      width += codePointCellWidth(character.codePointAt(0), character);
    }
    return width;
  }

  function shouldPreserveOverlayContent({
    pointerSelectionActive = false,
    hasActiveSelection = false,
  } = {}) {
    return Boolean(pointerSelectionActive || hasActiveSelection);
  }

  function sliceToCells(value, maxCells) {
    const limit = Math.max(Math.trunc(Number(maxCells) || 0), 0);
    let width = 0;
    let result = "";
    for (const character of String(value || "")) {
      const charWidth = codePointCellWidth(character.codePointAt(0), character);
      if (width + charWidth > limit) {
        break;
      }
      result += character;
      width += charWidth;
    }
    return result;
  }

  function truncateToCells(value, maxCells) {
    const text = String(value || "");
    const limit = Math.max(Math.trunc(Number(maxCells) || 0), 0);
    if (stringCellWidth(text) <= limit) {
      return text;
    }
    if (limit <= 0) {
      return "";
    }
    if (limit === 1) {
      return "…";
    }
    return `${sliceToCells(text, limit - 1)}…`;
  }

  function padToCells(value, cells) {
    const text = truncateToCells(value, cells);
    return `${text}${" ".repeat(Math.max(cells - stringCellWidth(text), 0))}`;
  }

  function findCodexStatusBlock(lines) {
    if (!Array.isArray(lines) || lines.length === 0) {
      return null;
    }
    for (let headingIndex = lines.length - 1; headingIndex >= 0; headingIndex -= 1) {
      const headingMatch = String(lines[headingIndex] || "").match(CODEX_STATUS_HEADING_RE);
      if (!headingMatch) {
        continue;
      }
      let start = headingIndex - 1;
      while (start >= 0 && !TOP_BORDER_RE.test(String(lines[start] || ""))) {
        start -= 1;
      }
      if (start < 0) {
        continue;
      }
      let end = headingIndex + 1;
      while (end < lines.length && !BOTTOM_BORDER_RE.test(String(lines[end] || ""))) {
        end += 1;
      }
      if (end >= lines.length) {
        continue;
      }
      return { start, end, version: headingMatch[1].trim() };
    }
    return null;
  }

  function parseSourceFields(lines) {
    const fields = new Map();
    for (const rawLine of lines || []) {
      const line = String(rawLine || "")
        .replace(/^\s*│\s*/, "")
        .replace(/\s*│\s*$/, "");
      const match = line.match(/^([^:]+):\s+(.*?)\s*$/);
      if (match) {
        fields.set(match[1].trim().toLowerCase(), match[2].trim());
      }
    }
    return fields;
  }

  function formatTokenCount(value, { fixedMillions = false } = {}) {
    const number = Number(value);
    if (!Number.isFinite(number) || number < 0) {
      return "";
    }
    if (number >= 1_000_000) {
      const formatted = (number / 1_000_000).toFixed(2);
      return `${fixedMillions ? formatted : formatted.replace(/\.00$/, "")}M`;
    }
    if (number >= 1_000) {
      return `${(number / 1_000).toFixed(1).replace(/\.0$/, "")}K`;
    }
    return String(Math.round(number));
  }

  function titleCase(value) {
    const text = String(value || "").trim();
    return text ? `${text[0].toUpperCase()}${text.slice(1)}` : "";
  }

  function compactStatusItems(status, session, sourceFields, version) {
    const tokenUsage = status?.token_usage || {};
    const contextWindow = status?.context_window || {};
    const modelParts = [status?.model, status?.reasoning_effort, status?.summary_mode].filter(Boolean);
    const permission = status?.permission || sourceFields.get("permissions");
    const collaborationMode =
      status?.collaboration_mode || sourceFields.get("collaboration mode");
    const provider = String(session?.codex_api_preset_name || "").trim();
    const providerUrl = String(session?.codex_api_base_url || "").trim();
    const agents = Array.isArray(status?.agents_md) ? status.agents_md.filter(Boolean).join(", ") : "";
    const rawSourceLimits = sourceFields.get("limits");
    const sourceLimits = rawSourceLimits?.startsWith("not available for this")
      ? "not available for this account"
      : rawSourceLimits;
    const sourceForkedFrom = sourceFields.get("forked from");
    const fallbackForkedFrom =
      sourceForkedFrom && /^[0-9a-f-]+$/i.test(sourceForkedFrom) && sourceForkedFrom.length < 36
        ? `${sourceForkedFrom}…`
        : sourceForkedFrom;
    const items = [
      { label: "", value: `>_ OpenAI Codex v${status?.version || version || ""}` },
      { label: "Model", value: modelParts.join(" | ") },
      { label: "Provider", value: provider || sourceFields.get("model provider") },
      { label: "URL", value: providerUrl },
      { label: "Dir", value: status?.cwd || sourceFields.get("directory") },
      {
        label: "Access",
        value: [permission, collaborationMode ? `Mode: ${titleCase(collaborationMode)}` : ""]
          .filter(Boolean)
          .join(" | "),
      },
      { label: "Thread", value: status?.thread_name || sourceFields.get("thread name") },
      { label: "Session", value: status?.session_id || sourceFields.get("session") },
      { label: "Forked", value: status?.forked_from || fallbackForkedFrom },
      {
        label: "Agents",
        value: agents || sourceFields.get("agents.md"),
        commaSeparated: true,
      },
      {
        label: "Tokens",
        value:
          tokenUsage.total_tokens != null
            ? [
                formatTokenCount(tokenUsage.total_tokens),
                `in ${formatTokenCount(tokenUsage.input_tokens)}`,
                `out ${formatTokenCount(tokenUsage.output_tokens)}`,
              ].join(" | ")
            : sourceFields.get("token usage"),
      },
      {
        label: "Context",
        value:
          contextWindow.total_tokens != null
            ? [
                contextWindow.percent_left != null ? `${contextWindow.percent_left}% left` : "",
                `${formatTokenCount(contextWindow.used_tokens)} / ${formatTokenCount(
                  contextWindow.total_tokens,
                  { fixedMillions: true },
                )}`,
              ]
                .filter(Boolean)
                .join(" | ")
            : sourceFields.get("context window"),
      },
      { label: "Limits", value: status?.limits || sourceLimits },
    ];
    return items.filter((item) => String(item.value || "").trim());
  }

  function statusItemRows(item) {
    const value = String(item.value || "").trim();
    const commaSeparatedValues = item.commaSeparated
      ? value
          .split(",")
          .map((part) => part.trim())
          .filter(Boolean)
      : [];
    if (commaSeparatedValues.length > 1) {
      return [
        { label: item.label, value: commaSeparatedValues[0] },
        { label: "", value: commaSeparatedValues.slice(1).join(", ") },
      ];
    }
    return [{ label: item.label, value }];
  }

  function formatCompactCodexStatus({
    status = {},
    session = {},
    sourceLines = [],
    columns,
    targetRows,
  } = {}) {
    const width = Math.max(Math.trunc(Number(columns) || 0), 8);
    const rows = Math.max(Math.trunc(Number(targetRows) || 0), 4);
    const innerWidth = width - 2;
    const sourceFields = parseSourceFields(sourceLines);
    const headingLine = (sourceLines || []).find((line) => CODEX_STATUS_HEADING_RE.test(String(line)));
    const headingMatch = String(headingLine || "").match(CODEX_STATUS_HEADING_RE);
    const items = compactStatusItems(status, session, sourceFields, headingMatch?.[1]);
    const title = items.find((item) => !item.label)?.value || "";
    const fieldItems = items.filter((item) => item.label);
    const availableCellWidth = width - 3;
    const desiredKeyCellWidth =
      Math.max(...fieldItems.map((item) => stringCellWidth(item.label)), 1) + 1;
    const reservedValueCellWidth = Math.min(8, Math.max(Math.floor(availableCellWidth / 2), 1));
    const keyCellWidth = Math.min(
      desiredKeyCellWidth,
      Math.max(availableCellWidth - reservedValueCellWidth, 1),
    );
    const valueCellWidth = availableCellWidth - keyCellWidth;
    const fieldRows = [];
    const maxFieldRows = rows - 4;
    for (const item of fieldItems) {
      const itemRows = statusItemRows(item);
      if (fieldRows.length + itemRows.length > maxFieldRows) {
        continue;
      }
      fieldRows.push(...itemRows);
    }
    while (fieldRows.length < maxFieldRows) {
      fieldRows.push({ label: "", value: "" });
    }
    const framed = fieldRows.map(
      (row) =>
        `│${padToCells(row.label, keyCellWidth)}│${padToCells(` ${row.value}`, valueCellWidth)}│`,
    );
    return [
      `╭${"─".repeat(width - 2)}╮`,
      `│${padToCells(` ${title}`, innerWidth)}│`,
      `├${"─".repeat(keyCellWidth)}┬${"─".repeat(valueCellWidth)}┤`,
      ...framed,
      `╰${"─".repeat(width - 2)}╯`,
    ];
  }

  function terminalCellDimensions(term) {
    const dimensions = term?._core?._renderService?.dimensions;
    const width = Number(dimensions?.actualCellWidth || dimensions?.css?.cell?.width || 0);
    const height = Number(dimensions?.actualCellHeight || dimensions?.css?.cell?.height || 0);
    return {
      width: Number.isFinite(width) && width > 0 ? width : 0,
      height: Number.isFinite(height) && height > 0 ? height : 0,
    };
  }

  function readRecentTerminalLines(term) {
    const buffer = term?.buffer?.active;
    if (!buffer) {
      return null;
    }
    const end = buffer.length;
    const start = Math.max(end - MAX_SCAN_LINES, 0);
    const lines = [];
    for (let index = start; index < end; index += 1) {
      lines.push(buffer.getLine(index)?.translateToString(true) || "");
    }
    return { buffer, start, lines };
  }

  function createOverlay(screen, onPointerSelectionStart) {
    const overlay = document.createElement("div");
    overlay.className = "terminal-codex-status-compact-overlay";
    overlay.setAttribute("role", "region");
    overlay.setAttribute("aria-label", "Codex status");
    overlay.tabIndex = -1;
    Object.assign(overlay.style, {
      position: "absolute",
      zIndex: "8",
      display: "none",
      overflow: "hidden",
      pointerEvents: "auto",
      whiteSpace: "pre",
      userSelect: "text",
      webkitUserSelect: "text",
      cursor: "text",
      color: "var(--terminal-fg)",
      background: "var(--terminal-bg)",
    });
    overlay.addEventListener("pointerdown", (event) => {
      if (event.button === 0) {
        onPointerSelectionStart?.();
      }
      event.stopPropagation();
      overlay.focus({ preventScroll: true });
    });
    overlay.addEventListener("mousedown", (event) => {
      event.stopPropagation();
    });
    overlay.addEventListener("click", (event) => {
      event.stopPropagation();
    });
    screen.appendChild(overlay);
    return overlay;
  }

  function overlayHasActiveSelection(overlay) {
    const selection = document.getSelection();
    return Boolean(
      selection &&
        !selection.isCollapsed &&
        selection.rangeCount > 0 &&
        overlay.contains(selection.anchorNode) &&
        overlay.contains(selection.focusNode),
    );
  }

  function createTerminalCodexStatusCompactor({
    term,
    sessionId,
    getSessionId,
    getSession,
    requestJson,
    isActive,
  }) {
    let disposed = false;
    let timer = null;
    let overlay = null;
    let currentBlock = null;
    let currentStatus = null;
    let requestKey = "";
    let pointerSelectionActive = false;

    function removeOverlay() {
      overlay?.remove();
      overlay = null;
      currentBlock = null;
    }

    function renderOverlay() {
      if (disposed || !currentBlock) {
        return;
      }
      if (typeof isActive === "function" && !isActive()) {
        if (overlay) {
          overlay.style.display = "none";
        }
        return;
      }
      if (overlay && shouldPreserveOverlayContent({
        pointerSelectionActive,
        hasActiveSelection: overlayHasActiveSelection(overlay),
      })) {
        return;
      }
      const screen = term?.element?.querySelector(".xterm-screen");
      const recent = readRecentTerminalLines(term);
      const dimensions = terminalCellDimensions(term);
      if (!(screen instanceof HTMLElement) || !recent || !dimensions.width || !dimensions.height) {
        removeOverlay();
        return;
      }
      if (!overlay || overlay.parentElement !== screen) {
        overlay = createOverlay(screen, () => {
          pointerSelectionActive = true;
        });
      }
      const top = (currentBlock.absoluteStart - recent.buffer.viewportY) * dimensions.height;
      const height = currentBlock.rows * dimensions.height;
      if (top + height <= 0 || top >= term.rows * dimensions.height) {
        overlay.style.display = "none";
        return;
      }
      const session = typeof getSession === "function" ? getSession() || {} : {};
      const lines = formatCompactCodexStatus({
        status: currentStatus || {},
        session,
        sourceLines: currentBlock.sourceLines,
        columns: currentBlock.columns,
        targetRows: currentBlock.rows,
      });
      if (!shouldPreserveOverlayContent({
        pointerSelectionActive,
        hasActiveSelection: overlayHasActiveSelection(overlay),
      })) {
        overlay.replaceChildren();
        lines.forEach((line, lineIndex) => {
          const characters = Array.from(line);
          const rightBorder = characters.pop() || "";
          const row = document.createElement("div");
          row.className = "terminal-codex-status-compact-row";
          Object.assign(row.style, {
            position: "relative",
            height: `${dimensions.height}px`,
            whiteSpace: "pre",
          });
          const content = document.createElement("span");
          content.textContent = characters.join("");
          const boundary = document.createElement("span");
          boundary.className = "terminal-codex-status-compact-right-border";
          boundary.textContent = rightBorder;
          Object.assign(boundary.style, {
            position: "absolute",
            left: `${(currentBlock.columns - 1) * dimensions.width}px`,
            top: "0",
          });
          if (lineIndex >= 4 && lineIndex < lines.length - 1) {
            const horizontalRule = document.createElement("span");
            horizontalRule.className = "terminal-codex-status-compact-horizontal-rule";
            Object.assign(horizontalRule.style, {
              position: "absolute",
              left: "0",
              right: "0",
              top: "0",
              height: "1px",
              background: "currentColor",
              opacity: "0.45",
              pointerEvents: "none",
            });
            row.append(horizontalRule);
          }
          row.append(content, boundary);
          overlay.appendChild(row);
        });
      }
      Object.assign(overlay.style, {
        display: "block",
        left: `${currentBlock.leftCells * dimensions.width}px`,
        top: `${top}px`,
        width: `${currentBlock.columns * dimensions.width}px`,
        height: `${height}px`,
        fontFamily: term.options.fontFamily,
        fontSize: `${term.options.fontSize}px`,
        lineHeight: `${dimensions.height}px`,
      });
    }

    const endPointerSelection = () => {
      if (!pointerSelectionActive) {
        return;
      }
      pointerSelectionActive = false;
      schedule();
    };

    async function refreshStatus(blockKey) {
      const resolvedSessionId = String(
        (typeof getSessionId === "function" ? getSessionId() : sessionId) || "",
      ).trim();
      const resolvedRequestKey = `${resolvedSessionId}:${blockKey}`;
      if (
        typeof requestJson !== "function" ||
        !resolvedSessionId ||
        requestKey === resolvedRequestKey
      ) {
        return;
      }
      requestKey = resolvedRequestKey;
      try {
        const response = await requestJson(
          `/api/terminal/sessions/${encodeURIComponent(resolvedSessionId)}/agent-session`,
        );
        if (!disposed && requestKey === resolvedRequestKey) {
          currentStatus = response?.codex_status || null;
          renderOverlay();
        }
      } catch {
        // The source rows still get compacted when structured status is unavailable.
      }
    }

    function scan() {
      timer = null;
      if (disposed) {
        return;
      }
      if (typeof isActive === "function" && !isActive()) {
        if (overlay) {
          overlay.style.display = "none";
        }
        return;
      }
      if (overlay && shouldPreserveOverlayContent({
        pointerSelectionActive,
        hasActiveSelection: overlayHasActiveSelection(overlay),
      })) {
        return;
      }
      const recent = readRecentTerminalLines(term);
      if (!recent) {
        removeOverlay();
        return;
      }
      const found = findCodexStatusBlock(recent.lines);
      if (!found) {
        removeOverlay();
        return;
      }
      const sourceLines = recent.lines.slice(found.start, found.end + 1);
      const topLine = sourceLines[0] || "";
      const leftText = topLine.slice(0, Math.max(topLine.indexOf("╭"), 0));
      const columns = Math.max(stringCellWidth(topLine.trim()), 8);
      const absoluteStart = recent.start + found.start;
      const blockKey = `${absoluteStart}:${found.end - found.start + 1}:${columns}:${found.version}`;
      if (currentBlock?.key !== blockKey) {
        currentStatus = null;
        currentBlock = {
          key: blockKey,
          absoluteStart,
          rows: found.end - found.start + 1,
          columns,
          leftCells: stringCellWidth(leftText),
          sourceLines,
        };
      }
      renderOverlay();
      void refreshStatus(blockKey);
    }

    function schedule() {
      if (disposed || timer !== null) {
        return;
      }
      timer = window.setTimeout(scan, 40);
    }

    const disposables = [];
    const handleSelectionChange = () => {
      if (overlay && !overlayHasActiveSelection(overlay)) {
        schedule();
      }
    };
    document.addEventListener("selectionchange", handleSelectionChange);
    disposables.push({
      dispose() {
        document.removeEventListener("selectionchange", handleSelectionChange);
      },
    });
    window.addEventListener("pointerup", endPointerSelection, true);
    window.addEventListener("pointercancel", endPointerSelection, true);
    disposables.push({
      dispose() {
        window.removeEventListener("pointerup", endPointerSelection, true);
        window.removeEventListener("pointercancel", endPointerSelection, true);
      },
    });
    if (typeof term?.onRender === "function") {
      disposables.push(term.onRender(schedule));
    }
    if (typeof term?.onScroll === "function") {
      disposables.push(term.onScroll(() => {
        renderOverlay();
        schedule();
      }));
    }
    if (typeof term?.onResize === "function") {
      disposables.push(term.onResize(schedule));
    }
    schedule();

    return {
      refresh: schedule,
      dispose() {
        disposed = true;
        if (timer !== null) {
          window.clearTimeout(timer);
          timer = null;
        }
        disposables.forEach((disposable) => disposable?.dispose?.());
        pointerSelectionActive = false;
        removeOverlay();
      },
    };
  }

  return {
    findCodexStatusBlock,
    formatCompactCodexStatus,
    shouldPreserveOverlayContent,
    stringCellWidth,
    createTerminalCodexStatusCompactor,
  };
});
