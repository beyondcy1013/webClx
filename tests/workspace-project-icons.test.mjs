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
const stylesBaseCss = readFileSync(new URL("../static/styles-base.css", import.meta.url), "utf8");
const terminalCommonJs = readFileSync(new URL("../static/terminal-common.js", import.meta.url), "utf8");

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
  helpers.workspaceProjectKey("third_party/zcodeOfficial/src"),
  "third_party/zcodeOfficial",
);
assert.equal(
  helpers.workspaceProjectKey("../third_party/sub2api/frontend/src"),
  "../third_party/sub2api",
);
assert.equal(helpers.workspaceProjectTextIcon("webClx"), "WC");
assert.equal(helpers.workspaceProjectTextIcon("webClx/src"), "WC");
assert.equal(helpers.workspaceProjectTextIcon("third_party/zcodeOfficial"), "ZO");
assert.equal(helpers.workspaceProjectTextIcon("../third_party/sub2api"), "S2");
const colorSlots = helpers.workspaceProjectColorSlots(["beta", "alpha/src", "alpha"]);
assert.equal(colorSlots.get("alpha"), 0);
assert.equal(colorSlots.get("beta"), 1);
assert.equal(helpers.workspaceProjectHue("alpha", colorSlots), 210);
assert.equal(helpers.workspaceProjectHue("alpha/src", colorSlots), 210);
assert.equal(helpers.workspaceProjectHue("beta", colorSlots), 347.508);
assert.notEqual(
  helpers.workspaceProjectHue("third_party/zcodeOfficial"),
  helpers.workspaceProjectHue("third_party/sub2api"),
);
assert.equal(
  helpers.workspaceProjectHue("webClx"),
  helpers.workspaceProjectHue("webClx/src"),
);
assert.equal(
  helpers.workspaceProjectIconUrl("demo/src", "static/favicon.svg", true),
  "/api/workspace-icon?path=demo%2Fsrc&icon_path=static%2Ffavicon.svg&search=nearest",
);

// Reviewers are a distinct role: they must not reuse the executor emoji
// (random-by-project or the fixed override), otherwise both roles render the
// same icon in the terminal dropdown.
const reviewerOption = { value: "s1", dataset: { groupRole: "reviewer", workspacePath: "demo" } };
const executorOption = { value: "s2", dataset: { groupRole: "executor", workspacePath: "demo" } };
const managerOption = { value: "s3", dataset: { groupRole: "manager", workspacePath: "demo" } };
assert.equal(helpers.terminalRoleEmoji({ value: "", dataset: {} }), "");
assert.equal(helpers.terminalRoleEmoji(managerOption), "🔱");
assert.notEqual(helpers.terminalRoleEmoji(managerOption), "👑");
assert.equal(helpers.terminalRoleEmoji({ dataset: { groupRole: "super_admin" } }), "👑");
context.localStorage = { getItem: (key) => key.includes("manager") ? "⚔️" : "🛡️" };
assert.equal(helpers.terminalRoleEmoji(managerOption), "🔱");
context.localStorage = { getItem: () => "🧭" };
assert.equal(helpers.terminalRoleEmoji(managerOption), "🧭");
delete context.localStorage;
assert.equal(helpers.terminalRoleEmoji(reviewerOption), "🔍");
assert.notEqual(helpers.terminalRoleEmoji(reviewerOption), helpers.terminalRoleEmoji(executorOption));
// A missing/invalid preference uses the fixed rocket; explicit custom choices
// and the opt-in project-stable random scheme remain supported.
assert.equal(helpers.terminalRoleEmoji(executorOption), "🚀");
assert.equal(helpers.terminalRoleEmoji({ value: "s4", dataset: { groupRole: "executor", workspacePath: "other" } }), "🚀");
const emojiPreferences = new Map();
context.localStorage = { getItem: (key) => emojiPreferences.get(key) ?? null };
emojiPreferences.set("webclx:terminal-group-executor-emoji-mode", "invalid");
assert.equal(helpers.terminalRoleEmoji(executorOption), "🚀");
emojiPreferences.set("webclx:terminal-group-executor-emoji-mode", "fixed");
assert.equal(helpers.terminalRoleEmoji(executorOption), "🚀");
emojiPreferences.set("webclx:terminal-group-executor-emoji", "🛠️");
assert.equal(helpers.terminalRoleEmoji(executorOption), "🛠️");
assert.equal(helpers.terminalRoleEmoji(reviewerOption), "🔍");
emojiPreferences.set("webclx:terminal-group-executor-emoji-mode", "random");
const randomEmoji = helpers.terminalRoleEmoji(executorOption);
assert.equal(helpers.terminalRoleEmoji({ ...executorOption, value: "s5" }), randomEmoji);
assert.notEqual(randomEmoji, "🔍");
delete context.localStorage;

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
assert.match(iconsJs, /const groupBadge = `\[\$\{groupName\}\]`;/);
assert.match(iconsJs, /manager: "普通管理员"/);
assert.match(iconsJs, /marker\.hidden = true;\s*marker\.style\.display = "none";/);
assert.match(stylesBaseCss, /\.workspace-icon-select-group-marker\s*\{[^}]*display:\s*none/);
assert.match(terminalCommonJs, /const groupBadge = groupName \? `\[\$\{groupName\}\] ` : "";/);
assert.match(settingsLoadSaveJs, /state\.webUserRole === "super_admin" \? "👑 " : ""/);
assert.match(settingsLoadSaveJs, /超级管理员 \| 终端用户/);
assert.match(settingsLoadSaveJs, /state\.webUserRole = state\.webUserRole \|\| settings\.web_user_role \|\| "";/);
assert.match(appHtml, /workspace-project-icons\.js\?v=20261111-executor-rocket-spacing/);
assert.match(terminalHtml, /workspace-project-icons\.js\?v=20261111-executor-rocket-spacing/);
assert.match(appHtml, /皇冠仅表示超级管理员/);
assert.match(stylesBaseCss, /\.workspace-icon-select-text \{[^}]*padding-left: 4px;/);
