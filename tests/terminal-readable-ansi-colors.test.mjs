import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalJs = readEntryScriptBundle("terminal.html");
const stylesBaseCss = readFileSync(new URL("../static/styles-base.css", import.meta.url), "utf8");

assert.match(
  stylesBaseCss,
  /--terminal-ansi-black:\s*#[0-9a-fA-F]{6};[\s\S]*--terminal-ansi-bright-black:\s*#[0-9a-fA-F]{6};/,
  "terminal theme should define readable ANSI black colors instead of inheriting xterm's near-black defaults",
);

assert.match(
  stylesBaseCss,
  /:root\[data-theme="dark"\][\s\S]*--terminal-ansi-black:\s*#[0-9a-fA-F]{6};[\s\S]*--terminal-ansi-bright-black:\s*#[0-9a-fA-F]{6};/,
  "dark theme should also override ANSI black colors for Codex dim/history input text",
);

assert.match(
  terminalJs,
  /black:\s*readCssCustomProperty\("--terminal-ansi-black",\s*"#[0-9a-fA-F]{6}"\),[\s\S]*brightBlack:\s*readCssCustomProperty\("--terminal-ansi-bright-black",\s*"#[0-9a-fA-F]{6}"\),/,
  "xterm theme should pass readable ANSI black colors into the renderer",
);

assert.match(
  terminalJs,
  /minimumContrastRatio:\s*4\.5,/,
  "xterm should keep a WCAG AA contrast floor so dark ANSI input remains visible on the terminal background",
);

assert.match(
  terminalJs,
  /function terminalRendererType\([\s\S]*?userAgent = navigator\.userAgent,[\s\S]*?androidBridge = globalThis\.WebClxAndroid[\s\S]*?webClxAndroid\\\/[\s\S]*?legacyAndroidWebView[\s\S]*?\? "dom" : "canvas"/,
  "Android WebView should use xterm's DOM renderer so TUI clears cannot leave stale canvas layers",
);

assert.match(
  terminalJs,
  /rendererType:\s*terminalRendererType\(\),/,
  "each terminal instance should select its renderer from the current client environment",
);
