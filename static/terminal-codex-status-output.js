(function attachCodexStatusOutput(root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.WebClxCodexStatusOutput = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this, function createCodexStatusOutput() {
  const CODEX_STATUS_HEADING_RE = />_\s+OpenAI Codex\s+\(v([^)]+)\)/;
  const TOP_BORDER_RE = /^\s*╭─+╮\s*$/;
  const BOTTOM_BORDER_RE = /^\s*╰─+╯\s*$/;
  const ANSI_SEQUENCE_RE = /\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))/g;
  const MAX_CANDIDATE_BYTES = 64 * 1024;
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  const TOP_BORDER_START_BYTES = encoder.encode("╭");
  const CARRIAGE_RETURN_BYTES = encoder.encode("\r");

  function stripTerminalAnsi(value) {
    return String(value || "").replace(ANSI_SEQUENCE_RE, "");
  }

  function compactStatusLeadingPrefix(value) {
    const prefix = String(value || "");
    if (stripTerminalAnsi(prefix).trim()) {
      return prefix;
    }
    return Array.from(prefix.matchAll(ANSI_SEQUENCE_RE), (match) => match[0]).join("");
  }

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

  function sliceToCells(value, maxCells) {
    const limit = Math.max(Math.trunc(Number(maxCells) || 0), 0);
    let width = 0;
    let result = "";
    for (const character of String(value || "")) {
      const characterWidth = codePointCellWidth(character.codePointAt(0), character);
      if (width + characterWidth > limit) {
        break;
      }
      result += character;
      width += characterWidth;
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

  function splitLinesWithOffsets(value) {
    const text = String(value || "");
    const lines = [];
    let start = 0;
    while (start < text.length) {
      const newlineIndex = text.indexOf("\n", start);
      const end = newlineIndex >= 0 ? newlineIndex + 1 : text.length;
      const bodyEnd =
        newlineIndex > start && text[newlineIndex - 1] === "\r"
          ? newlineIndex - 1
          : newlineIndex >= 0
            ? newlineIndex
            : text.length;
      lines.push({
        start,
        end,
        body: text.slice(start, bodyEnd),
        newline: newlineIndex >= 0 ? text.slice(bodyEnd, end) : "",
      });
      start = end;
    }
    return lines;
  }

  function findCodexStatusTextBlock(value) {
    const lines = splitLinesWithOffsets(value);
    for (let topIndex = 0; topIndex < lines.length; topIndex += 1) {
      const topPlain = stripTerminalAnsi(lines[topIndex].body);
      if (!TOP_BORDER_RE.test(topPlain)) {
        continue;
      }
      let headingIndex = topIndex + 1;
      while (headingIndex < Math.min(topIndex + 4, lines.length)) {
        if (CODEX_STATUS_HEADING_RE.test(stripTerminalAnsi(lines[headingIndex].body))) {
          break;
        }
        headingIndex += 1;
      }
      if (headingIndex >= Math.min(topIndex + 4, lines.length)) {
        continue;
      }
      for (let bottomIndex = headingIndex + 1; bottomIndex < lines.length; bottomIndex += 1) {
        if (!BOTTOM_BORDER_RE.test(stripTerminalAnsi(lines[bottomIndex].body))) {
          continue;
        }
        return {
          start: lines[topIndex].start,
          end: lines[bottomIndex].end,
          sourceLines: lines.slice(topIndex, bottomIndex + 1).map((line) => line.body),
          leading: lines[topIndex].body.slice(0, lines[topIndex].body.indexOf("╭")),
          trailing: lines[bottomIndex].body.slice(lines[bottomIndex].body.lastIndexOf("╯") + 1),
          lineBreak:
            lines.slice(topIndex, bottomIndex + 1).find((line) => line.newline)?.newline || "\n",
          newline: lines[bottomIndex].newline,
        };
      }
    }
    return null;
  }

  function parseSourceFields(lines) {
    const fields = new Map();
    for (const rawLine of lines || []) {
      const line = stripTerminalAnsi(rawLine)
        .replace(/^\s*│\s*/, "")
        .replace(/\s*│\s*$/, "");
      const match = line.match(/^([^:]+):\s+(.*?)\s*$/);
      if (match) {
        fields.set(match[1].trim().toLowerCase(), match[2].trim());
      }
    }
    return fields;
  }

  function titleCase(value) {
    const text = String(value || "").trim();
    return text ? `${text[0].toUpperCase()}${text.slice(1)}` : "";
  }

  function compactStatusItems(sourceLines) {
    const sourceFields = parseSourceFields(sourceLines);
    const headingLine = (sourceLines || []).find((line) =>
      CODEX_STATUS_HEADING_RE.test(stripTerminalAnsi(line)),
    );
    const headingMatch = stripTerminalAnsi(headingLine || "").match(CODEX_STATUS_HEADING_RE);
    const rawLimits = sourceFields.get("limits");
    const limits = rawLimits?.startsWith("not available for this")
      ? "not available for this account"
      : rawLimits;
    const collaborationMode = sourceFields.get("collaboration mode");
    return [
      { label: "", value: `>_ OpenAI Codex v${headingMatch?.[1] || ""}` },
      { label: "Model", value: sourceFields.get("model") },
      { label: "Provider", value: sourceFields.get("model provider") },
      { label: "Dir", value: sourceFields.get("directory"), commaSeparated: true },
      { label: "Access", value: sourceFields.get("permissions") },
      { label: "Mode", value: titleCase(collaborationMode) },
      { label: "Thread", value: sourceFields.get("thread name") },
      { label: "Session", value: sourceFields.get("session") },
      { label: "Forked", value: sourceFields.get("forked from") },
      { label: "Agents", value: sourceFields.get("agents.md"), commaSeparated: true },
      { label: "Tokens", value: sourceFields.get("token usage") },
      { label: "Context", value: sourceFields.get("context window") },
      { label: "Limits", value: limits },
    ].filter((item) => String(item.value || "").trim());
  }

  function statusItemRows(item) {
    const value = String(item.value || "").trim();
    const parts = item.commaSeparated
      ? value
          .split(",")
          .map((part) => part.trim())
          .filter(Boolean)
      : [];
    if (parts.length <= 1) {
      return [{ label: item.label, value }];
    }
    return [
      { label: item.label, value: `${parts[0]},` },
      { label: "", value: parts.slice(1).join(", ") },
    ];
  }

  function formatCompactCodexStatus(sourceLines) {
    const plainTop = stripTerminalAnsi(sourceLines?.[0] || "").trim();
    const sourceWidth = Math.max(stringCellWidth(plainTop), 8);
    const items = compactStatusItems(sourceLines);
    const title = items.find((item) => !item.label)?.value || "";
    const fields = items.filter((item) => item.label);
    const fieldRows = fields
      .flatMap((item) => statusItemRows(item))
      .map((row) => (row.label ? `${row.label}: ${row.value}` : row.value));
    const contentWidth = Math.max(
      stringCellWidth(title),
      ...fieldRows.map((row) => stringCellWidth(row)),
      6,
    );
    const width = Math.min(sourceWidth, contentWidth + 2);
    const innerWidth = width - 2;
    return [
      `╭${"─".repeat(width - 2)}╮`,
      `│${padToCells(title, innerWidth)}│`,
      `├${"─".repeat(innerWidth)}┤`,
      ...fieldRows.map((row) => `│${padToCells(row, innerWidth)}│`),
      `╰${"─".repeat(width - 2)}╯`,
    ];
  }

  function compactCodexStatusOutputText(value) {
    let remaining = String(value || "");
    let result = "";
    while (remaining) {
      const block = findCodexStatusTextBlock(remaining);
      if (!block) {
        result += remaining;
        break;
      }
      const compactLines = formatCompactCodexStatus(block.sourceLines);
      result += remaining.slice(0, block.start);
      result += `${compactStatusLeadingPrefix(block.leading)}${compactLines.join(block.lineBreak)}${block.trailing}${block.newline}`;
      remaining = remaining.slice(block.end);
    }
    return result;
  }

  function concatBytes(chunks) {
    const usable = chunks.filter((chunk) => chunk instanceof Uint8Array && chunk.length > 0);
    if (usable.length === 0) {
      return new Uint8Array();
    }
    if (usable.length === 1) {
      return usable[0];
    }
    const result = new Uint8Array(usable.reduce((total, chunk) => total + chunk.length, 0));
    let offset = 0;
    usable.forEach((chunk) => {
      result.set(chunk, offset);
      offset += chunk.length;
    });
    return result;
  }

  function findByteSequence(source, target, start = 0) {
    const limit = source.length - target.length;
    for (let index = Math.max(start, 0); index <= limit; index += 1) {
      let matches = true;
      for (let offset = 0; offset < target.length; offset += 1) {
        if (source[index + offset] !== target[offset]) {
          matches = false;
          break;
        }
      }
      if (matches) {
        return index;
      }
    }
    return -1;
  }

  function partialSequenceTailLength(source, target) {
    const maxLength = Math.min(source.length, target.length - 1);
    for (let length = maxLength; length > 0; length -= 1) {
      let matches = true;
      for (let offset = 0; offset < length; offset += 1) {
        if (source[source.length - length + offset] !== target[offset]) {
          matches = false;
          break;
        }
      }
      if (matches) {
        return length;
      }
    }
    return 0;
  }

  function candidateHasConfirmedHeading(value) {
    const lines = splitLinesWithOffsets(value);
    return lines
      .slice(1, 4)
      .some((line) => CODEX_STATUS_HEADING_RE.test(stripTerminalAnsi(line.body)));
  }

  function candidateCanStillBecomeStatus(value) {
    const lines = splitLinesWithOffsets(value);
    if (lines.length < 2 || !lines[0].newline) {
      return true;
    }
    if (candidateHasConfirmedHeading(value)) {
      return true;
    }
    const completedFollowingLines = lines.slice(1, 4).filter((line) => line.newline).length;
    return completedFollowingLines < 2;
  }

  function createCodexStatusOutputTransformer() {
    let pending = new Uint8Array();

    function process(bytes, final = false) {
      let buffer = concatBytes([pending, bytes]);
      pending = new Uint8Array();
      const emitted = [];

      while (buffer.length > 0) {
        const topIndex = findByteSequence(buffer, TOP_BORDER_START_BYTES);
        if (topIndex < 0) {
          if (final) {
            emitted.push(buffer);
            buffer = new Uint8Array();
            break;
          }
          const tailLength = partialSequenceTailLength(buffer, TOP_BORDER_START_BYTES);
          const emitLength = buffer.length - tailLength;
          if (emitLength > 0) {
            emitted.push(buffer.slice(0, emitLength));
          }
          pending = tailLength > 0 ? buffer.slice(emitLength) : new Uint8Array();
          break;
        }

        if (topIndex > 0) {
          emitted.push(buffer.slice(0, topIndex));
          buffer = buffer.slice(topIndex);
        }

        const candidateText = decoder.decode(buffer);
        const block = findCodexStatusTextBlock(candidateText);
        if (block?.start === 0) {
          const rawBlockText = candidateText.slice(0, block.end);
          const rawBlockLength = encoder.encode(rawBlockText).length;
          emitted.push(CARRIAGE_RETURN_BYTES);
          emitted.push(encoder.encode(compactCodexStatusOutputText(rawBlockText)));
          buffer = buffer.slice(rawBlockLength);
          continue;
        }

        if (final) {
          emitted.push(buffer);
          buffer = new Uint8Array();
          break;
        }

        if (
          buffer.length <= MAX_CANDIDATE_BYTES &&
          candidateCanStillBecomeStatus(candidateText)
        ) {
          pending = buffer;
          break;
        }

        emitted.push(buffer.slice(0, TOP_BORDER_START_BYTES.length));
        buffer = buffer.slice(TOP_BORDER_START_BYTES.length);
      }

      return concatBytes(emitted);
    }

    return {
      transform(bytes) {
        if (!(bytes instanceof Uint8Array) || bytes.length === 0) {
          return new Uint8Array();
        }
        return process(bytes, false);
      },
      flush() {
        return process(new Uint8Array(), true);
      },
      reset() {
        pending = new Uint8Array();
      },
    };
  }

  return {
    compactCodexStatusOutputText,
    createCodexStatusOutputTransformer,
    findCodexStatusTextBlock,
    formatCompactCodexStatus,
    stringCellWidth,
    stripTerminalAnsi,
  };
});
