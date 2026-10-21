import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const {
  TERMINAL_REPORT_SEQUENCE_PATTERN,
  isTerminalReportSequence,
} = require("../static/terminal-report-filter.js");
const terminalJs = readFileSync(new URL("../static/terminal.js", import.meta.url), "utf8");
const terminalInputTransportJs = readFileSync(
  new URL("../static/terminal-input-transport.js", import.meta.url),
  "utf8",
);
const terminalHtml = readFileSync(
  new URL("../static/terminal.html", import.meta.url),
  "utf8",
);

assert.ok(TERMINAL_REPORT_SEQUENCE_PATTERN instanceof RegExp);
assert.ok(TERMINAL_REPORT_SEQUENCE_PATTERN.global, "report pattern must be global so replace strips every sequence");

const reportSequences = [
  "\u001b[O",
  "\u001b[I",
  "\u001b]10;rgb:d9d9/e9e9/e3e3\u001b\\",
  "\u001b]11;rgb:0808/1010/0f0f\u0007",
  "\u001b[?1;2c",
  "\u001b[>0;276;0c",
  "\u001b[0n",
  "\u001b[12;34R",
  "\u001b[?2026;2$y",
  "\u001b[8;45;202t",
  "\u001bP1$r0 q\u001b\\",
  "\u001b[O\u001b]11;rgb:0808/1010/0f0f\u001b\\",
];
for (const data of reportSequences) {
  assert.equal(
    isTerminalReportSequence(data),
    true,
    `xterm report sequence should be filtered: ${JSON.stringify(data)}`,
  );
}

const userInputs = [
  "",
  "a",
  "\r",
  "\u0003",
  "\u001b[A",
  "\u001b[1;5C",
  "\u001bOP",
  "\u001b[15~",
  "\u001b[200~text\u001b[201~",
  "\u001bc",
  "\u001b",
  "\u001b]",
  "\u001b[97;5u",
  "\u001b[<0;10;20M",
  "abc\u001b[O",
];
for (const data of userInputs) {
  assert.equal(
    isTerminalReportSequence(data),
    false,
    `real user input must not be filtered: ${JSON.stringify(data)}`,
  );
}

assert.match(
  terminalJs,
  /terminalReportFilter\.isTerminalReportSequence\(filtered\)[\s\S]*terminalInitialReplaySettled\(context\)[\s\S]*sendTerminalInput\(filtered, \{ terminalReport: true, flush: true \}\);[\s\S]*return;/,
  "terminal onData should route pure report replies through a non-interrupting send after the initial replay settles",
);
assert.match(
  terminalJs,
  /const terminalReportFilter = globalThis\.WebClxTerminalReportFilter \|\| \{[\s\S]*isTerminalReportSequence: \(\) => false/,
  "terminal.js should tolerate the report filter asset being unavailable",
);
assert.match(
  terminalInputTransportJs,
  /if \(!options\.terminalReport\) \{[\s\S]*interruptTerminalBacklogReplayForInput\(\);[\s\S]*refreshTerminalInputVisibilityAfterUserInput\(\);[\s\S]*\}/,
  "report replies must not interrupt the backlog replay or refresh input visibility",
);

const reportFilterTag = terminalHtml.indexOf("/assets/terminal-report-filter.js");
const terminalJsTag = terminalHtml.indexOf("/assets/terminal.js");
assert.ok(reportFilterTag >= 0, "terminal.html should load terminal-report-filter.js");
assert.ok(
  reportFilterTag < terminalJsTag,
  "terminal-report-filter.js must load before terminal.js so the global filter exists",
);

console.log("terminal report filter regression tests passed");
