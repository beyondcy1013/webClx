import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const staticRoot = new URL("../static/", import.meta.url);
const navigationSource = readFileSync(new URL("terminal-navigation-layout.js", staticRoot), "utf8");
const terminalStyles = readFileSync(new URL("styles-terminal.css", staticRoot), "utf8");
const baseStyles = readFileSync(new URL("styles-base.css", staticRoot), "utf8");
const responsiveStyles = readFileSync(new URL("styles-responsive.css", staticRoot), "utf8");

test("terminal workspace height is enlarged on desktop and remains keyboard-safe on mobile", () => {
  assert.match(navigationSource, /availableHeight[\s\S]*heightScale[\s\S]*\? 1 : 1\.2/);
  assert.match(navigationSource, /availableHeight \* heightScale/);
  assert.match(terminalStyles, /height: var\(--terminal-host-height, calc\(\(100dvh - 240px\) \* 1\.2\)\);/);
  assert.match(baseStyles, /--terminal-host-height: calc\(\(100dvh - 240px\) \* 1\.2\);/);
  assert.match(responsiveStyles, /height: var\(--terminal-host-height, calc\(\(100dvh - 360px\) \* 1\.2\)\);/);
});
