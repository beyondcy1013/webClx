import assert from "node:assert/strict";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const fs = require("node:fs");
const vm = require("node:vm");
const { fileURLToPath } = require("node:url");
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

// AGY screen prompts share the Ctrl+Alt+R extractor.
const agyId = "056e1735-a2a8-4a4b-ac05-86a77528e643";
const agyCommand = `agy --conversation=${agyId}`;
for (const prompt of [
  agyCommand,
  `Resume with: agy --conversation="${agyId}"`,
  `agy --conversation ${agyId}`,
  "agy --conversation=056e1735-a2a8-4a4b-ac05-86a7\n7528e643",
]) {
  assert.equal(extractLatestResumeCommand(prompt), agyCommand);
  assert.deepEqual(parseResumeInputInfo(prompt), {
    id: agyId, program: "agy", command: agyCommand,
  });
}
assert.equal(
  extractLatestResumeCommand(`codex resume ${resumeId}\n${agyCommand}`),
  agyCommand,
);
assert.equal(
  extractLatestResumeCommand(`${agyCommand}\nclaude --resume ${resumeId}`),
  `claude --resume ${resumeId}`,
);
assert.equal(
  extractLatestResumeCommand(`${agyCommand}\nagy --conversation=056e1735-a2a8-`),
  agyCommand,
);
assert.equal(extractLatestResumeCommand("agy --conversation=056e1735-a2a8-"), "");
assert.equal(
  extractLatestResumeCommand(`agy --conversation=\nclaude --resume ${resumeId}`),
  `claude --resume ${resumeId}`,
);
assert.equal(
  extractLatestResumeCommand(`${agyCommand}\ncodex resume then\nagy --conversation=`),
  agyCommand,
  "Codex contextual scanning must stop at an AGY invocation",
);

// CodeBuddy screen prompts share the Ctrl+Alt+R extractor (`codebuddy --resume=<id>`).
const codebuddyId = "01a0a60f-172b-7d7e-9b8f-c8aeb8508238";
const codebuddyCommand = `codebuddy --resume=${codebuddyId}`;
for (const prompt of [
  codebuddyCommand,
  `Resume with: codebuddy --resume="${codebuddyId}"`,
  `codebuddy --resume ${codebuddyId}`,
  "codebuddy --resume=01a0a60f-172b-7d7e-9b8f-c8aeb\n8508238",
]) {
  assert.equal(extractLatestResumeCommand(prompt), codebuddyCommand);
  assert.deepEqual(parseResumeInputInfo(prompt), {
    id: codebuddyId, program: "codebuddy", command: codebuddyCommand,
  });
}
assert.equal(
  extractLatestResumeCommand(`codex resume ${resumeId}\n${codebuddyCommand}`),
  codebuddyCommand,
);
assert.equal(
  extractLatestResumeCommand(`${codebuddyCommand}\nclaude --resume ${resumeId}`),
  `claude --resume ${resumeId}`,
);
assert.equal(
  extractLatestResumeCommand(`${codebuddyCommand}\ncodebuddy --resume=01a0a60f-172b-`),
  codebuddyCommand,
);
assert.equal(extractLatestResumeCommand("codebuddy --resume=01a0a60f-172b-"), "");
assert.equal(
  extractLatestResumeCommand(`codebuddy --resume=\nclaude --resume ${resumeId}`),
  `claude --resume ${resumeId}`,
);
assert.equal(
  extractLatestResumeCommand(`${agyCommand}\ncodex resume then\ncodebuddy --resume=`),
  agyCommand,
  "Codex contextual scanning must stop at a CodeBuddy invocation",
);

// Kiro screen prompts share the Ctrl+Alt+R extractor (`kiro-cli --resume-id=<id>`).
const kiroId = "da5ccebe-2b7f-48ae-a1f1-cac017923d4d";
const kiroCommand = `kiro-cli --resume-id ${kiroId}`;
for (const prompt of [
  kiroCommand,
  `Resume with: kiro-cli --resume-id="${kiroId}"`,
  `kiro-cli --resume-id=${kiroId}`,
  "kiro-cli --resume-id=da5ccebe-2b7f-48ae-a1f1-cac017\n923d4d",
]) {
  assert.equal(extractLatestResumeCommand(prompt), kiroCommand);
  assert.deepEqual(parseResumeInputInfo(prompt), {
    id: kiroId, program: "kiro", command: kiroCommand,
  });
}
assert.equal(
  extractLatestResumeCommand(`codex resume ${resumeId}\n${kiroCommand}`),
  kiroCommand,
);
assert.equal(
  extractLatestResumeCommand(`${kiroCommand}\nclaude --resume ${resumeId}`),
  `claude --resume ${resumeId}`,
);
assert.equal(
  extractLatestResumeCommand(`${kiroCommand}\nkiro-cli --resume-id=da5ccebe-2b7f-48ae-a1f1-cac017`),
  kiroCommand,
);
assert.equal(extractLatestResumeCommand("kiro-cli --resume-id=da5ccebe-"), "");
assert.equal(
  extractLatestResumeCommand(`kiro-cli --resume-id=\nclaude --resume ${resumeId}`),
  `claude --resume ${resumeId}`,
);
assert.equal(
  extractLatestResumeCommand(`${agyCommand}\ncodex resume then\nkiro-cli --resume-id=`),
  agyCommand,
  "Codex contextual scanning must stop at a Kiro invocation",
);

