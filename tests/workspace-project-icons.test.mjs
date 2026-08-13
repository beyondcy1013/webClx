import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const iconsJs = readFileSync(new URL("../static/workspace-project-icons.js", import.meta.url), "utf8");
const appHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const browserJs = readFileSync(new URL("../static/app-workspace-browser.js", import.meta.url), "utf8");
const terminalRenderJs = readFileSync(new URL("../static/terminal-session-render.js", import.meta.url), "utf8");
const workspaceRoutesRs = readFileSync(new URL("../src/routes/workspace.rs", import.meta.url), "utf8");
const settingsLoadSaveJs = readFileSync(new URL("../static/app-settings-load-save.js", import.meta.url), "utf8");

const context = vm.createContext({
  URLSearchParams,
  module: { exports: {} },
  exports: {},
});
vm.runInContext(iconsJs, context);
const helpers = context.module.exports;

assert.equal(helpers.normalizeProjectIconPath(" assets\\icon.png ", "icon.ico"), "assets/icon.png");
assert.equal(helpers.normalizeProjectIconPath("../escape.ico", "icon.ico"), "icon.ico");
assert.equal(helpers.normalizeProjectIconPath("/absolute/icon.ico", "icon.ico"), "icon.ico");
assert.equal(helpers.workspaceProjectKey("webClx/src"), "webClx");
assert.equal(
  helpers.workspaceProjectKey("../third_party/sub2api/frontend/src"),
  "../third_party/sub2api",
);
assert.equal(helpers.workspaceProjectTextIcon("webClx"), "WC");
assert.equal(helpers.workspaceProjectTextIcon("webClx/src"), "WC");
assert.equal(helpers.workspaceProjectTextIcon("../third_party/sub2api"), "S2");
const colorSlots = helpers.workspaceProjectColorSlots(["beta", "alpha/src", "alpha"]);
assert.equal(colorSlots.get("alpha"), 0);
assert.equal(colorSlots.get("beta"), 1);
assert.equal(helpers.workspaceProjectHue("alpha", colorSlots), 210);
assert.equal(helpers.workspaceProjectHue("alpha/src", colorSlots), 210);
assert.equal(helpers.workspaceProjectHue("beta", colorSlots), 347.508);
assert.equal(
  helpers.workspaceProjectHue("webClx"),
  helpers.workspaceProjectHue("webClx/src"),
);
assert.equal(
  helpers.workspaceProjectIconUrl("demo/src", "static/favicon.svg", true),
  "/api/workspace-icon?path=demo%2Fsrc&icon_path=static%2Ffavicon.svg&search=nearest",
);

assert.match(appHtml, /workspace-project-icons\.js\?v=/);
assert.match(terminalHtml, /workspace-project-icons\.js\?v=/);
assert.match(appHtml, /<th[^>]*>图标<\/th>/);
assert.match(browserJs, /createWorkspaceProjectIcon\(\s*entry\.path,\s*state\.workspaceBrowserIconPath/);
assert.match(terminalRenderJs, /option\.dataset\.workspacePath\s*=/);
assert.match(iconsJs, /trigger\.setAttribute\("aria-label"/);
assert.match(iconsJs, /menu\.addEventListener\("keydown"/);
assert.match(iconsJs, /event\.key === "Escape"/);
assert.match(workspaceRoutesRs, /"\/api\/workspace-icon",\s*get\(filesystem::read_workspace_icon\)/);
assert.match(settingsLoadSaveJs, /workspace_browser_icon_path:/);
assert.match(settingsLoadSaveJs, /terminal_workspace_icon_path:/);
