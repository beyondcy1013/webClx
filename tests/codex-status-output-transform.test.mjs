import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const {
  compactCodexStatusOutputText,
  createCodexStatusOutputTransformer,
  stripTerminalAnsi,
} = require("../static/terminal-codex-status-output.js");

const encoder = new TextEncoder();
const decoder = new TextDecoder();
const nativeStatus = [
  "\u001b[2m\u001b[39m╭───────────────────────────────────────────────╮",
  "│  >_ \u001b[0;1mOpenAI Codex\u001b[0;2m (v0.145.0)                   │",
  "│                                               │",
  "│  Model:                \u001b[0mgpt-5.6-sol\u001b[2m (reasoning │",
  "│  Model provider:       \u001b[0msub2api_gpt-5.6_1M - h\u001b[2m │",
  "│  Directory:            \u001b[0m/home/codes/webClx\u001b[2m     │",
  "│  Permissions:          \u001b[0mFull Access\u001b[2m            │",
  "│  Agents.md:            \u001b[0m/home/root/.codex/AGEN\u001b[2m │",
  "│  Collaboration mode:   \u001b[0mDefault\u001b[2m                │",
  "│  Session:              \u001b[0m019f9a23-21c5-7bc1-b1f\u001b[2m │",
  "│                                               │",
  "│  Token usage:          \u001b[0m1.43M total\u001b[2m (1.34M in │",
  "│  Context window:       \u001b[0m55% left\u001b[2m (122K used /  │",
  "│  Limits:               not available for this │",
  "╰───────────────────────────────────────────────╯",
].join("\n");

const compact = compactCodexStatusOutputText(nativeStatus);
const compactPlain = stripTerminalAnsi(compact);
assert.ok(compactPlain.includes(">_ OpenAI Codex v0.145.0"));
assert.ok(compactPlain.includes("│Model: gpt-5.6-sol (reasoning"));
assert.ok(compactPlain.includes("│Provider: sub2api_gpt-5.6_1M - h"));
assert.match(compactPlain, /│Access: Full Access\s*│/);
assert.match(compactPlain, /│Mode: Default\s*│/);
assert.ok(!compactPlain.includes("Access: Full Access | Mode: Default"));
assert.ok(!compactPlain.includes("Collaboration mode:"));
assert.ok(compactPlain.includes("│Session: 019f9a23-21c5-7bc1-b1f"));
assert.doesNotMatch(compactPlain, /│(?:Model|Provider|Dir|Access)\s+│/);
assert.ok(!compactPlain.includes("Model:                "));
assert.ok(
  compactPlain.split("\n")[0].length < stripTerminalAnsi(nativeStatus.split("\n")[0]).length,
  "the compact status border should shrink to its content instead of keeping native padding",
);

const nativeStatusWithDirectories = nativeStatus.replace(
  /│  Directory:.*│/,
  "│  Directory:            /srv/alpha, /srv/beta    │",
);
const compactDirectories = stripTerminalAnsi(
  compactCodexStatusOutputText(nativeStatusWithDirectories),
);
assert.ok(compactDirectories.includes("│Dir: /srv/alpha,"));
assert.ok(compactDirectories.includes("\n│/srv/beta"));
assert.ok(!compactDirectories.includes("Dir: /srv/alpha, /srv/beta"));
assert.ok(
  compactPlain.split("\n").length < nativeStatus.split("\n").length,
  "the actual terminal block should use fewer rows than Codex's native status output",
);

const nativeStatusCrLf = nativeStatus.replaceAll("\n", "\r\n");
const compactCrLf = compactCodexStatusOutputText(nativeStatusCrLf);
assert.doesNotMatch(
  compactCrLf,
  /(?<!\r)\n/,
  "PTY CRLF rows must stay CRLF so rewritten xterm rows return to column zero",
);

const indentedStatus = nativeStatus.replace(
  "\u001b[2m\u001b[39m╭",
  "  \u001b[2m\u001b[39m  ╭",
);
const compactIndented = compactCodexStatusOutputText(indentedStatus);
assert.ok(compactIndented.startsWith("\u001b[2m\u001b[39m╭"));
assert.ok(stripTerminalAnsi(compactIndented).startsWith("╭"));

const byteByByte = createCodexStatusOutputTransformer();
const source = encoder.encode(`before\n${nativeStatus}\nafter\n`);
const transformedChunks = [];
for (const byte of source) {
  const output = byteByByte.transform(Uint8Array.of(byte));
  if (output.length > 0) {
    transformedChunks.push(output);
  }
}
transformedChunks.push(byteByByte.flush());
const transformed = decoder.decode(
  Buffer.concat(transformedChunks.map((chunk) => Buffer.from(chunk))),
);
assert.ok(transformed.startsWith("before\n"));
assert.ok(transformed.endsWith("\nafter\n"));
assert.ok(stripTerminalAnsi(transformed).includes("│Model: gpt-5.6-sol"));
assert.ok(!transformed.includes("�"), "UTF-8 characters split across frames must remain intact");

const ordinaryBox = "prefix\n╭────╮\n│ note │\n╰────╯\nsuffix\n";
const ordinaryTransformer = createCodexStatusOutputTransformer();
const ordinaryOutput = Buffer.concat([
  Buffer.from(ordinaryTransformer.transform(encoder.encode(ordinaryBox))),
  Buffer.from(ordinaryTransformer.flush()),
]).toString("utf8");
assert.equal(ordinaryOutput, ordinaryBox, "unrelated terminal boxes must remain byte-for-byte unchanged");

const incomplete = `${nativeStatus.split("\n").slice(0, 5).join("\n")}\n`;
const incompleteTransformer = createCodexStatusOutputTransformer();
const incompleteOutput = Buffer.concat([
  Buffer.from(incompleteTransformer.transform(encoder.encode(incomplete))),
  Buffer.from(incompleteTransformer.flush()),
]).toString("utf8");
assert.equal(incompleteOutput, incomplete, "an incomplete status block must flush without data loss");

const moduleSource = readFileSync(
  new URL("../static/terminal-codex-status-output.js", import.meta.url),
  "utf8",
);
assert.doesNotMatch(moduleSource, /document\.|createElement|overlay/i);

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const outputPipeline = readFileSync(
  new URL("../static/terminal-output-scroll.js", import.meta.url),
  "utf8",
);
assert.match(terminalHtml, /terminal-codex-status-output\.js\?v=20260726c/);
assert.match(outputPipeline, /createCodexStatusOutputTransformer/);
assert.match(outputPipeline, /flushCodexStatusOutputTransformer/);

console.log("codex status output transform tests passed");
