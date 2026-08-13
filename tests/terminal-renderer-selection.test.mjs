import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const terminalShellSettings = readFileSync(
  new URL("../static/terminal-shell-settings.js", import.meta.url),
  "utf8",
);
const start = terminalShellSettings.indexOf("function terminalRendererType(");
const end = terminalShellSettings.indexOf("function createTerminalInstance()", start);
assert.ok(start >= 0 && end > start, "terminal renderer selector should exist");

const terminalRendererType = Function(
  `${terminalShellSettings.slice(start, end)}; return terminalRendererType;`,
)();

const legacyAndroidWebView =
  "Mozilla/5.0 (Linux; Android 14; Pixel Build/UP1A; wv) " +
  "AppleWebKit/537.36 Version/4.0 Chrome/125.0 Mobile Safari/537.36";
const androidChrome =
  "Mozilla/5.0 (Linux; Android 14; Pixel) AppleWebKit/537.36 " +
  "Chrome/125.0 Mobile Safari/537.36";
const desktopChrome =
  "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/125.0 Safari/537.36";

assert.equal(
  terminalRendererType(legacyAndroidWebView, null),
  "dom",
  "legacy Android WebView clients should avoid the stale canvas layer after Codex clears its logo",
);
assert.equal(
  terminalRendererType(`${androidChrome} webClxAndroid/1.0.0`, null),
  "dom",
  "current Android clients should use the explicit application marker",
);
assert.equal(
  terminalRendererType(androidChrome, {}),
  "dom",
  "Android clients with the JavaScript bridge should use the DOM renderer",
);
assert.equal(terminalRendererType(androidChrome, null), "canvas");
assert.equal(terminalRendererType(desktopChrome, null), "canvas");

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
assert.match(
  terminalHtml,
  /terminal-shell-settings\.js\?v=20260803b/,
  "terminal page should invalidate cached renderer selection code",
);
