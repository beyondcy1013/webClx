import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalJs = readFileSync(new URL("../static/terminal.js", import.meta.url), "utf8");
const terminalSessionRenderJs = readFileSync(
  new URL("../static/terminal-session-render.js", import.meta.url),
  "utf8",
);
const terminalSettingsLoaderJs = readFileSync(
  new URL("../static/terminal-settings-loader.js", import.meta.url),
  "utf8",
);
const stylesBaseCss = readFileSync(new URL("../static/styles-base.css", import.meta.url), "utf8");
const workspaceProjectIconsJs = readFileSync(
  new URL("../static/workspace-project-icons.js", import.meta.url),
  "utf8",
);

assert.match(
  terminalJs,
  /enhanceWorkspaceIconSelect\(idleSessionSelectEl, \(\) => state\.terminalWorkspaceIconPath\)/,
  "idle terminal selection should use the shared project-icon dropdown",
);

assert.match(
  terminalSessionRenderJs,
  /function renderIdleSessions\(\)[\s\S]*idleSessionSelectEl\.value = "";[\s\S]*idleSessionSelectEl\.workspaceIconSelectController\?\.sync\(\)/,
  "idle terminal rendering should refresh its project-icon dropdown after resetting the placeholder",
);

assert.match(
  terminalSettingsLoaderJs,
  /idleSessionSelectEl\?\.workspaceIconSelectController\?\.sync\(\)/,
  "changing the configured terminal icon path should refresh idle terminal icons",
);

assert.match(
  stylesBaseCss,
  /\.terminal-toolbar > \.workspace-icon-select\s*\{[\s\S]*width:\s*clamp\(120px,\s*14vw,\s*220px\)/,
  "idle terminal project-icon dropdown should remain compact inside the terminal toolbar",
);

assert.match(
  workspaceProjectIconsJs,
  /const focusTarget = \(\) => host\.contains\(root\.document\.activeElement\)\s*\?\s*trigger\s*:\s*selectEl;[\s\S]*selectEl\.addEventListener\("focus"[\s\S]*focusTarget\(\)\.focus\?/,
  "focus moved to the hidden native select should be redirected to the visible shared dropdown",
);

for (const asset of [
  "styles-base.css?v=",
  "terminal-settings-loader.js?v=",
  "terminal-session-render.js?v=",
  "terminal.js?v=",
  "workspace-project-icons.js?v=",
]) {
  const file = asset.replace("?v=", "");
  const pattern = new RegExp(file.replaceAll(".", "\\.") + "\\?v=[0-9a-z]+");
  assert.match(terminalHtml, pattern, `terminal page should load a versioned ${file}`);
}
