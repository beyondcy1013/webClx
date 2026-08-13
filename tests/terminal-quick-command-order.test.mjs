import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const terminalSettingsJs = readFileSync(
  new URL("../static/terminal-settings.js", import.meta.url),
  "utf8",
);
const context = { module: { exports: {} }, exports: {} };
vm.runInNewContext(terminalSettingsJs, context);

const settings = context.module.exports;
const legacyCommands = [
  { key: "status", label: "/status", action: "send_slash_command", command: "/status" },
  { key: "copy_id_and_ask", label: "旧提问", action: "send_text", command: "旧命令" },
  { key: "quota", label: "套餐", action: "open_quota_dialog", command: "" },
  { key: "webui", label: "WebUI", action: "open_project_url", command: "" },
  { key: "current_resume_id", label: "session ID", action: "copy_current_resume_id", command: "" },
  { key: "resume", label: "/resume", action: "send_slash_command", command: "/resume" },
  { key: "extract_current_session", label: "高级复制", action: "extract_current_session", command: "" },
  { key: "copy_resume_id", label: "屏幕提取id", action: "copy_resume_id", command: "" },
  { key: "extract_resume", label: "屏幕提取id并恢复", action: "extract_resume", command: "" },
  { key: "copy_terminal_name", label: "复制终端名", action: "copy_terminal_name", command: "" },
];

const ordered = settings.ensureBuiltInTerminalSlashCommands(legacyCommands);
const keys = ordered.map((command) => command.key);
const idKeys = [
  "extract_resume",
  "copy_resume_id",
  "extract_current_session",
  "current_resume_id",
  "copy_id_and_ask",
];

assert.equal(
  ordered.some((command) => command.key === "quota" || command.action === "open_quota_dialog"),
  false,
  "套餐 should be removed from 快捷 because 全能 already owns it",
);
assert.equal(
  ordered.some((command) => command.key === "webui" || command.action === "open_project_url"),
  false,
  "WebUI should be removed from 快捷 because 项目管理 already owns 项目 URL",
);

const firstSlashIndex = ordered.findIndex(
  (command) => command.action === "send_slash_command" || command.command.startsWith("/"),
);
assert.notEqual(firstSlashIndex, -1, "快捷 should retain slash commands");
assert.deepEqual(
  Array.from(keys.slice(firstSlashIndex - idKeys.length - 1, firstSlashIndex - 1)),
  idKeys,
  "Session ID actions should be contiguous above copy-terminal-name and slash commands",
);
assert.equal(
  keys[firstSlashIndex - 1],
  "copy_terminal_name",
  "复制终端名 should be fifth from the bottom immediately above /resume",
);
assert.equal(
  ordered.slice(firstSlashIndex).every(
    (command) => command.action === "send_slash_command" || command.command.startsWith("/"),
  ),
  true,
  "all slash commands should be concentrated at the bottom",
);