// Grok screen prompts share the Ctrl+Alt+R extractor (`grok --resume <id>`).
const grokId = "01a0c6e3-8c2c-7e92-b493-707061b15e48";
const grokCommand = `grok --resume ${grokId}`;
for (const prompt of [
  grokCommand,
  `Resume with: grok --resume "${grokId}"`,
  "grok --resume 01a0c6e3-8c2c-7e92-b493-70706\n1b15e48",
]) {
  assert.equal(extractLatestResumeCommand(prompt), grokCommand);
  assert.deepEqual(parseResumeInputInfo(prompt), {
    id: grokId, program: "grok", command: grokCommand,
  });
}
assert.equal(
  extractLatestResumeCommand(`codex resume ${resumeId}\n${grokCommand}`),
  grokCommand,
);
assert.equal(
  extractLatestResumeCommand(`${grokCommand}\nclaude --resume ${resumeId}`),
  `claude --resume ${resumeId}`,
);
assert.equal(
  extractLatestResumeCommand(`${grokCommand}\ngrok --resume 01a0c6e3-8c2c-7e92-b493-70706`),
  grokCommand,
);
assert.equal(extractLatestResumeCommand("grok --resume 01a0c6e3-"), "");
assert.equal(
  extractLatestResumeCommand(`grok --resume \nclaude --resume ${resumeId}`),
  `claude --resume ${resumeId}`,
);
assert.equal(
  extractLatestResumeCommand(`${agyCommand}\ncodex resume then\ngrok --resume `),
  agyCommand,
  "Codex contextual scanning must stop at a Grok invocation",
);

// Cline screen prompts share the Ctrl+Alt+R extractor (`cline --id <id>`);
// task ids are not UUIDs, so a short hex token must still be accepted.
const clineId = "abc123def";
const clineSessionId = "1759123456789abc";
const clineCommand = `cline --id ${clineSessionId}`;
for (const prompt of [
  clineCommand,
  `cline --id=${clineSessionId}`,
  `cline -T ${clineId}`,
  `cline --taskId=${clineId}`,
]) {
  assert.ok(extractLatestResumeCommand(prompt).startsWith("cline --id "), prompt);
}
assert.deepEqual(parseResumeInputInfo(clineCommand), {
  id: clineSessionId, program: "cline", command: clineCommand,
});
assert.deepEqual(parseResumeInputInfo(`cline -T ${clineId}`), {
  id: clineId, program: "cline", command: `cline --id ${clineId}`,
});
assert.equal(
  extractLatestResumeCommand(`${clineCommand}\ncodex resume ${resumeId}`),
  `codex resume ${resumeId}`,
  "a UUID candidate still wins over an earlier non-UUID cline id",
);

// OpenCode screen prompts share the Ctrl+Alt+R extractor (`opencode --session <id>`).
const opencodeId = "ses_3xJabcDEF012";
const opencodeCommand = `opencode --session ${opencodeId}`;
for (const prompt of [
  opencodeCommand,
  `opencode --session=${opencodeId}`,
  `opencode -s ${opencodeId}`,
]) {
  assert.equal(extractLatestResumeCommand(prompt), opencodeCommand, prompt);
}
assert.deepEqual(parseResumeInputInfo(opencodeCommand), {
  id: opencodeId, program: "opencode", command: opencodeCommand,
});
assert.equal(
  extractLatestResumeCommand(`${opencodeCommand}\nclaude --resume ${resumeId}`),
  `claude --resume ${resumeId}`,
);
assert.equal(extractLatestResumeCommand("opencode --session "), "");

// CDX screen prompts share the Ctrl+Alt+R extractor (`cdx resume <id>`).
const cdxId = "019f2350-db5f-7cf0-b476-1cf14855b05d";
const cdxCommand = `cdx resume ${cdxId}`;
assert.equal(extractLatestResumeCommand(cdxCommand), cdxCommand);
assert.deepEqual(parseResumeInputInfo(cdxCommand), {
  id: cdxId, program: "cdx", command: cdxCommand,
});
assert.equal(
  extractLatestResumeCommand(`${opencodeCommand}\n${cdxCommand}`),
  cdxCommand,
);

