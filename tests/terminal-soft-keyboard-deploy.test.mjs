import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalJs = readEntryScriptBundle("terminal.html");
const terminalMobileKeysJs = readFileSync(
  new URL("../static/terminal-mobile-keys.js", import.meta.url),
  "utf8",
);
const terminalSettingsLoaderJs = readFileSync(
  new URL("../static/terminal-settings-loader.js", import.meta.url),
  "utf8",
);
const terminalHtml = readFileSync(
  new URL("../static/terminal.html", import.meta.url),
  "utf8",
);
const terminalStyles = readFileSync(
  new URL("../static/styles-terminal.css", import.meta.url),
  "utf8",
);
const terminalResponsiveStyles = readFileSync(
  new URL("../static/styles-responsive.css", import.meta.url),
  "utf8",
);
const terminalToolsMenuStart = terminalHtml.indexOf('id="terminal-tools-menu"');
const terminalToolsMenuEnd = terminalHtml.indexOf("</div>", terminalToolsMenuStart);
const terminalToolsMenu = terminalHtml.slice(terminalToolsMenuStart, terminalToolsMenuEnd);
const terminalSoftKeyboardStart = terminalHtml.indexOf('id="terminal-mobile-keys"');
const terminalSoftKeyboardEnd = terminalHtml.indexOf('id="terminal-fab"', terminalSoftKeyboardStart);
const terminalSoftKeyboard = terminalHtml.slice(terminalSoftKeyboardStart, terminalSoftKeyboardEnd);

assert.match(
  terminalJs,
  /workspaceDir:\s*""/,
  "terminal state should declare a workspaceDir field initialized to an empty string",
);

assert.match(
  terminalSettingsLoaderJs,
  /state\.workspaceDir\s*=\s*[\s\S]*?settings\.workspace_dir/,
  "terminal settings loader should populate state.workspaceDir from /api settings",
);

