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

console.log("terminal copy feedback tests passed");
