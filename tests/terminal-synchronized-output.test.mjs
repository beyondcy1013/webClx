import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { createSynchronizedOutputTransformer } = require(
  "../static/terminal-synchronized-output.js",
);

const encoder = new TextEncoder();
const decoder = new TextDecoder();
const bytes = (value) => encoder.encode(value);
const text = (value) => decoder.decode(value);

const transformer = createSynchronizedOutputTransformer();
assert.equal(text(transformer.transform(bytes("before\n"))), "before\n");
assert.equal(text(transformer.transform(bytes("\u001b[?202"))), "");
assert.equal(text(transformer.transform(bytes("6h\u001b[2J"))), "");
assert.equal(text(transformer.transform(bytes("redraw"))), "");
assert.equal(
  text(transformer.transform(bytes("\u001b[?2026lafter\n"))),
  "\u001b[?2026h\u001b[2Jredraw\u001b[?2026lafter\n",
  "a synchronized redraw split across websocket frames should reach xterm atomically",
);

const incomplete = createSynchronizedOutputTransformer();
assert.equal(text(incomplete.transform(bytes("prefix\u001b[?2026hpartial"))), "prefix");
assert.equal(
  text(incomplete.flush()),
  "\u001b[?2026hpartial",
  "disconnect/replay boundaries should flush an incomplete synchronized update without data loss",
);

const ordinary = createSynchronizedOutputTransformer();
assert.equal(
  text(ordinary.transform(bytes("plain terminal output"))),
  "plain terminal output",
  "ordinary output should not wait for another websocket frame",
);

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const outputPipeline = readFileSync(
  new URL("../static/terminal-output-scroll.js", import.meta.url),
  "utf8",
);
assert.match(terminalHtml, /terminal-synchronized-output\.js\?v=20260805a/);
assert.match(terminalHtml, /terminal-output-scroll\.js\?v=20260807a/);
assert.match(
  terminalHtml,
  /terminal-synchronized-output\.js[^\n]*[\s\S]*terminal-codex-status-output\.js[^\n]*[\s\S]*terminal-output-scroll\.js/,
  "the synchronized-output helper must load before the terminal output pipeline",
);
assert.match(
  outputPipeline,
  /transformTerminalSynchronizedOutput\(bytes, context\)[\s\S]*const replay = context\.backlogReplayActive[\s\S]*replay[\s\S]*transformTerminalCodexStatusOutput\(synchronizedBytes, context\)[\s\S]*: synchronizedBytes/,
  "live output should stay byte-for-byte intact while backlog output may compact Codex status blocks",
);
assert.match(
  outputPipeline,
  /flushTerminalSynchronizedOutput\(context\)[\s\S]*transformTerminalCodexStatusOutput\(synchronizedPendingBytes, context\)[\s\S]*flushCodexStatusOutputTransformer\(context\)/,
  "backlog completion should flush synchronized bytes through the status transformer in order",
);

console.log("terminal synchronized output tests passed");
