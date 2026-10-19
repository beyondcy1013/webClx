import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const indexSource = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const appSource = readFileSync(new URL("../static/app.js", import.meta.url), "utf8");
const loadedScriptSource = Array.from(
  indexSource.matchAll(/<script\b[^>]*\bsrc="\/assets\/([^"?]+)(?:\?[^"?]*)?"[^>]*><\/script>/g),
  (match) => readFileSync(new URL(`../static/${match[1]}`, import.meta.url), "utf8"),
).join("\n");

for (const removedBinder of ["bindDesktopFrameEvents", "bindUpdateEventHandlers"]) {
  const callPattern = new RegExp(`^${removedBinder}\\(\\);$`, "m");
  const definitionPattern = new RegExp(`function\\s+${removedBinder}\\(`);
  assert.ok(
    !callPattern.test(appSource) || definitionPattern.test(loadedScriptSource),
    `home-page startup binder ${removedBinder} must be defined by a loaded script`,
  );
}

assert.match(
  appSource,
  /function init\(\)[\s\S]*?setActiveSettingsTab\(state\.activeSettingsTab\);[\s\S]*?setActiveTab\(state\.activeTab\);/,
  "the initial semantic route must be applied during home-page startup",
);

console.log("settings first-navigation regression tests passed");
