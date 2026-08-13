import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const terminalSettings = require("../static/terminal-settings.js");
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");

const legacyDeployCommand = {
  key: "deploy_project",
  label: "本项目部署脚本",
  action: "deploy_project",
  command: "",
  shortcut: "Ctrl+B",
};

assert.equal(
  terminalSettings
    .ensureBuiltInTerminalFunctionCommands([legacyDeployCommand])
    .some((command) => command.key === "deploy_project" || command.action === "deploy_project"),
  false,
  "saved deploy commands should be removed from the general-purpose menu",
);

assert.equal(
  terminalSettings
    .ensureBuiltInTerminalSlashCommands([legacyDeployCommand])
    .some((command) => command.key === "deploy_project" || command.action === "deploy_project"),
  false,
  "saved deploy commands should be removed from the slash/quick menu",
);

assert.equal(
  terminalSettings.DEFAULT_TERMINAL_FUNCTION_COMMANDS.some(
    (command) => command.key === "deploy_project" || command.action === "deploy_project",
  ),
  false,
  "general-purpose defaults should not own the project deploy command",
);

assert.equal(
  terminalSettings.DEFAULT_TERMINAL_SLASH_COMMANDS.some(
    (command) => command.key === "deploy_project" || command.action === "deploy_project",
  ),
  false,
  "slash/quick defaults should not own the project deploy command",
);

assert.match(
  terminalHtml,
  /<option value="deploy_project" data-shortcut="Ctrl\+B">本项目部署脚本<\/option>/,
  "the project commands menu should own both deploy and its Ctrl+B shortcut",
);
