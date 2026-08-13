import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const {
  findCodexStatusBlock,
  formatCompactCodexStatus,
  shouldPreserveOverlayContent,
  stringCellWidth,
} = require("../static/terminal-codex-status-compact.js");

const narrowStatusLines = [
  "before",
  "╭───────────────────────────────────────────────╮",
  "│  >_ OpenAI Codex (v0.144.5)                   │",
  "│                                               │",
  "│  Model:                gpt-5.6-sol (reasoning │",
  "│  Model provider:       sub2api_gpt-5.6_1M - h │",
  "│  Directory:            /home/…/stockScreener  │",
  "│  Permissions:          Full Access            │",
  "│  Agents.md:            /home/root/.codex/AGEN │",
  "│  Thread name:          扩展字段基础上注册为ds │",
  "│  Collaboration mode:   Default                │",
  "│  Session:              019f741e-6bb4-7a03-ac4 │",
  "│  Forked from:          019f73d6-ece8-72d0-add │",
  "│                                               │",
  "│  Token usage:          1.73M total  (1.61M in │",
  "│  Context window:       98% left (15.8K used / │",
  "│  Limits:               not available for this │",
  "╰───────────────────────────────────────────────╯",
  "after",
];

const block = findCodexStatusBlock(narrowStatusLines);
assert.deepEqual(
  block,
  { start: 1, end: 17, version: "0.144.5" },
  "the versioned Codex heading should identify exactly one bordered status block",
);

assert.equal(
  findCodexStatusBlock(["╭────╮", "│ ordinary output │", "╰────╯"]),
  null,
  "unrelated terminal boxes must remain untouched",
);

const status = {
  version: "0.144.5",
  model: "gpt-5.6-sol",
  reasoning_effort: "xhigh",
  summary_mode: "auto",
  cwd: "/home/codes/stockScreener",
  permission: "Full Access",
  collaboration_mode: "Default",
  session_id: "019f741e-6bb4-7a03-ac49-d28a60ef3765",
  forked_from: "019f73d6-ece8-72d0-addc-e74da1b25a1a",
  thread_name: "扩展字段基础上注册为dsl",
  agents_md: ["/home/root/.codex/AGENTS.md", "/home/codes/webClx/AGENTS.md"],
  token_usage: {
    input_tokens: 1_610_000,
    output_tokens: 45_700,
    total_tokens: 1_730_000,
  },
  context_window: {
    used_tokens: 15_800,
    total_tokens: 1_000_000,
    percent_left: 98,
  },
};

const compact = formatCompactCodexStatus({
  status,
  session: {
    codex_api_preset_name: "sub2api_gpt-5.6_1M",
    codex_api_base_url: "http://192.168.3.2:18381/v1",
  },
  sourceLines: narrowStatusLines.slice(block.start, block.end + 1),
  columns: 49,
  targetRows: block.end - block.start + 1,
});

assert.equal(compact.length, 17, "the compact overlay must preserve the original block height");
assert.equal(compact[0], "╭───────────────────────────────────────────────╮");
assert.equal(compact.at(-1), "╰───────────────────────────────────────────────╯");
assert.ok(compact[1].includes(">_ OpenAI Codex v0.144.5"));
assert.equal(compact[2], `├${"─".repeat(9)}┬${"─".repeat(37)}┤`);
assert.ok(compact.some((line) => line.includes("│Model    │ gpt-5.6-sol | xhigh | auto")));
assert.ok(compact.some((line) => line.includes("│Provider │ sub2api_gpt-5.6_1M")));
assert.ok(compact.some((line) => line.includes("│URL      │ http://192.168.3.2:18381/v1")));
assert.ok(compact.some((line) => line.includes(status.session_id)));
assert.ok(compact.some((line) => line.includes("│Agents   │ /home/root/.codex/AGENTS.md")));
assert.ok(compact.some((line) => line.includes("│         │ /home/codes/webClx/AGENTS.md")));
assert.ok(compact.some((line) => line.includes("│Context  │ 98% left | 15.8K / 1.00M")));
assert.ok(
  compact.slice(3, -1).every((line) => line.split("│").length === 4),
  "every status row should keep separate key and value cells",
);
assert.ok(
  compact.every((line) => stringCellWidth(line) === 49),
  "ASCII and CJK rows should all end at the same terminal column",
);
assert.ok(
  compact.every((line) => !/Model:|Provider:|Session:/.test(line)),
  "field labels should rely on the table separator instead of colons",
);

const veryNarrow = formatCompactCodexStatus({
  status,
  session: {},
  sourceLines: narrowStatusLines.slice(block.start, block.end + 1),
  columns: 36,
  targetRows: 17,
});
assert.equal(veryNarrow.length, 17);
assert.ok(veryNarrow.every((line) => stringCellWidth(line) === 36));
assert.ok(
  veryNarrow.some((line) => line.includes("扩展字段")),
  "CJK content should be retained without overflowing the border",
);

assert.equal(
  shouldPreserveOverlayContent({ pointerSelectionActive: true, hasActiveSelection: false }),
  true,
  "the compact status overlay must not replace its DOM while a pointer selection is starting",
);
assert.equal(
  shouldPreserveOverlayContent({ pointerSelectionActive: false, hasActiveSelection: true }),
  true,
  "an established browser selection must continue to freeze overlay content",
);
assert.equal(
  shouldPreserveOverlayContent({ pointerSelectionActive: false, hasActiveSelection: false }),
  false,
  "the overlay may refresh after pointer selection ends without a retained selection",
);

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalFrontend = readFileSync(new URL("../static/terminal.js", import.meta.url), "utf8");
const terminalBackend = readFileSync(new URL("../src/terminal.rs", import.meta.url), "utf8");
assert.doesNotMatch(
  terminalHtml,
  /terminal-codex-status-compact\.js/,
  "the terminal page must not load the compact status overlay renderer",
);
assert.doesNotMatch(
  terminalFrontend,
  /createTerminalCodexStatusCompactor/,
  "terminal instances must leave native /status output unobscured by a DOM overlay",
);
assert.match(
  readFileSync(new URL("../static/terminal-codex-status-compact.js", import.meta.url), "utf8"),
  /shouldPreserveOverlayContent\(\{[\s\S]*?pointerSelectionActive,[\s\S]*?hasActiveSelection:/,
  "overlay rendering must consult both in-progress pointer selection and established selection",
);
assert.match(
  terminalBackend,
  /codex_status:\s*codex_status::detect_current_codex_status\(&session_id\)/,
  "fresh Codex sessions should expose rollout status before resume detection is available",
);

console.log("codex status compact tests passed");
