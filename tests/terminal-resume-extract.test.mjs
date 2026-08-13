import assert from "node:assert/strict";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const {
  extractLatestResumeCommand,
  extractLatestResumeInfo,
  extractLatestResumeId,
  parseResumeInputInfo,
  parseResumeIdInput,
} = require("../static/terminal-resume-extract.js");

const resumeId = "019d1ba6-f772-7452-a391-6553ccbc0a50";
const laterResumeId = "019d2091-73ef-7522-a073-e5a4b8195fe7";
const selectResumeId = "019f741e-6bb4-7a03-ac43-80226f0aaced";
const selectResumePrompt = [
  "Token usage: total=6,594,994 input=6,180,327 (+ 119,623,680 cached) output=414,667 (reasoning 160,001)",
  `To continue this session, run codex resume, then select 扩展字段基础上注册为dsl字段 (${selectResumeId})`,
].join("\n");
const interruptedSelectionResumeId = "019f8d03-c14d-7712-b5ac-2a63ebd7af36";
const interruptedSelectionPrompt = [
  "exceeded retry limit, last status: 429 Too Many",
  "Requests",
  "Token usage: total=52,954,470 input=52,452,152 (+",
  " 106,606,875 cached) output=502,318 (reasoning 16",
  "0,154)",
  "To continue this session, run codex resume, then",
  "select glm接着修 (019f8d03-c14d-7712-b5ac-2a63ebd",
  "7af36)",
  "[root@openeuler longzijue]# codex resume then",
  "bash: /home/root/.local/bin/codex: No such file or directory",
  "[root@openeuler longzijue]# codex resume then",
  "bash: /home/root/.local/bin/codex: No such file or directory",
  "[root@openeuler longzijue]#",
].join("\n");
const forkOutputResumeId = "019f971b-6e12-74e0-bb97-73293ed6d4c8";
const forkOutputWithMcpFailure = [
  "Token usage: total=298,446 input=269,724 (+ 3,912,960 cached) output=28,722 (reasoning 11,865)",
  `To continue this session, run codex resume ${forkOutputResumeId}`,
  "MCP client for `openchatcut` failed to start: MCP startup failed",
  "handshaking with MCP server failed: Send message error Transport",
  "HTTP request failed: http/request failed",
  "error sending request for url (http://localhost:5199/api/external-mcp/mcp)",
  "when send initialize request",
  "MCP startup incomplete (failed: openchatcut)",
  "",
  "Improve documentation in @filename",
].join("\n");

assert.equal(
  extractLatestResumeId(`To continue this session, run codex resume ${resumeId}`),
  resumeId,
  "single-line codex resume commands should still be detected",
);

assert.equal(
  extractLatestResumeCommand(forkOutputWithMcpFailure),
  `codex resume ${forkOutputResumeId}`,
  "fork output should remain extractable when MCP failures push the Session line upward",
);

assert.deepEqual(
  extractLatestResumeInfo(selectResumePrompt),
  { id: selectResumeId, program: "codex" },
  "Codex selection prompts should tolerate explanatory text between resume and the UUID",
);

assert.equal(
  extractLatestResumeCommand(selectResumePrompt),
  `codex resume ${selectResumeId}`,
  "Codex selection prompts should produce a directly runnable resume command",
);

assert.equal(
  parseResumeIdInput(selectResumePrompt),
  selectResumeId,
  "manual resume parsing should accept the complete Codex selection prompt",
);

assert.equal(
  extractLatestResumeCommand(interruptedSelectionPrompt),
  `codex resume ${interruptedSelectionResumeId}`,
  "later failed plain-token resume attempts must not override a canonical UUID selection prompt",
);

assert.equal(
  extractLatestResumeId(`To continue this session, run codex resume 019d1ba6-f772-7452-a391-\n6553ccbc0a50`),
  resumeId,
  "hard wrapping after a UUID hyphen should not truncate the resume id",
);

assert.equal(
  extractLatestResumeId(`To continue this session, run codex resume 019d1ba6-f772-7452-a391-65\n53ccbc0a50`),
  resumeId,
  "hard wrapping inside a UUID segment should be rejoined before extraction",
);

assert.equal(
  extractLatestResumeId(
    [
      `old: codex resume ${resumeId}`,
      "new: codex resume 019d2091-73ef-7522-a073-",
      "e5a4b8195fe7",
    ].join("\n"),
  ),
  laterResumeId,
  "the latest complete resume id should win even when it is hard-wrapped",
);

assert.equal(
  extractLatestResumeCommand(`codex resume 019d1ba6-f772-\n7452-a391-6553ccbc0a50`),
  `codex resume ${resumeId}`,
  "command extraction should send the rejoined resume id",
);

assert.equal(
  extractLatestResumeCommand(`To continue this session, run claude --resume ${resumeId}`),
  `claude --resume ${resumeId}`,
  "Claude resume prompts should keep the claude --resume command",
);

