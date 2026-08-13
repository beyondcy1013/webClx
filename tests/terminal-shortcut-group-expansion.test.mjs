import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const appJs = readFileSync(new URL("../static/app.js", import.meta.url), "utf8");
const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const terminalCommandsJs = readFileSync(
  new URL("../static/app-terminal-commands.js", import.meta.url),
  "utf8",
);
const stylesSettingsCss = readFileSync(
  new URL("../static/styles-settings.css", import.meta.url),
  "utf8",
);

// 快捷键子 Tab 的“多功能按钮”应该使用选择复选框控制子命令行的可见性：
// 未勾选（未选择）时不渲染对应子命令行，勾选后才显示子命令快捷键行。
assert.match(
  appJs,
  /terminalShortcutExpandedGroups:\s*\{\s*slash:\s*false,\s*function:\s*false\s*\}/,
  "settings state should declare default shortcut expansion flags per multi-function group",
);

assert.match(
  indexHtml,
  /<th>\s*选择\s*<\/th>[\s\S]*<th>\s*菜单\s*<\/th>[\s\S]*<th>\s*命令\s*<\/th>[\s\S]*<th>\s*动作\s*<\/th>[\s\S]*<th>\s*快捷键\s*<\/th>/,
  "shortcuts table should expose a leading 选择 column to control sub-command visibility",
);

assert.match(
  indexHtml,
  /勾选多功能按钮（\/斜杠、功能）才会显示该组下的子命令快捷键/,
  "shortcuts panel help text should explain the new select-to-expand behavior",
);

assert.match(
  terminalCommandsJs,
  /const TERMINAL_SHORTCUT_EXPANDED_GROUPS_STORAGE_KEY\s*=\s*"webclx:terminal-shortcut-expanded-groups"/,
  "shortcut group expansion state should be persisted via localStorage",
);

assert.match(
  terminalCommandsJs,
  /const TERMINAL_SHORTCUT_ALL_GROUP_KEYS\s*=\s*\[\s*"slash"\s*,\s*"function"\s*\]/,
  "shortcut group expansion registry should enumerate the two multi-function groups",
);

assert.match(
  terminalCommandsJs,
  /function\s+loadTerminalShortcutExpandedGroups\s*\(\)\s*\{[\s\S]*?TERMINAL_SHORTCUT_ALL_GROUP_KEYS\.forEach\(\(key\)\s*=>\s*\{\s*result\[key\]\s*=\s*false;\s*\}\)/,
  "expansion state should default every multi-function group to collapsed (未选择)",
);

assert.match(
  terminalCommandsJs,
  /function\s+persistTerminalShortcutExpandedGroups[\s\S]*?window\.localStorage\.setItem\(\s*TERMINAL_SHORTCUT_EXPANDED_GROUPS_STORAGE_KEY/,
  "expansion toggles should persist through window.localStorage",
);

assert.match(
  terminalCommandsJs,
  /function\s+setTerminalShortcutGroupExpanded[\s\S]*?state\.terminalShortcutExpandedGroups\[groupKey\]\s*=\s*Boolean\(expanded\)[\s\S]*?persistTerminalShortcutExpandedGroups/,
  "setTerminalShortcutGroupExpanded should update state and persist",
);

// 渲染逻辑：仅在对应组被勾选时才追加子命令行；未勾选时只渲染组头复选框行。
assert.match(
  terminalCommandsJs,
  /function\s+renderTerminalShortcutSettings\s*\(\)\s*\{[\s\S]*?renderTerminalShortcutGroupHeader[\s\S]*?if\s*\(\s*!expanded\s*\)\s*\{\s*return;\s*\}/,
  "renderTerminalShortcutSettings must skip sub-command rows when the group is not selected",
);

// 头行的复选框必须联动展开状态。
assert.match(
  terminalCommandsJs,
  /function\s+renderTerminalShortcutGroupHeader[\s\S]*?checkbox\.addEventListener\(\s*"change"\s*,\s*\(\)\s*=>\s*\{\s*setTerminalShortcutGroupExpanded\(group\.key,\s*checkbox\.checked\);\s*\}\s*\)/,
  "group header checkbox should drive setTerminalShortcutGroupExpanded",
);

// 状态读取 helper 必须按 groupKey 查询，未勾选时返回 false。
assert.match(
  terminalCommandsJs,
  /function\s+isTerminalShortcutGroupExpanded[\s\S]*?Boolean\(map\[groupKey\]\)/,
  "isTerminalShortcutGroupExpanded should read the per-group boolean",
);

// CSS 必须给出头行的视觉强调以及选择列的居中样式。
assert.match(
  stylesSettingsCss,
  /\.terminal-shortcut-group-row\s*\{[\s\S]*?background:/,
  "group header row should have a distinct background to separate from sub-command rows",
);

assert.match(
  stylesSettingsCss,
  /\.terminal-shortcut-group-toggle\s*\{[\s\S]*?cursor:\s*pointer/,
  "group header checkbox should be styled as an interactive control",
);

assert.match(
  stylesSettingsCss,
  /\.terminal-shortcut-subcommand-row\s+\.terminal-shortcut-input\s*\{/,
  "sub-command shortcut input should keep a dedicated font size override",
);
