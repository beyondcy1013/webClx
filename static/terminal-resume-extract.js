(function attachTerminalResumeExtract(root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
    return;
  }

  root.WebClxTerminalResumeExtract = factory();
})(typeof globalThis !== "undefined" ? globalThis : this, function createTerminalResumeExtract() {
  const CODEX_RESUME_COMMAND_PATTERN =
    /\bcodex\s+resume\s+[`'"]?([^\s`"'，。；；：:<>()[\]{}]+)[`'"]?/gi;
  const CODEX_RESUME_INVOKE_PATTERN = /\bcodex\s+resume\b/gi;
  const CLAUDE_RESUME_COMMAND_PATTERN =
    /\bclaude\s+--resume\s+[`'"]?([^\s`"'，。；；：:<>()[\]{}]+)[`'"]?/gi;
  const CLAUDE_RESUME_INVOKE_PATTERN = /\bclaude\s+--resume\b/gi;
  const CODEX_RESUME_ID_PATTERN = /^[A-Za-z0-9._-]{1,160}$/;
  const CODEX_RESUME_UUID_PATTERN =
    /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/;
  const CODEX_RESUME_UUID_PREFIX_PATTERN =
    /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}/;
  const CODEX_RESUME_UUID_HEX_PREFIX_FRAGMENT_PATTERN = /^[0-9a-fA-F]{6,31}$/;
  const CODEX_RESUME_UUID_FRAGMENT_PATTERN =
    /^[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{0,4}){1,3}(?:-[0-9a-fA-F]{0,12})?$/;
  const CODEX_RESUME_FRAGMENT_SCAN_LIMIT = 260;
  const CODEX_RESUME_FRAGMENT_WHITESPACE_LIMIT = 16;
  const RESUME_UUID_CONTEXT_SCAN_LIMIT = 1024;
  const NEXT_RESUME_INVOKE_PATTERN = /\b(?:codex\s+resume|claude\s+--resume)\b/i;

  // Codex/Claude startup banner prints the active session id in a labelled
  // box, e.g.:
  //   │  Session:              019f2350-db5f-7cf0-b476-1cf14855b05d              │
  // When no `codex resume <id>` / `claude --resume <id>` line is present we
  // still want to recover the id from that banner. The label is matched
  // case-insensitively and tolerates the surrounding box-drawing characters
  // and the large whitespace column.
  const BANNER_SESSION_LABEL_PATTERN =
    /(?:^|\n)[^\S\r\n]*[│|]?\s*Session(?:\s*:|\s*[│|])\s*([0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12})\s*[│|]?/g;
  const CLAUDE_BANNER_SESSION_LABEL_PATTERN =
    /(?:^|\n)[^\S\r\n]*[│|]?\s*session\s+id:\s*([0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12})\s*[│|]?/gi;

  function sanitizeResumeToken(rawValue) {
    return String(rawValue || "").replace(/^[`'"]+|[`'".,!?，。；：:;)\]}]+$/g, "");
  }

  function isValidResumeId(resumeId) {
    const value = String(resumeId || "");
    return CODEX_RESUME_ID_PATTERN.test(value) && !isIncompleteUuidFragment(value);
  }

  function isIncompleteUuidFragment(resumeId) {
    const value = String(resumeId || "");
    return (
      Boolean(value) &&
      !CODEX_RESUME_UUID_PATTERN.test(value) &&
      (CODEX_RESUME_UUID_FRAGMENT_PATTERN.test(value) ||
        CODEX_RESUME_UUID_HEX_PREFIX_FRAGMENT_PATTERN.test(value))
    );
  }

  function resumeCommandFromId(resumeId, program) {
    if (program === "claude") {
      return `claude --resume ${resumeId}`;
    }
    return `codex resume ${resumeId}`;
  }

  function exactResumeIdAfterCommand(rawTail) {
    const match = String(rawTail || "").match(/^\s+[`'"]?([^\s`"'，。；；：:<>()[\]{}]+)[`'"]?/);
    if (!match) {
      return "";
    }

    const resumeId = sanitizeResumeToken(match[1]);
    return isValidResumeId(resumeId) ? resumeId : "";
  }

  function compactResumeFragmentTail(rawTail) {
    const tail = String(rawTail || "").slice(0, CODEX_RESUME_FRAGMENT_SCAN_LIMIT);
    let compacted = "";
    let sawToken = false;
    let whitespaceRun = 0;

    for (const character of tail) {
      if (/^[A-Za-z0-9._-]$/.test(character)) {
        compacted += character;
        sawToken = true;
        whitespaceRun = 0;
        continue;
      }

      if (/^\s$/.test(character)) {
        if (!sawToken) {
          continue;
        }
        whitespaceRun += 1;
        if (whitespaceRun <= CODEX_RESUME_FRAGMENT_WHITESPACE_LIMIT) {
          continue;
        }
        break;
      }

      if (!sawToken && /[`'"]/.test(character)) {
        continue;
      }

      break;
    }

    return compacted;
  }

  function fragmentedUuidResumeIdAfterCommand(rawTail) {
    const compacted = compactResumeFragmentTail(rawTail);
    const match = compacted.match(CODEX_RESUME_UUID_PREFIX_PATTERN);
    return match ? match[0].toLowerCase() : "";
  }

  function contextualUuidResumeIdAfterCommand(rawTail) {
    let context = String(rawTail || "").slice(0, RESUME_UUID_CONTEXT_SCAN_LIMIT);
    const nextInvokeIndex = context.search(NEXT_RESUME_INVOKE_PATTERN);
    if (nextInvokeIndex >= 0) {
      context = context.slice(0, nextInvokeIndex);
    }

    for (const match of context.matchAll(/[0-9a-fA-F]{8}/g)) {
      const compacted = compactResumeFragmentTail(context.slice(match.index));
      const uuidMatch = compacted.match(CODEX_RESUME_UUID_PREFIX_PATTERN);
      if (uuidMatch) {
        return uuidMatch[0].toLowerCase();
      }
    }
    return "";
  }

  function resumeIdAfterCommand(rawTail) {
    return (
      fragmentedUuidResumeIdAfterCommand(rawTail) ||
      contextualUuidResumeIdAfterCommand(rawTail) ||
      exactResumeIdAfterCommand(rawTail)
    );
  }

  function extractLatestResumeInfoFromBanner(bufferText) {
    const text = String(bufferText || "");
    if (!text) {
      return { id: "", program: "codex", bannerIndex: -1 };
    }

    let latestId = "";
    let latestProgram = "codex";
    let latestIndex = -1;

    BANNER_SESSION_LABEL_PATTERN.lastIndex = 0;
    for (const match of text.matchAll(BANNER_SESSION_LABEL_PATTERN)) {
      const resumeId = match[1].toLowerCase();
      if (CODEX_RESUME_UUID_PATTERN.test(resumeId) && match.index > latestIndex) {
        latestId = resumeId;
        latestProgram = "codex";
        latestIndex = match.index;
      }
    }

    CLAUDE_BANNER_SESSION_LABEL_PATTERN.lastIndex = 0;
    for (const match of text.matchAll(CLAUDE_BANNER_SESSION_LABEL_PATTERN)) {
      const resumeId = match[1].toLowerCase();
      if (CODEX_RESUME_UUID_PATTERN.test(resumeId) && match.index > latestIndex) {
        latestId = resumeId;
        latestProgram = "claude";
        latestIndex = match.index;
      }
    }

    return { id: latestId, program: latestProgram, bannerIndex: latestIndex };
  }

  function extractLatestResumeInfo(bufferText) {
    const text = String(bufferText || "");
    if (!text) {
      return { id: "", program: "codex" };
    }

    let latestId = "";
    let latestProgram = "codex";
    let latestIndex = -1;
    let latestUuid = "";
    let latestUuidProgram = "codex";
    let latestUuidIndex = -1;

    const rememberCandidate = (resumeId, program, index) => {
      if (!resumeId) {
        return;
      }
      if (CODEX_RESUME_UUID_PATTERN.test(resumeId) && index > latestUuidIndex) {
        latestUuid = resumeId.toLowerCase();
        latestUuidProgram = program;
        latestUuidIndex = index;
      }
      if (index > latestIndex) {
        latestId = resumeId;
        latestProgram = program;
        latestIndex = index;
      }
    };

    CODEX_RESUME_INVOKE_PATTERN.lastIndex = 0;
    for (const match of text.matchAll(CODEX_RESUME_INVOKE_PATTERN)) {
      const resumeId = resumeIdAfterCommand(text.slice(match.index + match[0].length));
      rememberCandidate(resumeId, "codex", match.index);
    }

    CODEX_RESUME_COMMAND_PATTERN.lastIndex = 0;
    for (const match of text.matchAll(CODEX_RESUME_COMMAND_PATTERN)) {
      const resumeId = sanitizeResumeToken(match[1]);
      if (isValidResumeId(resumeId)) {
        rememberCandidate(resumeId, "codex", match.index);
      }
    }

    CLAUDE_RESUME_INVOKE_PATTERN.lastIndex = 0;
    for (const match of text.matchAll(CLAUDE_RESUME_INVOKE_PATTERN)) {
      const resumeId = resumeIdAfterCommand(text.slice(match.index + match[0].length));
      rememberCandidate(resumeId, "claude", match.index);
    }

    CLAUDE_RESUME_COMMAND_PATTERN.lastIndex = 0;
    for (const match of text.matchAll(CLAUDE_RESUME_COMMAND_PATTERN)) {
      const resumeId = sanitizeResumeToken(match[1]);
      if (isValidResumeId(resumeId)) {
        rememberCandidate(resumeId, "claude", match.index);
      }
    }

    if (latestUuid) {
      return { id: latestUuid, program: latestUuidProgram };
    }

    if (!latestId) {
      const banner = extractLatestResumeInfoFromBanner(text);
      if (banner.id) {
        return { id: banner.id, program: banner.program };
      }
    }

    return { id: latestId, program: latestProgram };
  }

  function extractLatestResumeId(bufferText) {
    return extractLatestResumeInfo(bufferText).id;
  }

  function parseResumeIdInput(rawValue) {
    return parseResumeInputInfo(rawValue).id;
  }

  function parseResumeInputInfo(rawValue) {
    const text = String(rawValue || "").trim();
    if (!text) {
      return { id: "", program: "codex", command: "" };
    }

    const commandInfo = extractLatestResumeInfo(text);
    if (commandInfo.id) {
      return {
        ...commandInfo,
        command: resumeCommandFromId(commandInfo.id, commandInfo.program),
      };
    }

    const resumeId = sanitizeResumeToken(text);
    if (!isValidResumeId(resumeId)) {
      return { id: "", program: "codex", command: "" };
    }

    return {
      id: resumeId,
      program: "codex",
      command: resumeCommandFromId(resumeId, "codex"),
    };
  }

  function extractLatestResumeCommand(bufferText) {
    const { id, program } = extractLatestResumeInfo(bufferText);
    return id ? resumeCommandFromId(id, program) : "";
  }

  return {
    BANNER_SESSION_LABEL_PATTERN,
    CLAUDE_BANNER_SESSION_LABEL_PATTERN,
    CLAUDE_RESUME_COMMAND_PATTERN,
    CLAUDE_RESUME_INVOKE_PATTERN,
    CODEX_RESUME_COMMAND_PATTERN,
    CODEX_RESUME_ID_PATTERN,
    CODEX_RESUME_UUID_PATTERN,
    extractLatestResumeCommand,
    extractLatestResumeId,
    extractLatestResumeInfo,
    isIncompleteUuidFragment,
    isValidResumeId,
    parseResumeIdInput,
    parseResumeInputInfo,
    resumeCommandFromId,
    sanitizeResumeToken,
  };
});
