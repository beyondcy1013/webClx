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
  const CDX_RESUME_COMMAND_PATTERN =
    /\bcdx\s+resume\s+[`'"]?([^\s`"'，。；；：:<>()[\]{}]+)[`'"]?/gi;
  const CDX_RESUME_INVOKE_PATTERN = /\bcdx\s+resume\b/gi;
  const CLAUDE_RESUME_COMMAND_PATTERN =
    /\bclaude\s+--resume\s+[`'"]?([^\s`"'，。；；：:<>()[\]{}]+)[`'"]?/gi;
  const CLAUDE_RESUME_INVOKE_PATTERN = /\bclaude\s+--resume\b/gi;
  const AGY_RESUME_INVOKE_PATTERN = /\bagy\s+--conversation(?:=|\s+)/gi;
  const CODEBUDDY_RESUME_INVOKE_PATTERN = /\bcodebuddy\s+--resume(?:=|\s+)/gi;
  const KIRO_RESUME_INVOKE_PATTERN = /\bkiro-cli\s+--resume-id(?:=|\s+)/gi;
  const GROK_RESUME_INVOKE_PATTERN = /\bgrok\s+--resume\s+/gi;
  const CLINE_RESUME_INVOKE_PATTERN = /\bcline\s+(?:--id|--taskId|-T)(?:=|\s+)/gi;
  const OPENCODE_RESUME_INVOKE_PATTERN = /\bopencode\s+(?:--session|-s)(?:=|\s+)/gi;
  const DEVIN_RESUME_INVOKE_PATTERN = /\bdevin[ \t]+(?:-r|--resume)[ \t]+/gi;
  const DEVIN_RESUME_SLUG_PATTERN = /^[A-Za-z0-9]+(?:-[A-Za-z0-9]+)+$/;
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
  const NEXT_RESUME_INVOKE_PATTERN =
    /\b(?:codex\s+resume|cdx\s+resume|claude\s+--resume|agy\s+--conversation|codebuddy\s+--resume|kiro-cli\s+--resume-id|grok\s+--resume|cline\s+(?:--id|--taskId|-T)|opencode\s+(?:--session|-s)|devin\s+(?:-r|--resume))\b/i;

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

  function isValidDevinResumeSlug(slug) {
    const value = String(slug || "");
    return value.length <= 160 && DEVIN_RESUME_SLUG_PATTERN.test(value);
  }

  function devinResumeSlugAfterInvoke(rawTail) {
    const match = String(rawTail || "").match(/^[`'"]?([^\s`"'，。；；：:<>()[\]{}]+)/);
    if (!match) {
      return "";
    }

    const slug = sanitizeResumeToken(match[1]);
    return isValidDevinResumeSlug(slug) ? slug : "";
  }

  function resumeCommandFromId(resumeId, program) {
    if (program === "agy") {
      return `agy --conversation=${resumeId}`;
    }
    if (program === "codebuddy") {
      return `codebuddy --resume=${resumeId}`;
    }
    if (program === "kiro") {
      return `kiro-cli --resume-id ${resumeId}`;
    }
    if (program === "grok") {
      return `grok --resume ${resumeId}`;
    }
    if (program === "cline") {
      return `cline --id ${resumeId}`;
    }
    if (program === "opencode") {
      return `opencode --session ${resumeId}`;
    }
    if (program === "claude") {
      return `claude --resume ${resumeId}`;
    }
    if (program === "devin") {
      return `devin -r ${resumeId}`;
    }
    if (program === "cdx") {
      return `cdx resume ${resumeId}`;
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

  // The invoke pattern already consumed the `=` or whitespace separator, so
  // the tail starts directly at the id token.
  function exactResumeTokenAfterInvoke(rawTail, { allowNonUuid = false } = {}) {
    const match = String(rawTail || "").match(/^[`'"]?([^\s`"'，。；；：:<>()[\]{}]+)[`'"]?/);
    if (!match) {
      return "";
    }
    const resumeId = sanitizeResumeToken(match[1]);
    if (allowNonUuid) {
      return CODEX_RESUME_ID_PATTERN.test(resumeId) ? resumeId : "";
    }
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
    let latestDevinId = "";
    let latestDevinIndex = -1;

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

    CDX_RESUME_INVOKE_PATTERN.lastIndex = 0;
    for (const match of text.matchAll(CDX_RESUME_INVOKE_PATTERN)) {
      const resumeId = resumeIdAfterCommand(text.slice(match.index + match[0].length));
      rememberCandidate(resumeId, "cdx", match.index);
    }

    CDX_RESUME_COMMAND_PATTERN.lastIndex = 0;
    for (const match of text.matchAll(CDX_RESUME_COMMAND_PATTERN)) {
      const resumeId = sanitizeResumeToken(match[1]);
      if (isValidResumeId(resumeId)) {
        rememberCandidate(resumeId, "cdx", match.index);
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

    AGY_RESUME_INVOKE_PATTERN.lastIndex = 0;
    for (const match of text.matchAll(AGY_RESUME_INVOKE_PATTERN)) {
      // AGY prints a UUID directly after the option; do not borrow IDs from later output.
      const resumeId = fragmentedUuidResumeIdAfterCommand(
        text.slice(match.index + match[0].length),
      );
      rememberCandidate(resumeId, "agy", match.index);
    }

    CODEBUDDY_RESUME_INVOKE_PATTERN.lastIndex = 0;
    for (const match of text.matchAll(CODEBUDDY_RESUME_INVOKE_PATTERN)) {
      // CodeBuddy prints a UUID directly after `--resume=` or `--resume `; the
      // pattern above already consumes the `=` separator, so the tail starts
      // at the id itself. Do not borrow IDs from later output.
      const resumeId = fragmentedUuidResumeIdAfterCommand(
        text.slice(match.index + match[0].length),
      );
      rememberCandidate(resumeId, "codebuddy", match.index);
    }

    KIRO_RESUME_INVOKE_PATTERN.lastIndex = 0;
    for (const match of text.matchAll(KIRO_RESUME_INVOKE_PATTERN)) {
      // Kiro accepts either `--resume-id=` or a space-separated UUID; rebuild
      // the command with the space-separated form.
      const resumeId = fragmentedUuidResumeIdAfterCommand(
        text.slice(match.index + match[0].length),
      );
      rememberCandidate(resumeId, "kiro", match.index);
    }

    GROK_RESUME_INVOKE_PATTERN.lastIndex = 0;
    for (const match of text.matchAll(GROK_RESUME_INVOKE_PATTERN)) {
      // Grok prints a UUID directly after `--resume`; do not borrow IDs from
      // later output.
      const resumeId = fragmentedUuidResumeIdAfterCommand(
        text.slice(match.index + match[0].length),
      );
      rememberCandidate(resumeId, "grok", match.index);
    }

    CLINE_RESUME_INVOKE_PATTERN.lastIndex = 0;
    for (const match of text.matchAll(CLINE_RESUME_INVOKE_PATTERN)) {
      // Cline task/session ids are not UUIDs; take the token right after the flag.
      const resumeId = exactResumeTokenAfterInvoke(
        text.slice(match.index + match[0].length),
        { allowNonUuid: true },
      );
      rememberCandidate(resumeId, "cline", match.index);
    }

    OPENCODE_RESUME_INVOKE_PATTERN.lastIndex = 0;
    for (const match of text.matchAll(OPENCODE_RESUME_INVOKE_PATTERN)) {
      // OpenCode session ids look like `ses_...`; take the token right after the flag.
      const resumeId = exactResumeTokenAfterInvoke(
        text.slice(match.index + match[0].length),
        { allowNonUuid: true },
      );
      rememberCandidate(resumeId, "opencode", match.index);
    }

    DEVIN_RESUME_INVOKE_PATTERN.lastIndex = 0;
    for (const match of text.matchAll(DEVIN_RESUME_INVOKE_PATTERN)) {
      const slug = devinResumeSlugAfterInvoke(
        text.slice(match.index + match[0].length),
      );
      if (slug && match.index > latestDevinIndex) {
        latestDevinId = slug;
        latestDevinIndex = match.index;
      }
    }

    if (latestDevinId && latestDevinIndex > latestUuidIndex) {
      if (
        (latestProgram === "cline" || latestProgram === "opencode") &&
        latestIndex > latestDevinIndex &&
        latestIndex > latestUuidIndex
      ) {
        return { id: latestId, program: latestProgram };
      }
      return { id: latestDevinId, program: "devin" };
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