const devinSlug = "forest-fennel";
const devinCommand = `devin -r ${devinSlug}`;
const devinCompactOutput = [
  "❭ /compact",
  "",
  " ✱ Context compacted",
  "",
  ` devin -r ${devinSlug}`,
  " resumes this session · devin -r lists recent sessions",
].join("\n");

assert.equal(extractLatestResumeCommand(devinCompactOutput), devinCommand);
assert.deepEqual(extractLatestResumeInfo(devinCompactOutput), {
  id: devinSlug, program: "devin",
});
assert.deepEqual(parseResumeInputInfo(devinCompactOutput), {
  id: devinSlug, program: "devin", command: devinCommand,
});

for (const prompt of [
  "devin -r worried-plain",
  "devin --resume worried-plain",
  'Resume with: devin -r "worried-plain"',
  "devin -r `worried-plain`",
  "devin -r 'worried-plain'.",
]) {
  assert.equal(extractLatestResumeCommand(prompt), "devin -r worried-plain", prompt);
  assert.deepEqual(parseResumeInputInfo(prompt), {
    id: "worried-plain", program: "devin", command: "devin -r worried-plain",
  });
}

assert.equal(
  extractLatestResumeCommand(`codex resume ${resumeId}\n${devinCommand}`),
  devinCommand,
  "a newer Devin slug wins over an older Codex UUID",
);
assert.equal(
  extractLatestResumeCommand(`${devinCommand}\ncodex resume ${resumeId}`),
  `codex resume ${resumeId}`,
  "a newer Codex UUID still wins over an older Devin slug",
);
assert.equal(
  extractLatestResumeCommand(`${devinCommand}\n${clineCommand}`),
  clineCommand,
  "a newer Cline command still wins over an older Devin slug",
);
assert.equal(
  extractLatestResumeCommand(`${clineCommand}\n${devinCommand}`),
  devinCommand,
  "a newer Devin slug wins over an older Cline command",
);
assert.equal(
  extractLatestResumeCommand(`${devinCommand}\n${opencodeCommand}`),
  opencodeCommand,
  "a newer OpenCode command still wins over an older Devin slug",
);
assert.equal(
  extractLatestResumeCommand(`${opencodeCommand}\n${devinCommand}`),
  devinCommand,
  "a newer Devin slug wins over an older OpenCode command",
);
assert.equal(
  extractLatestResumeCommand("devin -r first-slug\ndevin --resume second-slug"),
  "devin -r second-slug",
  "the latest Devin slug wins",
);

for (const prompt of [
  "devin -r",
  "devin --resume",
  "devin -r ",
  "devin -r plainword",
  "devin -r forest-",
  "devin -r --verbose",
  "devin -r $(rm -rf /)",
  "devin -r\nworried-plain",
]) {
  assert.equal(extractLatestResumeCommand(prompt), "", prompt);
}

assert.equal(
  extractLatestResumeCommand(`${devinCommand}\ndevin -r`),
  devinCommand,
  "a trailing bare Devin picker must not replace the earlier valid slug",
);
assert.equal(
  extractLatestResumeCommand(`${devinCommand}\ncodex resume then\ndevin -r`),
  devinCommand,
  "later Codex prose must not mask an earlier Devin slug",
);
assert.equal(
  extractLatestResumeCommand(
    [devinCommand, "codex resume", "devin -r", laterResumeId].join("\n"),
  ),
  devinCommand,
  "Codex contextual scanning must stop at a Devin invocation",
);
assert.equal(
  parseResumeInputInfo("forest-fennel").command,
  "codex resume forest-fennel",
  "a raw unlabelled slug keeps the existing codex interpretation",
);

const sentResumeCommands = [];
const resumeAgentSandbox = vm.createContext({
  console,
  terminalResumeExtract: require("../static/terminal-resume-extract.js"),
  readTerminalBufferTailText: () => devinCompactOutput,
  ensureTerminalReadyForInput: () => true,
  mobileKeySendQueue: Promise.resolve(),
  sendTerminalAutoTypedInput: async (command) => {
    sentResumeCommands.push(command);
    return true;
  },
  updateStatus: () => {},
  focusTerminalAfterSoftKeyboardInput: () => {},
});
vm.runInContext(
  fs.readFileSync(
    fileURLToPath(new URL("../static/terminal-resume-agent.js", import.meta.url)),
    "utf8",
  ),
  resumeAgentSandbox,
);
resumeAgentSandbox.injectLatestResumeCommand();
await resumeAgentSandbox.mobileKeySendQueue;
assert.deepEqual(
  sentResumeCommands,
  [devinCommand],
  "the extract_resume action should auto-type the Devin resume command from the compact output",
);