assert.match(
  terminalMobileKeysJs,
  /if \(!workspaceRoot\) \{[\s\S]*?updateStatus\(\"\u7ec8\u7aef\u8bbe\u7f6e\u5c1a\u672a\u52a0\u8f7d[\s\S]*?return;/,
  "soft-keyboard deploy should bail out with a clear status when the workspace root is unknown",
);

assert.match(
  terminalMobileKeysJs,
  /const projectDir = relativePath \? `\$\{workspaceRoot\}\/\$\{relativePath\}` : workspaceRoot;/,
  "soft-keyboard deploy should build the absolute project directory by joining workspace root and relative path",
);

assert.match(
  terminalMobileKeysJs,
  /async function resolveCurrentDeploySourceTerminal\(\)[\s\S]*?requestJson\("\/api\/terminal\/sessions\?all=true"\)[\s\S]*?item\.id === terminalId/,
  "deploy should refresh the current terminal identity by stable session ID immediately before queueing",
);

assert.match(
  terminalMobileKeysJs,
  /const sourceTerminal = await resolveCurrentDeploySourceTerminal\(\);[\s\S]*?source_terminal_name: sourceTerminal\.name,[\s\S]*?source_terminal_id: sourceTerminal\.id,[\s\S]*?source_tmux_session: sourceTerminal\.tmuxSessionName/,
  "deploy payload should use the dynamically refreshed terminal name, ID, and tmux session",
);

assert.match(
  terminalHtml,
  /<select[\s\S]*?id=\"terminal-project-command-select\"[\s\S]*?>[\s\S]*?<option value=\"\">\u9879\u76ee\u6307\u4ee4<\/option>[\s\S]*?<option value=\"open_project_url\">\u9879\u76ee URL<\/option>[\s\S]*?<option value=\"open_artifact_downloads\">\u4e0b\u8f7d\u4e2d\u5fc3<\/option>[\s\S]*?<option value=\"codes_backup\">!codes_backup<\/option>[\s\S]*?<option value=\"deploy_project\" data-shortcut=\"Ctrl\+B\">\u672c\u9879\u76ee\u90e8\u7f72\u811a\u672c<\/option>[\s\S]*?<\/select>/,
  "soft keyboard should expose project URL, 下载中心, codes_backup and deploy as project commands",
);

assert.match(
  terminalToolsMenu,
  /id=\"terminal-codex-full-access-toggle\"[\s\S]*?id=\"terminal-quick-command-buttons\"[\s\S]*?id=\"terminal-copy-all\"[\s\S]*?data-action=\"copy_all_text\"/,
  "terminal tools should contain full access, quick commands, and copy-all in that order",
);

assert.doesNotMatch(
  terminalSoftKeyboard,
  /id=\"terminal-quick-command-buttons\"|id=\"terminal-copy-all\"/,
  "soft keyboard should leave quick commands and copy-all inside terminal tools",
);

assert.match(
  terminalStyles,
  /#terminal-project-command-select \{[\s\S]*?width: 82px;[\s\S]*?min-width: 82px;[\s\S]*?max-width: 82px;/,
  "project commands should have enough fixed width for its longer label",
);

assert.match(
  terminalStyles,
  /\.terminal-command-collections-body \{[\s\S]*?gap: 5px;[\s\S]*?\.terminal-command-collection-group \{[\s\S]*?gap: 3px;[\s\S]*?padding: 5px 0;[\s\S]*?\.terminal-command-collection-grid \{[\s\S]*?gap: 2px;[\s\S]*?\.terminal-command-collection-item \{[\s\S]*?gap: 1px;[\s\S]*?min-height: 36px;[\s\S]*?padding: 1px 7px;/,
  "soft-keyboard command collections should keep a roughly 38px command-row pitch",
);

assert.match(
  terminalStyles,
  /\.terminal-fab-menu \{[\s\S]*?gap: 2px;[\s\S]*?\.terminal-fab-item \{[\s\S]*?min-width: 94px;[\s\S]*?min-height: 38px;[\s\S]*?gap: 7px;[\s\S]*?border-radius: 6px;[\s\S]*?padding: 10px 16px 10px 13px;[\s\S]*?background: transparent;[\s\S]*?font-size: 13px;[\s\S]*?box-shadow: none;/,
  "terminal FAB menu items should form one compact, stable-width vertical group",
);

assert.match(
  terminalStyles,
  /\.terminal-fab-item-icon \{[\s\S]*?width: 18px;[\s\S]*?height: 18px;[\s\S]*?font-size: 14px;[\s\S]*?opacity: 1;[\s\S]*?\.terminal-fab-item-label \{[\s\S]*?opacity: 1;/,
  "terminal FAB menu symbols and labels should inherit only the configurable item opacity",
);

assert.match(
  terminalStyles,
  /\.terminal-fab-toggle\[aria-expanded="true"\] \{[\s\S]*?background: transparent;[\s\S]*?color: rgba\(244, 255, 249, 0\.6\);[\s\S]*?box-shadow: none;[\s\S]*?backdrop-filter: none;[\s\S]*?-webkit-backdrop-filter: none;/,
  "the expanded FAB close button should remove its surface and render the cross at sixty percent opacity",
);

assert.match(
  terminalStyles,
  /\.terminal-fab-menu:not\(\[hidden\]\) \{[\s\S]*?position: static;[\s\S]*?bottom: auto;[\s\S]*?justify-content: flex-start;[\s\S]*?max-height: max\([\s\S]*?--terminal-visible-viewport-height[\s\S]*?overflow-y: auto;/,
  "the expanded terminal FAB menu should flow downward inside the visible viewport",
);

assert.doesNotMatch(
  terminalResponsiveStyles,
  /\.terminal-fab-item\s*\{/,
  "mobile responsive rules should not restore the old large FAB item dimensions",
);

assert.match(
  terminalStyles,
  /\.terminal-paste-schedule-chip \{[\s\S]*?display: inline-flex;[\s\S]*?max-width: 100%;[\s\S]*?background: transparent;[\s\S]*?box-shadow: none;[\s\S]*?\.terminal-paste-schedule-chip\[data-pending="true"\] \{[\s\S]*?color: #90f0cf;/,
  "the terminal schedule count should live inside the merged FAB item",
);

assert.match(
  terminalMobileKeysJs,
  /const PROJECT_WEB_CONFIG_FILE = "\.webclx\.json";/,
  "project URL should be read from a project-owned .webclx.json file",
);

assert.match(
  terminalMobileKeysJs,
  /function resolveProjectWebUrl\(config, locationLike = window\.location\)/,
  "project URL resolution should be isolated in a testable helper",
);

assert.match(
  terminalMobileKeysJs,
  /requestJson\(`\/api\/file\?path=\$\{encodeURIComponent\(configPath\)\}`\)/,
  "project URL action should read the current project's configuration through the workspace API",
);

assert.match(
  terminalMobileKeysJs,
  /popup\.location\.replace\(projectUrl\)/,
  "project URL action should open the resolved web entry in a new tab",
);

assert.match(
  terminalHtml,
  /styles-terminal\.css\?v=20260804b/,
  "terminal html should reference the split terminal stylesheet version",
);

assert.match(
  terminalHtml,
  /styles-responsive\.css\?v=20260727b/,
  "terminal html should refresh the responsive stylesheet after FAB sizing changes",
);

assert.match(
  terminalHtml,
  /terminal-command-quickstart\.js\?v=20260803b/,
  "terminal html should reference the bumped terminal-command-quickstart version",
);

assert.match(
  terminalHtml,
  /terminal-tools\.js\?v=20260725a/,
  "terminal html should reference the bumped terminal-tools version",
);

assert.match(
  terminalHtml,
  /terminal-settings-loader\.js\?v=20260803f/,
  "terminal html should reference the bumped terminal-settings-loader version",
);

assert.match(
  terminalHtml,
  /terminal-mobile-keys\.js\?v=20260812a/,
  "terminal html should reference the bumped terminal-mobile-keys version",
);

assert.match(
  terminalHtml,
  /terminal\.js\?v=20260810a/,
  "terminal html should reference the bumped terminal.js version",
);
