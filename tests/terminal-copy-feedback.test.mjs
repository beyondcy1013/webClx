import assert from "node:assert/strict";
import fs from "node:fs";

const source = fs.readFileSync(
  new URL("../static/terminal-resume-agent.js", import.meta.url),
  "utf8",
);
const commonSource = fs.readFileSync(
  new URL("../static/terminal-common.js", import.meta.url),
  "utf8",
);
const terminalHtml = fs.readFileSync(
  new URL("../static/terminal.html", import.meta.url),
  "utf8",
);
const terminalSource = fs.readFileSync(
  new URL("../static/terminal.js", import.meta.url),
  "utf8",
);

assert.match(
  commonSource,
  /WebClxAndroid\?\.copyText[\s\S]*WebClxAndroid\.copyText\(text\) === true[\s\S]*return true/,
  "terminal copy actions should use the Android native clipboard bridge before browser fallbacks",
);

assert.match(
  commonSource,
  /const openDialogs = document\.querySelectorAll\("dialog\[open\]"\);[\s\S]*?const container = openDialogs\.length > 0 \? openDialogs\[openDialogs\.length - 1\] : document\.body;[\s\S]*?container\.appendChild\(textarea\);/,
  "clipboard fallback helper must be appended inside the active open dialog to prevent inert focus rejection during modal dialogs",
);

const functionBody = (name) =>
  source.match(new RegExp(`async function ${name}\\(\\) \\{([\\s\\S]*?)\\n\\}`))?.[1] || "";

const copyTerminalNameBody = functionBody("copyCurrentTerminalName");
assert.match(
  copyTerminalNameBody,
  /updateStatus\(`已复制终端名：\$\{terminalName\}`/,
  "copy-terminal-name feedback should show the complete copied terminal name",
);
assert.match(
  terminalHtml,
  /id="copy-current-terminal-name"[^>]*>\s*终端\s*<\/button>[\s\S]*id="session-switcher"/,
  "the terminal label before the session dropdown should be a copy button",
);
assert.match(
  terminalSource,
  /currentTerminalNameCopyButton\?\.addEventListener\("click", \(\) => \{[\s\S]*copyCurrentTerminalName\(\);/,
  "clicking the terminal label should copy the active terminal name",
);

const copyIdAndAskBody = functionBody("copyCurrentSessionIdAndAsk");
assert.match(
  copyIdAndAskBody,
  /已复制 Session 提问文字：\$\{detected\.resumeId\}/,
  "copy-ID-and-ask feedback should show the complete copied Session ID",
);
assert.doesNotMatch(
  copyIdAndAskBody,
  /shortResumeId\(/,
  "copy-ID-and-ask feedback must not abbreviate the copied Session ID",
);

const extractSessionBody = functionBody("extractCurrentAgentSessionId");
assert.match(
  extractSessionBody,
  /已提取并复制 Session ID：\$\{sessionId\}/,
  "advanced-copy feedback should show the complete copied Session ID",
);
assert.doesNotMatch(
  extractSessionBody,
  /shortResumeId\(/,
  "advanced-copy feedback must not abbreviate the copied Session ID",
);

assert.match(
  source,
  /function buildSessionQuestionPrompt\(sessionId[\s\S]*?调用codex db维护skill读取\$\{normalizedAgent\} session id为 \$\{id\}并接受指令：/,
  "buildSessionQuestionPrompt should interpolate normalized agent type before 'session id'",
);

// 提取并在微环境中执行 buildSessionQuestionPrompt 和 resolveSessionAgentType
const promptScript = new Function(
  "state",
  "activeTerminalContext",
  "activeSession",
  `${source}\nreturn { buildSessionQuestionPrompt, resolveSessionAgentType };`,
);
const { buildSessionQuestionPrompt, resolveSessionAgentType } = promptScript(
  { activeSessionId: "s-1", sessions: [] },
  null,
  () => null,
);

const testId = "01a1d546-1928-7291-b3ee-1c6a18b6c6cc";
assert.equal(
  buildSessionQuestionPrompt(testId, "codex"),
  `调用codex db维护skill读取codex session id为 ${testId}并接受指令：`,
);
assert.equal(
  buildSessionQuestionPrompt(testId, "agy"),
  `调用codex db维护skill读取agy session id为 ${testId}并接受指令：`,
);
assert.equal(
  buildSessionQuestionPrompt(testId, "claude"),
  `调用codex db维护skill读取claude session id为 ${testId}并接受指令：`,
);
assert.equal(
  buildSessionQuestionPrompt({ resumeId: testId, program: "agy" }),
  `调用codex db维护skill读取agy session id为 ${testId}并接受指令：`,
);

console.log("terminal copy feedback tests passed");
