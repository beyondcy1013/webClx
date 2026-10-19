import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const css = readFileSync(new URL("../static/styles-base.css", import.meta.url), "utf8");
const html = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const settings = readFileSync(new URL("../static/terminal-settings.js", import.meta.url), "utf8");

assert.match(css, /--page-max-width:\s*1440px/);
assert.match(css, /\.terminal-page\s*\{[\s\S]*?width:\s*min\(var\(--page-max-width\),\s*calc\(100vw - var\(--page-inline-margin\)\)\);[\s\S]*?\}/);
assert.doesNotMatch(css, /\.terminal-page\s*\{[\s\S]*?width:\s*min\(100vw,\s*70vw\)/);
assert.match(css, /--font-size-tier-4:\s*1\.06rem/);
assert.match(settings, /const DEFAULT_FONT_SIZE_TIER_4 = 1\.06;/);
assert.doesNotMatch(html, /styles-base\.css\?v=20260904a/);
assert.match(html, /styles-base\.css\?v=20260925nav1/);
assert.match(html, /terminal-settings\.js\?v=20260925nav1/);

console.log("terminal page font and width checks passed");
