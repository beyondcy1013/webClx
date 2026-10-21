import assert from "node:assert/strict";
import fs from "node:fs";

const html = fs.readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const app = fs.readFileSync(new URL("../static/app.js", import.meta.url), "utf8");
const bindings = fs.readFileSync(
  new URL("../static/app-settings-event-bindings.js", import.meta.url),
  "utf8",
);
const styles = fs.readFileSync(new URL("../static/styles-settings.css", import.meta.url), "utf8");

assert.match(html, /id="font-settings-open"[^>]*type="button"[^>]*aria-haspopup="dialog"/);
assert.match(html, /<dialog id="font-settings-dialog"/);
assert.match(html, /id="font-settings-close"[^>]*type="button"/);
assert.equal((html.match(/id="font-size-tier-[1-4]-input"/g) || []).length, 4);
assert.match(app, /const fontSettingsDialogEl = document\.getElementById\("font-settings-dialog"\)/);
assert.match(bindings, /fontSettingsOpenButtonEl\?\.addEventListener\("pointerdown"/);
assert.match(bindings, /event\.preventDefault\(\)/);
assert.match(bindings, /fontSettingsDialogEl\.showModal\(\)/);
assert.match(bindings, /fontSettingsCloseButtonEl\?\.focus\(\{ preventScroll: true \}\)/);
assert.doesNotMatch(bindings, /fontSizeTier1InputEl\?\.focus/);
assert.match(styles, /\.font-settings-dialog/);

console.log("font settings dialog contract checks passed");
