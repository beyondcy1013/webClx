import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const activity = readFileSync(
  new URL("../android/app/src/main/java/com/webclx/app/MainActivity.java", import.meta.url),
  "utf8",
);
const responsiveCss = readFileSync(
  new URL("../static/styles-responsive.css", import.meta.url),
  "utf8",
);
const preferences = readFileSync(
  new URL("../android/app/src/main/java/com/webclx/app/AppPreferences.java", import.meta.url),
  "utf8",
);

test("Android opens terminal management after choosing a healthy source", () => {
  assert.match(preferences, /getString\(KEY_START_PATH, "terminal"\)/);
  assert.match(
    activity,
    /webView\.loadUrl\(SourceRegistry\.URLS\[source\] \+ AppPreferences\.startPath\(this\)\)/,
  );
});

test("mobile workspace actions and directory names have dedicated space", () => {
  assert.match(
    responsiveCss,
    /\.file-browser-action-cell \.actions\s*\{[^}]*display:\s*grid;[^}]*grid-template-columns:\s*repeat\(3,\s*minmax\(0,\s*1fr\)\);[^}]*gap:\s*4px;/s,
  );
  assert.match(
    responsiveCss,
    /\.file-browser-action-cell \.mini-button\s*\{[^}]*width:\s*100%;[^}]*min-width:\s*0;/s,
  );
  assert.match(
    responsiveCss,
    /\.file-browser-table th:last-child,\s*\.file-browser-table td:last-child\s*\{[^}]*display:\s*none;/s,
  );
  assert.match(
    responsiveCss,
    /\.file-browser-table \.entry-name\s*\{[^}]*overflow-x:\s*visible;[^}]*white-space:\s*normal;/s,
  );
  assert.match(
    responsiveCss,
    /\.file-browser-table \.entry-link\s*\{[^}]*white-space:\s*normal;[^}]*overflow-wrap:\s*anywhere;/s,
  );
});