assert.equal(
  extractLatestResumeCommand(
    [
      "Resume this session with:",
      "claude --resume 9b5d6d8e-b5d3-49d7-9f62-22b366ca1c99",
      "[root@openeuler codes]# ^C",
      "[root@openeuler codes]# claude --resume 9b5d6d8e-b5d3-49d7-9f62-22b366c",
    ].join("\n"),
  ),
  "claude --resume 9b5d6d8e-b5d3-49d7-9f62-22b366ca1c99",
  "an incomplete latest UUID fragment should not override an earlier complete Claude resume command",
);

assert.equal(
  extractLatestResumeCommand(
    [
      "To continue this session, run codex resume 019ee6ec-7424-7963-91e8-97ff5ef250af",
      "[root@openeuler webClx]# codex resume 019ee6",
      "ERROR: No saved session found with ID 019ee6.",
    ].join("\n"),
  ),
  "codex resume 019ee6ec-7424-7963-91e8-97ff5ef250af",
  "a mistyped short UUID prefix should not override an earlier complete Codex resume command",
);

assert.deepEqual(
  parseResumeInputInfo(`To continue this session, run claude --resume ${resumeId}`),
  {
    id: resumeId,
    program: "claude",
    command: `claude --resume ${resumeId}`,
  },
  "manual parsing should preserve the Claude resume program",
);

assert.equal(
  extractLatestResumeCommand(
    [
      `old: claude --resume ${resumeId}`,
      `new: codex resume ${laterResumeId}`,
    ].join("\n"),
  ),
  `codex resume ${laterResumeId}`,
  "the latest resume command should win across Codex and Claude prompts",
);

assert.equal(
  parseResumeIdInput(`codex resume 019d1ba6-f772-\n7452-a391-6553ccbc0a50`),
  resumeId,
  "manual or archive input parsing should share the same hard-wrap handling",
);

assert.equal(
  extractLatestResumeId("codex resume local-test_id.1"),
  "local-test_id.1",
  "non-UUID resume tokens should remain supported when they are not fragmented",
);

// --- Banner session extraction -------------------------------------------
const bannerSessionId = "019f2350-db5f-7cf0-b476-1cf14855b05d";

const fullCodexBanner = [
  "╭──────────────────────────────────────────────────────────────────────────────────────────╮",
  "│  >_ OpenAI Codex (v0.142.5)                                                              │",
  "│                                                                                          │",
  "│  Model:                GLM-5.2 (reasoning high, summaries auto)                          │",
  "│  Model provider:       ZCode API GLM-5.2 - http://127.0.0.1:11111/api/upstream/openai/v1 │",
  "│  Directory:            /home/codes/newsKB                                                │",
  "│  Permissions:          Full Access                                                       │",
  "│  Agents.md:            /home/root/.codex/AGENTS.md, AGENTS.md                            │",
  "│  Collaboration mode:   Default                                                           │",
  `│  Session:              ${bannerSessionId}                              │`,
  "│                                                                                          │",
  "│  Token usage:          10.3M total  (10.2M input + 45.7K output)                         │",
  "│  Context window:       83% left (50.6K used / 243K)                                      │",
  "│  Limits:               not available for this account                                    │",
  "╰──────────────────────────────────────────────────────────────────────────────────────────╯",
].join("\n");

assert.equal(
  extractLatestResumeId(fullCodexBanner),
  bannerSessionId,
  "the Codex startup banner Session: line should be extracted when no resume command is present",
);

assert.equal(
  extractLatestResumeCommand(fullCodexBanner),
  `codex resume ${bannerSessionId}`,
  "the banner session id should produce a codex resume command",
);

assert.deepEqual(
  parseResumeInputInfo(fullCodexBanner),
  {
    id: bannerSessionId,
    program: "codex",
    command: `codex resume ${bannerSessionId}`,
  },
  "parseResumeInputInfo should expose the banner session id as a codex session",
);

assert.equal(
  extractLatestResumeId(`│  Session:    ${bannerSessionId}    │`),
  bannerSessionId,
  "a Session: label without a surrounding box should also be detected",
);

assert.equal(
  extractLatestResumeId(`Session: ${bannerSessionId}`),
  bannerSessionId,
  "a bare Session: line should be detected as a fallback",
);

assert.equal(
  extractLatestResumeId(`│Session  │ ${bannerSessionId} │`),
  bannerSessionId,
  "the compact native status table should remain a Session extraction source",
);

assert.equal(
  extractLatestResumeId(`session id: ${bannerSessionId}`),
  bannerSessionId,
  "a lowercase 'session id:' Claude-style label should map to the claude program",
);

assert.deepEqual(
  extractLatestResumeInfo(`session id: ${bannerSessionId}`),
  { id: bannerSessionId, program: "claude" },
  "a Claude banner session label should set the program to claude",
);

assert.equal(
  extractLatestResumeId(`│  Session:    ${bannerSessionId}    │\ncodex resume ${resumeId}`),
  resumeId,
  "an explicit codex resume command should still win over the banner session id",
);
